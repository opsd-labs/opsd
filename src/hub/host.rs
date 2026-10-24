//! 通用主机盘点入口；对象存储的适合度与拓扑判断由插件负责。
use super::*;
use axum::{Json, extract::State as S};

pub type SharedInventories =
    std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, HostInventory>>>;

/// 触发一次主机盘点。只读操作，但走任务机制以便看到执行结果。
pub async fn host_inspect(
    S(s): S<State>,
    Path(node_id): Path<String>,
    Json(input): Json<ActionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let task = TaskEnvelope::new(
        node_id.clone(),
        input.idempotency_key,
        Action::HostInspect {},
    );
    let task = s.db.accept_task(&task).await?;
    if task.status == TaskStatus::Pending {
        if let Some(tx) = s.channels.read().await.get(&node_id).map(|c| c.1.clone()) {
            let _ = tx.send(Frame::Task { task: task.clone() }).await;
        }
        let _ = s.events.send(serde_json::json!({"type":"tasks"}));
    }
    Ok(Json(serde_json::json!({ "task_id": task.id })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub idempotency_key: String,
}
