//! 存储集群的节点侧执行。
//!
//! # 这里是全系统唯一会销毁数据的地方
//!
//! `prepare` 会清空磁盘。因此它在动手之前**重新核对现实**，而不是信任计划：
//!
//! 1. 设备必须仍然存在；
//! 2. 容量必须与计划记录的一致——换盘、扩容都会让这条不成立；
//! 3. 设备必须仍然处于「无任何使用痕迹」的状态（未挂载、无文件系统、无分区表、
//!    非系统盘、不属于 LVM/RAID/加密卷）；
//! 4. 挂载点必须位于 `/data` 之下，且不能已在使用。
//!
//! 任何一条不成立都直接拒绝，整个节点一块盘都不动。主控已经做过同样的判断，
//! 这里再判一次是刻意的冗余：计划与执行之间现实可能已经变了。
use opsd::protocol::StorageDeviceTarget;
use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::command;
use super::host;

/// 允许的挂载点前缀。避免把盘挂到系统目录上。
pub const MOUNT_PREFIX: &str = "/data";
/// 允许的文件系统。
pub const FILESYSTEMS: &[&str] = &["xfs", "ext4"];

/// 挂载点是否落在 `MOUNT_PREFIX` 之下。
///
/// 必须按路径段比较：`starts_with("/data")` 会连 `/database` 一起放行。
pub fn mount_allowed(mount: &str) -> bool {
    mount == MOUNT_PREFIX
        || mount
            .strip_prefix(MOUNT_PREFIX)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// 对单块设备重新核对现实——**格式化前的最后一道防线**。
///
/// 做成纯函数（只读 `HostInventory`，不碰任何设备）是为了让它可被穷举测试：
/// 这段判断一旦放松，就会真的清掉一块有数据的盘。
///
/// 四条：设备还在、容量与计划一致、有可用性判断、且当前仍无任何使用痕迹。
pub fn verify_device<'a>(
    target: &StorageDeviceTarget,
    inventory: &'a opsd::protocol::HostInventory,
) -> std::result::Result<&'a opsd::protocol::BlockDevice, String> {
    let name = target.path.rsplit('/').next().unwrap_or_default();
    let Some(device) = inventory.devices.iter().find(|d| d.name == name) else {
        return Err(format!("{}：设备已不存在", target.path));
    };
    if device.size != target.expected_size {
        return Err(format!(
            "{}：容量与计划不符（计划 {} 字节，实际 {} 字节）",
            target.path, target.expected_size, device.size
        ));
    }
    match inventory.risks.iter().find(|r| r.device == name) {
        None => Err(format!("{}：缺少该设备的可用性判断", target.path)),
        Some(risk) if !risk.usable() => Err(format!(
            "{}：当前不满足「无任何使用痕迹」条件（{}）",
            target.path,
            risk.reasons.join("；")
        )),
        Some(_) => Ok(device),
    }
}

/// 准备单块磁盘：清空、建文件系统、挂载。
///
/// 返回每一步的结果，便于在界面上说明到底做到了哪一步。
async fn prepare_device(
    target: &StorageDeviceTarget,
    filesystem: &str,
) -> Result<Value> {
    anyhow::ensure!(
        FILESYSTEMS.contains(&filesystem),
        "不支持的文件系统：{filesystem}"
    );
    anyhow::ensure!(
        target.path.starts_with("/dev/"),
        "设备路径不合法：{}",
        target.path
    );
    anyhow::ensure!(
        mount_allowed(&target.mount),
        "挂载点必须位于 {MOUNT_PREFIX} 之下：{}",
        target.mount
    );

    // ---- 重新核对现实 ----
    let inventory = host::inspect().await;
    let device = verify_device(target, &inventory)
        .map_err(|problem| anyhow::anyhow!("{problem}，已拒绝格式化"))?;

    // 挂载点不能已经被占用
    if std::path::Path::new(&target.mount).exists() {
        let occupied = std::fs::read_dir(&target.mount)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
        anyhow::ensure!(
            !occupied,
            "挂载点 {} 已存在且非空，请先清理",
            target.mount
        );
    }

    // ---- 清空并建文件系统 ----
    // wipefs 只抹除签名，比 dd 覆盖整盘快得多，且足够让内核忘掉旧的卷
    command("/usr/sbin/wipefs", &["-a", &target.path], 60)
        .await
        .with_context(|| format!("清除 {} 的旧签名失败", target.path))?;
    // mkfs 会重建超级块；-f 用于覆盖可能残留的文件系统。
    // 具体命令由文件系统参数决定，不接受调用方给出任意可执行文件。
    let (mkfs, arguments): (&str, Vec<&str>) = match filesystem {
        "xfs" => ("/usr/sbin/mkfs.xfs", vec!["-f", &target.path]),
        "ext4" => ("/usr/sbin/mkfs.ext4", vec!["-F", &target.path]),
        other => anyhow::bail!("不支持的文件系统：{other}"),
    };
    command(mkfs, &arguments, 300)
        .await
        .with_context(|| format!("在 {} 上创建 {filesystem} 失败", target.path))?;

    std::fs::create_dir_all(&target.mount)
        .with_context(|| format!("创建挂载点失败：{}", target.mount))?;
    command(
        "/usr/bin/mount",
        &["-o", "defaults,noatime", &target.path, &target.mount],
        60,
    )
    .await
    .with_context(|| format!("挂载 {} 到 {} 失败", target.path, target.mount))?;

    Ok(json!({
        "device": target.path,
        "size": device.size,
        "mount": target.mount,
        "filesystem": filesystem,
    }))
}

