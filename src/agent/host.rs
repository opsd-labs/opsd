//! 主机与块设备盘点。
//!
//! 这是存储预检的**唯一数据来源**：预检不在节点上跑任何判断，只把事实带回来，
//! 由控制台集中分析。因此这里的首要目标是**如实**——采集不到的部分要进 `gaps`，
//! 而不是略过；SMART 读不到时 `smart` 保持 `None`，表示「未采集」而不是「健康」。
//!
//! 数据来源：`lsblk -J -O`（块设备拓扑）、`/proc`、`/sys`、`uname`、`/etc/os-release`。
//! 不写任何东西，全部只读。
use opsd::protocol::{
    BlockDevice, DriveRisk, ExistingStorage, HostInterface, HostInventory,
};
use serde_json::Value;

use super::command;

/// 承载这些挂载点的设备一律视为系统盘，绝不参与格式化。
const SYSTEM_MOUNTS: &[&str] = &["/", "/boot", "/boot/efi", "/usr", "/var"];

/// 采集一次主机盘点。任何子项失败都记入 `gaps` 并继续，
/// 因为"一部分读不到"比"整体失败"更常见，也更有诊断价值。
pub async fn inspect() -> HostInventory {
    let mut gaps = Vec::new();

    let devices = match lsblk().await {
        Ok(devices) => devices,
        Err(error) => {
            gaps.push(format!("读取块设备失败：{error}"));
            Vec::new()
        }
    };
    let risks = assess(&devices);
    let existing = existing_storage().await;
    let interfaces = match interfaces().await {
        Ok(list) => list,
        Err(error) => {
            gaps.push(format!("读取网卡失败：{error}"));
            Vec::new()
        }
    };
    // SMART 需要 root 与 smartctl；读不到就明确标记未采集
    let smart = match smart_summary(&devices).await {
        Ok(summary) => summary,
        Err(error) => {
            gaps.push(format!("SMART 未采集：{error}"));
            None
        }
    };

    HostInventory {
        collected_at: opsd::protocol::now(),
        hostname: read_trim("/proc/sys/kernel/hostname").unwrap_or_else(|| "未知".into()),
        distro: distro(),
        kernel: read_trim("/proc/sys/kernel/osrelease").unwrap_or_else(|| "未知".into()),
        arch: std::env::consts::ARCH.into(),
        cpu_model: cpu_model(),
        cpu_cores: std::thread::available_parallelism()
            .map(|n| n.get() as u64)
            .unwrap_or(0),
        memory_total: memory_total(),
        uptime: uptime(),
        devices,
        risks,
        interfaces,
        existing,
        smart,
        gaps,
    }
}

fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// `lsblk -J -O` 的 JSON 输出解析。
///
/// `-O` 会输出全部可用列，不同 util-linux 版本差异较大：
/// 2.37 起挂载点是数组 `mountpoints`，更早是单值 `mountpoint`。两者都要认。
pub fn parse_lsblk(text: &str) -> anyhow::Result<Vec<BlockDevice>> {
    let root: Value = serde_json::from_str(text)?;
    let list = root
        .get("blockdevices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("lsblk 输出缺少 blockdevices"))?;
    let mut devices = Vec::new();
    for device in list {
        flatten(device, &mut devices);
    }
    Ok(devices)
}

fn flatten(value: &Value, out: &mut Vec<BlockDevice>) {
    let text = |key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let name = text("name").unwrap_or_default();
    if name.is_empty() {
        return;
    }
    let size = value
        .get("size")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let mut mountpoints: Vec<String> = Vec::new();
    if let Some(list) = value.get("mountpoints").and_then(Value::as_array) {
        mountpoints.extend(
            list.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "null")
                .map(str::to_owned),
        );
    }
    if let Some(single) = text("mountpoint") {
        if !mountpoints.contains(&single) {
            mountpoints.push(single);
        }
    }
    let children: Vec<String> = value
        .get("children")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|child| child.get("name").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let filesystem = text("fstype").filter(|t| t != "null");
    out.push(BlockDevice {
        path: text("path").unwrap_or_else(|| format!("/dev/{name}")),
        name,
        kind: text("type").unwrap_or_else(|| "disk".into()),
        size,
        model: text("model").filter(|m| m != "null"),
        serial: text("serial").filter(|s| s != "null"),
        // lsblk 的 rota 在 JSON 里可能是布尔或 0/1
        rotational: match value.get("rota") {
            Some(Value::Bool(flag)) => *flag,
            Some(Value::Number(number)) => number.as_u64().unwrap_or(0) == 1,
            _ => false,
        },
        transport: text("tran").filter(|t| t != "null"),
        mountpoints,
        filesystem,
        uuid: text("uuid").filter(|u| u != "null"),
        children,
    });
    for child in value
        .get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        flatten(child, out);
    }
}

