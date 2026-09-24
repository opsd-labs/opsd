//! 对端延迟探测。
//!
//! 用途是**线路质量观察**：12 台节点两两之间的延迟与丢包，是判断覆盖网
//! （EasyTier）是否退化、哪条链路在拖慢 Galera 复制的直接依据。
//!
//! # 三条约束
//!
//! 1. **不猜地址**。探测目标由主控下发（只有它知道全部节点），带版本号，
//!    与 `PeerSync` 走同一套任务机制，因此也会落盘、重启后仍在。
//! 2. **没测到就不是 0**。`latency_ms` 为 `None` 表示没有测到；
//!    `0.0 毫秒` 是"同机"，两者绝不能混为一谈。
//! 3. **探测本身要廉价**。默认走 ICMP（`ping -n -c 1`），不产生任何服务端日志；
//!    只有显式给出端口的对端才走 TCP 握手计时。两者都不需要额外特权。
//!
//! 与已有的 `Frame::Probe` 的关系：那一个是主控发起的**一次性可达性**检查
//! （从某台节点的视角确认某个地址通不通），不产生历史；这里是节点自己**周期性**
//! 测量的延迟与可达性，进时序库，用于看线路趋势。两者互补，不互相替代。
use anyhow::Result;
use opsd::protocol::{PeerLatency, ProbeTarget, ProbeTargetSet};

use super::command;

/// 单次探测的超时（秒）。
const PROBE_TIMEOUT: u64 = 4;
/// 两轮探测之间的间隔（秒）。线路质量不需要 15 秒的粒度，
/// 更低的频率既省资源，也让"调度抖动"不会混进延迟数字。
pub const INTERVAL: u64 = 60;
/// `ping` 的等待上限（秒），略小于进程超时，让它自己先退出并给出可读输出。
const PING_DEADLINE: &str = "3";
/// TCP 握手超时（毫秒）。
const TCP_TIMEOUT_MS: u64 = 2_000;
/// 能接受的 `ping` 可执行文件位置。
const PING_PATHS: [&str; 3] = ["/usr/bin/ping", "/bin/ping", "/usr/sbin/ping"];
/// 本地命令缓存的是"是否探测过"，因此每次重建都从零开始。
pub const METHOD_ICMP: &str = "icmp";
pub const METHOD_TCP: &str = "tcp";

/// 探测目标集合的持久化键。
pub fn store_key() -> (&'static str, &'static str) {
    ("agent", "probe-targets")
}

/// 从 `ping` 输出里取出延迟（毫秒）。
///
/// 只认 `time=<数字> ms` 这一种写法：`iputils` 与 `busybox` 都是这个格式，
/// 而解析失败时返回 `None`，不会退化成 0。
pub fn parse_ping_latency(output: &str) -> Option<f64> {
    for line in output.lines() {
        let Some(index) = line.find("time=") else {
            continue;
        };
        let rest = &line[index + "time=".len()..];
        let value: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if value.is_empty() {
            continue;
        }
        if let Ok(parsed) = value.parse::<f64>()
            && parsed.is_finite()
            && parsed >= 0.0
        {
            return Some(parsed);
        }
    }
    None
}

/// `ping` 是否报告了丢包（用于把"超时"和"缺少工具"区分开）。
pub fn ping_lost(output: &str) -> bool {
    output.contains("100% packet loss") || output.contains("100% loss")
}

/// 一次 ICMP 探测。返回 `(延迟, 是否可达, 失败原因)`。
pub async fn icmp(target: &ProbeTarget) -> (Option<f64>, bool, Option<String>) {
    let Some(ping) = PING_PATHS
        .iter()
        .find(|path| std::path::Path::new(path).exists())
        .copied()
    else {
        // 工具缺失是"测不了"，不是"网络不通"，必须分开说
        return (None, false, Some("本机没有 ping 命令".into()));
    };
    let arguments = ["-n", "-c", "1", "-W", PING_DEADLINE, &target.address];
    match command(ping, &arguments, PROBE_TIMEOUT).await {
        Ok(output) => match parse_ping_latency(&output) {
            Some(latency) => (Some(latency), true, None),
            None if ping_lost(&output) => (None, false, Some("ICMP 无回应（100% 丢包）".into())),
            None => (None, false, Some("ICMP 有输出但没有延迟字段".into())),
        },
        // 非零退出码时 command 只给出 stderr 摘要；丢包也会走到这里
        Err(error) => {
            let text = format!("{error:#}");
            let reason = if text.contains("packet loss") || text.contains("100%") {
                "ICMP 无回应（100% 丢包）".to_string()
            } else {
                text
            };
            (None, false, Some(reason))
        }
    }
}

