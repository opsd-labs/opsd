//! 指标历史：分层降采样、保留策略与查询。
//!
//! 三个层级共用同一张表，按 `tier` 区分：
//!
//! | 层级 | 桶宽 | 保留 | 用途 |
//! |---|---|---|---|
//! | `raw` | 15 秒（即采样间隔） | 7 天 | 详情与近期曲线 |
//! | `m5` | 5 分钟 | 90 天 | 中期趋势 |
//! | `h1` | 1 小时 | 1 年 | 长期趋势 |
//!
//! 桶内对**标量**做增量平均，对明细（挂载点、网卡）保留该桶最后一次的值：
//! 明细用于详情展示，平均它们没有意义，而标量曲线需要平均才能反映趋势。
//!
//! 没有数据的时间段就是没有数据——不插值、不补零。离线区间在曲线上自然断开。
use anyhow::Result;
use opsd::{
    protocol::{MetricsRecord, MetricsReport, MetricsSample, PeerLatency, now},
    store::Store,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// (层级名, 桶宽秒, 保留秒)
pub const TIERS: [(&str, i64, i64); 3] = [
    ("raw", 15, 7 * 86400),
    ("m5", 300, 90 * 86400),
    ("h1", 3600, 365 * 86400),
];

/// 单次查询返回的最大点数，避免一次拉取过多数据。
pub const MAX_POINTS: i64 = 2000;

/// 时钟偏移超过该值时提示管理员：过大的偏差会让曲线时间轴错位。
pub const CLOCK_SKEW_WARN: i64 = 60;

/// 降采样桶。`count` 用于对标量做增量平均。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bucket {
    pub count: u64,
    pub sample: MetricsSample,
}

impl Bucket {
    /// 把新样本并入桶：标量取平均，明细取最后一次。
    fn merge(&mut self, incoming: &MetricsSample) {
        let previous = self.count as f64;
        let total = previous + 1.0;
        let blend = |old: f64, new: f64| (old * previous + new) / total;
        let blend_int =
            |old: u64, new: u64| (((old as f64) * previous + new as f64) / total).round() as u64;
        let target = &mut self.sample;
        target.at = incoming.at;
        // 先把延迟合并算出来，再借用 self.sample 的其它字段：
        // merge_peers 需要同时读旧值，不能和可变借用重叠。
        let peers = merge_peers(&target.peers, &incoming.peers, previous);
        target.cpu_usage = blend(target.cpu_usage, incoming.cpu_usage);
        target.load1 = blend(target.load1, incoming.load1);
        target.load5 = blend(target.load5, incoming.load5);
        target.load15 = blend(target.load15, incoming.load15);
        target.memory_total = incoming.memory_total;
        target.memory_used = blend_int(target.memory_used, incoming.memory_used);
        target.memory_available = blend_int(target.memory_available, incoming.memory_available);
        target.swap_total = incoming.swap_total;
        target.swap_used = blend_int(target.swap_used, incoming.swap_used);
        target.disk_read_bytes_per_second = blend(
            target.disk_read_bytes_per_second,
            incoming.disk_read_bytes_per_second,
        );
        target.disk_write_bytes_per_second = blend(
            target.disk_write_bytes_per_second,
            incoming.disk_write_bytes_per_second,
        );
        // 明细不参与平均，只保留桶内最后一次
        target.disks = incoming.disks.clone();
        target.interfaces = incoming.interfaces.clone();
        // 对端延迟相反：它是要看趋势的标量，而且探测稀疏（每 60 秒一次），
        // 因此必须像其它标量一样按对端平均，否则 5 分钟与 1 小时桶里
        // 只会剩下"桶内最后一次探测"，曲线的抖动会被放大成假趋势。
        target.peers = peers;
        target.processes = incoming.processes;
        target.tcp_connections = incoming.tcp_connections;
        target.udp_connections = incoming.udp_connections;
        target.uptime = incoming.uptime;
        target.cpu_per_core = incoming.cpu_per_core.clone();
        self.count += 1;
    }
}

