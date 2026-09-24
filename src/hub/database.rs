//! 数据库只读运维视图（Galera / 主机侧 / ProxySQL / 备份链路）。
//!
//! # 判断在控制台，事实在节点
//!
//! Agent 只带回事实，所有阈值与比较都在这里完成。这样控制台的结论可以被复核，
//! 而且**阈值与既有告警规则保持一致**——否则控制台说"正常"、告警说"异常"，
//! 运维人员就再也不能相信任何一个。对齐关系：
//!
//! | 判断 | 阈值 | 对应告警 |
//! |---|---|---|
//! | 集群成员数不等于期望值 | 5 | `GaleraClusterSizeMismatch` |
//! | 非 Primary / Ready 或 Synced 不成立 | — | `GaleraNotPrimary`、`GaleraNodeNotReadyOrSynced` |
//! | 接收队列过高 | > 32 | `GaleraReceiveQueueHigh` |
//! | 流控暂停比例过高 | > 0.01 | `GaleraFlowControlPausedHigh` |
//! | 认证冲突 | > 0 | `GaleraCertificationConflicts` |
//! | ProxySQL writer 不等于 1 | ≠ 1 | `ProxySQLWriterCountInvalid` |
//! | ProxySQL 备用 writer 不足 | < 4 | `ProxySQLBackupWritersInsufficient` |
//! | 小时备份过期 | > 90 分钟 | `HourlyBackupStale` |
//! | 每日备份过期 | > 26 小时 | `DailyBackupStale` |
//!
//! # 「未知」不是「正常」
//!
//! 连不上、没配置、没权限都产出**未知**，并在结论里如实说明。
//! 未知不参与严重程度排序（`tone` 只描述已经确定的问题），而是单独列出，
//! 因为把未知混进"正常"或"警告"都会误导人。
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::*;
use axum::{Extension, Json, extract::State as S};
use opsd::protocol::*;

/// 接收队列告警阈值，与 `galera-alerts.yml` 一致。
pub const RECV_QUEUE_LIMIT: u64 = 32;
/// 流控暂停比例阈值。
pub const FLOW_CONTROL_LIMIT: f64 = 0.01;
/// 小时备份允许的最大间隔（秒）。
pub const HOURLY_STALE: i64 = 5_400;
/// 每日备份允许的最大间隔（秒）。
pub const DAILY_STALE: i64 = 93_600;
/// 期望的 ProxySQL writer 数量。
pub const WRITER_EXPECTED: u64 = 1;
/// 期望的 ProxySQL 备用 writer 数量下限。
pub const BACKUP_WRITERS_MIN: u64 = 4;

/// 各节点最近一次只读巡检结果。
pub type SharedDbReports = Arc<RwLock<HashMap<String, DbInspectReport>>>;

/// 结论的语气。只描述**已经确定**的问题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Ok,
    Warning,
    Critical,
}

/// 一条结论：语气 + 一句话 + 具体依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub tone: Tone,
    pub title: String,
    pub detail: String,
}

impl Finding {
    fn critical(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            tone: Tone::Critical,
            title: title.into(),
            detail: detail.into(),
        }
    }
    fn warning(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            tone: Tone::Warning,
            title: title.into(),
            detail: detail.into(),
        }
    }
}

/// 单个节点的巡检结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeVerdict {
    pub node_id: String,
    pub name: String,
    pub online: bool,
    /// 最近一次巡检时刻。`None` 表示从未巡检过。
    #[serde(default)]
    pub collected_at: Option<i64>,
    /// 是否在本机配置了只读巡检账号。
    pub configured: bool,
    /// 已确定问题的严重程度。**不含未知**。
    pub tone: Tone,
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// 未知项的原因列表。界面必须把它们显示出来，而不是只显示 tone。
    #[serde(default)]
    pub unknown: Vec<String>,
    /// 原始报告，供界面展开细节。
    #[serde(default)]
    pub report: Option<DbInspectReport>,
}

/// 整份运维视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub generated_at: i64,
    pub nodes: Vec<NodeVerdict>,
    /// 跨节点的一致性检查（同一集群的成员应当互相吻合）。
    #[serde(default)]
    pub checks: Vec<Finding>,
    /// 全集群结论。只有一台节点说"不一致"时也必须把它说出来。
    pub summary: String,
    pub expected_cluster_size: u64,
    pub tone: Tone,
    /// 全集群范围内未知项的数量，便于一眼看出"还有多少没看到"。
    pub unknown_count: usize,
}

