//! 存储集群：定义、计划与执行。
//!
//! # 这个模块里最重要的一件事
//!
//! 格式化磁盘是**全系统唯一会不可逆销毁数据的操作**。因此这里的结构围绕一条原则组织：
//!
//! > **计划与执行分离，且执行前必须在节点上重新核对现实。**
//!
//! 具体做法：
//!
//! 1. 计划由预检结论生成，**精确列出将被清空的每一块设备**及其计划时的容量；
//! 2. 计划带指纹与 5 分钟有效期，过期或指纹变化都必须重新生成；
//! 3. 计划里含破坏性步骤时，执行请求必须显式声明 `acknowledge_destructive`；
//! 4. Agent 在执行前**重新读取设备实际状态**：容量不符、或设备不再是「无任何使用痕迹」
//!    的状态，一律拒绝——计划与执行之间设备被换掉是最危险的场景；
//! 5. 逐节点串行执行，任一节点失败即阻断后续节点。
//!
//! 计划生成与校验是纯函数，因此可以被穷举测试；真正落盘的动作全部在 Agent 侧。
use opsd::protocol::{
    Action, HostInventory, Node, StorageDeviceTarget, digest, id, now,
};
use anyhow::Result;
use opsd::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 计划在控制库中的存放位置。
pub const BUCKET: &str = "storage_plans";
/// 集群定义的存放位置。
pub const CLUSTER_BUCKET: &str = "storage_clusters";
/// 计划有效期（秒）。与防火墙保持一致。
pub const PLAN_TTL: i64 = 300;
/// 分布式对象存储允许的最小节点数。
pub const MIN_NODES: usize = 4;
/// 固定文件系统：XFS 是对象存储的通行选择。
pub const FILESYSTEM: &str = "xfs";
/// 数据目录前缀。
pub const DATA_ROOT: &str = "/data";
/// 允许的镜像仓库前缀与标签要求。
pub const IMAGE_PREFIX: &str = "pgsty/silo";

/// 集群定义。由管理员在预检结论基础上指定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSpec {
    pub name: String,
    /// 参与集群的节点编号。
    pub nodes: Vec<String>,
    /// 固定 release 标签，例如 `RELEASE.2026-09-03T13-18-01Z`。
    pub image_tag: String,
    /// 对外服务地址，用于生成分布式拓扑。
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    /// 由服务端填写，因此客户端可以不传：它只是"这个定义什么时候建的"。
    #[serde(default)]
    pub created_at: i64,
}

/// 计划中的单个节点步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedNode {
    pub node_id: String,
    pub name: String,
    /// 即将被清空的设备，含计划时的容量。
    pub devices: Vec<StorageDeviceTarget>,
    /// 该节点执行完成后对外提供的挂载点。
    pub mounts: Vec<String>,
}

/// 存储计划。**它同时是"将要发生什么"的说明和执行的凭据。**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub cluster: String,
    pub created_at: i64,
    pub expires: i64,
    /// 计划生成时依据的预检指纹；现实变化后指纹不再匹配，计划作废。
    pub fingerprint: String,
    pub nodes: Vec<PlannedNode>,
    pub image: String,
    /// 是否包含不可逆步骤。含格式化时为 true。
    pub destructive: bool,
    /// 不可逆步骤的明确告知，界面必须原文展示。
    pub irreversible_notice: String,
    /// 执行进度：已完成的节点。
    #[serde(default)]
    pub completed: Vec<String>,
    /// 执行失败的节点与原因。非空即视为计划失败，后续节点不再执行。
    #[serde(default)]
    pub failed: Vec<(String, String)>,
}