/// 准备一个节点上的全部磁盘。
///
/// **先在内存里判定完所有设备，再动手**：只要有一块不符合条件就整节点拒绝，
/// 不会出现"清了一半才发现第三块盘不对"的局面。
pub async fn prepare(
    cluster: &str,
    plan_id: &str,
    devices: &[StorageDeviceTarget],
    filesystem: &str,
) -> Result<Value> {
    // 先校验参数，再看环境：参数错了就该立刻报参数错，
    // 而不是走到一半才说设备不存在
    anyhow::ensure!(!devices.is_empty(), "计划中没有设备");
    anyhow::ensure!(
        !cluster.is_empty() && !plan_id.is_empty(),
        "缺少集群或计划标识"
    );
    anyhow::ensure!(
        FILESYSTEMS.contains(&filesystem),
        "不支持的文件系统：{filesystem}"
    );
    for target in devices {
        anyhow::ensure!(
            target.path.starts_with("/dev/"),
            "设备路径必须以 /dev/ 开头：{}",
            target.path
        );
        anyhow::ensure!(
            mount_allowed(&target.mount),
            "挂载点必须位于 {MOUNT_PREFIX} 之下：{}",
            target.mount
        );
    }
    let inventory = host::inspect().await;
    // 先在内存里把所有设备判完；判定与真正动手时用的是同一个函数
    let problems: Vec<String> = devices
        .iter()
        .filter_map(|target| verify_device(target, &inventory).err())
        .collect();
    anyhow::ensure!(
        problems.is_empty(),
        "以下设备不满足格式化条件，本次未做任何改动：{}",
        problems.join("；")
    );

    let mut results = Vec::new();
    for target in devices {
        results.push(prepare_device(target, filesystem).await?);
    }
    tracing::warn!(
        cluster,
        plan_id,
        devices = devices.len(),
        "已完成磁盘格式化与挂载"
    );
    Ok(json!({
        "cluster": cluster,
        "plan_id": plan_id,
        "prepared": results,
        "notice": "这些磁盘上的原有数据已被清除，且不可恢复",
    }))
}

/// 生成 Compose 文件内容。
///
/// 由结构化参数生成，**不接收来自主控的任意 Compose 文本**：这样命令与配置都不是
/// 拼出来的，也就不存在注入面。
pub fn compose(
    cluster: &str,
    image: &str,
    mounts: &[String],
    peers: &[String],
    access_key: &str,
    secret_key: &str,
) -> Result<String> {
    anyhow::ensure!(!mounts.is_empty(), "没有可用挂载点");
    anyhow::ensure!(!peers.is_empty(), "缺少集群拓扑");
    anyhow::ensure!(
        !image.contains(char::is_whitespace),
        "镜像名不能包含空白"
    );
    anyhow::ensure!(
        image.starts_with("pgsty/silo:"),
        "只允许使用固定的 silo 镜像"
    );
    anyhow::ensure!(
        !image.ends_with(":latest"),
        "不接受 latest 标签"
    );
    let volumes: Vec<String> = mounts
        .iter()
        .map(|mount| format!("      - {mount}:/data{mount}"))
        .collect();
    Ok(format!(
        "# 由 opsd 生成；请勿手工修改，改动会在下次部署时被覆盖\n\
         services:\n  \
         {cluster}:\n    \
         image: {image}\n    \
         container_name: {cluster}\n    \
         restart: unless-stopped\n    \
         command: server {args} --console-address :9001\n    \
         environment:\n      \
         MINIO_ROOT_USER: \"{access_key}\"\n      \
         MINIO_ROOT_PASSWORD: \"{secret_key}\"\n    \
         volumes:\n{volumes}\n    \
         ports:\n      \
         - \"9000:9000\"\n      \
         - \"127.0.0.1:9001:9001\"\n",
        args = peers.join(" "),
        volumes = volumes.join("\n"),
    ))
}