/// 一台节点的巡检结论。
pub fn verdict_for(
    node: &Node,
    online: bool,
    report: Option<&DbInspectReport>,
    now_at: i64,
) -> NodeVerdict {
    let mut verdict = NodeVerdict {
        node_id: node.id.clone(),
        name: node.name.clone(),
        online,
        collected_at: report.map(|report| report.collected_at),
        configured: report.is_some_and(|report| report.configured),
        tone: Tone::Ok,
        findings: Vec::new(),
        unknown: Vec::new(),
        report: report.cloned(),
    };
    let Some(report) = report else {
        // 没巡检过就是未知：既不是 0 也不是健康
        verdict
            .unknown
            .push("尚未巡检（请先触发一次只读巡检）".into());
        return verdict;
    };
    if !online {
        // 报告是过去的，但节点现在离线：事实仍可展示，结论必须说明时间点
        verdict.unknown.push(format!(
            "节点当前离线，以下为 {} 秒前的巡检结果",
            (now_at - report.collected_at).max(0)
        ));
    }
    if !report.configured {
        verdict
            .unknown
            .push("本机未配置数据库只读巡检账号".into());
    }

    // ---- Galera ----
    match &report.galera.data {
        None => verdict.unknown.push(format!(
            "Galera 状态未知：{}",
            report.galera.reason.as_deref().unwrap_or("未说明原因")
        )),
        Some(galera) => galera_findings(galera, &mut verdict),
    }

    // ---- 主机侧 ----
    match &report.host.data {
        None => verdict.unknown.push(format!(
            "主机侧状态未知：{}",
            report.host.reason.as_deref().unwrap_or("未说明原因")
        )),
        Some(host) => {
            if host.read_only == Some(false) {
                // 只读副本上的写入会破坏集群一致性假设，值得提示（不一定是错误）
                verdict.findings.push(Finding::warning(
                    "节点可写",
                    "read_only 为 OFF；若是被提升的 writer 则属正常，否则应确认写入来源",
                ));
            }
        }
    }

    // ---- ProxySQL ----
    match &report.proxysql.data {
        None => verdict.unknown.push(format!(
            "ProxySQL 未知：{}",
            report.proxysql.reason.as_deref().unwrap_or("未说明原因")
        )),
        Some(proxysql) => proxysql_findings(proxysql, &mut verdict),
    }

    // ---- 备份链路 ----
    match &report.backup.data {
        None => verdict.unknown.push(format!(
            "备份链路未知：{}",
            report.backup.reason.as_deref().unwrap_or("未说明原因")
        )),
        Some(backup) => backup_findings(backup, &mut verdict),
    }

    verdict.tone = verdict
        .findings
        .iter()
        .map(|finding| finding.tone)
        .max()
        .unwrap_or(Tone::Ok);
    verdict
}

/// Galera 的判定。阈值全部对齐告警规则。
pub fn galera_findings(galera: &GaleraState, verdict: &mut NodeVerdict) {
    let expected = if galera.expected_cluster_size == 0 {
        0
    } else {
        galera.expected_cluster_size
    };
    if galera.connected == Some(false) {
        verdict.findings.push(Finding::critical(
            "与集群断开",
            "wsrep_connected 为 OFF：该节点已不参与复制",
        ));
    }
    if galera.cluster_status != "Primary" {
        verdict.findings.push(Finding::critical(
            "不在 Primary 组件",
            format!(
                "wsrep_cluster_status={}；该组件不接受写入",
                galera.cluster_status
            ),
        ));
    }
    if galera.ready == Some(false) {
        verdict.findings.push(Finding::critical(
            "未就绪",
            "wsrep_ready 为 OFF：节点无法接收写入",
        ));
    }
    // 4 = Synced。Donor/Desynced 等状态值得高亮，但用词要准确
    if let Some(state) = galera.local_state
        && state != 4
    {
        verdict.findings.push(Finding::critical(
            format!("本地状态不是 Synced（{}）", galera.local_state_comment),
            format!("wsrep_local_state={state}，期望 4"),
        ));
    }
    if expected > 0
        && let Some(size) = galera.cluster_size
        && size != expected
    {
        verdict.findings.push(Finding::critical(
            "集群成员数与期望不符",
            format!("本节点看到的成员数是 {size}，期望 {expected}；可能有节点掉线或多余节点"),
        ));
    }
    if galera.desync == Some(true) {
        verdict.findings.push(Finding::warning(
            "节点处于 desync",
            "该节点不参与复制确认，但仍可能接受读取流量",
        ));
    }
    if let Some(queue) = galera.recv_queue
        && queue > RECV_QUEUE_LIMIT
    {
        verdict.findings.push(Finding::warning(
            "接收队列积压",
            format!("wsrep_local_recv_queue={queue}，超过阈值 {RECV_QUEUE_LIMIT}"),
        ));
    }
    if let Some(paused) = galera.flow_control_paused
        && paused > FLOW_CONTROL_LIMIT
    {
        verdict.findings.push(Finding::warning(
            "流控暂停比例偏高",
            format!("wsrep_flow_control_paused={paused:.4}，超过阈值 {FLOW_CONTROL_LIMIT}"),
        ));
    }
    let conflicts = galera.cert_failures.unwrap_or(0) + galera.bf_aborts.unwrap_or(0);
    if conflicts > 0 {
        verdict.findings.push(Finding::warning(
            "存在认证冲突",
            format!("认证失败 {} 次、BF 中止 {} 次（累计值）", galera.cert_failures.unwrap_or(0), galera.bf_aborts.unwrap_or(0)),
        ));
    }
}