/// 生成计划时可能出现的阻断。
pub fn validate_spec(spec: &ClusterSpec) -> Result<()> {
    anyhow::ensure!(
        !spec.name.trim().is_empty() && spec.name.len() <= 40,
        "集群名称长度不合法"
    );
    anyhow::ensure!(
        spec.name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "集群名称只能包含字母、数字、连字符与下划线"
    );
    anyhow::ensure!(
        spec.nodes.len() >= MIN_NODES,
        "分布式部署至少需要 {MIN_NODES} 个节点，当前 {} 个",
        spec.nodes.len()
    );
    // 同一节点重复出现会让拓扑算错
    let mut unique = spec.nodes.clone();
    unique.sort();
    unique.dedup();
    anyhow::ensure!(unique.len() == spec.nodes.len(), "节点列表存在重复");
    // 固定 release 标签：latest 无法复现，也无法回滚到确定版本
    anyhow::ensure!(
        !spec.image_tag.is_empty() && spec.image_tag.len() <= 80,
        "镜像标签不合法"
    );
    anyhow::ensure!(
        !spec.image_tag.eq_ignore_ascii_case("latest"),
        "不接受 latest 标签；请固定到具体的 release，便于复现与回滚"
    );
    anyhow::ensure!(
        spec.image_tag.starts_with("RELEASE.")
            || spec.image_tag.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "镜像标签应形如 RELEASE.<时间戳> 或具体版本号"
    );
    anyhow::ensure!(
        !spec.endpoint.trim().is_empty(),
        "必须给出集群对外服务地址"
    );
    anyhow::ensure!(
        spec.access_key.len() >= 3 && !spec.secret_key.is_empty() && spec.secret_key.len() >= 16,
        "访问凭据不合法：密钥至少 16 个字符"
    );
    Ok(())
}

/// 预检指纹：把候选节点与它们的候选盘固化下来。
///
/// 计划生成后只要候选集合发生变化（节点离线被移除、磁盘被占用），指纹就不再匹配，
/// 必须重新生成计划。
pub fn fingerprint(nodes: &[PlannedNode]) -> String {
    let mut parts: Vec<String> = nodes
        .iter()
        .map(|node| {
            let devices: Vec<String> = node
                .devices
                .iter()
                .map(|d| format!("{}:{}", d.path, d.expected_size))
                .collect();
            format!("{}[{}]", node.node_id, devices.join(","))
        })
        .collect();
    parts.sort();
    digest(parts.join("|"))
}

/// 依据预检结论生成计划。
///
/// 这里**只接受预检判定为可用的磁盘**：任何一处不符合就整体拒绝，不做"尽力而为"的裁剪，
/// 否则管理员看到的设备清单与实际被清空的设备可能不一致。
pub fn build_plan(
    spec: &ClusterSpec,
    nodes: &HashMap<String, Node>,
    inventories: &HashMap<String, HostInventory>,
    online: &std::collections::HashSet<String>,
) -> Result<Plan> {
    validate_spec(spec)?;
    let mut planned = Vec::new();
    for node_id in &spec.nodes {
        let node = nodes
            .get(node_id)
            .ok_or_else(|| anyhow::anyhow!("节点不存在：{node_id}"))?;
        anyhow::ensure!(!node.revoked, "节点已撤销：{node_id}");
        anyhow::ensure!(online.contains(node_id), "节点离线，不能纳入计划：{node_id}");
        let inventory = inventories
            .get(node_id)
            .ok_or_else(|| anyhow::anyhow!("节点尚未盘点，不能纳入计划：{node_id}"))?;
        let mut devices = Vec::new();
        let mut mounts = Vec::new();
        for (index, device) in inventory
            .devices
            .iter()
            .filter(|d| d.kind == "disk")
            .enumerate()
        {
            let Some(risk) = inventory.risks.iter().find(|r| r.device == device.name) else {
                continue;
            };
            if !risk.usable() || device.size == 0 {
                continue;
            }
            let mount = format!("{DATA_ROOT}/disk{}", index + 1);
            mounts.push(mount.clone());
            devices.push(StorageDeviceTarget {
                path: device.path.clone(),
                expected_size: device.size,
                mount,
            });
        }
        anyhow::ensure!(
            !devices.is_empty(),
            "节点没有可用磁盘，不能纳入计划：{}",
            node.name
        );
        planned.push(PlannedNode {
            node_id: node.id.clone(),
            name: node.name.clone(),
            devices,
            mounts,
        });
    }

    // 每节点磁盘数量一致：不一致时纠删码会按最小集合工作，多出来的盘等于浪费
    let counts: Vec<usize> = planned.iter().map(|n| n.devices.len()).collect();
    anyhow::ensure!(
        counts.windows(2).all(|w| w[0] == w[1]),
        "各节点可用磁盘数量不一致（{counts:?}），请先补齐再部署"
    );

    let fingerprint = fingerprint(&planned);
    Ok(Plan {
        id: id(),
        cluster: spec.name.clone(),
        created_at: now(),
        expires: now() + PLAN_TTL,
        fingerprint,
        nodes: planned,
        image: format!("{IMAGE_PREFIX}:{}", spec.image_tag),
        destructive: true,
        irreversible_notice: "即将清空下列磁盘上的全部数据。格式化不可回滚，\
残留的文件系统、分区表与数据都会被抹除；请在执行前逐项核对设备路径与容量。"
            .into(),
        completed: Vec::new(),
        failed: Vec::new(),
    })
}

