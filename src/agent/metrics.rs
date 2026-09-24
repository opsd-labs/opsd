//! 主机指标采样。
//!
//! 只读 `/proc` 与 `statvfs`，不依赖 node_exporter，也不执行外部命令。
//! 需要「两次采样之差」的指标（CPU 占用、磁盘与网络速率）由 [`Sampler`] 保存上一次的
//! 原始计数；因此首次采样只能给出容量类数值，速率类字段为 0，这是刻意的：
//! 没有基线时任何速率都是编造的。
use anyhow::{Context, Result};
use opsd::protocol::{DiskUsage, InterfaceRate, MetricsSample};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

/// 需要跳过的虚拟文件系统，避免把 tmpfs、overlay 等计入磁盘用量。
const SKIP_FILESYSTEMS: &[&str] = &[
    "tmpfs",
    "devtmpfs",
    "devpts",
    "sysfs",
    "proc",
    "cgroup",
    "cgroup2",
    "overlay",
    "squashfs",
    "ramfs",
    "autofs",
    "mqueue",
    "debugfs",
    "tracefs",
    "securityfs",
    "pstore",
    "bpf",
    "fusectl",
    "configfs",
    "nsfs",
    "binfmt_misc",
    // plan9/rootfs 不是块设备支撑的存储，报成「磁盘」会误导
    "9p",
    "rootfs",
];

/// 跨采样保留的原始计数，用于求速率。
#[derive(Default)]
pub struct Sampler {
    cpu_total: Vec<(u64, u64)>, // (total, idle) 每核，索引 0 为总 CPU
    network: HashMap<String, (u64, u64, u64, u64)>, // rx_bytes, tx_bytes, rx_err, tx_err
    disk: Option<(u64, u64)>,   // read_sectors, write_sectors
    previous: Option<i64>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn read(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("读取 {path} 失败"))
}

/// 解析 `/proc/stat` 的 cpu 行，返回 (总时间, 空闲时间)。
/// 空闲包含 idle 与 iowait：iowait 期间 CPU 并未执行任务。
fn parse_cpu(line: &str) -> Option<(u64, u64)> {
    let mut fields = line.split_whitespace();
    let name = fields.next()?;
    if !name.starts_with("cpu") {
        return None;
    }
    let values: Vec<u64> = fields.filter_map(|v| v.parse().ok()).collect();
    if values.len() < 4 {
        return None;
    }
    let total: u64 = values.iter().sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((total, idle))
}

fn cpu_usage(previous: (u64, u64), current: (u64, u64)) -> f64 {
    let total = current.0.saturating_sub(previous.0);
    let idle = current.1.saturating_sub(previous.1);
    if total == 0 {
        return 0.0;
    }
    let busy = total.saturating_sub(idle);
    (busy as f64) * 100.0 / (total as f64)
}

fn parse_meminfo(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|line| {
            let (key, rest) = line.split_once(':')?;
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            Some((key.to_owned(), kb * 1024))
        })
        .collect()
}

fn parse_network(text: &str) -> HashMap<String, (u64, u64, u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let name = name.trim();
            // 只统计物理与隧道接口，跳过 lo 与容器网桥，避免重复计算。
            if name == "lo" || name.starts_with("docker") || name.starts_with("br-") {
                return None;
            }
            let values: Vec<u64> = rest
                .split_whitespace()
                .map(|v| v.parse().unwrap_or(0))
                .collect();
            if values.len() < 11 {
                return None;
            }
            Some((name.to_owned(), (values[0], values[8], values[2], values[10])))
        })
        .collect()
}

/// `/sys/block` 里的整盘名单。用它判断某个设备名是整盘还是分区，
/// 不能靠「名字是否以数字结尾」——`nvme0n1`、`loop0` 都以数字结尾，但都是整盘。
fn whole_disks() -> std::collections::HashSet<String> {
    std::fs::read_dir("/sys/block")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// `/proc/diskstats` 的扇区总和。只统计整盘，避免与分区重复计数。
fn parse_diskstats(text: &str, disks: &std::collections::HashSet<String>) -> (u64, u64) {
    let mut read = 0u64;
    let mut write = 0u64;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // 前三个字段是 major/minor/name，其后至少到「写扇区」共需 14 个字段
        if fields.len() < 14 {
            continue;
        }
        let name = fields[2];
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }
        if !disks.contains(name) {
            continue;
        }
        read += fields[5].parse().unwrap_or(0);
        write += fields[9].parse().unwrap_or(0);
    }
    (read, write)
}