/// ProxySQL 的判定。
pub fn proxysql_findings(proxysql: &ProxySqlState, verdict: &mut NodeVerdict) {
    match proxysql.writer_online {
        Some(count) if count != WRITER_EXPECTED => verdict.findings.push(Finding::critical(
            "writer 数量异常",
            format!("在线 writer 为 {count}，期望 {WRITER_EXPECTED}"),
        )),
        None => verdict.unknown.push("ProxySQL writer 数量未知".into()),
        _ => {}
    }
    match proxysql.backup_writer_online {
        Some(count) if count < BACKUP_WRITERS_MIN => verdict.findings.push(Finding::warning(
            "备用 writer 不足",
            format!("在线备用 writer 为 {count}，期望至少 {BACKUP_WRITERS_MIN}"),
        )),
        None => verdict.unknown.push("ProxySQL 备用 writer 数量未知".into()),
        _ => {}
    }
    let down: Vec<&ProxySqlServer> = proxysql
        .servers
        .iter()
        .filter(|server| server.status != "ONLINE")
        .collect();
    if !down.is_empty() {
        verdict.findings.push(Finding::warning(
            "后端存在非 ONLINE 节点",
            down.iter()
                .map(|server| format!("{}:{}={}", server.hostname, server.port, server.status))
                .collect::<Vec<_>>()
                .join("，"),
        ));
    }
}

/// 备份链路的判定。判据是**最近一次成功**的时间，而不是目录是否存在。
pub fn backup_findings(backup: &BackupState, verdict: &mut NodeVerdict) {
    for tier in &backup.tiers {
        // weekly 是副本，新鲜度看的是 hourly/daily
        let limit = match tier.tier.as_str() {
            "hourly" => Some(HOURLY_STALE),
            "daily" => Some(DAILY_STALE),
            _ => None,
        };
        match (tier.last_success, limit) {
            (None, Some(_)) => verdict.unknown.push(format!(
                "{} 备份没有成功记录（{}）",
                tier.tier, tier.directory
            )),
            (Some(_), Some(limit)) => {
                let age = tier.age_seconds.unwrap_or(0);
                if age > limit {
                    verdict.findings.push(Finding::critical(
                        format!("{} 备份过期", tier.tier),
                        format!(
                            "最近一次成功在 {age} 秒前，超过允许的 {limit} 秒",
                        ),
                    ));
                }
            }
            _ => {}
        }
        // 归档格式不对说明备份产物已经损坏，这比"备份过期"更糟
        if tier.age_header_ok == Some(false) {
            verdict.findings.push(Finding::critical(
                format!("{} 备份产物不是 age 格式", tier.tier),
                format!("{} 里的 .age 文件头不正确", tier.directory),
            ));
        } else if tier.age_header_ok.is_none() && tier.files > 0 {
            verdict.unknown.push(format!(
                "{} 备份目录里没有 .age 归档，无法判断产物",
                tier.tier
            ));
        }
        if tier.files > 0 && !tier.has_checksums {
            verdict.findings.push(Finding::warning(
                format!("{} 备份缺少校验清单", tier.tier),
                format!("{} 里没有 SHA256SUMS", tier.directory),
            ));
        }
    }
    if backup.next_run.is_none() {
        verdict
            .unknown
            .push("未读到备份定时器的下次触发时刻".into());
    }
}