/// 按对端合并延迟。
///
/// - 两边都有的对端：对**测到的延迟**做增量平均；这一轮没测到（`None`）
///   而之前测到过时，保留之前的数字，不让一次丢包把曲线打成空洞；
///   但如果这一轮明确不可达，那就以最新事实为准。
/// - 只有新样本才有的对端：直接采用。
/// - 只有旧桶才有的对端：保留，直到它从目标集合里消失为止。
pub fn merge_peers(
    previous: &[PeerLatency],
    incoming: &[PeerLatency],
    previous_count: f64,
) -> Vec<PeerLatency> {
    let total = previous_count + 1.0;
    let mut merged: Vec<PeerLatency> = Vec::with_capacity(incoming.len().max(previous.len()));
    for fresh in incoming {
        let old = previous.iter().find(|peer| peer.node_id == fresh.node_id);
        let mut result = fresh.clone();
        result.latency_ms = match (old.and_then(|old| old.latency_ms), fresh.latency_ms) {
            (Some(old), Some(new)) => Some((old * previous_count + new) / total),
            (Some(old), None) if fresh.reachable => Some(old),
            (_, new) => new,
        };
        merged.push(result);
    }
    for old in previous {
        if !incoming.iter().any(|peer| peer.node_id == old.node_id) {
            merged.push(old.clone());
        }
    }
    merged.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    merged
}

/// 单节点的两个独立事实：最近一次上报 与 最近一次成功采样。
///
/// 一次采集失败**不得**冲掉上一次成功值——否则「成功后紧跟失败」会让新鲜度倒退成空。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// 最近一次上报（含失败），用于显示当前采集状态与时钟偏移。
    pub last_report: MetricsRecord,
    /// 最近一次**成功**采样；从未成功过时为 None。
    pub last_success: Option<MetricsRecord>,
}

impl NodeMetrics {
    /// 公开/控制台状态：从未上报 unknown，当前失败 collect_failed，否则 ok。
    pub fn status(&self) -> &'static str {
        if self.last_report.error.is_some() {
            "collect_failed"
        } else if self.last_success.is_some() {
            "ok"
        } else {
            // 上报过但既无成功采样也无 error：保守视为未知，不冒充 ok
            "unknown"
        }
    }

    /// 新鲜度时刻：只认成功采样。
    pub fn metrics_at(&self) -> Option<i64> {
        self.last_success.as_ref().map(|r| r.received_at)
    }

    /// 展示用采样：成功值优先；从未成功则为 None（不是零值）。
    pub fn sample(&self) -> Option<&MetricsSample> {
        self.last_success.as_ref().and_then(|r| r.sample.as_ref())
    }
}

/// 写入一次上报：更新内存中的「最近上报」；仅成功时更新「最近成功」并写历史。
///
/// 采集失败的上报不写历史——历史里不该出现零值或伪造时间点。
pub async fn ingest(
    db: &Store,
    latest: &tokio::sync::RwLock<HashMap<String, NodeMetrics>>,
    node_id: &str,
    report: MetricsReport,
) -> Result<()> {
    let received_at = now();
    let record = MetricsRecord {
        node_id: node_id.to_owned(),
        received_at,
        // 该估计包含单向网络时延，只用于识别明显偏差
        clock_offset: received_at - report.at,
        sample: report.sample.clone(),
        error: report.error.clone(),
    };
    let success = report.sample.is_some();
    {
        let mut cache = latest.write().await;
        let entry = cache
            .entry(node_id.to_owned())
            .or_insert_with(|| NodeMetrics {
                last_report: record.clone(),
                last_success: None,
            });
        entry.last_report = record.clone();
        if success {
            entry.last_success = Some(record);
        }
    }

    let Some(sample) = report.sample else {
        return Ok(());
    };
    for (tier, width, _) in TIERS {
        let bucket_at = sample.at / width * width;
        let value = if width <= 15 {
            serde_json::to_string(&sample)?
        } else {
            let existing = db
                .metrics_range(node_id, tier, bucket_at, bucket_at, 1)
                .await?
                .into_iter()
                .next()
                .and_then(|(_, value)| serde_json::from_str::<Bucket>(&value).ok());
            let mut bucket = existing.unwrap_or_default();
            bucket.merge(&sample);
            serde_json::to_string(&bucket)?
        };
        db.metrics_put(node_id, tier, bucket_at, &value).await?;
    }
    Ok(())
}