/// 执行前的校验。任何一项不通过都必须拒绝执行。
pub fn check_applicable(
    plan: &Plan,
    acknowledge_destructive: bool,
    online: &std::collections::HashSet<String>,
    inventories: &HashMap<String, HostInventory>,
) -> Result<()> {
    anyhow::ensure!(plan.expires > now(), "计划已过期，请重新生成");
    if plan.destructive {
        anyhow::ensure!(
            acknowledge_destructive,
            "该计划包含不可逆的磁盘格式化，必须显式确认后才能执行"
        );
    }
    anyhow::ensure!(plan.failed.is_empty(), "计划已有失败节点，不能继续执行");
    // 现实仍然一致：节点在线且候选集合未变
    for node in &plan.nodes {
        anyhow::ensure!(
            online.contains(&node.node_id),
            "节点 {} 已离线，计划作废",
            node.name
        );
        let inventory = inventories
            .get(&node.node_id)
            .ok_or_else(|| anyhow::anyhow!("节点 {} 的盘点结果已丢失，计划作废", node.name))?;
        for device in &node.devices {
            let current = inventory
                .devices
                .iter()
                .find(|d| d.path == device.path)
                .ok_or_else(|| {
                    anyhow::anyhow!("设备 {} 在 {} 上已不存在，计划作废", device.path, node.name)
                })?;
            anyhow::ensure!(
                current.size == device.expected_size,
                "设备 {} 容量已变化（计划 {} 字节，实际 {} 字节），计划作废",
                device.path,
                device.expected_size,
                current.size
            );
            let risk = inventory.risks.iter().find(|r| r.device == current.name);
            anyhow::ensure!(
                risk.is_some_and(|r| r.usable()),
                "设备 {} 已不再是无使用痕迹的状态，计划作废",
                device.path
            );
        }
    }
    // 参与计划的节点集合必须与指纹一致
    anyhow::ensure!(
        fingerprint(&plan.nodes) == plan.fingerprint,
        "计划的设备清单已被修改，请重新生成"
    );
    Ok(())
}

/// 为单个节点生成执行动作。
pub fn prepare_action(plan: &Plan, node_id: &str) -> Option<Action> {
    let node = plan.nodes.iter().find(|n| n.node_id == node_id)?;
    Some(Action::StoragePrepare {
        cluster: plan.cluster.clone(),
        plan_id: plan.id.clone(),
        devices: node.devices.clone(),
        filesystem: FILESYSTEM.into(),
    })
}

/// 为单个节点生成部署动作。
pub fn deploy_action(spec: &ClusterSpec, plan: &Plan, node_id: &str) -> Option<Action> {
    let node = plan.nodes.iter().find(|n| n.node_id == node_id)?;
    // 所有节点都要知道彼此，才能组成一个集群。
    // 挂载点本身以 / 开头，因此要把服务地址结尾的 / 去掉，否则会拼出双斜杠。
    let endpoint = spec.endpoint.trim_end_matches('/');
    let peers: Vec<String> = plan
        .nodes
        .iter()
        .flat_map(|n| n.devices.iter().map(|d| format!("{endpoint}{}", d.mount)))
        .collect();
    Some(Action::StorageDeploy {
        cluster: plan.cluster.clone(),
        plan_id: plan.id.clone(),
        image: plan.image.clone(),
        mounts: node.mounts.clone(),
        peers,
        access_key: spec.access_key.clone(),
        secret_key: spec.secret_key.clone(),
    })
}