/// 跨节点一致性检查。
///
/// 单节点自洽不等于集群自洽：**同一集群的成员必须互相吻合**。
/// 这里看的是那些"只有把节点放在一起才能发现"的问题。
pub fn cluster_checks(verdicts: &[&NodeVerdict], expected_cluster_size: u64) -> Vec<Finding> {
    let mut checks = Vec::new();
    let galeras: Vec<(&NodeVerdict, &GaleraState)> = verdicts
        .iter()
        .filter_map(|verdict| {
            verdict
                .report
                .as_ref()
                .and_then(|report| report.galera.data.as_ref())
                .map(|galera| (*verdict, galera))
        })
        .collect();

    // 集群 UUID 不一致 = 出现了脑裂或混入了别的集群的成员
    let mut by_uuid: HashMap<&str, Vec<&str>> = HashMap::new();
    for (verdict, galera) in &galeras {
        if let Some(uuid) = galera.cluster_state_uuid.as_deref() {
            by_uuid.entry(uuid).or_default().push(&verdict.name);
        }
    }
    if by_uuid.len() > 1 {
        let detail = by_uuid
            .iter()
            .map(|(uuid, names)| format!("{}：{}", &uuid[..8.min(uuid.len())], names.join("、")))
            .collect::<Vec<_>>()
            .join("；");
        checks.push(Finding::critical(
            "集群 UUID 不一致",
            format!("成员看到的 wsrep_cluster_state_uuid 不同，可能已分区：{detail}"),
        ));
    }

    // 节点 UUID 重复 = 有节点是从同一份数据目录克隆出来的
    let mut by_node: HashMap<&str, Vec<&str>> = HashMap::new();
    for (verdict, galera) in &galeras {
        if let Some(uuid) = galera.node_uuid.as_deref() {
            by_node.entry(uuid).or_default().push(&verdict.name);
        }
    }
    for (uuid, names) in by_node.iter().filter(|(_, names)| names.len() > 1) {
        checks.push(Finding::critical(
            "节点 UUID 重复",
            format!(
                "{} 共用了同一个 wsrep 节点 UUID（{}），通常意味着数据目录被克隆",
                names.join("、"),
                &uuid[..8.min(uuid.len())]
            ),
        ));
    }

    // 成员数不一致：各节点看到的规模不同，说明副本之间还没收敛
    let sizes: Vec<u64> = galeras
        .iter()
        .filter_map(|(_, galera)| galera.cluster_size)
        .collect();
    if sizes.len() > 1 && sizes.iter().any(|size| *size != sizes[0]) {
        checks.push(Finding::critical(
            "各节点看到的成员数不一致",
            format!("{sizes:?}；通常意味着有节点掉线或尚未完成状态传输"),
        ));
    }

    // 多数派：健康成员少于半数时集群无法接受写入。
    //
    // **只在看到全部成员时才谈多数派**：少巡检了几台不等于它们不健康，
    // 把"没看到"当成"不健康"会让结论凭空多出一堆假故障。
    let healthy = galeras
        .iter()
        .filter(|(_, galera)| galera.ready == Some(true) && galera.local_state == Some(4))
        .count();
    if expected_cluster_size > 0 && galeras.len() >= expected_cluster_size as usize {
        let quorum = (expected_cluster_size as usize / 2) + 1;
        if healthy < quorum {
            checks.push(Finding::critical(
                "健康成员少于多数派",
                format!("仅 {healthy} 台 Ready/Synced，多数派需要 {quorum} 台"),
            ));
        }
    }

    // 备份：至少有一台节点在正常备份
    let fresh = verdicts
        .iter()
        .filter(|verdict| {
            verdict.report.as_ref().is_some_and(|report| {
                report.backup.data.as_ref().is_some_and(|backup| {
                    backup.tiers.iter().any(|tier| {
                        tier.tier == "hourly" && tier.age_seconds.is_some_and(|age| age <= HOURLY_STALE)
                    })
                })
            })
        })
        .count();
    let backup_known = verdicts.iter().any(|verdict| {
        verdict
            .report
            .as_ref()
            .is_some_and(|report| report.backup.known())
    });
    if backup_known && fresh == 0 {
        checks.push(Finding::critical(
            "没有任何节点在按时备份",
            format!("看不到 90 分钟内成功的小时备份（阈值 {HOURLY_STALE} 秒）"),
        ));
    }
    checks
}