/// 按保留策略清理过期记录。由后台任务定期调用。
pub async fn prune(db: &Store) -> Result<()> {
    let now = now();
    for (tier, _, retention) in TIERS {
        let removed = db.metrics_prune(tier, now - retention).await?;
        if removed > 0 {
            tracing::info!(tier, removed, "清理过期指标");
        }
    }
    Ok(())
}

/// 曲线上一个点。字段扁平且单位明确，便于直接绘图。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsPoint {
    pub at: i64,
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub load1: f64,
    pub disk_read_bytes_per_second: f64,
    pub disk_write_bytes_per_second: f64,
    pub net_rx_bytes_per_second: f64,
    pub net_tx_bytes_per_second: f64,
}

impl MetricsPoint {
    /// 从一次采样折算，网卡速率按所有接口求和。
    pub fn from_sample(sample: &MetricsSample) -> Self {
        Self {
            at: sample.at,
            cpu_usage: sample.cpu_usage,
            memory_used: sample.memory_used,
            memory_total: sample.memory_total,
            swap_used: sample.swap_used,
            load1: sample.load1,
            disk_read_bytes_per_second: sample.disk_read_bytes_per_second,
            disk_write_bytes_per_second: sample.disk_write_bytes_per_second,
            net_rx_bytes_per_second: sample
                .interfaces
                .iter()
                .map(|i| i.rx_bytes_per_second)
                .sum(),
            net_tx_bytes_per_second: sample
                .interfaces
                .iter()
                .map(|i| i.tx_bytes_per_second)
                .sum(),
        }
    }
}

/// 依据请求步长选择层级：步长越大用越粗的层级，保证返回点数可控。
pub fn tier_for_step(step: i64) -> &'static str {
    if step < 300 {
        "raw"
    } else if step < 3600 {
        "m5"
    } else {
        "h1"
    }
}