/// 从 `/proc/mounts` 的内容中筛出需要统计的真实挂载点。
///
/// 排除虚拟文件系统（tmpfs、overlay、devtmpfs、9p 等）与非绝对路径的挂载，
/// 否则磁盘用量会被 tmpfs 与容器层稀释，失去参考意义。
/// **按设备去重**：同一块盘挂载多次（bind mount、WSL 的发行版根目录）是同一份用量，
/// 按挂载点去重会把它重复计入总量。
fn parse_mounts(text: &str) -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut mounts = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let (device, mount, filesystem) = (fields[0], fields[1], fields[2]);
        if SKIP_FILESYSTEMS.contains(&filesystem) || !mount.starts_with('/') {
            continue;
        }
        if !seen.insert(device.to_owned()) {
            continue;
        }
        mounts.push((
            device.to_owned(),
            mount.to_owned(),
            filesystem.to_owned(),
        ));
    }
    mounts
}

/// 真实挂载点的容量与 inode 用量。
fn disk_usage() -> Vec<DiskUsage> {
    let Ok(mounts) = read("/proc/mounts") else {
        return Vec::new();
    };
    let mut disks = Vec::new();
    for (device, mount, filesystem) in parse_mounts(&mounts) {
        let Some((total, used, inode_total, inode_used)) = statvfs(&mount) else {
            continue;
        };
        if total == 0 {
            continue;
        }
        disks.push(DiskUsage {
            mount,
            filesystem: if device.starts_with('/') {
                device
            } else {
                filesystem
            },
            total,
            used,
            inode_total,
            inode_used,
        });
    }
    disks.sort_by(|a, b| a.mount.cmp(&b.mount));
    disks
}

/// 通过 `statvfs` 取容量。此调用需要 libc，std 没有对应封装。
/// 非 Linux 平台没有 /proc，采样在此之前就会失败，这里只需保证可编译。
#[cfg(target_os = "linux")]
fn statvfs(path: &str) -> Option<(u64, u64, u64, u64)> {
    use std::ffi::CString;
    let path = CString::new(path).ok()?;
    // SAFETY: statvfs 只写我们提供的结构体，且 path 以 NUL 结尾。
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block = stats.f_frsize.max(1) as u64;
    let total = stats.f_blocks as u64 * block;
    // 已用 = 总量 − 空闲 − root 预留，与 df 的口径一致
    let free = (stats.f_bfree as u64).saturating_sub(stats.f_bavail as u64);
    let used = total.saturating_sub(stats.f_bfree as u64 * block).saturating_add(free * block);
    let inode_total = stats.f_files as u64;
    let inode_used = inode_total.saturating_sub(stats.f_ffree as u64);
    Some((total, used.min(total), inode_total, inode_used))
}

#[cfg(not(target_os = "linux"))]
fn statvfs(_path: &str) -> Option<(u64, u64, u64, u64)> {
    None
}

fn count_processes() -> u64 {
    std::fs::read_dir("/proc")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .chars()
                        .all(|c| c.is_ascii_digit())
                })
                .count() as u64
        })
        .unwrap_or(0)
}

/// 统计 `/proc/net/tcp` 与 `/proc/net/udp` 的连接数（去掉表头）。
fn connections() -> (u64, u64) {
    let count = |path: &str| {
        read(path)
            .map(|text| text.lines().count().saturating_sub(1) as u64)
            .unwrap_or(0)
    };
    (count("/proc/net/tcp"), count("/proc/net/udp"))
}