/// 整份视图。纯函数，因此可以被穷举测试。
pub fn analyze(
    nodes: &[Node],
    online: &std::collections::HashSet<String>,
    reports: &HashMap<String, DbInspectReport>,
    now_at: i64,
) -> Overview {
    let mut verdicts: Vec<NodeVerdict> = nodes
        .iter()
        .filter(|node| !node.revoked)
        .map(|node| verdict_for(node, online.contains(&node.id), reports.get(&node.id), now_at))
        .collect();
    verdicts.sort_by(|a, b| a.name.cmp(&b.name));

    let expected = verdicts
        .iter()
        .filter_map(|verdict| {
            verdict
                .report
                .as_ref()
                .and_then(|report| report.galera.data.as_ref())
                .map(|galera| galera.expected_cluster_size)
        })
        .find(|size| *size > 0)
        .unwrap_or(0);

    let refs: Vec<&NodeVerdict> = verdicts.iter().collect();
    let checks = cluster_checks(&refs, expected);

    let tone = verdicts
        .iter()
        .map(|verdict| verdict.tone)
        .chain(checks.iter().map(|finding| finding.tone))
        .max()
        .unwrap_or(Tone::Ok);
    let unknown_count = verdicts
        .iter()
        .map(|verdict| verdict.unknown.len())
        .sum::<usize>();
    let unknown_nodes = verdicts
        .iter()
        .filter(|verdict| !verdict.unknown.is_empty())
        .count();
    let inspected = verdicts
        .iter()
        .filter(|verdict| verdict.collected_at.is_some())
        .count();

    let critical = verdicts
        .iter()
        .filter(|verdict| verdict.tone == Tone::Critical)
        .count()
        + checks
            .iter()
            .filter(|finding| finding.tone == Tone::Critical)
            .count();
    let warning = verdicts
        .iter()
        .filter(|verdict| verdict.tone == Tone::Warning)
        .count()
        + checks
            .iter()
            .filter(|finding| finding.tone == Tone::Warning)
            .count();

    let summary = if inspected == 0 {
        format!(
            "还没有任何节点完成只读巡检（共 {} 台节点）。先触发一次巡检，再判断集群状态。",
            verdicts.len()
        )
    } else if critical > 0 {
        format!(
            "已巡检 {inspected}/{} 台：{critical} 项异常需要立即处理，{warning} 项需要注意。",
            verdicts.len()
        )
    } else if warning > 0 {
        format!(
            "已巡检 {inspected}/{} 台：没有发现硬性阻断，但有 {warning} 项需要注意。",
            verdicts.len()
        )
    } else if unknown_nodes > 0 {
        format!(
            "已巡检 {inspected}/{} 台，未发现异常；但 {unknown_nodes} 台节点仍有未知项，结论并不完整。",
            verdicts.len()
        )
    } else {
        format!(
            "已巡检 {inspected}/{} 台，未发现异常。",
            verdicts.len()
        )
    };

    Overview {
        generated_at: now_at,
        nodes: verdicts,
        checks,
        summary,
        expected_cluster_size: expected,
        tone,
        unknown_count,
    }
}

/* ------------------------------------------------------------------ *
 * 处理函数
 * ------------------------------------------------------------------ */

/// 数据库运维视图。
pub async fn overview(S(s): S<State>) -> ApiResult<Json<Overview>> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let online: std::collections::HashSet<String> =
        s.channels.read().await.keys().cloned().collect();
    let reports = s
        .db_reports
        .read()
        .map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "巡检缓存不可用".into()))?
        .clone();
    Ok(Json(analyze(&nodes, &online, &reports, now())))
}