/// 把设备列表折算成逐盘的可用性判断。
///
/// 关键在于**把分区的风险归到整盘上**：一块盘只要有任一子分区挂了 `/`，
/// 整块盘就不能动——否则会连带毁掉系统。
pub fn assess(devices: &[BlockDevice]) -> Vec<DriveRisk> {
    // 先把挂载点按「整盘」聚合
    let mut disk_mounts: std::collections::HashMap<&str, Vec<&str>> = Default::default();
    let mut disk_kind: std::collections::HashMap<&str, &str> = Default::default();
    for device in devices {
        if device.kind == "disk" {
            disk_kind.insert(device.name.as_str(), device.name.as_str());
        }
    }
    for device in devices {
        // 找到该设备所属的整盘：整盘自身，或某个把它列为子设备的盘
        let owner = if device.kind == "disk" {
            Some(device.name.as_str())
        } else {
            devices
                .iter()
                .find(|parent| {
                    parent.children.iter().any(|child| child == &device.name)
                })
                .map(|parent| parent.name.as_str())
        };
        if let Some(owner) = owner {
            let entry = disk_mounts.entry(owner).or_default();
            for mount in &device.mountpoints {
                entry.push(mount.as_str());
            }
        }
    }

    let mut risks = Vec::new();
    for device in devices.iter().filter(|d| d.kind == "disk") {
        let mut reasons = Vec::new();
        let mounts = disk_mounts.get(device.name.as_str());
        let system_disk = mounts.is_some_and(|list| {
            list.iter().any(|mount| {
                SYSTEM_MOUNTS.contains(mount) || *mount == "/boot/efi"
            })
        });
        if system_disk {
            let list = mounts.cloned().unwrap_or_default().join("、");
            reasons.push(format!("承载系统挂载点（{list}），不可格式化"));
        }

        // 子设备里有 LVM、软 RAID 或加密卷时同样不能动
        let children: Vec<&BlockDevice> = devices
            .iter()
            .filter(|child| {
                device.children.iter().any(|name| name == &child.name)
                    || child.name == device.name
            })
            .collect();
        let lvm_member = children
            .iter()
            .any(|child| child.kind == "lvm" || child.kind == "lvm-pv");
        let raid_member = children.iter().any(|child| {
            child.kind.starts_with("raid") || child.filesystem.as_deref() == Some("linux_raid_member")
        });
        let luks = children
            .iter()
            .any(|child| child.kind == "crypt" || child.filesystem.as_deref() == Some("crypto_LUKS"));
        if lvm_member {
            reasons.push("是 LVM 物理卷，需先确认是否有在用逻辑卷".into());
        }
        if raid_member {
            reasons.push("是软 RAID 成员，移除会降级阵列".into());
        }
        if luks {
            reasons.push("包含加密卷".into());
        }

        let mounted = mounts.is_some_and(|list| !list.is_empty());
        // 有挂载点且带文件系统，确定有数据
        let has_data = mounted && device.filesystem.is_some();
        if has_data {
            reasons.push("已挂载且上面有文件系统，确定存有数据".into());
        }
        // 有文件系统但没挂载：无法在不挂载的前提下确认里面有没有东西，
        // 因此同样不能自动放行——格式化这类盘是常见的误删来源。
        let unmounted_filesystem = !mounted && device.filesystem.is_some();
        if unmounted_filesystem {
            reasons.push(format!(
                "已有 {} 文件系统但未挂载，可能仍有数据；确认无用后请先 wipefs 再来",
                device.filesystem.as_deref().unwrap_or("未知")
            ));
        }
        // 有子设备说明存在分区表，也是"有人用过"的证据
        let has_partitions = !device.children.is_empty();
        if has_partitions {
            reasons.push(format!(
                "存在分区表（{} 个分区），需先确认可清除",
                device.children.len()
            ));
        }
        let removable = device
            .transport
            .as_deref()
            .is_some_and(|t| t == "usb");
        if removable {
            reasons.push("通过 USB 连接，不适合作为集群磁盘".into());
        }
        let zero_size = device.size == 0;
        if zero_size {
            reasons.push("容量为 0，可能是空读卡器或未就绪设备".into());
        }
        if reasons.is_empty() {
            reasons.push("未挂载、无文件系统、非系统盘，可用于存储集群".into());
        }
        risks.push(DriveRisk {
            device: device.name.clone(),
            system_disk,
            has_data,
            mounted,
            unmounted_filesystem,
            has_partitions,
            lvm_member,
            raid_member,
            luks,
            removable,
            zero_size,
            reasons,
        });
    }
    risks
}

