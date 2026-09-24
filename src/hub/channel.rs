use super::*;
use axum::{
    Extension,
    extract::{
        Path, Query, State as S,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::Response,
    Json,
};
use super::audit::{self as audit_log, AuditEntry};
use metrics::MAX_POINTS;
/// 单次文件读取的默认上限，与 Agent 侧保持一致。
const DEFAULT_READ_LIMIT: u64 = 1024 * 1024;
use serde_json::json;

#[derive(Deserialize)]
pub struct HistoryQuery {
    from: i64,
    to: i64,
    /// 期望的点间距（秒）。它同时决定使用哪个降采样层级。
    #[serde(default = "default_step")]
    step: i64,
}
fn default_step() -> i64 {
    60
}

/// 某节点在给定区间内的指标曲线。区间内没有数据的时段不会补点。
pub async fn metrics_history(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    check(q.to > q.from, "结束时间必须晚于开始时间")?;
    let step = q.step.clamp(15, 86400);
    let span = q.to - q.from;
    check(span / step <= MAX_POINTS, "区间与步长组合返回点数过多")?;
    let tier = metrics::tier_for_step(step);
    let rows = s
        .db
        .metrics_range(&node_id, tier, q.from, q.to, MAX_POINTS + 1)
        .await?;
    let truncated = rows.len() as i64 > MAX_POINTS;
    let points = metrics::decimate(rows, step, tier == "raw");
    Ok(Json(json!({
        "node_id": node_id,
        "tier": tier,
        "step": step,
        "from": q.from,
        "to": q.to,
        "truncated": truncated,
        "points": points,
    })))
}

/// 所有节点的最新指标。控制台用它填充分配列，未上报的节点不出现。
pub async fn metrics_overview(S(s): S<State>) -> Json<serde_json::Value> {
    let latest = s.latest_metrics.read().await;
    Json(json!({
        "clock_skew_warn_seconds": metrics::CLOCK_SKEW_WARN,
        "nodes": latest.values().cloned().collect::<Vec<_>>(),
    }))
}

/// 各层级的记录数与最早时间，用于观察保留策略是否按预期生效。
pub async fn metrics_status(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let mut tiers = Vec::new();
    for (tier, width, retention) in metrics::TIERS {
        let (count, oldest) = s.db.metrics_summary(tier).await?;
        tiers.push(json!({
            "tier": tier,
            "bucket_seconds": width,
            "retention_seconds": retention,
            "records": count,
            "oldest": oldest,
        }));
    }
    Ok(Json(json!({ "tiers": tiers })))
}

pub async fn identity(s: &State, peer: &transport::Peer) -> ApiResult<CertificateBinding> {
    let cert = peer
        .certificate
        .as_ref()
        .ok_or_else(|| bad("缺少节点证书"))?;
    let binding =
        s.db.get::<CertificateBinding>("certificates", cert)
            .await?
            .filter(|b| b.expires > now())
            .ok_or_else(|| bad("证书未登记或已过期"))?;
    s.db.get::<Node>("nodes", &binding.node_id)
        .await?
        .filter(|n| !n.revoked)
        .ok_or_else(|| bad("节点已撤销"))?;
    Ok(binding)
}
pub async fn connect(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    let binding = identity(&s, &peer).await?;
    Ok(ws
        .max_message_size(1024 * 1024)
        .on_upgrade(move |socket| agent_socket(s, binding.node_id, socket)))
}
async fn agent_socket(s: State, node_id: String, mut socket: WebSocket) {
    let Some(Ok(Message::Text(text))) =
        tokio::time::timeout(std::time::Duration::from_secs(10), socket.recv())
            .await
            .ok()
            .flatten()
    else {
        return;
    };
    let Ok(Frame::Hello {
        version,
        node_id: claimed,
        capabilities,
    }) = serde_json::from_str(&text)
    else {
        return;
    };
    if version != VERSION || claimed != node_id {
        return;
    }
    let registration_lock = s.gate.lock().await;
    let Some(mut node) = s.db.get::<Node>("nodes", &node_id).await.ok().flatten() else {
        return;
    };
    if node.revoked {
        return;
    }
    node.capabilities = Some(capabilities);
    node.last_seen = now();
    if s.db.put("nodes", &node_id, &node).await.is_err() {
        return;
    }
    let generation = id();
    let (tx, mut rx) = mpsc::channel::<Frame>(64);
    s.channels
        .write()
        .await
        .insert(node_id.clone(), (generation.clone(), tx));
    drop(registration_lock);
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
    let mut last = now();
    loop {
        tokio::select! {
            biased;
            _=heartbeat.tick()=>{
                if last<now()-45 {break}
                let active=s.channels.read().await.get(&node_id).is_some_and(|c|c.0==generation);if !active {break}
                if socket.send(Message::Ping(vec![].into())).await.is_err(){break}
            },
            Some(frame)=rx.recv()=>{if socket.send(Message::Text(serde_json::to_string(&frame).unwrap().into())).await.is_err(){break}},
            frame=socket.recv()=>{
                let Some(Ok(frame))=frame else{break};last=now();
                let Message::Text(text)=frame else{continue};
                let Ok(frame)=serde_json::from_str::<Frame>(&text) else{break};
                if handle(&s,&node_id,frame).await.is_err(){break}
            }
        }
    }
    let mut channels = s.channels.write().await;
    if channels.get(&node_id).is_some_and(|c| c.0 == generation) {
        channels.remove(&node_id);
    }
    let _ = s.events.send(json!({"type":"nodes"}));
}
async fn handle(s: &State, node_id: &str, frame: Frame) -> Result<()> {
    match frame {
        Frame::Heartbeat | Frame::Inventory { .. } => {
            let _lock = s.gate.lock().await;
            let mut n =
                s.db.get::<Node>("nodes", node_id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("节点不存在"))?;
            anyhow::ensure!(!n.revoked, "节点已撤销");
            n.last_seen = now();
            if let Frame::Inventory { value } = frame {
                if let Some(version) = value
                    .pointer("/firewall/data/policy/peers/version")
                    .and_then(serde_json::Value::as_i64)
                {
                    n.address_version = version;
                }
                n.inventory = Some(value);
            }
            s.db.put("nodes", node_id, &n).await?;
            let _ = s.events.send(json!({"type":"nodes"}));
        }
        Frame::Metrics { report } => {
            // 指标是周期性遥测，不占用节点变更锁，也不改节点记录；
            // 失败只记录日志，不能因为一次写入失败就断开 Agent 连接。
            if let Err(error) = metrics::ingest(&s.db, &s.latest_metrics, node_id, report).await {
                tracing::warn!(%error, node = node_id, "指标写入失败");
            }
            let _ = s.events.send(json!({"type":"metrics"}));
        }
        Frame::Report { task } => {
            let _lock = s.gate.lock().await;
            anyhow::ensure!(task.node_id == node_id, "报告目标不匹配");
            // 主机盘点的结果不进节点记录，而是单独缓存在内存里供预检分析使用：
            // 它体积较大、变化很慢，且只有存储预检会读。
            if let Action::HostInspect {} = task.action
                && task.status == TaskStatus::Succeeded
                && let Some(result) = &task.result
                && let Ok(inventory) = serde_json::from_value::<HostInventory>(result.clone())
            {
                if let Ok(mut cache) = s.host_inventories.write() {
                    cache.insert(node_id.to_owned(), inventory);
                }
                let _ = s.events.send(json!({"type":"storage"}));
            }
            // 数据库只读巡检的结果同样只缓存在内存里：它是"此刻的状态"，
            // 落库只会留下一份会被误读为现状的历史快照。
            if let Action::DbInspect {} = task.action
                && task.status == TaskStatus::Succeeded
                && let Some(result) = &task.result
                && let Ok(report) = serde_json::from_value::<DbInspectReport>(result.clone())
            {
                if let Ok(mut cache) = s.db_reports.write() {
                    cache.insert(node_id.to_owned(), report);
                }
                let _ = s.events.send(json!({"type":"database"}));
            }
            let old =
                s.db.task(&task.id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("未知任务"))?;
            anyhow::ensure!(
                old.node_id == node_id && old.digest == task.digest,
                "报告绑定不匹配"
            );
            if old.status.terminal() || task.sequence <= old.sequence {
                return Ok(());
            }
            let mut saved = old.clone();
            saved.status = task.status;
            saved.sequence = task.sequence;
            saved.result = task.result;
            saved.error = task.error;
            saved.updated_at = now();
            s.db.save_task(&saved).await?;
            if saved.status == TaskStatus::Succeeded
                && let Action::PeerSync { set } = &saved.action
            {
                let mut n = s.db.get::<Node>("nodes", node_id).await?.unwrap();
                n.address_version = n.address_version.max(set.version);
                s.db.put("nodes", node_id, &n).await?;
            }
            let bucket = format!("events:{}", saved.id);
            let seq = saved.sequence;
            let event = TaskEvent {
                id: id(),
                task_id: saved.id.clone(),
                sequence: seq,
                time: now(),
                status: saved.status.clone(),
                message: saved
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("任务状态：{:?}", saved.status)),
            };
            s.db.put(&bucket, &format!("{seq:012}"), &event).await?;
            let _ = s
                .events
                .send(json!({"type":"task","task":saved.redacted()}));
        }
        Frame::StreamData { .. } | Frame::StreamClose { .. } => {
            let _ = s.streams.send((node_id.into(), frame));
        }
        Frame::ProbeResult { request_id, ok } => {
            s.db.put(
                "probes",
                &request_id,
                &json!({"node":node_id,"ok":ok,"time":now()}),
            )
            .await?;
        }
        _ => anyhow::bail!("不允许的节点消息"),
    };
    Ok(())
}
pub async fn dispatch(s: State) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3));
    loop {
        tick.tick().await;
        let Ok(tasks) = s.db.tasks().await else {
            continue;
        };
        let channels = s.channels.read().await;
        let mut active = std::collections::HashSet::new();
        let mut firewall_busy = false;
        for t in tasks.iter().filter(|t| {
            !t.status.terminal()
                && t.status != TaskStatus::Pending
                && channels.contains_key(&t.node_id)
        }) {
            active.insert(t.node_id.clone());
            if matches!(
                t.action,
                Action::FirewallApply { .. } | Action::PeerSync { .. }
            ) {
                firewall_busy = true;
            }
        }
        let mut ordered = tasks;
        ordered.sort_by_key(|t| t.created_at);
        for t in ordered.into_iter().filter(|t| !t.status.terminal()) {
            let Some((_, tx)) = channels.get(&t.node_id) else {
                continue;
            };
            if t.status == TaskStatus::Pending {
                if active.contains(&t.node_id) || active.len() >= 3 {
                    continue;
                }
                let fw = matches!(
                    t.action,
                    Action::FirewallApply { .. } | Action::PeerSync { .. }
                );
                if fw && firewall_busy {
                    continue;
                }
                if fw {
                    firewall_busy = true
                }
                active.insert(t.node_id.clone());
            }
            let _ = tx.try_send(Frame::Task { task: t });
        }
    }
}
#[derive(Deserialize)]
pub struct StreamQuery {
    /// `container` / `shell` / `file_read` / `file_write`
    kind: String,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    workdir: Option<String>,
    csrf: String,
    command: Option<String>,
}