/// 触发一次只读巡检。
///
/// 走任务机制以便看到执行结果与失败原因；**只读**，因此不需要确认参数。
pub async fn db_inspect(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Extension(peer): Extension<transport::Peer>,
    Json(input): Json<super::storage::ActionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let task = TaskEnvelope::new(
        node_id.clone(),
        input.idempotency_key,
        Action::DbInspect {},
    );
    let task = s.db.accept_task(&task).await?;
    if task.status == TaskStatus::Pending {
        if let Some(tx) = s.channels.read().await.get(&node_id).map(|c| c.1.clone()) {
            let _ = tx.send(Frame::Task { task: task.clone() }).await;
        }
        let _ = s.events.send(serde_json::json!({"type":"database"}));
    }
    // 只读巡检也要留痕：谁在什么时候看了哪台数据库。
    // 审计里只有对象与结果，没有 SQL 正文，也没有凭据。
    let _ = audit::record(
        &s.db,
        audit::AuditEntry {
            actor: "管理员".into(),
            node_id: node_id.clone(),
            category: "数据库".into(),
            target: "只读巡检".into(),
            result: "已开始".into(),
            detail: "Galera / ProxySQL / 备份链路，语句固定且只读".into(),
            source: peer.address.ip().to_string(),
            bytes: 0,
            duration: 0,
        },
    )
    .await;
    Ok(Json(serde_json::json!({ "task_id": task.id })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::ProxySqlServer;

    fn node(id: &str) -> Node {
        Node {
            id: id.into(),
            name: format!("{id} · 测试"),
            public_addresses: vec![],
            overlay_address: None,
            ssh_port: 22,
            revoked: false,
            last_seen: 0,
            capabilities: None,
            inventory: None,
            address_version: 0,
            region: None,
            group: None,
            tags: vec![],
            weight: 1,
            hidden: false,
            public_remark: None,
        }
    }

    fn online(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn healthy_galera() -> GaleraState {
        GaleraState {
            cluster_status: "Primary".into(),
            cluster_size: Some(5),
            expected_cluster_size: 5,
            local_state: Some(4),
            local_state_comment: "Synced".into(),
            ready: Some(true),
            connected: Some(true),
            recv_queue: Some(0),
            send_queue: Some(0),
            flow_control_paused: Some(0.0),
            cluster_state_uuid: Some("abcdef01-0000-0000-0000-000000000000".into()),
            node_uuid: Some("11111111-0000-0000-0000-000000000000".into()),
            ..Default::default()
        }
    }

    /// 一份"看起来什么都好"的报告，便于逐项打破。
    fn report(_id: &str) -> DbInspectReport {
        DbInspectReport {
            collected_at: 1_000,
            configured: true,
            galera: DbSection::ok(healthy_galera()),
            host: DbSection::ok(DbHostState {
                version: "12.2.2-MariaDB".into(),
                read_only: Some(true),
                ..Default::default()
            }),
            proxysql: DbSection::ok(ProxySqlState {
                writer_online: Some(1),
                backup_writer_online: Some(4),
                servers: vec![ProxySqlServer {
                    hostgroup_id: 10,
                    hostname: "10.0.0.1".into(),
                    port: 3306,
                    status: "ONLINE".into(),
                    weight: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            backup: DbSection::ok(BackupState {
                root: "/data1/server/db/backups".into(),
                tiers: vec![BackupTier {
                    tier: "hourly".into(),
                    last_success: Some(900),
                    age_seconds: Some(100),
                    files: 3,
                    has_checksums: true,
                    age_header_ok: Some(true),
                    ..Default::default()
                }],
                next_run: Some(2_000),
                last_trigger: Some(900),
            }),
            gaps: vec![],
        }
    }

    fn map(reports: Vec<(&str, DbInspectReport)>) -> HashMap<String, DbInspectReport> {
        reports
            .into_iter()
            .map(|(id, report)| (id.to_string(), report))
            .collect()
    }

    #[test]
    fn 一切正常时没有异常也没有未知() {
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", report("C001"))]),
            1_000,
        );
        assert_eq!(overview.tone, Tone::Ok);
        assert_eq!(overview.unknown_count, 0);
        assert!(overview.nodes[0].findings.is_empty());
        assert!(overview.summary.contains("未发现异常"), "{}", overview.summary);
    }

    #[test]
    fn 没有巡检结果是未知而不是正常() {
        let overview = analyze(&[node("C001")], &online(&["C001"]), &HashMap::new(), 1_000);
        assert_eq!(overview.tone, Tone::Ok, "未知不参与严重程度排序");
        assert_eq!(overview.unknown_count, 1);
        assert!(overview.nodes[0].unknown[0].contains("尚未巡检"));
        assert!(overview.summary.contains("还没有任何节点"), "{}", overview.summary);
        assert!(!overview.summary.contains("未发现异常"), "{}", overview.summary);
    }

    #[test]
    fn 未配置账号时说明原因而不是静默() {
        let mut r = report("C001");
        r.configured = false;
        r.galera = DbSection::unknown("本机未配置这一部分的只读巡检账号");
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert!(overview.nodes[0].unknown.iter().any(|u| u.contains("未配置")));
        assert!(overview.nodes[0].unknown.iter().any(|u| u.contains("Galera")));
        assert_eq!(overview.unknown_count, 2);
    }

    #[test]
    fn 集群成员数不符与告警规则同阈值() {
        let mut r = report("C001");
        r.galera.data.as_mut().unwrap().cluster_size = Some(4);
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Critical);
        let finding = &overview.nodes[0].findings[0];
        assert!(finding.title.contains("成员数"), "{}", finding.title);
        assert!(finding.detail.contains("期望 5"), "{}", finding.detail);
    }

    #[test]
    fn 非_primary_与未就绪都是硬性异常() {
        for break_it in [
            (|g: &mut GaleraState| g.cluster_status = "non-Primary".into()) as fn(&mut GaleraState),
            |g: &mut GaleraState| g.ready = Some(false),
            |g: &mut GaleraState| g.connected = Some(false),
            |g: &mut GaleraState| g.local_state = Some(2),
        ] {
            let mut r = report("C001");
            break_it(r.galera.data.as_mut().unwrap());
            let overview = analyze(
                &[node("C001")],
                &online(&["C001"]),
                &map(vec![("C001", r)]),
                1_000,
            );
            assert_eq!(
                overview.nodes[0].tone,
                Tone::Critical,
                "必须被判为异常：{:?}",
                overview.nodes[0].findings
            );
        }
    }

    #[test]
    fn 队列与流控按阈值升级为警告() {
        let mut r = report("C001");
        {
            let galera = r.galera.data.as_mut().unwrap();
            galera.recv_queue = Some(RECV_QUEUE_LIMIT);
            galera.flow_control_paused = Some(FLOW_CONTROL_LIMIT);
        }
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r.clone())]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Ok, "刚好等于阈值不算超限");

        r.galera.data.as_mut().unwrap().recv_queue = Some(RECV_QUEUE_LIMIT + 1);
        r.galera.data.as_mut().unwrap().flow_control_paused = Some(FLOW_CONTROL_LIMIT + 0.001);
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Warning);
        assert_eq!(overview.nodes[0].findings.len(), 2);
    }

    #[test]
    fn proxysql_writer_数量按告警规则判定() {
        let mut r = report("C001");
        r.proxysql.data.as_mut().unwrap().writer_online = Some(0);
        r.proxysql.data.as_mut().unwrap().backup_writer_online = Some(3);
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Critical);
        assert!(
            overview.nodes[0]
                .findings
                .iter()
                .any(|f| f.title.contains("writer"))
        );
        assert!(
            overview.nodes[0]
                .findings
                .iter()
                .any(|f| f.title.contains("备用 writer") && f.tone == Tone::Warning)
        );
    }

    #[test]
    fn 备份过期按_hourly_与_daily_的阈值判定() {
        for (tier, age, expected) in [
            ("hourly", HOURLY_STALE, Tone::Ok),
            ("hourly", HOURLY_STALE + 1, Tone::Critical),
            ("daily", DAILY_STALE, Tone::Ok),
            ("daily", DAILY_STALE + 1, Tone::Critical),
        ] {
            let mut r = report("C001");
            r.backup.data.as_mut().unwrap().tiers = vec![BackupTier {
                tier: tier.into(),
                last_success: Some(1),
                age_seconds: Some(age),
                files: 3,
                has_checksums: true,
                age_header_ok: Some(true),
                ..Default::default()
            }];
            let overview = analyze(
                &[node("C001")],
                &online(&["C001"]),
                &map(vec![("C001", r)]),
                1_000,
            );
            assert_eq!(
                overview.nodes[0].tone, expected,
                "{tier} 在 {age} 秒时应为 {expected:?}"
            );
        }
    }

    #[test]
    fn 备份没有成功记录是未知而不是过期() {
        let mut r = report("C001");
        r.backup.data.as_mut().unwrap().tiers = vec![BackupTier {
            tier: "hourly".into(),
            last_success: None,
            age_seconds: None,
            ..Default::default()
        }];
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Ok);
        assert!(
            overview.nodes[0]
                .unknown
                .iter()
                .any(|u| u.contains("hourly 备份没有成功记录"))
        );
    }

    #[test]
    fn 归档格式损坏优先于缺失清单() {
        let mut r = report("C001");
        r.backup.data.as_mut().unwrap().tiers = vec![BackupTier {
            tier: "daily".into(),
            last_success: Some(900),
            age_seconds: Some(100),
            files: 2,
            has_checksums: false,
            age_header_ok: Some(false),
            ..Default::default()
        }];
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.nodes[0].tone, Tone::Critical);
        assert!(
            overview.nodes[0]
                .findings
                .iter()
                .any(|f| f.title.contains("不是 age 格式"))
        );
    }

    #[test]
    fn 集群_uuid_不一致会被指出来() {
        let mut first = report("C001");
        let mut second = report("C002");
        first.galera.data.as_mut().unwrap().cluster_state_uuid = Some("aaaa1111-0000".into());
        second.galera.data.as_mut().unwrap().cluster_state_uuid = Some("bbbb2222-0000".into());
        second.galera.data.as_mut().unwrap().node_uuid = Some("22222222-0000".into());
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", first), ("C002", second)]),
            1_000,
        );
        let check = overview
            .checks
            .iter()
            .find(|check| check.title.contains("UUID 不一致"))
            .expect("必须指出集群 UUID 不一致");
        assert_eq!(check.tone, Tone::Critical);
        assert!(check.detail.contains("C001") && check.detail.contains("C002"));
    }

    #[test]
    fn 节点_uuid_重复会被指出来() {
        let mut first = report("C001");
        let mut second = report("C002");
        let shared = "33333333-0000-0000-0000-000000000000".to_string();
        first.galera.data.as_mut().unwrap().node_uuid = Some(shared.clone());
        second.galera.data.as_mut().unwrap().node_uuid = Some(shared);
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", first), ("C002", second)]),
            1_000,
        );
        assert!(
            overview
                .checks
                .iter()
                .any(|check| check.title.contains("节点 UUID 重复")),
            "{:?}",
            overview.checks
        );
    }

    #[test]
    fn 各节点看到的成员数不一致会被指出来() {
        let mut first = report("C001");
        let mut second = report("C002");
        first.galera.data.as_mut().unwrap().cluster_size = Some(5);
        second.galera.data.as_mut().unwrap().cluster_size = Some(4);
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", first), ("C002", second)]),
            1_000,
        );
        assert!(
            overview
                .checks
                .iter()
                .any(|check| check.title.contains("成员数不一致")),
            "{:?}",
            overview.checks
        );
    }

    #[test]
    fn 健康成员少于多数派会被指出来() {
        let mut reports = Vec::new();
        for (index, id) in ["C001", "C002", "C003", "C004", "C005"].iter().enumerate() {
            let mut r = report(id);
            r.galera.data.as_mut().unwrap().node_uuid = Some(format!("uuid-{index}"));
            if index >= 2 {
                // 三台连不上
                let galera = r.galera.data.as_mut().unwrap();
                galera.ready = Some(false);
                galera.local_state = Some(1);
                galera.local_state_comment = "Joining".into();
            }
            reports.push((*id, r));
        }
        let nodes: Vec<Node> = ["C001", "C002", "C003", "C004", "C005"]
            .iter()
            .map(|id| node(id))
            .collect();
        let overview = analyze(
            &nodes,
            &online(&["C001", "C002", "C003", "C004", "C005"]),
            &map(reports),
            1_000,
        );
        let check = overview
            .checks
            .iter()
            .find(|check| check.title.contains("多数派"))
            .expect("必须指出多数派不足");
        assert!(check.detail.contains("需要 3 台"), "{}", check.detail);
    }

    #[test]
    fn 只看了一部分节点时不谈多数派() {
        // 5 台集群里只巡检了 2 台且都健康：不能说"多数派不足"，
        // 因为剩下 3 台只是还没看到，不是不健康
        let mut first = report("C001");
        let mut second = report("C002");
        second.galera.data.as_mut().unwrap().node_uuid = Some("22222222-0000".into());
        first.galera.data.as_mut().unwrap().node_uuid = Some("11111111-0000".into());
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", first), ("C002", second)]),
            1_000,
        );
        assert!(
            !overview
                .checks
                .iter()
                .any(|check| check.title.contains("多数派")),
            "未看到全部成员时不应下多数派结论：{:?}",
            overview.checks
        );
        assert_eq!(overview.tone, Tone::Ok);
    }

    #[test]
    fn 没有任何节点按时备份会被指出来() {
        let mut first = report("C001");
        let mut second = report("C002");
        for r in [&mut first, &mut second] {
            r.backup.data.as_mut().unwrap().tiers = vec![BackupTier {
                tier: "hourly".into(),
                last_success: Some(1),
                age_seconds: Some(HOURLY_STALE + 10),
                files: 3,
                has_checksums: true,
                age_header_ok: Some(true),
                ..Default::default()
            }];
        }
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", first), ("C002", second)]),
            1_000,
        );
        assert!(
            overview
                .checks
                .iter()
                .any(|check| check.title.contains("没有任何节点在按时备份")),
            "{:?}",
            overview.checks
        );
    }

    #[test]
    fn 备份完全未知时不会谎称没人备份() {
        let mut r = report("C001");
        r.backup = DbSection::unknown("备份目录不存在：/data1/server/db/backups");
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert!(
            !overview
                .checks
                .iter()
                .any(|check| check.title.contains("没有任何节点在按时备份")),
            "未知不等于'没人备份'"
        );
        assert!(
            overview.nodes[0]
                .unknown
                .iter()
                .any(|u| u.contains("备份链路未知"))
        );
    }

    #[test]
    fn 离线节点仍展示上次结果但说明时间点() {
        let overview = analyze(
            &[node("C001")],
            &online(&[]),
            &map(vec![("C001", report("C001"))]),
            1_500,
        );
        assert!(!overview.nodes[0].online);
        assert!(
            overview.nodes[0]
                .unknown
                .iter()
                .any(|u| u.contains("当前离线"))
        );
        assert_eq!(overview.nodes[0].collected_at, Some(1_000));
    }

    #[test]
    fn 已撤销节点不进入视图() {
        let mut revoked = node("C009");
        revoked.revoked = true;
        let overview = analyze(
            &[node("C001"), revoked],
            &online(&["C001"]),
            &map(vec![("C001", report("C001"))]),
            1_000,
        );
        assert_eq!(overview.nodes.len(), 1);
        assert_eq!(overview.nodes[0].node_id, "C001");
    }

    #[test]
    fn 总结会先说要紧的事() {
        let mut bad = report("C001");
        bad.galera.data.as_mut().unwrap().ready = Some(false);
        let mut ok = report("C002");
        ok.galera.data.as_mut().unwrap().node_uuid = Some("other".into());
        let overview = analyze(
            &[node("C001"), node("C002")],
            &online(&["C001", "C002"]),
            &map(vec![("C001", bad), ("C002", ok)]),
            1_000,
        );
        assert!(overview.summary.contains("需要立即处理"), "{}", overview.summary);
        assert_eq!(overview.tone, Tone::Critical);
    }

    #[test]
    fn 结论必须把未知说出来() {
        let mut r = report("C001");
        r.proxysql = DbSection::unknown("容器内客户端不可用");
        r.backup = DbSection::unknown("备份目录不存在");
        let overview = analyze(
            &[node("C001")],
            &online(&["C001"]),
            &map(vec![("C001", r)]),
            1_000,
        );
        assert_eq!(overview.unknown_count, 2);
        assert!(overview.summary.contains("结论并不完整"), "{}", overview.summary);
    }
}
