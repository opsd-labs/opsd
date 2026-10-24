use anyhow::{Context as _, Result, bail, ensure};
use opsd_plugin_sdk::{self as sdk, Request};
use serde_json::{Value, json};
use std::path::Path;

fn path(text: &str) -> Result<std::path::PathBuf> {
    ensure!(
        text.starts_with('/') && text != "/" && !text.contains(['\n', '\r', '\0', ':']),
        "数据目录必须是非根目录的绝对路径"
    );
    ensure!(
        Path::new(text).components().all(|c| matches!(
            c,
            std::path::Component::RootDir | std::path::Component::Normal(_)
        )),
        "数据路径不能包含相对跳转"
    );
    for denied in [
        "/proc",
        "/sys",
        "/dev",
        "/run",
        "/etc",
        "/var/lib/opsd-agent",
    ] {
        ensure!(
            text != denied && !text.starts_with(&format!("{denied}/")),
            "不能将系统或 Agent 目录作为数据路径"
        );
    }
    Ok(text.into())
}
async fn command(executable: &str, args: &[&str], timeout: u64) -> Result<String> {
    sdk::command(executable, &sdk::strings(args), &[], None, timeout).await
}

async fn filesystem_tools() -> Result<()> {
    if command("sh", &["-c", "command -v mkfs.xfs"], 10)
        .await
        .is_ok()
    {
        return Ok(());
    }
    if Path::new("/usr/bin/apt-get").exists() {
        command("apt-get", &["update"], 300).await?;
        sdk::command(
            "apt-get",
            &sdk::strings(&["install", "-y", "--no-install-recommends", "xfsprogs"]),
            &[("DEBIAN_FRONTEND".into(), "noninteractive".into())],
            None,
            300,
        )
        .await?;
    } else {
        let manager = if Path::new("/usr/bin/dnf").exists() {
            "dnf"
        } else {
            "yum"
        };
        command(manager, &["install", "-y", "xfsprogs"], 300).await?;
    }
    Ok(())
}