/// 把逐条记录按步长抽稀成一个点，桶内取最后一条。
/// 缺失区间自然形成空隙，不做插值。
pub fn decimate(rows: Vec<(i64, String)>, step: i64, raw: bool) -> Vec<MetricsPoint> {
    let mut points: Vec<MetricsPoint> = Vec::new();
    let mut last_bucket: Option<i64> = None;
    for (at, value) in rows {
        let bucket = at / step * step;
        if last_bucket == Some(bucket) {
            continue;
        }
        let sample = if raw {
            serde_json::from_str::<MetricsSample>(&value).ok()
        } else {
            serde_json::from_str::<Bucket>(&value)
                .ok()
                .map(|b| b.sample)
        };
        let Some(sample) = sample else { continue };
        last_bucket = Some(bucket);
        points.push(MetricsPoint::from_sample(&sample));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: i64, cpu: f64, memory_used: u64) -> MetricsSample {
        MetricsSample {
            at,
            cpu_usage: cpu,
            memory_used,
            memory_total: 1000,
            load1: cpu / 10.0,
            interfaces: vec![opsd::protocol::InterfaceRate {
                name: "eth0".into(),
                rx_bytes_per_second: 100.0,
                tx_bytes_per_second: 50.0,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn 桶内标量取平均而明细取最后一次() {
        let mut bucket = Bucket::default();
        bucket.merge(&sample(100, 10.0, 100));
        bucket.merge(&sample(200, 30.0, 300));
        assert_eq!(bucket.count, 2);
        assert!(
            (bucket.sample.cpu_usage - 20.0).abs() < 1e-9,
            "CPU 应取平均"
        );
        assert_eq!(bucket.sample.memory_used, 200, "内存应取平均");
        assert_eq!(bucket.sample.at, 200, "时间戳取最后一次");
        assert_eq!(bucket.sample.interfaces.len(), 1);
        // 第三次合并后平均应随之变化
        bucket.merge(&sample(300, 60.0, 600));
        assert_eq!(bucket.count, 3);
        assert!((bucket.sample.cpu_usage - 100.0 / 3.0).abs() < 1e-9);
    }

    fn peer(node_id: &str, latency: Option<f64>, reachable: bool) -> PeerLatency {
        PeerLatency {
            node_id: node_id.into(),
            address: format!("100.100.201.{node_id}"),
            method: "icmp".into(),
            latency_ms: latency,
            reachable,
            error: (!reachable).then(|| "ICMP 无回应".to_string()),
            probed_at: 0,
        }
    }

    #[test]
    fn 对端延迟在桶内按对端平均() {
        let mut first = sample(100, 1.0, 1);
        first.peers = vec![
            peer("C001", Some(10.0), true),
            peer("C002", Some(20.0), true),
        ];
        let mut second = sample(200, 1.0, 1);
        second.peers = vec![
            peer("C001", Some(30.0), true),
            peer("C002", Some(40.0), true),
        ];

        let mut bucket = Bucket::default();
        bucket.merge(&first);
        bucket.merge(&second);
        let peers = &bucket.sample.peers;
        assert_eq!(peers.len(), 2);
        // (10+30)/2 与 (20+40)/2：延迟是要看趋势的标量，必须平均，
        // 而不是像磁盘明细那样只留最后一次
        assert!((peers[0].latency_ms.unwrap() - 20.0).abs() < 1e-9);
        assert!((peers[1].latency_ms.unwrap() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn 一次丢包不会把延迟曲线打成空洞() {
        let mut first = sample(100, 1.0, 1);
        first.peers = vec![peer("C001", Some(10.0), true)];
        // 这一轮探测工具还没就绪/没测到，但也没判为不可达：保留上次的数字
        let mut second = sample(200, 1.0, 1);
        second.peers = vec![peer("C001", None, true)];

        let mut bucket = Bucket::default();
        bucket.merge(&first);
        bucket.merge(&second);
        assert_eq!(
            bucket.sample.peers[0].latency_ms,
            Some(10.0),
            "没有新数字时不应把曲线打断"
        );
    }

    #[test]
    fn 明确不可达时以最新事实为准而不是留着旧延迟() {
        let mut first = sample(100, 1.0, 1);
        first.peers = vec![peer("C001", Some(10.0), true)];
        let mut second = sample(200, 1.0, 1);
        second.peers = vec![peer("C001", None, false)];

        let mut bucket = Bucket::default();
        bucket.merge(&first);
        bucket.merge(&second);
        assert_eq!(
            bucket.sample.peers[0].latency_ms, None,
            "已经不可达了，就不能继续显示上一次的延迟"
        );
        assert!(!bucket.sample.peers[0].reachable);
    }

    #[test]
    fn 桶里保留旧对端直到它从目标集合消失() {
        let mut first = sample(100, 1.0, 1);
        first.peers = vec![
            peer("C001", Some(10.0), true),
            peer("C002", Some(20.0), true),
        ];
        let mut second = sample(200, 1.0, 1);
        // 这一轮只上报了 C001（例如 C002 暂时从目录里消失）
        second.peers = vec![peer("C001", Some(12.0), true)];

        let mut bucket = Bucket::default();
        bucket.merge(&first);
        bucket.merge(&second);
        let ids: Vec<&str> = bucket
            .sample
            .peers
            .iter()
            .map(|p| p.node_id.as_str())
            .collect();
        assert_eq!(ids, vec!["C001", "C002"], "顺序稳定，且旧对端不会凭空消失");
    }

    #[test]
    fn 没有探测的采样不会凭空造出对端() {
        let mut bucket = Bucket::default();
        bucket.merge(&sample(100, 1.0, 1));
        assert!(bucket.sample.peers.is_empty());
    }

    #[test]
    fn 步长决定层级() {
        assert_eq!(tier_for_step(15), "raw");
        assert_eq!(tier_for_step(299), "raw");
        assert_eq!(tier_for_step(300), "m5");
        assert_eq!(tier_for_step(3599), "m5");
        assert_eq!(tier_for_step(3600), "h1");
        assert_eq!(tier_for_step(86400), "h1");
    }

    #[test]
    fn 抽稀时同桶只保留首个且不插值() {
        // 原始记录每 15 秒一条，共 5 条；步长 60 秒 → 首条落在 0，其余同桶被跳过
        let rows: Vec<(i64, String)> = [0, 15, 30, 45, 60]
            .iter()
            .map(|at| (*at, serde_json::to_string(&sample(*at, 5.0, 10)).unwrap()))
            .collect();
        let points = decimate(rows, 60, true);
        assert_eq!(points.len(), 2, "60 与 120 两个桶各出一点");
        assert_eq!(points[0].at, 0);
        assert_eq!(points[1].at, 60);
    }

    #[test]
    fn 缺失区间不会补点或补零() {
        // 中间缺少 [30,60) 的数据，曲线上就只应有空档，不能出现零值点
        let rows: Vec<(i64, String)> = [0, 60, 90]
            .iter()
            .map(|at| (*at, serde_json::to_string(&sample(*at, 7.0, 20)).unwrap()))
            .collect();
        let points = decimate(rows, 15, true);
        assert_eq!(points.len(), 3);
        assert!(points.iter().all(|p| p.cpu_usage == 7.0));
        assert!(points.iter().all(|p| p.at != 15 && p.at != 45));
    }

    #[test]
    fn 网卡速率按接口求和() {
        let mut one = sample(0, 1.0, 1);
        one.interfaces = vec![
            opsd::protocol::InterfaceRate {
                name: "eth0".into(),
                rx_bytes_per_second: 10.0,
                tx_bytes_per_second: 1.0,
                ..Default::default()
            },
            opsd::protocol::InterfaceRate {
                name: "eth1".into(),
                rx_bytes_per_second: 5.0,
                tx_bytes_per_second: 2.0,
                ..Default::default()
            },
        ];
        let point = MetricsPoint::from_sample(&one);
        assert_eq!(point.net_rx_bytes_per_second, 15.0);
        assert_eq!(point.net_tx_bytes_per_second, 3.0);
    }

    #[test]
    fn 无法解析的记录被跳过而不是当作零值() {
        let rows = vec![
            (0, "不是 JSON".to_owned()),
            (15, serde_json::to_string(&sample(15, 9.0, 1)).unwrap()),
        ];
        let points = decimate(rows, 15, true);
        assert_eq!(points.len(), 1, "坏记录不能变成零值点");
        assert_eq!(points[0].cpu_usage, 9.0);
    }

    #[test]
    fn 保留策略按层级递增() {
        let retention: HashMap<&str, i64> = TIERS.iter().map(|(t, _, r)| (*t, *r)).collect();
        assert!(retention["raw"] < retention["m5"]);
        assert!(retention["m5"] < retention["h1"]);
        // 桶宽也必须递增，否则降采样没有意义
        let widths: Vec<i64> = TIERS.iter().map(|(_, w, _)| *w).collect();
        assert!(widths.windows(2).all(|w| w[0] < w[1]));
    }
}