pub async fn save(db: &Store, plan: &Plan) -> Result<()> {
    db.put(BUCKET, &plan.id, plan).await
}
pub async fn load(db: &Store, id: &str) -> Result<Option<Plan>> {
    db.get(BUCKET, id).await
}
pub async fn save_spec(db: &Store, spec: &ClusterSpec) -> Result<()> {
    db.put(CLUSTER_BUCKET, &spec.name, spec).await
}
pub async fn load_spec(db: &Store, name: &str) -> Result<Option<ClusterSpec>> {
    db.get(CLUSTER_BUCKET, name).await
}
pub async fn list_clusters(db: &Store) -> Result<Vec<ClusterSpec>> {
    db.list(CLUSTER_BUCKET).await
}

/* ------------------------------------------------------------------ *
 * 处理函数
 * ------------------------------------------------------------------ */

use super::*;
use axum::{
    Extension, Json,
    extract::{Path, State as S},
};

fn readwrite() -> String {
    "readwrite".into()
}

/// 列出已定义的集群。
///
/// **密钥不回传**：S3 密钥只在服务端生成 Compose 时用到，界面从来不需要读它，
/// 因此列表里一律抹掉，只说明"已经设置过"。
pub async fn clusters(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let clusters: Vec<serde_json::Value> = list_clusters(&s.db)
        .await?
        .into_iter()
        .map(|spec| {
            let has_secret = !spec.secret_key.is_empty();
            let mut value = serde_json::to_value(spec).unwrap_or(serde_json::Value::Null);
            if let Some(map) = value.as_object_mut() {
                map.remove("secret_key");
                map.insert("has_secret_key".into(), serde_json::Value::Bool(has_secret));
            }
            value
        })
        .collect();
    Ok(Json(serde_json::json!({
        "clusters": clusters,
        // 允许的镜像仓库与文件系统，界面据此生成选择项而不是让管理员手输
        "image_prefix": IMAGE_PREFIX,
        "filesystem": FILESYSTEM,
        "min_nodes": MIN_NODES,
    })))
}

/// 保存集群定义。只写定义，不碰任何节点。
pub async fn put_cluster(
    S(s): S<State>,
    Json(mut spec): Json<ClusterSpec>,
) -> ApiResult<Json<serde_json::Value>> {
    match load_spec(&s.db, &spec.name).await? {
        Some(existing) => spec.created_at = existing.created_at,
        None => spec.created_at = now(),
    }
    let created = spec.created_at;
    validate_spec(&spec).map_err(|e| bad(&e.to_string()))?;
    let mut spec = spec;
    spec.created_at = created;
    save_spec(&s.db, &spec).await?;
    Ok(Json(serde_json::json!({ "cluster": spec })))
}

#[derive(Deserialize)]
pub struct PlanRequest {
    pub name: String,
}

/// 生成部署计划。
///
/// 计划里精确列出将被清空的设备与容量，并带指纹与有效期。
/// **这一步不做任何写操作**，只是把"将要发生什么"固化下来供人核对。
pub async fn plan(
    S(s): S<State>,
    Json(input): Json<PlanRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let spec = load_spec(&s.db, &input.name)
        .await?
        .ok_or_else(|| bad("集群定义不存在"))?;
    let nodes: std::collections::HashMap<String, Node> = s
        .db
        .list::<Node>("nodes")
        .await?
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect();
    let online: std::collections::HashSet<String> =
        s.channels.read().await.keys().cloned().collect();
    let inventories = s
        .host_inventories
        .read()
        .map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "盘点缓存不可用".into()))?
        .clone();
    let plan = build_plan(&spec, &nodes, &inventories, &online)
        .map_err(|e| bad(&e.to_string()))?;
    save(&s.db, &plan).await?;
    Ok(Json(serde_json::json!({ "plan": plan })))
}

pub async fn read_plan(
    S(s): S<State>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let plan = load(&s.db, &id)
        .await?
        .ok_or_else(|| bad("计划不存在"))?;
    Ok(Json(serde_json::json!({
        "plan": plan,
        "expired": plan.expires <= now(),
    })))
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub plan_id: String,
    /// 含破坏性步骤时必须显式确认为 true。
    #[serde(default)]
    pub acknowledge_destructive: bool,
}