/// Compose 文件与凭据文件的存放位置。
pub fn cluster_directory(data_dir: &std::path::Path, cluster: &str) -> std::path::PathBuf {
    data_dir.join("storage").join(cluster)
}

/// 部署集群容器。
pub async fn deploy(
    data_dir: &std::path::Path,
    cluster: &str,
    plan_id: &str,
    image: &str,
    mounts: &[String],
    peers: &[String],
    access_key: &str,
    secret_key: &str,
) -> Result<Value> {
    let directory = cluster_directory(data_dir, cluster);
    std::fs::create_dir_all(&directory)?;
    let content = compose(cluster, image, mounts, peers, access_key, secret_key)?;
    // Compose 文件含凭据，因此权限收紧到 0600
    let file = directory.join("compose.yml");
    opsd::pki::private_write(&file, content.as_bytes())?;

    command(
        "/usr/bin/docker",
        &[
            "compose",
            "-f",
            &file.display().to_string(),
            "up",
            "-d",
            "--remove-orphans",
        ],
        600,
    )
    .await
    .with_context(|| format!("启动集群容器失败：{cluster}"))?;

    tracing::warn!(cluster, plan_id, "存储集群容器已启动");
    Ok(json!({
        "cluster": cluster,
        "plan_id": plan_id,
        "compose": file.display().to_string(),
        "image": image,
    }))
}

/// 读取集群健康与容量。用 `mcli` 而不是直连 API，避免在 Agent 里再实现一套 S3 客户端。
pub async fn status(cluster: &str) -> Result<Value> {
    let target = format!("local/{cluster}");
    let info = command(
        "/usr/local/bin/mcli",
        &["admin", "info", &target, "--json"],
        60,
    )
    .await;
    match info {
        Ok(text) => {
            let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            Ok(json!({ "cluster": cluster, "info": parsed }))
        }
        Err(error) => {
            // 采不到就明确说明，不给出"看起来正常"的空结果
            Ok(json!({
                "cluster": cluster,
                "error": format!("读取集群状态失败：{error}"),
            }))
        }
    }
}

/// 创建或删除桶。
pub async fn bucket(cluster: &str, bucket: &str, remove: bool) -> Result<Value> {
    validate_name(bucket)?;
    let target = format!("local/{cluster}/{bucket}");
    let arguments: Vec<&str> = if remove {
        vec!["rb", "--force", &target]
    } else {
        vec!["mb", &target]
    };
    command("/usr/local/bin/mcli", &arguments, 120).await?;
    Ok(json!({ "bucket": bucket, "removed": remove }))
}

/// 创建或删除访问用户。
pub async fn user(
    cluster: &str,
    user: &str,
    secret: &str,
    policy: &str,
    remove: bool,
) -> Result<Value> {
    validate_name(user)?;
    anyhow::ensure!(
        ["readonly", "readwrite", "writeonly", "diagnostics"].contains(&policy),
        "未知策略：{policy}"
    );
    let target = format!("local/{cluster}");
    if remove {
        command("/usr/local/bin/mcli", &["admin", "user", "rm", &target, user], 60).await?;
    } else {
        anyhow::ensure!(secret.len() >= 16, "密钥至少 16 个字符");
        command(
            "/usr/local/bin/mcli",
            &[
                "admin", "user", "add", &target, user, secret, "--policy", policy,
            ],
            60,
        )
        .await?;
    }
    Ok(json!({ "user": user, "policy": policy, "removed": remove }))
}