async fn lsblk() -> anyhow::Result<Vec<BlockDevice>> {
    // -J 输出 JSON，-O 输出全部列；-b 用字节避免单位换算误差
    let text = command("/usr/bin/lsblk", &["-J", "-O", "-b"], 15).await?;
    parse_lsblk(&text)
}

fn distro() -> String {
    let Ok(text) = std::fs::read_to_string("/etc/os-release") else {
        return "未知".into();
    };
    text.lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|value| value.trim_matches('"').to_owned())
        .unwrap_or_else(|| "未知".into())
}

fn cpu_model() -> String {
    let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") else {
        return "未知".into();
    };
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "model name").then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| "未知".into())
}

fn memory_total() -> u64 {
    let Ok(text) = std::fs::read_to_string("/proc/meminfo") else {
        return 0;
    };
    text.lines()
        .find_map(|line| {
            let rest = line.strip_prefix("MemTotal:")?;
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            Some(kb * 1024)
        })
        .unwrap_or(0)
}

fn uptime() -> u64 {
    read_trim("/proc/uptime")
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64
}

/// 物理网卡与链路速率。跳过回环与容器网桥，与指标采样保持一致。
async fn interfaces() -> anyhow::Result<Vec<HostInterface>> {
    let text = command(
        "/usr/bin/ls",
        &["-1", "/sys/class/net"],
        10,
    )
    .await?;
    let mut list = Vec::new();
    for name in text.lines().map(str::trim).filter(|n| !n.is_empty()) {
        if name == "lo" || name.starts_with("docker") || name.starts_with("br-") {
            continue;
        }
        let speed = std::fs::read_to_string(format!("/sys/class/net/{name}/speed"))
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|speed| *speed > 0);
        let address = read_trim(&format!("/sys/class/net/{name}/address"));
        list.push(HostInterface {
            name: name.to_owned(),
            speed_mbps: speed,
            address,
        });
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// 已有的对象存储痕迹。发现既有部署时预检必须提示冲突，而不是建议再部署一套。
async fn existing_storage() -> ExistingStorage {
    let mut existing = ExistingStorage::default();
    // 容器
    if let Ok(text) = command(
        "/usr/bin/docker",
        &["ps", "-a", "--format", "{{.Names}}\t{{.Image}}"],
        15,
    )
    .await
    {
        for line in text.lines() {
            let lowered = line.to_lowercase();
            if lowered.contains("minio") || lowered.contains("silo") {
                existing.containers.push(line.trim().to_owned());
            }
        }
    }
    // systemd 单元
    for unit in ["minio.service", "silo.service"] {
        if std::path::Path::new(&format!("/etc/systemd/system/{unit}")).exists()
            || std::path::Path::new(&format!("/lib/systemd/system/{unit}")).exists()
        {
            existing.systemd_units.push(unit.to_owned());
        }
    }
    // 数据目录特征
    for path in ["/data/.minio.sys", "/data/minio.sys", "/mnt/data/.minio.sys"] {
        if std::path::Path::new(path).exists() {
            existing.data_directories.push(path.to_owned());
        }
    }
    existing
}

/// SMART 摘要。`smartctl` 不存在或设备不支持时返回 `Ok(None)`，
/// 表示**未采集**——绝不能当作"健康"。
async fn smart_summary(devices: &[BlockDevice]) -> anyhow::Result<Option<String>> {
    let mut lines = Vec::new();
    let mut collected = false;
    for device in devices.iter().filter(|d| d.kind == "disk") {
        let Ok(output) = command(
            "/usr/sbin/smartctl",
            &["-H", "-A", &device.path],
            15,
        )
        .await
        else {
            continue;
        };
        collected = true;
        let health = output
            .lines()
            .find(|line| line.contains("SMART overall-health") || line.contains("SMART Health Status"))
            .map(str::trim)
            .unwrap_or("未给出整体健康结论");
        lines.push(format!("{}：{health}", device.name));
    }
    if !collected {
        return Ok(None);
    }
    Ok(Some(lines.join("；")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取自 lsblk -J -O 的真实输出形状（含 util-linux 2.4x 的 mountpoints 数组）。
    const REAL_LSBLK: &str = r#"{
   "blockdevices": [
      {"name":"sda","path":"/dev/sda","type":"disk","size":10737418240,"model":"Virtual Disk","serial":"600508b1","rota":true,"tran":"sata","mountpoints":[null],"fstype":null,"uuid":null,
       "children":[
          {"name":"sda1","path":"/dev/sda1","type":"part","size":10736369664,"model":null,"serial":null,"rota":true,"tran":null,"mountpoints":["/"],"fstype":"ext4","uuid":"aaaa-bbbb","children":[]}
       ]},
      {"name":"sdb","path":"/dev/sdb","type":"disk","size":21474836480,"model":"Samsung SSD","serial":"S1234","rota":false,"tran":"nvme","mountpoints":[null],"fstype":null,"uuid":null,"children":[]},
      {"name":"nvme0n1","path":"/dev/nvme0n1","type":"disk","size":536870912000,"model":"Samsung SSD 990","serial":"S999","rota":false,"tran":"nvme","mountpoints":[null],"fstype":null,"uuid":null,
       "children":[
          {"name":"nvme0n1p1","path":"/dev/nvme0n1p1","type":"part","size":536860000000,"model":null,"serial":null,"rota":false,"tran":null,"mountpoints":[],"fstype":"crypto_LUKS","uuid":"cccc","children":[]}
       ]},
      {"name":"sdc","path":"/dev/sdc","type":"disk","size":8589934592,"model":null,"serial":null,"rota":true,"tran":"usb","mountpoints":["/mnt/backup"],"fstype":"ext4","uuid":"dddd","children":[]}
   ]
}"#;

    #[test]
    fn 解析真实_lsblk_输出并展开子设备() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        // 4 个顶层设备 + 2 个子设备
        assert_eq!(devices.len(), 6);
        let sda = devices.iter().find(|d| d.name == "sda").unwrap();
        assert_eq!(sda.kind, "disk");
        assert_eq!(sda.size, 10737418240);
        assert!(sda.rotational, "rota:true 应解析为机械盘");
        assert_eq!(sda.transport.as_deref(), Some("sata"));
        assert_eq!(sda.children, vec!["sda1"]);
        let part = devices.iter().find(|d| d.name == "sda1").unwrap();
        assert_eq!(part.kind, "part");
        assert_eq!(part.mountpoints, vec!["/"], "应解析 mountpoints 数组");
        assert_eq!(part.filesystem.as_deref(), Some("ext4"));
        // null 字符串不能被当成有效值
        assert!(sda.model.is_none() || sda.model.as_deref() != Some("null"));
    }

    #[test]
    fn 兼容旧版的单值_mountpoint() {
        let text = r#"{"blockdevices":[{"name":"vda","type":"disk","size":100,"rota":false,"mountpoint":"/data","fstype":"xfs","children":[]}]}"#;
        let devices = parse_lsblk(text).unwrap();
        assert_eq!(devices[0].mountpoints, vec!["/data"]);
    }

    #[test]
    fn 缺少_blockdevices_时报错而不是当成空列表() {
        // 把解析失败当成"没有磁盘"会让预检得出错误结论
        assert!(parse_lsblk("{}").is_err());
        assert!(parse_lsblk("not json").is_err());
    }

    #[test]
    fn 系统盘按整盘识别并给出原因() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        let risks = assess(&devices);
        let sda = risks.iter().find(|r| r.device == "sda").unwrap();
        // sda1 挂了 /，因此整盘 sda 是系统盘
        assert!(sda.system_disk, "分区挂载必须归到整盘上");
        assert!(!sda.usable());
        assert!(sda.reasons.iter().any(|r| r.contains("承载系统挂载点")));
    }

    #[test]
    fn 加密卷所在整盘不可用() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        let risks = assess(&devices);
        let nvme = risks.iter().find(|r| r.device == "nvme0n1").unwrap();
        assert!(nvme.luks, "子分区是 crypto_LUKS，整盘应标记加密");
        assert!(!nvme.usable());
        assert!(nvme.reasons.iter().any(|r| r.contains("加密卷")));
    }

    #[test]
    fn 已挂载且有文件系统的盘视为有数据() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        let risks = assess(&devices);
        let sdc = risks.iter().find(|r| r.device == "sdc").unwrap();
        assert!(sdc.mounted && sdc.has_data);
        assert!(sdc.removable, "tran=usb 应标记为可移除");
        assert!(!sdc.usable());
    }

    #[test]
    fn 干净的空盘才被判为可用() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        let risks = assess(&devices);
        let sdb = risks.iter().find(|r| r.device == "sdb").unwrap();
        assert!(sdb.usable(), "未挂载、无文件系统、无分区表应可用");
        assert!(sdb.reasons.iter().any(|r| r.contains("可用于存储集群")));
    }

    #[test]
    fn 有文件系统但未挂载的盘不能自动放行() {
        // 这类盘最容易被误格式化：看起来"没挂载"，其实上面可能全是数据
        let text = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","size":1000,"rota":false,"fstype":"ext4","mountpoints":[null],"children":[]}]}"#;
        let devices = parse_lsblk(text).unwrap();
        let risk = &assess(&devices)[0];
        assert!(risk.unmounted_filesystem);
        assert!(!risk.usable(), "有文件系统的盘必须要求先人工确认");
        assert!(risk.reasons.iter().any(|r| r.contains("wipefs")));
    }

    #[test]
    fn 存在分区表的盘需要先确认() {
        let text = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","size":1000,"rota":false,"mountpoints":[null],"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","size":900,"mountpoints":[],"children":[]}]}]}"#;
        let devices = parse_lsblk(text).unwrap();
        let risk = assess(&devices)
            .into_iter()
            .find(|r| r.device == "sdb")
            .unwrap();
        assert!(risk.has_partitions);
        assert!(!risk.usable());
        assert!(risk.reasons.iter().any(|r| r.contains("分区表")));
    }

    #[test]
    fn 每个风险标记都必须给出具体原因() {
        let devices = parse_lsblk(REAL_LSBLK).unwrap();
        for risk in assess(&devices) {
            assert!(!risk.reasons.is_empty(), "{} 没有给出原因", risk.device);
        }
    }

    #[test]
    fn 零容量设备被标记出来() {
        let text = r#"{"blockdevices":[{"name":"sdz","type":"disk","size":0,"rota":false,"children":[]}]}"#;
        let devices = parse_lsblk(text).unwrap();
        let risks = assess(&devices);
        assert!(!risks[0].usable());
        assert!(risks[0].reasons.iter().any(|r| r.contains("容量为 0")));
    }

    #[test]
    fn 已有存储痕迹的判定() {
        let mut existing = ExistingStorage::default();
        assert!(!existing.found());
        existing.containers.push("minio".into());
        assert!(existing.found());
        let mut other = ExistingStorage::default();
        other.systemd_units.push("silo.service".into());
        assert!(other.found());
        let mut third = ExistingStorage::default();
        third.data_directories.push("/data/.minio.sys".into());
        assert!(third.found());
    }

    /// 真实环境下的盘点才算验证：lsblk 的实际输出、真实挂载关系与风险判定
    /// 都只能在真机上检查。
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn 真实主机盘点的字段与风险自洽() {
        let inventory = inspect().await;
        assert!(!inventory.kernel.is_empty() && inventory.kernel != "未知");
        assert!(inventory.memory_total > 0, "内存总量应可读");
        assert!(inventory.cpu_cores > 0);
        assert!(!inventory.devices.is_empty(), "应至少识别到一块块设备");

        let disks: Vec<_> = inventory
            .devices
            .iter()
            .filter(|d| d.kind == "disk")
            .collect();
        assert!(!disks.is_empty(), "应至少识别到一块整盘");

        // 每块整盘都要有对应的风险条目
        for disk in &disks {
            assert!(
                inventory.risks.iter().any(|r| r.device == disk.name),
                "{} 缺少风险判定",
                disk.name
            );
        }
        // 系统盘必须被识别出来，且不可用
        let system: Vec<_> = inventory.risks.iter().filter(|r| r.system_disk).collect();
        assert_eq!(system.len(), 1, "应恰好识别出一块系统盘：{:?}", system);
        assert!(!system[0].usable());

        // 每个风险判定都要给出原因，且布尔量与原因一致
        for risk in &inventory.risks {
            assert!(!risk.reasons.is_empty(), "{} 没有原因", risk.device);
            if risk.system_disk || risk.has_data || risk.luks || risk.removable || risk.zero_size {
                assert!(!risk.usable(), "{} 有风险却判为可用", risk.device);
            }
        }

        println!(
            "真实盘点：{} · {} · 内存 {} GiB · {} 块整盘 · 风险 {:?} · 缺口 {:?}",
            inventory.hostname,
            inventory.distro,
            inventory.memory_total / 1024 / 1024 / 1024,
            disks.len(),
            inventory
                .risks
                .iter()
                .map(|r| format!("{}={}", r.device, if r.usable() { "可用" } else { "不可用" }))
                .collect::<Vec<_>>(),
            inventory.gaps
        );
    }
}
