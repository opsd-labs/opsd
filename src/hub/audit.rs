//! 审计日志。
//!
//! 记录的是**谁在什么时候对哪个节点做了什么**，而不是操作内容的全文。
//! 特别是：凭据、环境变量、SQL 正文与文件内容都不得进入审计正文——
//! 审计本身要能被普通管理员查阅，就不能成为新的泄漏面。
use anyhow::Result;
use opsd::store::Store;
use serde::{Deserialize, Serialize};

use super::*;
use axum::extract::State as S;

/// 单条审计记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// 操作者。首版只有本地管理员，因此是固定标识。
    pub actor: String,
    pub node_id: String,
    /// 会话 / 文件 / 数据库 / 存储 / 防火墙 / 容器 / 分享页 / 主题
    pub category: String,
    /// 被操作的对象，例如 `宿主机终端（起始目录 /）`、`读取 /etc/hosts`。
    pub target: String,
    /// 结果：已开始 / 已结束 / 成功 / 失败。
    pub result: String,
    /// 补充说明。**不得写入内容或秘密。**
    pub detail: String,
    pub source: String,
    pub bytes: u64,
    pub duration: i64,
}

/// 落库时补上时间与自增序号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    #[serde(flatten)]
    pub entry: AuditEntry,
    pub at: i64,
    pub id: String,
}

/// 写入一条审计。审计失败**不应**让被审计的操作失败，因此返回值由调用方按需处理。
pub async fn record(db: &Store, entry: AuditEntry) -> Result<()> {
    let id = opsd::protocol::id();
    let record = AuditRecord {
        entry,
        at: opsd::protocol::now(),
        id: id.clone(),
    };
    db.put(BUCKET, &id, &record).await
}

/// 审计记录的存放位置。
pub const BUCKET: &str = "audit";

/// 查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub from: Option<i64>,
    #[serde(default)]
    pub to: Option<i64>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    /// 返回条数上限，默认 200，硬上限 1000。
    #[serde(default)]
    pub limit: Option<usize>,
}

/// 按条件筛选，最新的在前。
pub fn filter(mut records: Vec<AuditRecord>, query: &AuditQuery) -> Vec<AuditRecord> {
    records.retain(|record| {
        query.from.is_none_or(|from| record.at >= from)
            && query.to.is_none_or(|to| record.at <= to)
            && query
                .node
                .as_ref()
                .is_none_or(|node| &record.entry.node_id == node)
            && query
                .category
                .as_ref()
                .is_none_or(|category| &record.entry.category == category)
            && query
                .result
                .as_ref()
                .is_none_or(|result| &record.entry.result == result)
    });
    records.sort_by(|a, b| b.at.cmp(&a.at));
    let limit = query.limit.unwrap_or(200).min(1000);
    records.truncate(limit);
    records
}

/// 保留策略：审计保留一年。
pub const RETENTION_SECONDS: i64 = 365 * 86400;

/// 查询审计记录。只读，且受条数上限约束。
pub async fn query(
    S(s): S<State>,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    let records = filter(s.db.list::<AuditRecord>(BUCKET).await?, &q);
    Ok(axum::Json(serde_json::json!({
        "records": records,
        "retention_seconds": RETENTION_SECONDS,
    })))
}

/// 清理过期审计。与指标保留策略一起由后台任务调用。
pub async fn prune(db: &Store) -> Result<u64> {
    let cutoff = opsd::protocol::now() - RETENTION_SECONDS;
    let stale: Vec<String> = db
        .list::<AuditRecord>(BUCKET)
        .await?
        .into_iter()
        .filter(|record| record.at < cutoff)
        .map(|record| record.id)
        .collect();
    let mut removed = 0;
    for id in stale {
        removed += db.delete(BUCKET, &id).await?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_at(at: i64, node: &str, category: &str, result: &str) -> AuditRecord {
        AuditRecord {
            entry: AuditEntry {
                actor: "管理员".into(),
                node_id: node.into(),
                category: category.into(),
                target: "读取 /etc/hosts".into(),
                result: result.into(),
                detail: String::new(),
                source: "203.0.113.5".into(),
                bytes: 10,
                duration: 3,
            },
            at,
            id: format!("id-{at}"),
        }
    }

    #[test]
    fn 默认按时间倒序并限制条数() {
        let records: Vec<AuditRecord> = (0..10)
            .map(|i| record_at(i, "C052", "文件", "已结束"))
            .collect();
        let filtered = filter(records, &AuditQuery::default());
        assert_eq!(filtered.len(), 10);
        assert_eq!(filtered[0].at, 9, "最新的应在最前");
        assert_eq!(filtered[9].at, 0);
    }

    #[test]
    fn 筛选条件逐项生效() {
        let records = vec![
            record_at(100, "C052", "文件", "已结束"),
            record_at(200, "C071", "会话", "已开始"),
            record_at(300, "C052", "会话", "已结束"),
        ];
        let by_node = filter(
            records.clone(),
            &AuditQuery {
                node: Some("C052".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_node.len(), 2);
        let by_category = filter(
            records.clone(),
            &AuditQuery {
                category: Some("会话".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_category.len(), 2);
        let by_range = filter(
            records.clone(),
            &AuditQuery {
                from: Some(150),
                to: Some(250),
                ..Default::default()
            },
        );
        assert_eq!(by_range.len(), 1);
        assert_eq!(by_range[0].at, 200);
        // 组合条件
        let combined = filter(
            records,
            &AuditQuery {
                node: Some("C052".into()),
                category: Some("会话".into()),
                ..Default::default()
            },
        );
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].at, 300);
    }

    #[test]
    fn 条数上限有硬顶() {
        let records: Vec<AuditRecord> = (0..50)
            .map(|i| record_at(i, "C052", "文件", "已结束"))
            .collect();
        assert_eq!(
            filter(
                records.clone(),
                &AuditQuery {
                    limit: Some(5),
                    ..Default::default()
                }
            )
            .len(),
            5
        );
        // 超过硬上限时按硬上限截断，避免一次拉取过多
        assert_eq!(
            filter(
                records,
                &AuditQuery {
                    limit: Some(100_000),
                    ..Default::default()
                }
            )
            .len(),
            50
        );
    }

    #[test]
    fn 结果筛选可用于只看失败() {
        let records = vec![
            record_at(100, "C052", "文件", "失败"),
            record_at(200, "C052", "文件", "已结束"),
        ];
        let failed = filter(
            records,
            &AuditQuery {
                result: Some("失败".into()),
                ..Default::default()
            },
        );
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].entry.result, "失败");
    }
}