/// 桶名与用户名只允许 S3 允许的字符集。
fn validate_name(value: &str) -> Result<()> {
    anyhow::ensure!(!value.is_empty() && value.len() <= 63, "名称长度不合法");
    anyhow::ensure!(
        value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
        "名称只能包含小写字母、数字、连字符与点：{value}"
    );
    anyhow::ensure!(
        !value.starts_with('-') && !value.ends_with('-'),
        "名称不能以连字符开头或结尾"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::{BlockDevice, DriveRisk, HostInventory};

    fn target(path: &str, size: u64, mount: &str) -> StorageDeviceTarget {
        StorageDeviceTarget {
            path: path.into(),
            expected_size: size,
            mount: mount.into(),
        }
    }

    /// 一台"现实"：一块 1 TiB 的干净盘 sdb，加上给定的风险标记。
    fn inventory(size: u64, risk: DriveRisk) -> HostInventory {
        HostInventory {
            collected_at: 0,
            hostname: "h".into(),
            distro: "d".into(),
            kernel: "k".into(),
            arch: "x86_64".into(),
            cpu_model: "c".into(),
            cpu_cores: 1,
            memory_total: 1,
            uptime: 1,
            devices: vec![BlockDevice {
                name: "sdb".into(),
                path: "/dev/sdb".into(),
                kind: "disk".into(),
                size,
                rotational: false,
                ..Default::default()
            }],
            risks: vec![risk],
            interfaces: vec![],
            existing: Default::default(),
            smart: None,
            gaps: vec![],
        }
    }

    fn clean(device: &str) -> DriveRisk {
        DriveRisk {
            device: device.into(),
            ..Default::default()
        }
    }

    #[test]
    fn 干净且容量一致的设备可以通过最后一道防线() {
        let world = inventory(1024, clean("sdb"));
        let device = verify_device(&target("/dev/sdb", 1024, "/data/disk1"), &world).unwrap();
        assert_eq!(device.size, 1024);
    }

    #[test]
    fn 设备消失后拒绝格式化() {
        let world = inventory(1024, clean("sdb"));
        let error = verify_device(&target("/dev/sdc", 1024, "/data/disk1"), &world).unwrap_err();
        assert!(error.contains("已不存在"), "{error}");
    }

    #[test]
    fn 容量变化后拒绝格式化() {
        // 计划时 1 TiB，现在只有 500 GiB：换过盘，计划不能再执行
        let world = inventory(500, clean("sdb"));
        let error = verify_device(&target("/dev/sdb", 1024, "/data/disk1"), &world).unwrap_err();
        assert!(error.contains("容量与计划不符"), "{error}");
    }

    #[test]
    fn 计划采集之后盘上出现数据就拒绝格式化() {
        let mut risk = clean("sdb");
        risk.mounted = true;
        risk.has_data = true;
        risk.reasons = vec!["已挂载且存在文件系统".into()];
        let world = inventory(1024, risk);
        let error = verify_device(&target("/dev/sdb", 1024, "/data/disk1"), &world).unwrap_err();
        assert!(error.contains("无任何使用痕迹"), "{error}");
        assert!(error.contains("已挂载且存在文件系统"), "{error}");
    }

    #[test]
    fn 缺少可用性判断比照不安全处理() {
        // 没有风险记录时无法证明它干净，因此必须拒绝，而不是默认放行
        let mut world = inventory(1024, clean("sdb"));
        world.risks.clear();
        let error = verify_device(&target("/dev/sdb", 1024, "/data/disk1"), &world).unwrap_err();
        assert!(error.contains("缺少"), "{error}");
    }

    #[test]
    fn 预检把同一批设备的全部问题一次说清() {
        let mut world = inventory(1024, clean("sdb"));
        world.risks.push(DriveRisk {
            device: "sdc".into(),
            has_partitions: true,
            reasons: vec!["存在分区表".into()],
            ..Default::default()
        });
        world.devices.push(BlockDevice {
            name: "sdc".into(),
            path: "/dev/sdc".into(),
            kind: "disk".into(),
            size: 2048,
            ..Default::default()
        });
        let devices = [
            target("/dev/sdb", 512, "/data/disk1"),  // 容量不符
            target("/dev/sdc", 2048, "/data/disk2"), // 有分区表
            target("/dev/sdd", 2048, "/data/disk3"), // 已不存在
        ];
        let problems: Vec<String> = devices
            .iter()
            .filter_map(|t| verify_device(t, &world).err())
            .collect();
        assert_eq!(problems.len(), 3, "{problems:?}");
        // 一块盘不合格就整节点拒绝，因此问题必须逐块列出、不能只说第一块
        assert!(problems[0].contains("/dev/sdb"));
        assert!(problems[1].contains("/dev/sdc"));
        assert!(problems[2].contains("/dev/sdd"));
    }

    #[test]
    fn 挂载点必须在_data_之下() {
        let bad = target("/dev/sdb", 1, "/etc/data");
        // 通过 prepare 的整体校验检查挂载点前缀
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(prepare("c", "p", &[bad], "xfs"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("/data"), "{error}");
    }

    #[test]
    fn 挂载点前缀按路径段比较() {
        // "/database" 不是 "/data" 的子路径，不能靠 starts_with 蒙混过关
        assert!(mount_allowed("/data"));
        assert!(mount_allowed("/data/disk1"));
        assert!(!mount_allowed("/database"));
        assert!(!mount_allowed("/datax/disk1"));
        assert!(!mount_allowed("/etc/data"));
    }

    #[test]
    fn 拒绝不在_dev_下的设备路径() {
        let bad = target("sdb", 1, "/data/disk1");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(prepare("c", "p", &[bad], "xfs"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("设备"), "{error}");
    }

    #[test]
    fn 拒绝不支持的文件系统() {
        let device = target("/dev/sdb", 1, "/data/disk1");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(prepare("c", "p", &[device], "ntfs"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("文件系统"), "{error}");
    }

    #[test]
    fn 缺少标识时拒绝执行() {
        let device = target("/dev/sdb", 1, "/data/disk1");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(prepare("", "p", &[device], "xfs"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("集群或计划"), "{error}");
    }

    #[test]
    fn compose_只接受固定仓库与具体标签() {
        let mounts = vec!["/data/disk1".to_string()];
        let peers = vec!["http://a/data/disk1".to_string()];
        // 固定标签可用
        assert!(
            compose("c", "pgsty/silo:RELEASE.2026-09-03T13-18-01Z", &mounts, &peers, "u", "s")
                .is_ok()
        );
        // latest 被拒
        let error = compose("c", "pgsty/silo:latest", &mounts, &peers, "u", "s")
            .unwrap_err()
            .to_string();
        assert!(error.contains("latest"), "{error}");
        // 其他仓库被拒
        assert!(compose("c", "minio/minio:1.0", &mounts, &peers, "u", "s").is_err());
        // 含空白的镜像名被拒
        assert!(
            compose("c", "pgsty/silo:x --privileged", &mounts, &peers, "u", "s").is_err()
        );
    }

    #[test]
    fn compose_生成的拓扑与挂载完整() {
        let mounts = vec!["/data/disk1".to_string(), "/data/disk2".to_string()];
        let peers = vec![
            "http://a/data/disk1".to_string(),
            "http://a/data/disk2".to_string(),
            "http://b/data/disk1".to_string(),
            "http://b/data/disk2".to_string(),
        ];
        let text = compose(
            "silo-prod",
            "pgsty/silo:RELEASE.2026-09-03T13-18-01Z",
            &mounts,
            &peers,
            "opsdadmin",
            "a-very-long-secret-key",
        )
        .unwrap();
        assert!(text.contains("image: pgsty/silo:RELEASE."));
        // 全部对等节点都要出现在 server 参数里
        for peer in &peers {
            assert!(text.contains(peer), "缺少对等节点 {peer}");
        }
        // 本节点两块盘都要挂进去
        assert!(text.contains("/data/disk1:/data/data/disk1"));
        assert!(text.contains("/data/disk2:/data/data/disk2"));
        // 控制台只监听回环，不作为对外服务暴露
        assert!(text.contains("127.0.0.1:9001:9001"));
        assert!(text.contains("MINIO_ROOT_USER"));
    }

    #[test]
    fn compose_拒绝空挂载与空拓扑() {
        let peers = vec!["http://a/data/disk1".to_string()];
        assert!(compose("c", "pgsty/silo:1", &[], &peers, "u", "s").is_err());
        let mounts = vec!["/data/disk1".to_string()];
        assert!(compose("c", "pgsty/silo:1", &mounts, &[], "u", "s").is_err());
    }

    #[test]
    fn 名称校验只放行_s3_允许的字符集() {
        for good in ["photos", "my-bucket", "a.b.c", "b1"] {
            validate_name(good).unwrap_or_else(|e| panic!("{good} 应通过：{e}"));
        }
        for bad in [
            "",
            "Upper",
            "-leading",
            "trailing-",
            "with space",
            "with/slash",
            "with_underscore",
            &"x".repeat(64),
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?} 应被拒绝");
        }
    }

    #[test]
    fn 集群目录位于数据目录之下() {
        let base = std::path::Path::new("/var/lib/opsd-agent");
        let directory = cluster_directory(base, "silo-prod");
        assert!(directory.starts_with(base));
        assert!(directory.ends_with("storage/silo-prod"));
    }
}