/// 执行计划：逐节点串行，任一节点失败即阻断后续节点。
///
/// 顺序固定为「先准备磁盘，再部署容器」，因为部署依赖已挂载的数据盘。
pub async fn apply(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    Json(input): Json<ApplyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut plan = load(&s.db, &input.plan_id)
        .await?
        .ok_or_else(|| bad("计划不存在"))?;
    let spec = load_spec(&s.db, &plan.cluster)
        .await?
        .ok_or_else(|| bad("集群定义已不存在，请重新生成计划"))?;
    let online: std::collections::HashSet<String> =
        s.channels.read().await.keys().cloned().collect();
    let inventories = s
        .host_inventories
        .read()
        .map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "盘点缓存不可用".into()))?
        .clone();
    check_applicable(&plan, input.acknowledge_destructive, &online, &inventories)
        .map_err(|e| bad(&e.to_string()))?;

    let mut dispatched = Vec::new();
    for node in plan.nodes.clone() {
        // 先准备磁盘，成功后再部署容器；两步都走任务机制，结果可在任务面板追溯
        let steps = [
            ("磁盘准备", prepare_action(&plan, &node.node_id)),
            ("容器部署", deploy_action(&spec, &plan, &node.node_id)),
        ];
        let mut node_failed: Option<String> = None;
        for (label, action) in steps {
            let Some(action) = action else { continue };
            let task = TaskEnvelope::new(
                node.node_id.clone(),
                format!("{}-{}", plan.id, label),
                action,
            );
            match s.db.accept_task(&task).await {
                Ok(task) => {
                    if task.status == TaskStatus::Pending
                        && let Some(tx) =
                            s.channels.read().await.get(&node.node_id).map(|c| c.1.clone())
                    {
                        let _ = tx.send(Frame::Task { task: task.clone() }).await;
                    }
                    dispatched.push(serde_json::json!({
                        "node": node.name,
                        "step": label,
                        "task_id": task.id,
                    }));
                }
                Err(error) => {
                    node_failed = Some(format!("{label}：{error}"));
                    break;
                }
            }
        }
        match node_failed {
            Some(reason) => {
                // 失败即阻断：后续节点不再下发，避免出现"清了一半"的集群
                plan.failed.push((node.node_id.clone(), reason.clone()));
                save(&s.db, &plan).await?;
                let _ = audit::record(
                    &s.db,
                    audit::AuditEntry {
                        actor: "管理员".into(),
                        node_id: node.node_id.clone(),
                        category: "存储".into(),
                        target: format!("部署集群 {}", plan.cluster),
                        result: "失败".into(),
                        detail: format!("已阻断后续节点：{reason}"),
                        source: peer.address.ip().to_string(),
                        bytes: 0,
                        duration: 0,
                    },
                )
                .await;
                return Err(bad(&format!(
                    "节点 {} 执行失败，已阻断后续节点：{reason}",
                    node.name
                )));
            }
            None => plan.completed.push(node.node_id.clone()),
        }
    }
    save(&s.db, &plan).await?;
    // 磁盘格式化是不可逆的，必须留痕
    let _ = audit::record(
        &s.db,
        audit::AuditEntry {
            actor: "管理员".into(),
            node_id: plan.nodes.first().map(|n| n.node_id.clone()).unwrap_or_default(),
            category: "存储".into(),
            target: format!(
                "部署集群 {}：{} 个节点，共 {} 块盘",
                plan.cluster,
                plan.nodes.len(),
                plan.nodes.iter().map(|n| n.devices.len()).sum::<usize>()
            ),
            result: "已提交".into(),
            detail: "包含不可逆的磁盘格式化".into(),
            source: peer.address.ip().to_string(),
            bytes: 0,
            duration: 0,
        },
    )
    .await;
    Ok(Json(serde_json::json!({
        "plan_id": plan.id,
        "dispatched": dispatched,
        "completed": plan.completed,
    })))
}

#[derive(Deserialize)]
pub struct BucketRequest {
    pub cluster: String,
    pub bucket: String,
    #[serde(default)]
    pub remove: bool,
}

pub async fn bucket(
    S(s): S<State>,
    Json(input): Json<BucketRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let spec = load_spec(&s.db, &input.cluster)
        .await?
        .ok_or_else(|| bad("集群不存在"))?;
    dispatch_one(
        &s,
        &spec,
        Action::StorageBucket {
            cluster: input.cluster.clone(),
            bucket: input.bucket.clone(),
            remove: input.remove,
        },
        "桶",
    )
    .await
}

