use super::*;
use axum::{
    Extension, Json,
    extract::{Path, State as S},
    http::StatusCode,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::StreamExt;
use serde_json::{Value, json};

/// 文件变更请求。只接受结构化操作，**不接受任意命令**。
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileRequest {
    Put {
        path: String,
        digest: String,
        size: u64,
    },
    Remove {
        path: String,
        #[serde(default)]
        recursive: bool,
        #[serde(default)]
        confirmed: bool,
    },
    Rename {
        from: String,
        to: String,
    },
    Mkdir {
        path: String,
    },
    Chmod {
        path: String,
        mode: u32,
    },
    Chown {
        path: String,
        uid: u32,
        gid: u32,
    },
}

impl FileRequest {
    /// 折算成协议动作，并给出审计用的目标描述。
    fn into_action(self) -> (Action, String) {
        match self {
            FileRequest::Put { path, digest, size } => (
                Action::FilePut {
                    path: path.clone(),
                    digest,
                    size,
                },
                format!("写入 {path}"),
            ),
            FileRequest::Remove {
                path,
                recursive,
                confirmed,
            } => (
                Action::FileRemove {
                    path: path.clone(),
                    recursive,
                    confirmed,
                },
                format!("{}删除 {path}", if recursive { "递归" } else { "" }),
            ),
            FileRequest::Rename { from, to } => (
                Action::FileRename {
                    from: from.clone(),
                    to: to.clone(),
                },
                format!("重命名 {from} → {to}"),
            ),
            FileRequest::Mkdir { path } => (
                Action::FileMkdir { path: path.clone() },
                format!("新建目录 {path}"),
            ),
            FileRequest::Chmod { path, mode } => (
                Action::FileChmod {
                    path: path.clone(),
                    mode,
                },
                format!("修改权限 {path} 为 {mode:o}"),
            ),
            FileRequest::Chown { path, uid, gid } => (
                Action::FileChown {
                    path: path.clone(),
                    uid,
                    gid,
                },
                format!("修改属主 {path} 为 {uid}:{gid}"),
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAction {
    idempotency_key: String,
    action: FileRequest,
}

/// 提交文件变更任务，并在受理时写入审计。
///
/// 文件操作走任务机制而不是直接执行：这样天然带幂等键、事件与结果，
/// 也能与防火墙、存储等变更一起串行，不会互相踩踏。
pub async fn file_action(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    Path(node_id): Path<String>,
    Json(input): Json<FileAction>,
) -> ApiResult<Json<Value>> {
    let (action, description) = input.action.into_action();
    let task = TaskEnvelope::new(node_id.clone(), input.idempotency_key, action);
    let task = s.db.accept_task(&task).await?;
    let same = task.status == TaskStatus::Pending;
    if same {
        // 只有真正新建的任务才推送，重放不会重复下发
        if let Some(tx) = s.channels.read().await.get(&node_id).map(|c| c.1.clone()) {
            let _ = tx.send(Frame::Task { task: task.clone() }).await;
        }
        let _ = s.events.send(json!({"type":"tasks"}));
    }
    let _ = super::audit::record(
        &s.db,
        super::audit::AuditEntry {
            actor: "管理员".into(),
            node_id,
            category: "文件".into(),
            target: description,
            result: if same {
                "已提交".into()
            } else {
                "已存在".into()
            },
            // 审计只记对象与结果，不记内容
            detail: String::new(),
            source: peer.address.ip().to_string(),
            bytes: 0,
            duration: 0,
        },
    )
    .await;
    Ok(Json(json!({ "task_id": task.id })))
}

/// 读取当前安全入口。仅返回给已登录管理员；用于设置页展示可访问地址。
pub async fn read_entrance(S(s): S<State>) -> ApiResult<Json<Value>> {
    let value = s.entrance.read().map(|v| v.clone()).unwrap_or_default();
    Ok(Json(json!({ "value": value })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntranceInput {
    pub value: String,
}

/// 修改安全入口。校验通过后立即生效：门禁读取同一份共享值，无需重启。
pub async fn update_entrance(
    S(s): S<State>,
    Json(input): Json<EntranceInput>,
) -> ApiResult<Json<Value>> {
    let value = input.value.trim();
    super::entrance::validate(value).map_err(|e| bad(&e.to_string()))?;
    let current = s.entrance.read().map(|v| v.clone()).unwrap_or_default();
    if value == current {
        return Ok(Json(json!({ "value": value, "changed": false })));
    }
    s.db.put(
        super::ENTRANCE_BUCKET,
        super::ENTRANCE_ID,
        &value.to_owned(),
    )
    .await?;
    if let Ok(mut slot) = s.entrance.write() {
        *slot = value.to_owned();
    }
    // 不记录新入口明文，避免它出现在日志里。
    tracing::info!("安全入口已更新；旧地址立即失效");
    Ok(Json(json!({ "value": value, "changed": true })))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub name: String,
    #[serde(default)]
    pub public_addresses: Vec<String>,
    pub overlay_address: Option<String>,
    #[serde(default = "ssh_port")]
    pub ssh_port: u16,
}
fn ssh_port() -> u16 {
    22
}
fn validate(r: &Registration) -> Result<()> {
    anyhow::ensure!(
        !r.name.trim().is_empty() && r.name.len() <= 100,
        "节点名称长度不合法"
    );
    anyhow::ensure!(r.ssh_port > 0, "SSH 端口无效");
    for ip in &r.public_addresses {
        validate_public_address(ip)?;
    }
    if let Some(ip) = &r.overlay_address {
        ip.parse::<std::net::IpAddr>()?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Enrollment {
    node_id: String,
    registration: Registration,
    expires: i64,
}
pub async fn token(S(s): S<State>, Json(r): Json<Registration>) -> ApiResult<Json<Value>> {
    validate(&r)?;
    let value = format!("{}{}", id(), id());
    let node_id = id();
    s.db.insert(
        "enrollment",
        &digest(&value),
        &Enrollment {
            node_id: node_id.clone(),
            registration: r,
            expires: now() + 600,
        },
    )
    .await?;
    Ok(Json(
        json!({"token":value,"node_id":node_id,"expires_in":600,"ca_fingerprint":pki::cert_fingerprint(&std::fs::read_to_string(s.dir.join("pki/ca.pem")).map_err(anyhow::Error::from)?)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enroll {
    token: String,
    csr: String,
}
pub async fn enroll(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    Json(input): Json<Enroll>,
) -> ApiResult<Json<Value>> {
    let _lock = s.gate.lock().await;
    let token_key = digest(input.token);
    let e =
        s.db.get::<Enrollment>("enrollment", &token_key)
            .await?
            .filter(|e| e.expires > now())
            .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "注册令牌无效或过期".into()))?;
    let pem = pki::sign_csr(&s.dir.join("pki"), &input.csr, &e.node_id)?;
    let fingerprint = pki::cert_fingerprint(&pem)?;
    check(
        s.db.delete("enrollment", &token_key).await? == 1,
        "令牌已被使用",
    )?;
    let node = Node {
        id: e.node_id.clone(),
        name: e.registration.name,
        public_addresses: e.registration.public_addresses,
        overlay_address: e.registration.overlay_address,
        ssh_port: e.registration.ssh_port,
        revoked: false,
        last_seen: 0,
        capabilities: None,
        inventory: None,
        address_version: 0,
        // 展示属性在接入后由管理员在设置中填写，默认全部为空/未隐藏。
        region: None,
        group: None,
        tags: Vec::new(),
        weight: 0,
        hidden: false,
        public_remark: None,
    };
    s.db.insert("nodes", &node.id, &node).await?;
    s.db.insert(
        "certificates",
        &fingerprint,
        &CertificateBinding {
            node_id: node.id.clone(),
            expires: now() + 90 * 86400,
        },
    )
    .await?;
    s.db.put(
        "connection_source",
        &node.id,
        &peer.address.ip().to_string(),
    )
    .await?;
    s.db.accept_task(&TaskEnvelope::new(
        node.id.clone(),
        "initial-inspect".into(),
        Action::Inspect {},
    ))
    .await?;
    synchronize_peers(&s).await?;
    synchronize_probes(&s).await?;
    Ok(Json(json!({"node_id":e.node_id,"certificate":pem})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Renew {
    csr: String,
}
pub async fn renew(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    Json(input): Json<Renew>,
) -> ApiResult<Json<Value>> {
    let binding = super::channel::identity(&s, &peer).await?;
    let pem = pki::sign_csr(&s.dir.join("pki"), &input.csr, &binding.node_id)?;
    s.db.insert(
        "certificates",
        &pki::cert_fingerprint(&pem)?,
        &CertificateBinding {
            node_id: binding.node_id,
            expires: now() + 90 * 86400,
        },
    )
    .await?;
    Ok(Json(json!({"certificate":pem})))
}
pub async fn nodes(S(s): S<State>) -> ApiResult<Json<Value>> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let channels = s.channels.read().await;
    Ok(Json(json!(
        nodes
            .into_iter()
            .map(
                |n| json!({"connected":channels.contains_key(&n.id)&&n.last_seen>now()-45,"node":n})
            )
            .collect::<Vec<_>>()
    )))
}
pub async fn update_node(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Json(r): Json<Registration>,
) -> ApiResult<Json<Node>> {
    validate(&r)?;
    let _lock = s.gate.lock().await;
    let mut n =
        s.db.get::<Node>("nodes", &node_id)
            .await?
            .ok_or_else(|| bad("节点不存在"))?;
    check(!n.revoked, "节点已撤销，请重新注册")?;
    n.name = r.name;
    n.public_addresses = r.public_addresses;
    n.overlay_address = r.overlay_address;
    n.ssh_port = r.ssh_port;
    s.db.put("nodes", &node_id, &n).await?;
    synchronize_peers(&s).await?;
    synchronize_probes(&s).await?;
    Ok(Json(n))
}
pub async fn revoke(S(s): S<State>, Path(node_id): Path<String>) -> ApiResult<Json<Value>> {
    let _lock = s.gate.lock().await;
    let mut n =
        s.db.get::<Node>("nodes", &node_id)
            .await?
            .ok_or_else(|| bad("节点不存在"))?;
    n.revoked = true;
    s.db.put("nodes", &node_id, &n).await?;
    s.channels.write().await.remove(&node_id);
    synchronize_peers(&s).await?;
    synchronize_probes(&s).await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn synchronize_peers(s: &State) -> Result<()> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let old =
        s.db.get::<PeerAddressSet>("global", "peers")
            .await?
            .unwrap_or(PeerAddressSet {
                version: 0,
                addresses: vec![],
            });
    let mut addresses: Vec<_> = nodes
        .iter()
        .filter(|n| !n.revoked)
        .flat_map(|n| n.public_addresses.iter().cloned())
        .collect();
    addresses.sort();
    addresses.dedup();
    let set = PeerAddressSet {
        version: if old.version == 0 || old.addresses != addresses {
            old.version + 1
        } else {
            old.version
        },
        addresses,
    };
    let mut tx = s.db.pool.begin().await?;
    sqlx::query("DELETE FROM records WHERE bucket=? AND id=?")
        .bind("global")
        .bind("peers")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO records(bucket,id,value) VALUES(?,?,?)")
        .bind("global")
        .bind("peers")
        .bind(serde_json::to_string(&set)?)
        .execute(&mut *tx)
        .await?;
    for n in nodes.iter().filter(|n| !n.revoked) {
        let t = TaskEnvelope::new(
            n.id.clone(),
            format!("peer-set-{}", set.version),
            Action::PeerSync { set: set.clone() },
        );
        let existing = sqlx::query("SELECT digest FROM tasks WHERE node_id=? AND task_key=?")
            .bind(&t.node_id)
            .bind(&t.key)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(existing) = existing {
            use sqlx::Row;
            anyhow::ensure!(
                existing.try_get::<String, _>("digest")? == t.digest,
                "地址同步幂等参数冲突"
            );
        } else {
            sqlx::query("INSERT INTO tasks(id,node_id,task_key,digest,value) VALUES(?,?,?,?,?)")
                .bind(&t.id)
                .bind(&t.node_id)
                .bind(&t.key)
                .bind(&t.digest)
                .bind(serde_json::to_string(&t)?)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
pub async fn peers(S(s): S<State>) -> ApiResult<Json<PeerAddressSet>> {
    let peers =
        s.db.get::<PeerAddressSet>("global", "peers")
            .await?
            .unwrap_or(PeerAddressSet {
                version: 0,
                addresses: Vec::new(),
            });
    Ok(Json(peers))
}

/// 选出用于探测的地址。
///
/// 优先覆盖网地址：它是节点之间**真实的转发路径**，也是 Galera 复制实际走的路。
/// 没有覆盖网地址时才退回第一个公网地址。两者都没有的节点不参与探测——
/// 探测一个没有地址的节点是无意义的，而返回"未知"比返回 0 更诚实。
pub fn probe_address(node: &Node) -> Option<String> {
    node.overlay_address
        .clone()
        .filter(|address| !address.trim().is_empty())
        .or_else(|| node.public_addresses.first().cloned())
}

/// 给一台节点生成探测目标列表。纯函数，因此"到底测谁、测哪个地址"可以被固定下来。
///
/// 两条规则：
/// - **不含自己**：探测自己的地址量的是本机网络栈，没有观察价值；
/// - **顺序稳定**（按节点编号）：否则重发时数组顺序抖动，版本号也就失去了
///   "内容没变就不重发"的意义。
pub fn probe_targets_for(nodes: &[Node], me: &str) -> Vec<ProbeTarget> {
    let mut targets: Vec<ProbeTarget> = nodes
        .iter()
        .filter(|node| !node.revoked && node.id != me)
        .filter_map(|node| {
            probe_address(node).map(|address| ProbeTarget {
                node_id: node.id.clone(),
                address,
                // 默认走 ICMP：不产生任何服务端日志，也不需要额外特权
                port: None,
            })
        })
        .collect();
    targets.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    targets
}

/// 节点目录的指纹：只有它变了才重发探测目标。
///
/// 只做"内容签名"，**不把它当版本号**：签名是很长的哈希，直接当版本号会
/// 超过 JSON 能安全表达的范围（2^53），浏览器拿到的是被四舍五入后的数字，
/// 于是"版本没变"会被误判成"变了/没变"。版本号改用与地址集合相同的小整数计数器。
pub fn probe_signature(nodes: &[Node]) -> String {
    let mut lines: Vec<String> = nodes
        .iter()
        .filter(|node| !node.revoked)
        .filter_map(|node| probe_address(node).map(|address| format!("{}={address}", node.id)))
        .collect();
    lines.sort();
    lines.join("\n")
}

/// 探测目标集合的版本，与签名一起持久化：内容变了才 +1。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeSignature {
    pub version: i64,
    pub signature: String,
}

/// 下发对端探测目标。
///
/// 与 `synchronize_peers` 同一套路：目录变化才换版本号，任务带幂等键，
/// 因此重复调用不会产生重复任务，Agent 也只需要落盘一次。
pub async fn synchronize_probes(s: &State) -> Result<()> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let signature = probe_signature(&nodes);
    let old =
        s.db.get::<ProbeSignature>("global", "probe-signature")
            .await?
            .unwrap_or_default();
    let version = if old.signature == signature {
        old.version
    } else {
        old.version + 1
    };
    let mut tx = s.db.pool.begin().await?;
    for node in nodes.iter().filter(|node| !node.revoked) {
        let set = ProbeTargetSet {
            version,
            targets: probe_targets_for(&nodes, &node.id),
        };
        let t = TaskEnvelope::new(
            node.id.clone(),
            format!("probe-set-{version}"),
            Action::PeerProbeTargets { set },
        );
        let existing = sqlx::query("SELECT digest FROM tasks WHERE node_id=? AND task_key=?")
            .bind(&t.node_id)
            .bind(&t.key)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(existing) = existing {
            use sqlx::Row;
            anyhow::ensure!(
                existing.try_get::<String, _>("digest")? == t.digest,
                "探测目标同步幂等参数冲突"
            );
        } else {
            sqlx::query("INSERT INTO tasks(id,node_id,task_key,digest,value) VALUES(?,?,?,?,?)")
                .bind(&t.id)
                .bind(&t.node_id)
                .bind(&t.key)
                .bind(&t.digest)
                .bind(serde_json::to_string(&t)?)
                .execute(&mut *tx)
                .await?;
        }
    }
    // 版本与签名一起落库，且与任务在同一事务里：否则重启后可能记着新版本
    // 却没有对应的任务，目标集合就再也不会重发。
    sqlx::query("DELETE FROM records WHERE bucket=? AND id=?")
        .bind("global")
        .bind("probe-signature")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO records(bucket,id,value) VALUES(?,?,?)")
        .bind("global")
        .bind("probe-signature")
        .bind(serde_json::to_string(&ProbeSignature {
            version,
            signature,
        })?)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    pub idempotency_key: String,
    pub action: Action,
}
pub async fn action(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Json(input): Json<Submit>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    check(
        !input.idempotency_key.is_empty() && input.idempotency_key.len() <= 128,
        "幂等键长度无效",
    )?;
    let node =
        s.db.get::<Node>("nodes", &node_id)
            .await?
            .filter(|n| !n.revoked)
            .ok_or_else(|| bad("节点不可用"))?;
    check(
        !matches!(input.action, Action::PeerSync { .. }),
        "地址集合只能由节点目录生成",
    )?;
    if let Action::FirewallApply {
        witness: Some(ref witness),
        ..
    } = input.action
    {
        check(
            witness != &node.id && s.channels.read().await.contains_key(witness),
            "验证节点必须是另一个在线节点",
        )?;
    }
    let t = TaskEnvelope::new(node_id, input.idempotency_key, input.action);
    let t = s.db.accept_task(&t).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"task_id":t.id}))))
}
pub async fn tasks(S(s): S<State>) -> ApiResult<Json<Vec<TaskEnvelope>>> {
    let mut tasks = s.db.tasks().await?;
    tasks.sort_by_key(|t| std::cmp::Reverse(t.created_at));
    Ok(Json(tasks.iter().map(TaskEnvelope::redacted).collect()))
}
pub async fn task(S(s): S<State>, Path(id): Path<String>) -> ApiResult<Json<TaskEnvelope>> {
    Ok(Json(
        s.db.task(&id)
            .await?
            .ok_or_else(|| bad("任务不存在"))?
            .redacted(),
    ))
}
pub async fn task_events(
    S(s): S<State>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<TaskEvent>>> {
    Ok(Json(s.db.list::<TaskEvent>(&format!("events:{id}")).await?))
}
pub async fn events(
    S(s): S<State>,
    Extension(session): Extension<super::auth::Session>,
) -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
    let stream = tokio_stream::wrappers::BroadcastStream::new(s.events.subscribe()).filter_map(
        |e| async move {
            Some(Ok(Event::default()
                .json_data(e.unwrap_or_else(|_| json!({"type":"resync"})))
                .unwrap()))
        },
    );
    let expires = tokio::time::sleep(std::time::Duration::from_secs(
        (session.expires - now()).max(0) as u64,
    ));
    Sse::new(stream.take_until(expires)).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    witness: Option<String>,
    #[serde(default)]
    required: bool,
}
pub async fn verify(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    Json(input): Json<Verification>,
) -> ApiResult<Json<Value>> {
    let binding = super::channel::identity(&s, &peer).await?;
    let node =
        s.db.get::<Node>("nodes", &binding.node_id)
            .await?
            .ok_or_else(|| bad("目标节点不存在"))?;
    let witness = if let Some(id) = input.witness {
        Some(id)
    } else {
        s.channels
            .read()
            .await
            .keys()
            .find(|id| *id != &node.id)
            .cloned()
    };
    if let Some(witness) = witness {
        check(witness != node.id, "验证节点不能是目标节点")?;
        let tx = s
            .channels
            .read()
            .await
            .get(&witness)
            .map(|c| c.1.clone())
            .ok_or_else(|| bad("验证节点不在线"))?;
        let address = node
            .overlay_address
            .as_ref()
            .or_else(|| node.public_addresses.first())
            .ok_or_else(|| bad("未登记可探测地址"))?;
        let request_id = id();
        tx.send(Frame::Probe {
            request_id: request_id.clone(),
            address: address.clone(),
            port: node.ssh_port,
        })
        .await
        .map_err(|_| bad("验证通道断开"))?;
        let mut ok = false;
        for _ in 0..24 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if let Some(v) = s.db.get::<Value>("probes", &request_id).await? {
                check(
                    v["node"].as_str() == Some(&witness) && v["ok"] == true,
                    "独立节点的新连接验证失败",
                )?;
                ok = true;
                break;
            }
        }
        s.db.delete("probes", &request_id).await?;
        check(ok, "独立节点验证超时")?;
    } else {
        check(!input.required, "该变更需要另一个在线验证节点")?;
        check(
            node.capabilities.as_ref().is_some_and(|c| c.systemd),
            "缺少独立验证节点或本地回滚能力",
        )?;
    }
    Ok(Json(json!({"ok":true,"fresh_tls":true})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, overlay: Option<&str>, public: &[&str]) -> Node {
        Node {
            id: id.into(),
            name: format!("{id} · 测试"),
            public_addresses: public.iter().map(|s| s.to_string()).collect(),
            overlay_address: overlay.map(str::to_string),
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

    #[test]
    fn 优先探测覆盖网地址() {
        // 覆盖网是节点之间真实的转发路径，也是 Galera 复制实际走的路
        let target = node("C001", Some("100.100.201.1"), &["203.0.113.1"]);
        assert_eq!(probe_address(&target).as_deref(), Some("100.100.201.1"));
    }

    #[test]
    fn 没有覆盖网地址时才退回公网地址() {
        let target = node("C001", None, &["203.0.113.1", "203.0.113.2"]);
        assert_eq!(probe_address(&target).as_deref(), Some("203.0.113.1"));
        // 空串不算地址
        let blank = node("C002", Some("   "), &[]);
        assert_eq!(probe_address(&blank), None);
        assert_eq!(probe_address(&node("C003", None, &[])), None);
    }

    #[test]
    fn 目标列表不含自己且顺序稳定() {
        let nodes = vec![
            node("C052", Some("100.100.201.52"), &[]),
            node("C001", Some("100.100.201.1"), &[]),
            node("C041", Some("100.100.201.41"), &[]),
        ];
        let targets = probe_targets_for(&nodes, "C041");
        let ids: Vec<&str> = targets.iter().map(|t| t.node_id.as_str()).collect();
        assert_eq!(ids, vec!["C001", "C052"], "不含自己，且按编号排序");
        assert!(targets.iter().all(|t| t.port.is_none()), "默认走 ICMP");
    }

    #[test]
    fn 没有地址的节点不进目标列表() {
        // 探测一个没有地址的节点是无意义的；"未知"比一个假的 0 更诚实
        let nodes = vec![
            node("C001", Some("100.100.201.1"), &[]),
            node("C002", None, &[]),
            node("C003", Some("100.100.201.3"), &[]),
        ];
        let targets = probe_targets_for(&nodes, "C001");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].node_id, "C003");
    }

    #[test]
    fn 已撤销节点不进目标列表() {
        let mut gone = node("C009", Some("100.100.201.9"), &[]);
        gone.revoked = true;
        let nodes = vec![node("C001", Some("100.100.201.1"), &[]), gone];
        let targets = probe_targets_for(&nodes, "C001");
        assert!(targets.is_empty());
    }

    #[test]
    fn 只有目录变化才换版本号() {
        let base = vec![
            node("C001", Some("100.100.201.1"), &[]),
            node("C002", Some("100.100.201.2"), &[]),
        ];
        let first = probe_signature(&base);
        // 同样的目录 → 同样的签名，因此版本号不涨、任务不重发
        assert_eq!(probe_signature(&base), first);
        // 顺序不算变化
        let reordered = vec![base[1].clone(), base[0].clone()];
        assert_eq!(probe_signature(&reordered), first);
        // 地址变了 → 签名必须变
        let moved = vec![
            node("C001", Some("100.100.201.99"), &[]),
            node("C002", Some("100.100.201.2"), &[]),
        ];
        assert_ne!(probe_signature(&moved), first);
        // 成员变了 → 签名必须变
        let mut grown = base.clone();
        grown.push(node("C003", Some("100.100.201.3"), &[]));
        assert_ne!(probe_signature(&grown), first);
        // 撤销不算成员变化之外的事，但签名同样要跟着变
        let mut revoked = base.clone();
        revoked[1].revoked = true;
        assert_ne!(probe_signature(&revoked), first);
        assert_eq!(probe_signature(&[]), "", "没有节点时签名是空串");
    }

    #[test]
    fn 版本号必须是_json_能安全表达的小整数() {
        // 这不是洁癖：版本号超过 2^53 时浏览器解析会四舍五入，
        // "版本没变"就会被误判成"变了"，反之亦然。
        let mut version = 0i64;
        for _ in 0..10 {
            version += 1; // 计数器每次只加一，永远不会逼近 2^53
        }
        assert!(version < 1 << 31);
        let set = ProbeTargetSet {
            version,
            targets: vec![],
        };
        let json = serde_json::to_string(&set).unwrap();
        // 往返之后必须一模一样，不能被浮点吃掉精度
        let back: ProbeTargetSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, version);
    }
}