async fn new_disk(cluster: &str, selection: &Value) -> Result<Value> {
    let device = selection["device"].as_str().context("未选择具体设备")?;
    let mount = selection["mount"].as_str().context("未指定挂载目录")?;
    let mount_path = path(mount)?;
    ensure!(
        device.starts_with("/dev/") && !device.contains(['\n', '\r']),
        "设备路径无效"
    );
    let size = selection["expected_size"]
        .as_u64()
        .context("缺少用户确认的设备容量")?;
    let inventory = opsd::host::inspect().await;
    let disk = inventory
        .devices
        .iter()
        .find(|d| d.path == device)
        .context("所选磁盘已不存在")?;
    ensure!(sdk::identifier(&disk.name), "设备名称无效");
    ensure!(
        disk.kind == "disk" && disk.size == size,
        "设备类型或容量已变化"
    );
    let serial = selection["serial"].as_str().unwrap_or("");
    if !serial.is_empty() {
        ensure!(disk.serial.as_deref() == Some(serial), "设备序列号已变化");
    }
    let journal_path = sdk::node_directory()?.join(format!("disk-{}-{}.json", cluster, disk.name));
    if mount_path.exists() {
        ensure!(
            std::fs::canonicalize(&mount_path)? == mount_path,
            "挂载目录不能经过符号链接"
        );
    }
    if journal_path.exists() {
        let previous: Value = serde_json::from_slice(&std::fs::read(&journal_path)?)?;
        ensure!(
            previous["device"] == device && previous["mount"] == mount,
            "设备准备记录与当前选择不一致"
        );
        let uuid = previous["uuid"]
            .as_str()
            .context("之前的格式化执行中断，需要先核实设备，不能自动重试")?;
        ensure!(
            disk.uuid.as_deref() == Some(uuid),
            "已准备磁盘的文件系统身份发生变化"
        );
        let unit = previous["unit"].as_str().context("缺少挂载单元")?;
        command("systemctl", &["start", unit], 60).await?;
        verify_mount(mount, Some(uuid)).await?;
        return Ok(previous);
    }
    ensure!(
        inventory
            .risks
            .iter()
            .any(|r| r.device == disk.name && r.usable()),
        "所选磁盘仍有使用痕迹，不能作为新盘格式化"
    );
    std::fs::create_dir_all(&mount_path)?;
    ensure!(
        std::fs::canonicalize(&mount_path)? == mount_path,
        "挂载目录不能经过符号链接"
    );
    ensure!(
        std::fs::read_dir(&mount_path)?.next().is_none(),
        "挂载目录已有内容"
    );
    sdk::private_write(
        &journal_path,
        serde_json::to_string(&json!({"device":device,"mount":mount,"stage":"formatting"}))?
            .as_bytes(),
    )?;
    command("wipefs", &["-a", device], 60).await?;
    command("mkfs.xfs", &["-f", device], 300).await?;
    let uuid = command("blkid", &["-s", "UUID", "-o", "value", device], 10)
        .await?
        .trim()
        .to_owned();
    ensure!(
        !uuid.is_empty() && uuid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "未取得新文件系统 UUID"
    );
    std::fs::create_dir_all(&mount_path)?;
    let unit = command("systemd-escape", &["--path", "--suffix=mount", mount], 10)
        .await?
        .trim()
        .to_owned();
    ensure!(
        unit.ends_with(".mount") && !unit.contains('/'),
        "挂载单元名无效"
    );
    let unit_path = Path::new("/etc/systemd/system").join(&unit);
    if unit_path.exists() {
        ensure!(
            std::fs::read_to_string(&unit_path)?.starts_with("# OPSD Silo\n"),
            "已有挂载服务不属于 Silo 插件"
        );
    }
    let content = format!(
        "# OPSD Silo\n[Unit]\nDescription=OPSD Silo 数据盘\nBefore=local-fs.target docker.service\n\n[Mount]\nWhat=/dev/disk/by-uuid/{uuid}\nWhere={mount}\nType=xfs\nOptions=defaults,noatime\n\n[Install]\nWantedBy=local-fs.target\n"
    );
    sdk::private_write(&unit_path, content.as_bytes())?;
    let prepared = json!({"device":device,"mount":mount,"uuid":uuid,"unit":unit,"stage":"mounted"});
    sdk::private_write(&journal_path, serde_json::to_vec(&prepared)?.as_slice())?;
    command("systemctl", &["daemon-reload"], 30).await?;
    command("systemctl", &["enable", "--now", &unit], 60).await?;
    verify_mount(mount, Some(&uuid)).await?;
    Ok(prepared)
}

async fn verify_mount(directory: &str, uuid: Option<&str>) -> Result<()> {
    let _: Value = serde_json::from_str(&command("findmnt", &["-J", "-T", directory], 10).await?)?;
    if let Some(uuid) = uuid {
        let actual = command(
            "findmnt",
            &["-n", "-o", "UUID", "--mountpoint", directory],
            10,
        )
        .await?;
        ensure!(actual.trim() == uuid, "数据盘没有挂载到指定目录，停止部署");
    }
    Ok(())
}

async fn filesystem(directory: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(
        &command(
            "findmnt",
            &["-J", "-T", directory, "-o", "TARGET,SOURCE,FSTYPE,UUID"],
            10,
        )
        .await?,
    )?;
    value["filesystems"][0]
        .as_object()
        .context("没有取得数据目录所在文件系统")?;
    Ok(value["filesystems"][0].clone())
}
async fn verify_prepared(prepared: &Value) -> Result<()> {
    for detail in prepared["details"]
        .as_array()
        .context("缺少已准备目录记录")?
    {
        let mount = detail["mount"].as_str().context("缺少挂载目录")?;
        ensure!(Path::new(mount).is_dir(), "数据目录已消失，停止部署");
        let canonical = std::fs::canonicalize(path(mount)?)?;
        ensure!(canonical == Path::new(mount), "数据目录指向已变化");
        verify_mount(mount, detail["uuid"].as_str()).await?;
        if let Some(expected) = detail.get("filesystem") {
            ensure!(
                filesystem(mount).await? == *expected,
                "数据目录所在文件系统已变化"
            );
        }
    }
    Ok(())
}