#[derive(Deserialize)]
pub struct UserRequest {
    pub cluster: String,
    pub user: String,
    pub secret: String,
    #[serde(default = "readwrite")]
    pub policy: String,
    #[serde(default)]
    pub remove: bool,
}

pub async fn user(
    S(s): S<State>,
    Json(input): Json<UserRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let spec = load_spec(&s.db, &input.cluster)
        .await?
        .ok_or_else(|| bad("集群不存在"))?;
    dispatch_one(
        &s,
        &spec,
        Action::StorageUser {
            cluster: input.cluster.clone(),
            user: input.user.clone(),
            secret: input.secret.clone(),
            policy: input.policy.clone(),
            remove: input.remove,
        },
        "用户",
    )
    .await
}

/// 把单节点动作下发给集群里第一个在线节点。
///
/// 桶与用户是集群级操作，在任一节点执行即可，不需要逐节点下发。
async fn dispatch_one(
    s: &State,
    spec: &ClusterSpec,
    action: Action,
    label: &str,
) -> ApiResult<Json<serde_json::Value>> {
    let online = s.channels.read().await;
    let target = spec
        .nodes
        .iter()
        .find(|id| online.contains_key(*id))
        .cloned()
        .ok_or_else(|| bad("集群没有在线节点"))?;
    let tx = online
        .get(&target)
        .map(|c| c.1.clone())
        .ok_or_else(|| bad("节点通道不可用"))?;
    drop(online);
    let task = TaskEnvelope::new(target.clone(), id(), action);
    let task = s.db.accept_task(&task).await?;
    let _ = tx.send(Frame::Task { task: task.clone() }).await;
    Ok(Json(serde_json::json!({
        "task_id": task.id,
        "node": target,
        "label": label,
    })))
}