/// 一次 TCP 握手计时。不需要任何特权，但会在对端服务上留下连接记录，
/// 因此只有显式配置了端口的对端才走这条路。
pub async fn tcp(target: &ProbeTarget) -> (Option<f64>, bool, Option<String>) {
    let Some(port) = target.port else {
        return (None, false, Some("未指定端口".into()));
    };
    let address = format!("{}:{}", target.address, port);
    let started = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_millis(TCP_TIMEOUT_MS),
        tokio::net::TcpStream::connect(&address),
    )
    .await
    {
        Ok(Ok(stream)) => {
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            drop(stream);
            (Some(elapsed), true, None)
        }
        Ok(Err(error)) => (None, false, Some(format!("TCP 连接失败：{error}"))),
        Err(_) => (None, false, Some("TCP 连接超时".into())),
    }
}

/// 探测一个对端。给出端口就走 TCP，否则走 ICMP。
pub async fn probe_one(target: &ProbeTarget) -> PeerLatency {
    let method = if target.port.is_some() {
        METHOD_TCP
    } else {
        METHOD_ICMP
    };
    let (latency_ms, reachable, error) = if target.port.is_some() {
        tcp(target).await
    } else {
        icmp(target).await
    };
    PeerLatency {
        node_id: target.node_id.clone(),
        address: target.address.clone(),
        method: method.into(),
        latency_ms,
        reachable,
        error,
        probed_at: opsd::protocol::now(),
    }
}

/// 并发上限。适度并发可以缩短一轮探测的总时长，但放太多会把
/// "本机网络栈排队"混进测量结果里，反而让延迟数字不可信。
pub const CONCURRENCY: usize = 4;

/// 探测全部对端。
pub async fn probe_all(set: &ProbeTargetSet) -> Vec<PeerLatency> {
    use futures_util::StreamExt;
    let mut results: Vec<PeerLatency> = futures_util::stream::iter(set.targets.iter())
        .map(probe_one)
        .buffered(CONCURRENCY)
        .collect()
        .await;
    // 顺序固定下来，避免每次上报的数组顺序抖动导致无意义的差异
    results.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    results
}

/// 读取上次保存的目标集合。
pub async fn load(db: &opsd::store::Store) -> Result<Option<ProbeTargetSet>> {
    let (bucket, id) = store_key();
    db.get::<ProbeTargetSet>(bucket, id).await
}

/// 保存目标集合。
pub async fn save(db: &opsd::store::Store, set: &ProbeTargetSet) -> Result<()> {
    let (bucket, id) = store_key();
    db.put(bucket, id, set).await
}