async fn validate_selections(cluster: &str, selections: &[Value]) -> Result<()> {
    ensure!(
        sdk::identifier(cluster) && !selections.is_empty(),
        "集群或数据盘选择无效"
    );
    let mut unique = std::collections::HashSet::new();
    let mut devices = std::collections::HashSet::new();
    // 先核对全部选择，再执行任何格式化。
    let inventory = opsd::host::inspect().await;
    for selected in selections {
        ensure!(
            ["new", "existing"].contains(&selected["mode"].as_str().unwrap_or("")),
            "必须明确选择新盘或已有路径"
        );
        let directory = if selected["mode"] == "new" {
            selected["mount"].as_str()
        } else {
            selected["path"].as_str()
        }
        .context("缺少目录")?;
        let selected_path = path(directory)?;
        if selected_path.exists() {
            ensure!(
                std::fs::canonicalize(&selected_path)? == selected_path,
                "数据目录不能经过符号链接"
            );
        }
        ensure!(
            unique
                .iter()
                .all(|other: &&str| !Path::new(directory).starts_with(other)
                    && !Path::new(other).starts_with(directory)),
            "数据目录不能相互嵌套"
        );
        ensure!(unique.insert(directory), "数据目录重复");
        if selected["mode"] == "new" {
            let device = selected["device"].as_str().context("缺少设备")?;
            ensure!(devices.insert(device), "同一设备不能重复选择");
            let disk = inventory
                .devices
                .iter()
                .find(|d| d.path == device)
                .context("设备不存在")?;
            ensure!(
                sdk::identifier(&disk.name) && disk.kind == "disk",
                "必须选择完整的新磁盘"
            );
            let serial = selected["serial"].as_str().unwrap_or("");
            ensure!(
                serial.is_empty() || disk.serial.as_deref() == Some(serial),
                "磁盘序列号已变化"
            );
            ensure!(
                disk.size == selected["expected_size"].as_u64().context("缺少容量")?,
                "磁盘容量已变化"
            );
            let journal = sdk::node_directory()?.join(format!("disk-{cluster}-{}.json", disk.name));
            let unit = command(
                "systemd-escape",
                &["--path", "--suffix=mount", directory],
                10,
            )
            .await?;
            let unit_path = Path::new("/etc/systemd/system").join(unit.trim());
            if unit_path.exists() {
                ensure!(
                    journal.exists()
                        && std::fs::read_to_string(unit_path)?.starts_with("# OPSD Silo\n"),
                    "挂载单元已有其他用途，不能覆盖"
                );
            }
            if journal.exists() {
                let previous: Value = serde_json::from_slice(&std::fs::read(&journal)?)?;
                ensure!(
                    previous["device"] == device && previous["mount"] == directory,
                    "磁盘准备记录与选择不同"
                );
                let uuid = previous["uuid"]
                    .as_str()
                    .context("上次格式化中断，需要人工核实设备")?;
                ensure!(disk.uuid.as_deref() == Some(uuid), "已准备磁盘身份不同");
            } else if selected_path.exists() {
                ensure!(
                    std::fs::read_dir(&selected_path)?.next().is_none(),
                    "新盘挂载目录已有内容"
                );
            }
            ensure!(
                journal.exists()
                    || inventory
                        .risks
                        .iter()
                        .any(|r| r.device == disk.name && r.usable()),
                "磁盘仍有使用痕迹"
            );
        } else {
            ensure!(Path::new(directory).is_dir(), "已有路径不存在或不是目录");
            ensure!(
                !Path::new(directory).join(".minio.sys").exists(),
                "目录已有对象存储数据，不能作为新集群初始化"
            );
            filesystem(directory).await?;
        }
    }

    Ok(())
}