impl Sampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// 采集一次。任何必需数据源缺失都会返回错误，由调用方上报「采集失败」。
    pub fn sample(&mut self) -> Result<MetricsSample> {
        let at = now();
        let stat = read("/proc/stat")?;
        let mut cpus: Vec<(u64, u64)> = stat.lines().filter_map(parse_cpu).collect();
        anyhow::ensure!(!cpus.is_empty(), "/proc/stat 缺少 cpu 行");

        let elapsed = self.previous.map(|p| (at - p).max(1) as f64);
        let mut per_core = Vec::new();
        for (index, current) in cpus.iter().enumerate().skip(1) {
            let value = match self.cpu_total.get(index) {
                Some(previous) => cpu_usage(*previous, *current),
                None => 0.0,
            };
            per_core.push((value * 10.0).round() / 10.0);
        }
        let total_cpu = cpus.remove(0);
        let cpu_usage_value = match self.cpu_total.first() {
            Some(previous) => cpu_usage(*previous, total_cpu),
            None => 0.0,
        };
        // 索引 0 固定为总 CPU，其后依次为各核，与上面读取历史值的下标一致。
        self.cpu_total = std::iter::once(total_cpu).chain(cpus.iter().copied()).collect();

        let meminfo = parse_meminfo(&read("/proc/meminfo")?);
        let memory_total = meminfo.get("MemTotal").copied().unwrap_or(0);
        let memory_available = meminfo.get("MemAvailable").copied().unwrap_or(0);
        let memory_used = memory_total.saturating_sub(memory_available);
        let swap_total = meminfo.get("SwapTotal").copied().unwrap_or(0);
        let swap_free = meminfo.get("SwapFree").copied().unwrap_or(0);

        let (load1, load5, load15) = read("/proc/loadavg")
            .ok()
            .and_then(|text| {
                let f: Vec<f64> = text
                    .split_whitespace()
                    .take(3)
                    .filter_map(|v| v.parse().ok())
                    .collect();
                (f.len() == 3).then_some((f[0], f[1], f[2]))
            })
            .unwrap_or((0.0, 0.0, 0.0));

        let network = read("/proc/net/dev")
            .map(|text| parse_network(&text))
            .unwrap_or_default();
        let mut interfaces = Vec::new();
        for (name, (rx, tx, rx_err, tx_err)) in &network {
            let (rate_rx, rate_tx) = match (self.network.get(name), elapsed) {
                (Some((prx, ptx, _, _)), Some(seconds)) => (
                    (rx.saturating_sub(*prx)) as f64 / seconds,
                    (tx.saturating_sub(*ptx)) as f64 / seconds,
                ),
                _ => (0.0, 0.0),
            };
            interfaces.push(InterfaceRate {
                name: name.clone(),
                rx_bytes_per_second: rate_rx.round(),
                tx_bytes_per_second: rate_tx.round(),
                rx_errors: *rx_err,
                tx_errors: *tx_err,
            });
        }
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));
        self.network = network;

        let (read_sectors, write_sectors) = read("/proc/diskstats")
            .map(|text| parse_diskstats(&text, &whole_disks()))
            .unwrap_or((0, 0));
        let (disk_read, disk_write) = match (self.disk, elapsed) {
            (Some((pread, pwrite)), Some(seconds)) => (
                (read_sectors.saturating_sub(pread) as f64 * 512.0 / seconds).round(),
                (write_sectors.saturating_sub(pwrite) as f64 * 512.0 / seconds).round(),
            ),
            _ => (0.0, 0.0),
        };
        self.disk = Some((read_sectors, write_sectors));

        let uptime = read("/proc/uptime")
            .ok()
            .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0) as u64;
        let (tcp_connections, udp_connections) = connections();
        self.previous = Some(at);

        Ok(MetricsSample {
            at,
            cpu_usage: (cpu_usage_value * 10.0).round() / 10.0,
            cpu_per_core: per_core,
            load1,
            load5,
            load15,
            memory_total,
            memory_used,
            memory_available,
            swap_total,
            swap_used: swap_total.saturating_sub(swap_free),
            disks: disk_usage(),
            disk_read_bytes_per_second: disk_read,
            disk_write_bytes_per_second: disk_write,
            interfaces,
            uptime,
            processes: count_processes(),
            tcp_connections,
            udp_connections,
            // 延迟由探测循环单独测，采样本身不碰网络
            peers: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析_cpu_行并计算占用() {
        assert_eq!(parse_cpu("cpu  100 0 100 800 0 0 0 0"), Some((1000, 800)));
        // iowait 计入空闲
        assert_eq!(parse_cpu("cpu  100 0 100 700 100 0 0 0"), Some((1000, 800)));
        assert_eq!(parse_cpu("cpu0 1 2 3 4"), Some((10, 4)));
        // 非 cpu 行与字段不足都要拒绝
        assert_eq!(parse_cpu("intr 1 2 3"), None);
        assert_eq!(parse_cpu("cpu 1 2"), None);
    }

    #[test]
    fn 两次采样的差值给出占用率() {
        // 1000 总时间里 200 空闲 → 80% 占用
        assert!((cpu_usage((0, 0), (1000, 200)) - 80.0).abs() < 1e-9);
        // 计数未推进时不能返回 NaN，也不要谎报 100%
        assert_eq!(cpu_usage((10, 5), (10, 5)), 0.0);
        // 计数回绕时按饱和处理，不产生负数
        assert_eq!(cpu_usage((100, 50), (10, 5)), 0.0);
    }

    #[test]
    fn 解析_meminfo_并把_kb_换算成字节() {
        let values = parse_meminfo("MemTotal:       1000 kB\nMemAvailable:    400 kB\nSwapFree: 0 kB\n");
        assert_eq!(values["MemTotal"], 1000 * 1024);
        assert_eq!(values["MemAvailable"], 400 * 1024);
    }

    #[test]
    fn 解析网络时跳过回环与容器网桥() {
        let text = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0\n  eth0: 2000 2 3 0 0 0 0 0 4000 4 5 0 0 0 0 0\nbr-abc: 9 9 9 0 0 0 0 0 9 9 9 0 0 0 0 0\n";
        let parsed = parse_network(text);
        assert_eq!(parsed.len(), 1, "只应保留物理接口");
        assert_eq!(parsed["eth0"], (2000, 4000, 3, 5));
    }

    #[test]
    fn 解析磁盘统计只累加整盘() {
        // /proc/diskstats 每条至少 14 个字段：major minor name + 11 个计数
        let text = "\
   8       0 sda 1000 0 2000 100 3000 0 4000 200 0 0 0
   8       1 sda1 1000 0 2000 100 3000 0 4000 200 0 0 0
 259       0 nvme0n1 10 0 20 5 30 0 40 6 0 0 0
 259       1 nvme0n1p1 10 0 20 5 30 0 40 6 0 0 0
   7       0 loop0 1 0 2 0 3 0 4 0 0 0 0
 253       0 dm-0 1 0 2 0 3 0 4 0 0 0 0
";
        let disks: std::collections::HashSet<String> =
            ["sda", "nvme0n1", "loop0", "dm-0"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        // 分区 sda1 / nvme0n1p1 不在 /sys/block 中，因此被跳过，避免与整盘重复
        assert_eq!(parse_diskstats(text, &disks), (2000 + 20, 4000 + 40));
    }

    #[test]
    fn 磁盘统计在字段不足时跳过整行而不是误读() {
        // 只有 13 个字段的截断行必须被忽略，不能把别的列当成扇区数
        let text = "   8       0 sda 1000 0 2000 100 3000 0 4000 200 0 0\n";
        let disks: std::collections::HashSet<String> =
            ["sda"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_diskstats(text, &disks), (0, 0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn 首次采样不给速率而给出容量() {
        let mut sampler = Sampler::new();
        let sample = sampler.sample().expect("/proc 应可读");
        // 缺少基线时速率必须是 0，不能编造
        assert_eq!(sample.disk_read_bytes_per_second, 0.0);
        assert_eq!(sample.disk_write_bytes_per_second, 0.0);
        assert!(sample.interfaces.iter().all(|i| i.rx_bytes_per_second == 0.0));
        assert!(sample.memory_total > 0, "内存总量应可读");
        assert!(!sample.cpu_per_core.is_empty(), "应给出每核数据");
        assert!(sample.cpu_usage >= 0.0 && sample.cpu_usage <= 100.0);
        assert!(sample.uptime > 0);
    }

    /// 真实环境下的第二次采样才是有意义的验证：差值路径、单位换算与跨字段一致性
    /// 都只能在真实数据上检查。需要约一秒的运行时间。
    #[cfg(target_os = "linux")]
    #[test]
    fn 真实连续两次采样的数值自洽() {
        let mut sampler = Sampler::new();
        let first = sampler.sample().expect("/proc 应可读");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let second = sampler.sample().expect("/proc 应可读");

        // 时间在推进
        assert!(second.at >= first.at);
        // CPU 必须是合法百分比
        assert!(
            (0.0..=100.0).contains(&second.cpu_usage),
            "CPU 占用越界：{}",
            second.cpu_usage
        );
        assert!(
            second.cpu_per_core.iter().all(|c| (0.0..=100.0).contains(c)),
            "每核占用越界：{:?}",
            second.cpu_per_core
        );
        // 内存自洽：已用不超过总量，总量两次一致
        assert!(second.memory_used <= second.memory_total);
        assert_eq!(second.memory_total, first.memory_total, "总量不应漂移");
        assert!(second.swap_used <= second.swap_total.max(second.swap_used));
        // 速率不为负，且换算后不应大到荒谬（超过 100 GiB/s 说明单位算错了）
        let ceiling = 100.0 * 1024.0 * 1024.0 * 1024.0;
        assert!(second.disk_read_bytes_per_second >= 0.0);
        assert!(second.disk_write_bytes_per_second <= ceiling);
        for nic in &second.interfaces {
            assert!(nic.rx_bytes_per_second >= 0.0 && nic.tx_bytes_per_second >= 0.0);
            assert!(nic.rx_bytes_per_second <= ceiling, "{} 入站速率异常", nic.name);
        }
        // 运行时长在增长
        assert!(second.uptime >= first.uptime);
        assert!(second.processes > 0, "进程数应大于 0");
        // 真实主机上磁盘用量必须能采到，且每个挂载点自洽
        assert!(!second.disks.is_empty(), "应至少有一个真实挂载点");
        for disk in &second.disks {
            assert!(disk.used <= disk.total, "{} 已用超过总量", disk.mount);
            assert!(disk.total > 0);
        }
        println!(
            "真实采样：CPU {:.1}% · 每核 {:?} · 内存 {}/{} · 负载 {:.2} · 进程 {} · 挂载点 {} · 接口 {}",
            second.cpu_usage,
            second.cpu_per_core,
            second.memory_used,
            second.memory_total,
            second.load1,
            second.processes,
            second.disks.len(),
            second.interfaces.len()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn 磁盘用量跳过虚拟文件系统且容量自洽() {
        for disk in disk_usage() {
            assert!(disk.mount.starts_with('/'));
            assert!(disk.used <= disk.total, "{} 已用不应超过总量", disk.mount);
            assert!(!SKIP_FILESYSTEMS.contains(&disk.filesystem.as_str()));
        }
    }

    /// 以下夹具取自 WSL Debian 的真实 `/proc` 输出（内核 6.6），
    /// 用于锁定 Linux 的实际格式；字段一旦错位，这些断言会立刻失败。
    const REAL_STAT: &str = "\
cpu  83 0 240 10295 41 0 41 0 0 0
cpu0 5 0 52 1248 14 0 32 0 0 0
cpu1 8 0 40 1278 6 0 6 0 0 0
cpu2 6 0 39 1282 5 0 0 0 0 0
cpu3 8 0 18 1306 1 0 0 0 0 0
cpu4 8 0 36 1287 4 0 0 0 0 0
cpu5 31 0 11 1289 2 0 0 0 0 0
cpu6 7 0 14 1307 1 0 2 0 0 0
cpu7 6 0 26 1296 4 0 0 0 0 0
intr 12345 1 2 3
ctxt 99999
";

    const REAL_NETDEV: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:     498       4    0    0    0     0          0         0      498       4    0    0    0     0       0          0
  eth0:    1146       9    0    0    0     0          0         4     1550      21    0    0    0     0       0          0
";

    const REAL_MOUNTS: &str = "\
none /usr/lib/modules/6.6.87.2-microsoft-standard-WSL2 overlay rw,nosuid,nodev,noatime 0 0
none /mnt/wsl tmpfs rw,relatime 0 0
drivers /usr/lib/wsl/drivers 9p ro,nosuid 0 0
/dev/sdd / ext4 rw,relatime,discard,errors=remount-ro,data=ordered 0 0
none /mnt/wslg tmpfs rw,relatime 0 0
/dev/sdd /mnt/wslg/distro ext4 ro,relatime,discard 0 0
none /usr/lib/wsl/lib overlay rw,nosuid,nodev,noatime 0 0
rootfs /init rootfs ro,size=6119616k 0 0
none /dev devtmpfs rw,nosuid,relatime 0 0
sysfs /sys sysfs rw,nosuid,nodev,noexec,noatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,noatime 0 0
devpts /dev/pts devpts rw,nosuid,noexec,noatime 0 0
";

    const REAL_DISKSTATS: &str = "\
   1       0 ram0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
   8       0 sda 1145 420 147970 249 0 0 0 0 0 168 249 0 0 0 0 0 0
   8       1 sda1 1145 420 147970 249 0 0 0 0 0 168 249 0 0 0 0 0 0
   8      16 sdb 12 0 34 5 0 0 0 0 0 6 5 0 0 0 0 0 0
";

    #[test]
    fn 真实_proc_stat_按内核格式解析并忽略非_cpu_行() {
        let cpus: Vec<(u64, u64)> = REAL_STAT.lines().filter_map(parse_cpu).collect();
        // 1 个总行 + 8 个核；intr 与 ctxt 行必须被忽略
        assert_eq!(cpus.len(), 9);
        let total: u64 = [83, 0, 240, 10295, 41, 0, 41, 0, 0, 0].iter().sum();
        assert_eq!(cpus[0], (total, 10295 + 41));
    }

    #[test]
    fn 真实_proc_net_dev_只保留物理接口() {
        let parsed = parse_network(REAL_NETDEV);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["eth0"], (1146, 1550, 0, 0));
        assert!(!parsed.contains_key("lo"));
    }

    #[test]
    fn 真实_proc_mounts_只保留真实文件系统并按设备去重() {
        let mounts = parse_mounts(REAL_MOUNTS);
        // overlay/tmpfs/9p/rootfs 都被排除；/dev/sdd 挂了两次，按设备只保留首个
        assert_eq!(mounts.len(), 1, "只应保留 ext4 的 /：{mounts:?}");
        assert_eq!(mounts[0].0, "/dev/sdd");
        assert_eq!(mounts[0].1, "/");
        assert_eq!(mounts[0].2, "ext4");
    }

    #[test]
    fn 真实_proc_diskstats_只累加整盘并跳过虚拟设备() {
        let disks: std::collections::HashSet<String> =
            ["ram0", "sda", "sdb"].iter().map(|s| s.to_string()).collect();
        // 采集时两块盘都还没有写入，因此写扇区为 0。
        // 字段位置：0-2 为 major/minor/name，5 为读扇区，9 为写扇区。
        assert_eq!(parse_diskstats(REAL_DISKSTATS, &disks), (147970 + 34, 0));
    }

    #[test]
    fn 磁盘统计按真实字段位置读取写扇区() {
        // 与真实布局完全一致，只是把写相关列填成非零，用来锁定第 9 列是写扇区
        let text = "   8       0 sdb 12 0 34 5 7 0 56 9 0 6 5 0 0 0 0 0 0\n";
        let disks: std::collections::HashSet<String> =
            ["sdb"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse_diskstats(text, &disks), (34, 56));
    }
}