impl StreamQuery {
    /// 把查询参数折算成流目标。非法组合明确拒绝，不做猜测。
    fn target(&self) -> ApiResult<StreamTarget> {
        match self.kind.as_str() {
            "container" => Ok(StreamTarget::Container {
                id: self
                    .container
                    .clone()
                    .ok_or_else(|| bad("容器会话缺少容器编号"))?,
            }),
            "shell" => Ok(StreamTarget::Host {
                shell: "/bin/bash".into(),
                workdir: self.workdir.clone(),
            }),
            "file_list" => Ok(StreamTarget::Dir {
                path: self.path.clone().ok_or_else(|| bad("目录列表缺少路径"))?,
            }),
            "file_read" | "file_write" => Ok(StreamTarget::File {
                path: self.path.clone().ok_or_else(|| bad("文件会话缺少路径"))?,
                offset: self.offset.unwrap_or(0),
                limit: self.limit.unwrap_or(DEFAULT_READ_LIMIT),
            }),
            other => Err(bad(&format!("未知的流类型：{other}"))),
        }
    }
}

/// 浏览器侧的流会话：容器终端/日志、宿主机终端、文件读写共用同一条通道。
///
/// 建立会话前必须通过来源与 CSRF 校验；宿主机终端与文件操作另外写入审计。
pub async fn browser_stream(
    S(s): S<State>,
    Path(node): Path<String>,
    Query(q): Query<StreamQuery>,
    Extension(session): Extension<auth::Session>,
    Extension(peer): Extension<transport::Peer>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    check(
        h.get("origin").and_then(|v| v.to_str().ok()) == Some(s.origin.as_str())
            && q.csrf == session.csrf,
        "终端来源或授权无效",
    )?;
    let target = q.target()?;
    let kind = match q.kind.as_str() {
        "container" => StreamKind::ContainerTerminal,
        "shell" => StreamKind::HostShell,
        "file_list" => StreamKind::FileList,
        "file_read" => StreamKind::FileRead,
        _ => StreamKind::FileWrite,
    };
    let tx = s
        .channels
        .read()
        .await
        .get(&node)
        .map(|c| c.1.clone())
        .ok_or_else(|| bad("节点离线"))?;
    let stream_id = id();
    // 宿主机终端与文件读写都记审计：谁、在哪个节点、开了什么会话
    let audit = matches!(kind, StreamKind::HostShell | StreamKind::FileList | StreamKind::FileRead | StreamKind::FileWrite)
        .then(|| session_audit(&kind, &target));
    if let Some((category, target_text)) = audit.clone() {
        let _ = audit_log::record(
            &s.db,
            AuditEntry {
                actor: "管理员".into(),
                node_id: node.clone(),
                category,
                target: target_text,
                result: "已开始".into(),
                detail: String::new(),
                source: peer.address.ip().to_string(),
                bytes: 0,
                duration: 0,
            },
        )
        .await;
    }
    let mut events = s.streams.subscribe();
    let database = s.db.clone();
    Ok(ws.max_message_size(65536).on_upgrade(move|mut socket|async move{
        let command=q.command.map(|s|vec![s]).unwrap_or_else(||vec!["/bin/sh".into()]);
        if tx.send(Frame::StreamOpen{stream_id:stream_id.clone(),kind,target,command}).await.is_err(){return}
        let started=now();
        let mut received: u64 = 0;
        loop{tokio::select!{
            event=events.recv()=>{match event{Ok((n,f)) if n==node=>{match f{Frame::StreamData{stream_id:i,data} if i==stream_id=>{received+=data.len() as u64;let failed=socket.send(Message::Text(data.into())).await.is_err();if failed {break}},Frame::StreamClose{stream_id:i,error} if i==stream_id=>{let _=socket.send(Message::Text(json!({"error":error}).to_string().into())).await;break},_=>{}}},Err(_)=>break,_=>{}}},
            message=socket.recv()=>{match message{Some(Ok(Message::Text(data)))=>{let failed=tx.try_send(Frame::StreamData{stream_id:stream_id.clone(),data:data.to_string()}).is_err();if failed {break}},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,_=>{}}},
            _=tokio::time::sleep(std::time::Duration::from_secs((session.expires-now()).max(0) as u64))=>break,
        }}
        let _=tx.send(Frame::StreamClose{stream_id,error:None}).await;
        // 会话结束时补一条审计，记录时长与传输量
        if let Some((category,target_text))=audit{
            let _=audit_log::record(&database,AuditEntry{
                actor:"管理员".into(),node_id:node,category,target:target_text,
                result:"已结束".into(),detail:String::new(),
                source:peer.address.ip().to_string(),bytes:received,duration:now()-started,
            }).await;
        }
    }))
}

/// 会话的审计分类与目标描述。目标里**不写内容**，只写被操作的对象。
fn session_audit(kind: &StreamKind, target: &StreamTarget) -> (String, String) {
    match (kind, target) {
        (StreamKind::HostShell, StreamTarget::Host { workdir, .. }) => (
            "会话".into(),
            format!("宿主机终端（起始目录 {}）", workdir.as_deref().unwrap_or("/")),
        ),
        (StreamKind::FileRead, StreamTarget::File { path, .. }) => {
            ("文件".into(), format!("读取 {path}"))
        }
        (StreamKind::FileWrite, StreamTarget::File { path, .. }) => {
            ("文件".into(), format!("上传 {path}"))
        }
        _ => ("会话".into(), "流会话".into()),
    }
}