/// 触发一次集群状态采集。
pub async fn status(
    S(s): S<State>,
    Json(input): Json<PlanRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let spec = load_spec(&s.db, &input.name)
        .await?
        .ok_or_else(|| bad("集群不存在"))?;
    dispatch_one(
        &s,
        &spec,
        Action::StorageStatus {
            cluster: spec.name.clone(),
        },
        "状态",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::{BlockDevice, DriveRisk};
    use std::collections::HashSet;

    fn spec(nodes: Vec<&str>) -> ClusterSpec {
        ClusterSpec {
            name: "silo-prod".into(),
            nodes: nodes.into_iter().map(str::to_owned).collect(),
            image_tag: "RELEASE.2026-09-03T13-18-01Z".into(),
            endpoint: "https://s3.example.com".into(),
            access_key: "opsdadmin".into(),
            secret_key: "a-very-long-secret-key".into(),
            created_at: 0,
        }
    }

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
            weight: 0,
            hidden: false,
            public_remark: None,
        }
    }

    /// 一台带 n 块干净盘的节点。
    fn inventory(disks: usize, size: u64) -> HostInventory {
        let mut devices = Vec::new();
        let mut risks = Vec::new();
        for index in 0..disks {
            let name = format!("sd{}", (b'b' + index as u8) as char);
            devices.push(BlockDevice {
                name: name.clone(),
                path: format!("/dev/{name}"),
                kind: "disk".into(),
                size,
                ..Default::default()
            });
            risks.push(DriveRisk {
                device: name,
                reasons: vec!["未挂载、无文件系统、非系统盘，可用于存储集群".into()],
                ..Default::default()
            });
        }
        HostInventory {
            collected_at: 1,
            hostname: "n".into(),
            distro: "Debian 13".into(),
            kernel: "6.6".into(),
            arch: "x86_64".into(),
            cpu_model: "x".into(),
            cpu_cores: 4,
            memory_total: 1,
            uptime: 1,
            devices,
            risks,
            interfaces: vec![],
            existing: Default::default(),
            smart: Some("健康".into()),
            gaps: vec![],
        }
    }

    fn world(
        count: usize,
        disks: usize,
        size: u64,
    ) -> (
        HashMap<String, Node>,
        HashMap<String, HostInventory>,
        HashSet<String>,
    ) {
        let mut nodes = HashMap::new();
        let mut inventories = HashMap::new();
        let mut online = HashSet::new();
        for index in 1..=count {
            let id = format!("C00{index}");
            nodes.insert(id.clone(), node(&id));
            inventories.insert(id.clone(), inventory(disks, size));
            online.insert(id);
        }
        (nodes, inventories, online)
    }

    fn ids(count: usize) -> Vec<String> {
        (1..=count).map(|i| format!("C00{i}")).collect()
    }

    #[test]
    fn 拒绝_latest_标签() {
        let mut s = spec(ids(4).iter().map(String::as_str).collect());
        s.image_tag = "latest".into();
        let error = validate_spec(&s).unwrap_err().to_string();
        assert!(error.contains("latest"), "应说明不接受 latest：{error}");
    }

    #[test]
    fn 拒绝不足四台与重复节点() {
        let mut s = spec(vec!["C001", "C002", "C003"]);
        assert!(validate_spec(&s).is_err());
        s.nodes = vec!["C001", "C001", "C002", "C003"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let error = validate_spec(&s).unwrap_err().to_string();
        assert!(error.contains("重复"));
    }

    #[test]
    fn 拒绝非法集群名与短密钥() {
        let mut s = spec(ids(4).iter().map(String::as_str).collect());
        s.name = "有 空格".into();
        assert!(validate_spec(&s).is_err());
        s.name = "ok".into();
        s.secret_key = "short".into();
        assert!(validate_spec(&s).is_err());
    }

    #[test]
    fn 计划精确列出将被清空的设备与容量() {
        let (nodes, inventories, online) = world(4, 2, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        assert_eq!(plan.nodes.len(), 4);
        for node in &plan.nodes {
            assert_eq!(node.devices.len(), 2);
            for device in &node.devices {
                assert_eq!(device.expected_size, 1000, "必须记下计划时的容量");
                assert!(device.path.starts_with("/dev/"));
                assert!(device.mount.starts_with("/data/disk"));
            }
            // 挂载点不能重复
            let mut mounts = node.mounts.clone();
            mounts.sort();
            mounts.dedup();
            assert_eq!(mounts.len(), node.devices.len());
        }
        assert!(plan.destructive);
        assert!(plan.irreversible_notice.contains("不可回滚"));
        assert!(plan.image.starts_with("pgsty/silo:RELEASE."));
    }

    #[test]
    fn 未盘点的节点不能被纳入计划() {
        let (nodes, mut inventories, online) = world(4, 1, 1000);
        inventories.remove("C002");
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let error = build_plan(&s, &nodes, &inventories, &online)
            .unwrap_err()
            .to_string();
        assert!(error.contains("尚未盘点"), "{error}");
    }

    #[test]
    fn 离线节点不能被纳入计划() {
        let (nodes, inventories, mut online) = world(4, 1, 1000);
        online.remove("C003");
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let error = build_plan(&s, &nodes, &inventories, &online)
            .unwrap_err()
            .to_string();
        assert!(error.contains("离线"), "{error}");
    }

    #[test]
    fn 没有干净磁盘的节点不能被纳入计划() {
        let (nodes, mut inventories, online) = world(4, 1, 1000);
        // 把 C002 的盘标成系统盘
        let inv = inventories.get_mut("C002").unwrap();
        inv.risks[0].system_disk = true;
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let error = build_plan(&s, &nodes, &inventories, &online)
            .unwrap_err()
            .to_string();
        assert!(error.contains("没有可用磁盘"), "{error}");
    }

    #[test]
    fn 磁盘数量不一致时整体拒绝() {
        let (nodes, mut inventories, online) = world(4, 2, 1000);
        // 给 C003 再加一块盘，造成不一致
        let inv = inventories.get_mut("C003").unwrap();
        inv.devices.push(BlockDevice {
            name: "sdz".into(),
            path: "/dev/sdz".into(),
            kind: "disk".into(),
            size: 1000,
            ..Default::default()
        });
        inv.risks.push(DriveRisk {
            device: "sdz".into(),
            reasons: vec!["可用于存储集群".into()],
            ..Default::default()
        });
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let error = build_plan(&s, &nodes, &inventories, &online)
            .unwrap_err()
            .to_string();
        assert!(error.contains("数量不一致"), "{error}");
    }

    #[test]
    fn 格式化必须显式确认才能执行() {
        let (nodes, inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        // 未确认：拒绝
        let error = check_applicable(&plan, false, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("不可逆"), "{error}");
        // 确认后通过
        check_applicable(&plan, true, &online, &inventories).unwrap();
    }

    #[test]
    fn 过期的计划不能执行() {
        let (nodes, inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let mut plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        plan.expires = now() - 1;
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("过期"), "{error}");
    }

    #[test]
    fn 设备容量变化会让计划作废() {
        // 计划与执行之间设备被换掉是最危险的场景：容量是最后的可核对特征
        let (nodes, mut inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        inventories.get_mut("C001").unwrap().devices[0].size = 2000;
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("容量已变化"), "{error}");
    }

    #[test]
    fn 设备被占用后计划作废() {
        let (nodes, mut inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        // 执行前该盘被挂载了
        let inv = inventories.get_mut("C001").unwrap();
        inv.risks[0].mounted = true;
        inv.risks[0].has_data = true;
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("不再是无使用痕迹"), "{error}");
    }

    #[test]
    fn 节点离线后计划作废() {
        let (nodes, inventories, mut online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        online.remove("C004");
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("已离线"), "{error}");
    }

    #[test]
    fn 篡改设备清单会被指纹发现() {
        let (nodes, inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let mut plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        // 偷偷往计划里塞一块盘
        plan.nodes[0].devices[0].path = "/dev/sda".into();
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("已被修改") || error.contains("已不存在"),
            "指纹或存在性检查应拦住篡改：{error}"
        );
    }

    #[test]
    fn 已有失败节点时不能继续执行() {
        let (nodes, inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let mut plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        plan.failed.push(("C002".into(), "磁盘准备失败".into()));
        let error = check_applicable(&plan, true, &online, &inventories)
            .unwrap_err()
            .to_string();
        assert!(error.contains("失败节点"), "{error}");
    }

    #[test]
    fn 指纹对设备集合敏感() {
        let (nodes, inventories, online) = world(4, 1, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        let mut changed = plan.nodes.clone();
        changed[0].devices[0].expected_size += 1;
        assert_ne!(fingerprint(&plan.nodes), fingerprint(&changed));
        // 顺序不同不应影响指纹
        let mut reordered = plan.nodes.clone();
        reordered.reverse();
        assert_eq!(fingerprint(&plan.nodes), fingerprint(&reordered));
    }

    #[test]
    fn 部署动作带上全部节点的拓扑与固定镜像() {
        let (nodes, inventories, online) = world(4, 2, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        let action = deploy_action(&s, &plan, "C001").unwrap();
        match action {
            Action::StorageDeploy {
                image,
                mounts,
                peers,
                ..
            } => {
                assert!(image.starts_with("pgsty/silo:"), "镜像必须来自固定仓库");
                assert!(!image.ends_with("latest"));
                assert_eq!(mounts.len(), 2, "本节点只挂载自己的盘");
                assert_eq!(peers.len(), 8, "拓扑要包含全部节点的全部盘位");
                assert!(
                    peers.iter().all(|p| p.starts_with("https://s3.example.com/data/disk")),
                    "不应拼出双斜杠：{peers:?}"
                );
            }
            other => panic!("应为部署动作，实际 {other:?}"),
        }
    }

    #[test]
    fn 准备动作只包含本节点的盘() {
        let (nodes, inventories, online) = world(4, 2, 1000);
        let s = spec(ids(4).iter().map(String::as_str).collect());
        let plan = build_plan(&s, &nodes, &inventories, &online).unwrap();
        match prepare_action(&plan, "C002").unwrap() {
            Action::StoragePrepare {
                plan_id,
                devices,
                filesystem,
                ..
            } => {
                assert_eq!(plan_id, plan.id, "执行要能追溯到计划");
                assert_eq!(devices.len(), 2);
                assert_eq!(filesystem, "xfs");
            }
            other => panic!("应为准备动作，实际 {other:?}"),
        }
        // 不在计划里的节点没有动作
        assert!(prepare_action(&plan, "C099").is_none());
    }
}