pub async fn execute(request: Request) -> Result<Value> {
    match request.path.as_str() {
        "identity.prepare" => sdk::identity().await,
        "inventory" => Ok(serde_json::to_value(opsd::host::inspect().await)?),
        "storage.validate" => {
            sdk::require_local_address(
                request.body["address"]
                    .as_str()
                    .context("缺少节点 ET 地址")?,
            )
            .await?;
            let cluster = request.body["cluster"].as_str().context("缺少集群名称")?;
            let selections = request.body["selections"].as_array().context("缺少选择")?;
            let record = sdk::node_directory()?.join(format!("storage-{cluster}.json"));
            if record.exists() {
                let previous: Value = serde_json::from_slice(&std::fs::read(record)?)?;
                ensure!(
                    previous["selections"] == json!(selections),
                    "已有目录准备记录与选择不同"
                );
                verify_prepared(&previous).await?;
            } else {
                validate_selections(cluster, selections).await?;
            }
            Ok(json!({"ready":true}))
        }
        "storage.verify" => {
            verify_prepared(&request.body).await?;
            Ok(request.body)
        }
        "tls.install" => {
            let tls = sdk::node_directory()?.join("tls");
            ensure!(tls.join("server-key.pem").exists(), "服务私钥不存在");
            sdk::private_write(
                &tls.join("public.crt"),
                request.body["certificate"]
                    .as_str()
                    .context("缺少服务证书")?
                    .as_bytes(),
            )?;
            sdk::private_write(
                &tls.join("CAs/ca.crt"),
                request.body["ca"]
                    .as_str()
                    .context("缺少主控 CA")?
                    .as_bytes(),
            )?;
            let key = tls.join("private.key");
            if !key.exists() {
                sdk::private_write(&key, &std::fs::read(tls.join("server-key.pem"))?)?;
            }
            Ok(json!({"directory":tls}))
        }
        "storage.prepare" => {
            sdk::require_local_address(
                request.body["address"]
                    .as_str()
                    .context("缺少节点 ET 地址")?,
            )
            .await?;
            let cluster = request.body["cluster"].as_str().context("缺少集群名称")?;
            ensure!(sdk::identifier(cluster), "集群名称无效");
            let selections = request.body["selections"]
                .as_array()
                .context("缺少数据盘选择")?;
            ensure!(!selections.is_empty(), "至少选择一个数据目录");
            let destructive = selections.iter().any(|s| s["mode"] == "new");
            ensure!(
                !destructive || request.body["confirmed"] == true,
                "新盘格式化需要明确确认"
            );
            let record = sdk::node_directory()?.join(format!("storage-{cluster}.json"));
            if record.exists() {
                let previous: Value = serde_json::from_slice(&std::fs::read(&record)?)?;
                ensure!(
                    previous["selections"] == json!(selections),
                    "已有目录准备记录与新选择不一致"
                );
                verify_prepared(&previous).await?;
                return Ok(previous);
            }
            let mut mounts = Vec::new();
            let mut details = Vec::new();
            validate_selections(cluster, selections).await?;
            if destructive {
                filesystem_tools().await?;
            }
            for selected in selections {
                if selected["mode"] == "new" {
                    let mut prepared = new_disk(cluster, selected).await?;
                    let mount = prepared["mount"].as_str().context("挂载路径无效")?;
                    prepared["filesystem"] = filesystem(mount).await?;
                    mounts.push(prepared["mount"].clone());
                    details.push(prepared);
                } else {
                    let directory = selected["path"].as_str().context("缺少目录")?;
                    let canonical = std::fs::canonicalize(path(directory)?)?;
                    path(canonical.to_str().context("目录编码无效")?)?;
                    ensure!(
                        !canonical.join(".minio.sys").exists(),
                        "目录已有对象存储数据，不能作为新集群初始化"
                    );
                    verify_mount(directory, None).await?;
                    mounts.push(json!(canonical));
                    details.push(json!({"mount":canonical,"formatted":false,"filesystem":filesystem(directory).await?}));
                }
            }
            let prepared = json!({"selections":selections,"mounts":mounts,"details":details});
            sdk::private_write(&record, &serde_json::to_vec(&prepared)?)?;
            Ok(prepared)
        }
        _ => bail!("未知 Silo 节点动作"),
    }
}