/// 周期探测循环。
///
/// **独立于采样循环**：一轮要连十几个对端，串在采样里会把采样节奏带偏。
/// 结果放在共享槽里，采样分支只取"最近一轮已完成的结果"，
/// 因此采样永远不会被探测阻塞，而带上来的也永远是一次真实测量。
pub async fn probe_loop(
    db: opsd::store::Store,
    results: std::sync::Arc<tokio::sync::RwLock<Vec<PeerLatency>>>,
) {
    let mut set = load(&db).await.ok().flatten().unwrap_or_default();
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(INTERVAL));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        // 目标集合可能刚被主控更新过，因此每轮都重新读一次
        if let Ok(Some(latest)) = load(&db).await {
            set = latest;
        }
        if set.targets.is_empty() {
            continue;
        }
        let fresh = probe_all(&set).await;
        *results.write().await = fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(node_id: &str, address: &str, port: Option<u16>) -> ProbeTarget {
        ProbeTarget {
            node_id: node_id.into(),
            address: address.into(),
            port,
        }
    }

    #[test]
    fn 解析真实_ping_输出() {
        // iputils 的典型输出
        let output = "PING 100.100.201.52 (100.100.201.52) 56(84) bytes of data.\n\
64 bytes from 100.100.201.52: icmp_seq=1 ttl=64 time=0.42 ms\n\n\
--- 100.100.201.52 ping statistics ---\n\
1 packets transmitted, 1 received, 0% packet loss, time 0ms\n\
rtt min/avg/max/mdev = 0.420/0.420/0.420/0.000 ms\n";
        assert_eq!(parse_ping_latency(output), Some(0.42));
        assert!(!ping_lost(output));
    }

    #[test]
    fn 解析较大延迟与整数延迟() {
        assert_eq!(
            parse_ping_latency("64 bytes from 1.2.3.4: icmp_seq=1 ttl=52 time=187 ms"),
            Some(187.0)
        );
        // 统计行里的 rtt 数字不能被误当成单次延迟
        assert_eq!(
            parse_ping_latency("rtt min/avg/max/mdev = 0.420/0.420/0.420/0.000 ms"),
            None
        );
    }

    #[test]
    fn 丢包输出不会伪造出延迟() {
        let output = "PING 10.0.0.9 (10.0.0.9) 56(84) bytes of data.\n\n\
--- 10.0.0.9 ping statistics ---\n\
1 packets transmitted, 0 received, 100% packet loss, time 0ms\n";
        assert_eq!(parse_ping_latency(output), None, "没测到就不能给出数字");
        assert!(ping_lost(output));
    }

    #[test]
    fn 空输出与异常输出都返回未知() {
        assert_eq!(parse_ping_latency(""), None);
        assert_eq!(parse_ping_latency("time=abc ms"), None);
        assert_eq!(parse_ping_latency("time= ms"), None);
        // 负值不是有效延迟
        assert_eq!(parse_ping_latency("time=-1 ms"), None);
    }

    #[tokio::test]
    async fn 没有端口时走_icmp_并给出方法名() {
        let result = probe_one(&target("C001", "203.0.113.1", None)).await;
        assert_eq!(result.method, METHOD_ICMP);
        assert_eq!(result.node_id, "C001");
        // 测试环境里没有 ping 或目标不可达：两种都是"没有延迟"，绝不能是 0
        assert_eq!(result.latency_ms, None);
        assert!(!result.reachable);
        assert!(result.error.is_some(), "失败必须给出原因");
    }

    #[tokio::test]
    async fn 给定端口时走_tcp() {
        // 127.0.0.1 上几乎一定有东西监听（测试进程自己开的端口）
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let result = probe_one(&target("self", "127.0.0.1", Some(port))).await;
        assert_eq!(result.method, METHOD_TCP);
        assert!(result.reachable, "{:?}", result.error);
        assert!(result.latency_ms.unwrap() >= 0.0);
    }

    #[tokio::test]
    async fn tcp_连不上时给原因而不是零延迟() {
        // 保留地址 + 未监听端口：失败必须被如实报告
        let result = probe_one(&target("dead", "127.0.0.1", Some(1))).await;
        assert!(!result.reachable);
        assert_eq!(result.latency_ms, None);
        assert!(result.error.unwrap().contains("TCP"));
    }

    #[tokio::test]
    async fn 一轮探测按节点编号排序且失败也如实上报() {
        let set = ProbeTargetSet {
            version: 1,
            targets: vec![
                target("C052", "127.0.0.1", Some(1)),
                target("C001", "127.0.0.1", Some(1)),
                target("C041", "127.0.0.1", Some(1)),
            ],
        };
        let results = probe_all(&set).await;
        assert_eq!(
            results.iter().map(|r| r.node_id.as_str()).collect::<Vec<_>>(),
            vec!["C001", "C041", "C052"],
            "顺序必须稳定，否则每次上报的数组都在无意义地抖动"
        );
        for result in &results {
            // 127.0.0.1:1 上不会有服务：必须是"不可达 + 原因"，而不是 0 延迟
            assert!(!result.reachable);
            assert_eq!(result.latency_ms, None);
            assert!(result.error.is_some());
            assert!(result.probed_at > 0, "每次测量都要带上测量时刻");
        }
    }

    #[test]
    fn 目标集合按版本下发且可序列化往返() {
        let set = ProbeTargetSet {
            version: 3,
            targets: vec![
                target("C001", "100.100.201.1", None),
                target("C052", "100.100.201.52", Some(65522)),
            ],
        };
        let json = serde_json::to_string(&set).unwrap();
        let back: ProbeTargetSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, 3);
        assert_eq!(back.targets.len(), 2);
        assert_eq!(back.targets[1].port, Some(65522));
        // 端口可省略：旧主控或手写配置里没有这个字段也必须能解析
        let minimal: ProbeTargetSet =
            serde_json::from_str(r#"{"version":1,"targets":[{"node_id":"A","address":"1.2.3.4"}]}"#)
                .unwrap();
        assert_eq!(minimal.targets[0].port, None);
    }

    #[test]
    fn 延迟与采样一起序列化且缺省为空() {
        use opsd::protocol::MetricsSample;
        // 老 Agent 没有 peers 字段：必须能解析成空列表，而不是报错
        let sample: MetricsSample = serde_json::from_str(
            r#"{"at":1,"cpu_usage":0.0,"cpu_per_core":[],"load1":0.0,"load5":0.0,"load15":0.0,
                "memory_total":0,"memory_used":0,"memory_available":0,"swap_total":0,"swap_used":0,
                "disks":[],"disk_read_bytes_per_second":0.0,"disk_write_bytes_per_second":0.0,
                "interfaces":[],"uptime":0,"processes":0,"tcp_connections":0,"udp_connections":0}"#,
        )
        .unwrap();
        assert!(sample.peers.is_empty());
    }
}
