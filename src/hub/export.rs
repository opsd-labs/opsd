use super::metrics::NodeMetrics;
use super::*;
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub label: String,
    pub digest: String,
    pub created_at: i64,
    pub revoked: bool,
    pub nodes: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyInput {
    pub label: String,
    pub nodes: Option<Vec<String>>,
}
pub async fn keys(S(s): S<State>) -> ApiResult<Json<Value>> {
    let records = s.db.list::<ApiKey>("export_keys").await?;
    Ok(Json(
        json!({"keys":records.into_iter().map(|k|json!({"id":k.id,"label":k.label,"created_at":k.created_at,"revoked":k.revoked,"nodes":k.nodes})).collect::<Vec<_>>()}),
    ))
}
pub async fn create(S(s): S<State>, Json(input): Json<KeyInput>) -> ApiResult<Response> {
    check(
        !input.label.trim().is_empty() && input.label.len() <= 100,
        "密钥名称无效",
    )?;
    if let Some(nodes) = &input.nodes {
        for node in nodes {
            check(
                s.db.get::<Node>("nodes", node)
                    .await?
                    .is_some_and(|n| !n.revoked),
                "密钥包含未知节点",
            )?;
        }
    }
    let token = format!("opsd_{}{}", id().replace('-', ""), id().replace('-', ""));
    let record = ApiKey {
        id: id(),
        label: input.label,
        digest: digest(&token),
        created_at: now(),
        revoked: false,
        nodes: input.nodes,
    };
    s.db.insert("export_keys", &record.id, &record).await?;
    Ok((
        [("cache-control", "no-store")],
        Json(json!({"id":record.id,"key":token,"label":record.label,"nodes":record.nodes})),
    )
        .into_response())
}
pub async fn revoke(S(s): S<State>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut key =
        s.db.get::<ApiKey>("export_keys", &id)
            .await?
            .ok_or_else(|| bad("输出密钥不存在"))?;
    key.revoked = true;
    s.db.put("export_keys", &id, &key).await?;
    Ok(Json(json!({"revoked":true})))
}
async fn visible(s: &State, headers: &HeaderMap) -> ApiResult<Vec<PublicNode>> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "缺少只读 API Key".into()))?;
    let hash = digest(token);
    let key =
        s.db.list::<ApiKey>("export_keys")
            .await?
            .into_iter()
            .find(|k| !k.revoked && k.digest == hash)
            .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "只读 API Key 无效".into()))?;
    let nodes = s.db.list::<Node>("nodes").await?;
    let channels = s.channels.read().await;
    let metrics = s.latest_metrics.read().await;
    Ok(nodes
        .iter()
        .filter(|n| {
            !n.revoked && !n.hidden && key.nodes.as_ref().is_none_or(|ids| ids.contains(&n.id))
        })
        .map(|n| public_node(n, channels.contains_key(&n.id), metrics.get(&n.id)))
        .collect())
}
pub async fn nodes(S(s): S<State>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"schema_version":1,"generated_at":now(),"nodes":visible(&s,&headers).await?}),
    ))
}
pub async fn summary(S(s): S<State>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let nodes = visible(&s, &headers).await?;
    Ok(Json(
        json!({"schema_version":1,"generated_at":now(),"node_count":nodes.len(),"online_count":nodes.iter().filter(|n|n.online).count()}),
    ))
}
pub async fn metrics(S(s): S<State>, headers: HeaderMap) -> ApiResult<Response> {
    Ok((
        [
            ("content-type", "text/plain; version=0.0.4; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        render_prometheus(&visible(&s, &headers).await?),
    )
        .into_response())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicNode {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub region: Option<String>,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub weight: i64,
    pub remark: Option<String>,
    /// 以下指标字段在无采样时为 `None`，前端显示「—」而不是 0。
    pub cpu_usage: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub disk_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub load1: Option<f64>,
    pub net_rx_bytes_per_second: Option<u64>,
    pub net_tx_bytes_per_second: Option<u64>,
    pub uptime: Option<u64>,
    /// 最近一次成功采样的时刻，供访客判断数据新鲜度。
    pub metrics_at: Option<i64>,
    /// 当前采集状态：ok / collect_failed / unknown（从未上报）。
    pub metrics_status: String,
}

/// 把节点与指标折算成公开结构。**只读取白名单字段**，
/// 结构体本身即是白名单，不依赖逐字段删除，避免漏删。
pub fn public_node(node: &Node, online: bool, metrics: Option<&NodeMetrics>) -> PublicNode {
    // 指标与新鲜度只认最近一次**成功**采样；当前失败通过 metrics_status 显式暴露。
    let sample = metrics.and_then(|m| m.sample());
    let disk_used: u64 = sample
        .map(|s| s.disks.iter().map(|d| d.used).sum())
        .unwrap_or(0);
    let disk_total: u64 = sample
        .map(|s| s.disks.iter().map(|d| d.total).sum())
        .unwrap_or(0);
    PublicNode {
        id: node.id.clone(),
        name: node.name.clone(),
        online,
        region: node.region.clone(),
        group: node.group.clone(),
        tags: node.tags.clone(),
        weight: node.weight,
        remark: node.public_remark.clone(),
        cpu_usage: sample.map(|s| s.cpu_usage),
        memory_used: sample.map(|s| s.memory_used),
        memory_total: sample.map(|s| s.memory_total),
        disk_used: sample.map(|_| disk_used),
        disk_total: sample.map(|_| disk_total),
        load1: sample.map(|s| s.load1),
        net_rx_bytes_per_second: sample.map(|s| {
            s.interfaces
                .iter()
                .map(|i| i.rx_bytes_per_second)
                .sum::<f64>() as u64
        }),
        net_tx_bytes_per_second: sample.map(|s| {
            s.interfaces
                .iter()
                .map(|i| i.tx_bytes_per_second)
                .sum::<f64>() as u64
        }),
        uptime: sample.map(|s| s.uptime),
        metrics_at: metrics.and_then(|m| m.metrics_at()),
        metrics_status: metrics
            .map(|m| m.status().to_owned())
            .unwrap_or_else(|| "unknown".into()),
    }
}

/* ------------------------------------------------------------------ *
 * Prometheus 文本格式
 * ------------------------------------------------------------------ */

/// 指标名与帮助文本。标签里**不放地址**，只放节点编号与显示名。
pub const PROMETHEUS_METRICS: [(&str, &str); 10] = [
    ("opsd_node_online", "节点是否在线，1 为在线"),
    ("opsd_node_cpu_usage_percent", "CPU 使用率（百分比）"),
    ("opsd_node_memory_used_bytes", "已用内存（字节）"),
    ("opsd_node_memory_total_bytes", "内存总量（字节）"),
    ("opsd_node_disk_used_bytes", "已用磁盘（字节）"),
    ("opsd_node_disk_total_bytes", "磁盘总量（字节）"),
    ("opsd_node_load1", "1 分钟平均负载"),
    (
        "opsd_node_network_receive_bytes_per_second",
        "入站速率（字节/秒）",
    ),
    (
        "opsd_node_network_transmit_bytes_per_second",
        "出站速率（字节/秒）",
    ),
    ("opsd_node_uptime_seconds", "运行时长（秒）"),
];

/// Prometheus 文本格式要求标签值转义反斜杠、双引号与换行。
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// 渲染 Prometheus 文本格式。缺失的采样**不输出样本行**，
/// 这样面板上表现为断档，而不是被记成 0。
pub fn render_prometheus(nodes: &[PublicNode]) -> String {
    let mut out = String::new();
    for (name, help) in PROMETHEUS_METRICS {
        out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n"));
    }
    for node in nodes {
        let labels = format!(
            "node=\"{}\",display_name=\"{}\"",
            escape_label(&node.id),
            escape_label(&node.name)
        );
        out.push_str(&format!(
            "opsd_node_online{{{labels}}} {}\n",
            if node.online { 1 } else { 0 }
        ));
        let mut sample = |name: &str, value: Option<f64>| {
            if let Some(value) = value
                && value.is_finite()
            {
                out.push_str(&format!("{name}{{{labels}}} {value}\n"));
            }
        };
        sample("opsd_node_cpu_usage_percent", node.cpu_usage);
        sample(
            "opsd_node_memory_used_bytes",
            node.memory_used.map(|v| v as f64),
        );
        sample(
            "opsd_node_memory_total_bytes",
            node.memory_total.map(|v| v as f64),
        );
        sample(
            "opsd_node_disk_used_bytes",
            node.disk_used.map(|v| v as f64),
        );
        sample(
            "opsd_node_disk_total_bytes",
            node.disk_total.map(|v| v as f64),
        );
        sample("opsd_node_load1", node.load1);
        sample(
            "opsd_node_network_receive_bytes_per_second",
            node.net_rx_bytes_per_second.map(|v| v as f64),
        );
        sample(
            "opsd_node_network_transmit_bytes_per_second",
            node.net_tx_bytes_per_second.map(|v| v as f64),
        );
        sample("opsd_node_uptime_seconds", node.uptime.map(|v| v as f64));
    }
    out
}
