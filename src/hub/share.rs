//! 分享页：对外只读状态面，以及供面板消费的公开 API。
//!
//! 隔离是本模块的第一原则：
//!
//! - 分享面使用**自己的路径与自己的令牌**，不经过控制台安全入口。
//! - 控制台会话 Cookie 的 `Path` 限定在安全入口之下，浏览器**不会**把它发给分享路径。
//! - 公开 API 只输出字段白名单中的内容，白名单**默认拒绝**。
//! - 永不下发：公网地址、EasyTier 地址、SSH 端口、凭据、容器/防火墙/数据库细节、
//!   终端入口、任务与审计内容。
//!
//! 因此分享页即使被完全攻破，也拿不到控制台的任何凭据。
use anyhow::Result;
use opsd::{
    protocol::{MetricsRecord, Node, digest, id, now},
    store::Store,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/* ------------------------------------------------------------------ *
 * 处理函数
 *
 * 关于凭据的分工（对应方案里的「API Key 决定能否读、分享令牌决定能读哪些节点」）：
 *
 * - `/share/{令牌}/data/` 下的接口：分享页自身使用，**仅凭分享令牌**授权。
 *   令牌本来就在地址里，页面上藏不住任何密钥，因此不再要求 API Key。
 * - `/share/{令牌}/api/v1/public/` 下的接口：供面板等机器消费，**需要分享令牌 + API Key**。
 *   两者角色不同：令牌限定可见的节点范围，密钥决定是否有权以机器方式读取。
 * ------------------------------------------------------------------ */

use super::*;
use super::metrics::{MAX_POINTS, decimate, tier_for_step};
use axum::{
    Json,
    extract::{Path, Query, State as S},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

/// 当前分享配置。整体开关与站点信息都可在线修改。
#[derive(Default)]
pub struct Settings {
    pub enabled: bool,
    pub site: SitePublic,
}

pub type SharedSettings = std::sync::Arc<std::sync::RwLock<Settings>>;

/// 从请求头里取出 Bearer 密钥。
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

/// 校验机器访问凭据：分享令牌已由门禁验证，这里再要求有效 API Key，
/// 并且密钥的节点范围与令牌的节点范围**取交集**。
async fn authorize_key(
    s: &State,
    headers: &HeaderMap,
    token: &ShareToken,
) -> ApiResult<Vec<String>> {
    let presented = bearer(headers)
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "缺少 API Key".into()))?;
    let digest = digest(presented);
    let key = s
        .db
        .list::<ApiKey>(KEY_BUCKET)
        .await?
        .into_iter()
        .find(|k| k.digest == digest && !k.revoked);
    let mut key = key.ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "API Key 无效".into()))?;
    // 记录最近使用时间，便于管理员判断哪些密钥还在用
    key.last_used = Some(now());
    // 每次读取都写回会放大写压力，只在超过一分钟时才更新
    if key
        .last_used
        .is_none_or(|t| now() - t > 60)
    {
        let _ = s.db.put(KEY_BUCKET, &key.id, &key).await;
    }
    // 取交集：密钥范围与令牌范围都必须覆盖
    let allowed: Vec<String> = s
        .db
        .list::<Node>("nodes")
        .await?
        .into_iter()
        .filter(|n| !n.revoked)
        .filter(|n| token.covers(&n.id))
        .filter(|n| match &key.nodes {
            Some(list) => list.iter().any(|x| x == &n.id),
            None => true,
        })
        .map(|n| n.id)
        .collect();
    Ok(allowed)
}

/// 汇总当前分享面可见的节点。隐藏节点对访客不可见；
/// 令牌限定时只看得到被限定的节点。
async fn visible_nodes(s: &State, token: &ShareToken) -> ApiResult<Vec<PublicNode>> {
    let nodes = s.db.list::<Node>("nodes").await?;
    let connected: std::collections::HashSet<String> =
        s.channels.read().await.keys().cloned().collect();
    let metrics = s.latest_metrics.read().await;
    let mut public: Vec<PublicNode> = nodes
        .into_iter()
        .filter(|n| !n.revoked && !n.hidden && token.covers(&n.id))
        .map(|n| {
            let online = connected.contains(&n.id);
            public_node(&n, online, metrics.get(&n.id))
        })
        .collect();
    // 权重大的排前面，其次按名称，保证顺序稳定
    public.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.name.cmp(&b.name)));
    Ok(public)
}

fn token_of(s: &State, presented: &str) -> ApiResult<ShareToken> {
    let index = s
        .share_index
        .read()
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "分享索引不可用".into()))?;
    index
        .find(presented)
        .cloned()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "分享链接无效或已失效".into()))
}

fn ensure_enabled(s: &State) -> ApiResult<()> {
    let enabled = s
        .share_settings
        .read()
        .map(|settings| settings.enabled)
        .unwrap_or(false);
    check(enabled, "分享页已关闭")
}

/// 分享页自身使用的站点信息。
pub async fn data_summary(
    S(s): S<State>,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let site = s
        .share_settings
        .read()
        .map(|settings| settings.site.clone())
        .unwrap_or_default();
    let nodes = visible_nodes(&s, &token).await?;
    Ok(Json(serde_json::json!({
        "site": site,
        "node_count": nodes.len(),
        "online_count": nodes.iter().filter(|n| n.online).count(),
        "generated_at": now(),
    })))
}

pub async fn data_nodes(
    S(s): S<State>,
    Path(token): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let nodes = visible_nodes(&s, &token).await?;
    Ok(Json(serde_json::json!({ "nodes": nodes })))
}

#[derive(Deserialize)]
pub struct RecordsQuery {
    node: String,
    from: i64,
    to: i64,
    #[serde(default = "default_step")]
    step: i64,
}
fn default_step() -> i64 {
    300
}

/// 分享面的历史曲线。与内部分享同一套降采样规则，但不返回明细字段。
pub async fn data_records(
    S(s): S<State>,
    Path(token): Path<String>,
    Query(q): Query<RecordsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    check(token.covers(&q.node), "该节点不在分享范围内")?;
    check(q.to > q.from, "结束时间必须晚于开始时间")?;
    let step = q.step.clamp(60, 86400);
    check(
        (q.to - q.from) / step <= MAX_POINTS,
        "区间与步长组合返回点数过多",
    )?;
    let tier = tier_for_step(step);
    let rows = s
        .db
        .metrics_range(&q.node, tier, q.from, q.to, MAX_POINTS + 1)
        .await?;
    let points = decimate(rows, step, tier == "raw");
    Ok(Json(serde_json::json!({
        "node": q.node,
        "tier": tier,
        "step": step,
        "points": points,
    })))
}

/// 机器接口的汇总。
pub async fn public_summary(
    S(s): S<State>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let allowed = authorize_key(&s, &headers, &token).await?;
    let all = visible_nodes(&s, &token).await?;
    let nodes: Vec<PublicNode> = all
        .into_iter()
        .filter(|n| allowed.iter().any(|a| a == &n.id))
        .collect();
    Ok(Json(serde_json::json!({
        "node_count": nodes.len(),
        "online_count": nodes.iter().filter(|n| n.online).count(),
        "generated_at": now(),
    })))
}

pub async fn public_nodes(
    S(s): S<State>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let allowed = authorize_key(&s, &headers, &token).await?;
    let nodes: Vec<PublicNode> = visible_nodes(&s, &token)
        .await?
        .into_iter()
        .filter(|n| allowed.iter().any(|a| a == &n.id))
        .collect();
    Ok(Json(serde_json::json!({ "nodes": nodes })))
}

/// 单节点最近一次采样。
pub async fn public_recent(
    S(s): S<State>,
    Path((token, node_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let allowed = authorize_key(&s, &headers, &token).await?;
    check(allowed.iter().any(|a| a == &node_id), "该节点不在访问范围内")?;
    let nodes = visible_nodes(&s, &token).await?;
    let node = nodes
        .into_iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| bad("节点不可见"))?;
    Ok(Json(serde_json::json!({ "node": node })))
}

pub async fn public_records(
    S(s): S<State>,
    Path(token): Path<String>,
    Query(q): Query<RecordsQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let allowed = {
        ensure_enabled(&s)?;
        let token = token_of(&s, &token)?;
        let allowed = authorize_key(&s, &headers, &token).await?;
        check(allowed.iter().any(|a| a == &q.node), "该节点不在访问范围内")?;
        allowed
    };
    let _ = allowed;
    data_records(S(s), Path(token), Query(q)).await
}

/// Prometheus 文本格式。需要分享令牌与 API Key 双重校验。
pub async fn public_metrics(
    S(s): S<State>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    ensure_enabled(&s)?;
    let token = token_of(&s, &token)?;
    let allowed = authorize_key(&s, &headers, &token).await?;
    let nodes: Vec<PublicNode> = visible_nodes(&s, &token)
        .await?
        .into_iter()
        .filter(|n| allowed.iter().any(|a| a == &n.id))
        .collect();
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        render_prometheus(&nodes),
    )
        .into_response())
}

/* ------------------------------------------------------------------ *
 * 管理接口（需要控制台会话）
 * ------------------------------------------------------------------ */

fn refresh(s: &State, index: Index) -> ApiResult<()> {
    let mut slot = s
        .share_index
        .write()
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "分享索引不可用".into()))?;
    *slot = index;
    Ok(())
}

pub async fn read_settings(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let settings = s
        .share_settings
        .read()
        .map(|settings| (settings.enabled, settings.site.clone()))
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "enabled": settings.0,
        "site": settings.1,
    })))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    enabled: bool,
    site: SitePublic,
}

pub async fn update_settings(
    S(s): S<State>,
    Json(input): Json<SettingsInput>,
) -> ApiResult<Json<serde_json::Value>> {
    check(input.site.name.len() <= 60, "站点名称过长")?;
    check(input.site.description.len() <= 200, "站点描述过长")?;
    check(input.site.footer.len() <= 200, "页脚文字过长")?;
    s.db.put(SETTINGS_BUCKET, ENABLED_ID, &input.enabled).await?;
    s.db.put(SITE_BUCKET, SITE_ID, &input.site).await?;
    if let Ok(mut settings) = s.share_settings.write() {
        settings.enabled = input.enabled;
        settings.site = input.site.clone();
    }
    Ok(Json(serde_json::json!({ "enabled": input.enabled, "site": input.site })))
}

pub async fn list_tokens(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let mut tokens = s.db.list::<ShareToken>(BUCKET).await?;
    tokens.sort_by_key(|t| std::cmp::Reverse(t.created_at));
    Ok(Json(serde_json::json!({ "tokens": tokens })))
}

#[derive(Deserialize)]
pub struct TokenInput {
    label: String,
    /// 有效小时数；省略表示长期有效。
    #[serde(default)]
    expires_hours: Option<i64>,
    /// 限定可见节点；省略表示全部未隐藏节点。
    #[serde(default)]
    nodes: Option<Vec<String>>,
}

/// 创建分享令牌。**明文只在此响应中返回一次**，控制库只保存摘要。
pub async fn create_token(
    S(s): S<State>,
    Json(input): Json<TokenInput>,
) -> ApiResult<Json<serde_json::Value>> {
    check(
        !input.label.trim().is_empty() && input.label.len() <= 60,
        "分享名称长度不合法",
    )?;
    if let Some(hours) = input.expires_hours {
        check((1..=24 * 365).contains(&hours), "有效时长不合法")?;
    }
    let raw = new_token();
    let token = ShareToken {
        id: id(),
        label: input.label.trim().to_owned(),
        digest: digest(&raw),
        created_at: now(),
        expires_at: input.expires_hours.map(|h| now() + h * 3600),
        nodes: input.nodes.clone(),
        revoked: false,
    };
    s.db.put(BUCKET, &token.id, &token).await?;
    refresh(&s, Index::load(&s.db).await?)?;
    Ok(Json(serde_json::json!({
        // 明文只出现这一次，之后无法再取回
        "token": raw,
        "record": token,
    })))
}

pub async fn revoke_token(
    S(s): S<State>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut token = s
        .db
        .get::<ShareToken>(BUCKET, &id)
        .await?
        .ok_or_else(|| bad("分享令牌不存在"))?;
    token.revoked = true;
    s.db.put(BUCKET, &id, &token).await?;
    refresh(&s, Index::load(&s.db).await?)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn list_keys(S(s): S<State>) -> ApiResult<Json<serde_json::Value>> {
    let mut keys = s.db.list::<ApiKey>(KEY_BUCKET).await?;
    keys.sort_by_key(|k| std::cmp::Reverse(k.created_at));
    Ok(Json(serde_json::json!({ "keys": keys })))
}

#[derive(Deserialize)]
pub struct KeyInput {
    label: String,
    #[serde(default)]
    nodes: Option<Vec<String>>,
}

/// 创建 API Key。明文同样只返回一次。
pub async fn create_key(
    S(s): S<State>,
    Json(input): Json<KeyInput>,
) -> ApiResult<Json<serde_json::Value>> {
    check(
        !input.label.trim().is_empty() && input.label.len() <= 60,
        "密钥名称长度不合法",
    )?;
    let raw = new_key();
    let key = ApiKey {
        id: id(),
        label: input.label.trim().to_owned(),
        digest: digest(&raw),
        created_at: now(),
        last_used: None,
        revoked: false,
        nodes: input.nodes,
    };
    s.db.put(KEY_BUCKET, &key.id, &key).await?;
    Ok(Json(serde_json::json!({ "key": raw, "record": key })))
}

pub async fn revoke_key(
    S(s): S<State>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut key = s
        .db
        .get::<ApiKey>(KEY_BUCKET, &id)
        .await?
        .ok_or_else(|| bad("API Key 不存在"))?;
    key.revoked = true;
    s.db.put(KEY_BUCKET, &id, &key).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// 分享页静态资源目录（构建产物根）由调用方传入。
/// 若启用了包级分享页主题，则由主题提供页面与资源；否则回落到内置页面。
pub async fn share_index_page(S(s): S<State>) -> Response {
    if let Some(themed) = super::theme::share_theme_index(S(s.clone())).await {
        return themed;
    }
    match tokio::fs::read_to_string(s.web.join("share.html")).await {
        Ok(html) => (
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "分享页尚未构建").into_response(),
    }
}

/// 分享页的静态资源。路径必须落在构建产物的 assets 目录内，
/// 且不允许出现 `..`，避免被用来读取目录以外的文件。
pub async fn share_asset(
    S(s): S<State>,
    Path((_token, path)): Path<(String, String)>,
) -> Response {
    if path.contains("..") || path.starts_with('/') {
        return (StatusCode::NOT_FOUND, "").into_response();
    }
    let target = s.web.join("assets").join(&path);
    let Ok(bytes) = tokio::fs::read(&target).await else {
        return (StatusCode::NOT_FOUND, "").into_response();
    };
    let mime = match target.extension().and_then(|e| e.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
}

/// 分享令牌在控制库中的存放位置。
pub const BUCKET: &str = "share_tokens";
/// API Key 在控制库中的存放位置。
pub const KEY_BUCKET: &str = "api_keys";
/// 分享页整体开关。
pub const SETTINGS_BUCKET: &str = "settings";
pub const ENABLED_ID: &str = "share_enabled";
/// 分享页站点公开属性。
pub const SITE_BUCKET: &str = "share_site";
pub const SITE_ID: &str = "public";

/// 站点公开属性，可由管理员在设置中修改。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitePublic {
    pub name: String,
    pub description: String,
    /// 页脚自定义文字，不允许 HTML。
    pub footer: String,
}

impl Default for SitePublic {
    fn default() -> Self {
        Self {
            name: "opsd".into(),
            description: "节点状态".into(),
            footer: String::new(),
        }
    }
}

/// 分享令牌。明文只在创建时返回一次，控制库中只保存摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareToken {
    pub id: String,
    pub label: String,
    /// sha256(明文令牌)，用于门禁的常量时间查找。
    pub digest: String,
    pub created_at: i64,
    /// 到期时间；`None` 表示长期有效。
    pub expires_at: Option<i64>,
    /// 限定可见节点；`None` 表示全部未隐藏节点。
    pub nodes: Option<Vec<String>>,
    pub revoked: bool,
}

impl ShareToken {
    pub fn expired(&self, at: i64) -> bool {
        self.revoked || self.expires_at.is_some_and(|e| e <= at)
    }
    /// 是否允许看到该节点。
    pub fn covers(&self, node_id: &str) -> bool {
        match &self.nodes {
            Some(list) => list.iter().any(|n| n == node_id),
            None => true,
        }
    }
}

/// API Key。同样只保存摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub label: String,
    pub digest: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
    pub revoked: bool,
    /// 限定可见节点；`None` 表示全部。
    #[serde(default)]
    pub nodes: Option<Vec<String>>,
}

/// 门禁使用的活跃令牌索引：摘要 → 令牌。由控制台在增删改后刷新。
#[derive(Default)]
pub struct Index {
    pub tokens: HashMap<String, ShareToken>,
}

impl Index {
    pub async fn load(db: &Store) -> Result<Self> {
        let tokens = db
            .list::<ShareToken>(BUCKET)
            .await?
            .into_iter()
            .filter(|t| !t.expired(now()))
            .map(|t| (t.digest.clone(), t))
            .collect();
        Ok(Self { tokens })
    }
    /// 按出示的明文令牌查找。查找走摘要，因此内存里不保留明文。
    pub fn find(&self, presented: &str) -> Option<&ShareToken> {
        let digest = digest(presented);
        self.tokens.get(&digest).filter(|t| !t.expired(now()))
    }
}

pub type SharedIndex = std::sync::Arc<std::sync::RwLock<Index>>;

pub fn new_token() -> String {
    format!("{}{}", id().replace('-', ""), id().replace('-', ""))
}
pub fn new_key() -> String {
    format!("opsd_{}", id().replace('-', ""))
}

/// 从路径中取出分享令牌的逻辑位于 `opsd::entrance::share_token`，
/// 因为门禁需要在最外层做同样的判断；这里不再重复实现。

/* ------------------------------------------------------------------ *
 * 公开字段白名单
 * ------------------------------------------------------------------ */

/// 分享页允许下发的字段。**默认拒绝**：不在此列表中的任何内容都不会出现在响应里。
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
}

/// 把节点与指标折算成公开结构。**只读取白名单字段**，
/// 结构体本身即是白名单，不依赖逐字段删除，避免漏删。
pub fn public_node(node: &Node, online: bool, record: Option<&MetricsRecord>) -> PublicNode {
    let sample = record.and_then(|r| r.sample.as_ref());
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
        metrics_at: record.map(|r| r.received_at),
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
    ("opsd_node_network_receive_bytes_per_second", "入站速率（字节/秒）"),
    ("opsd_node_network_transmit_bytes_per_second", "出站速率（字节/秒）"),
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
        sample("opsd_node_disk_used_bytes", node.disk_used.map(|v| v as f64));
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
        sample(
            "opsd_node_uptime_seconds",
            node.uptime.map(|v| v as f64),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use opsd::protocol::{DiskUsage, InterfaceRate, MetricsSample};

    fn node(id: &str, name: &str) -> Node {
        Node {
            id: id.into(),
            name: name.into(),
            public_addresses: vec!["203.0.113.9".into()],
            overlay_address: Some("100.100.201.52".into()),
            ssh_port: 65522,
            revoked: false,
            last_seen: 1,
            capabilities: None,
            inventory: Some(serde_json::json!({"docker": {"state": "ok"}})),
            address_version: 3,
            region: Some("日本".into()),
            group: Some("数据库".into()),
            tags: vec!["galera".into()],
            weight: 5,
            hidden: false,
            public_remark: Some("东京节点".into()),
        }
    }

    fn record(cpu: f64) -> MetricsRecord {
        MetricsRecord {
            node_id: "C052".into(),
            received_at: 1000,
            clock_offset: 0,
            error: None,
            sample: Some(MetricsSample {
                at: 1000,
                cpu_usage: cpu,
                memory_used: 100,
                memory_total: 200,
                load1: 0.5,
                disks: vec![DiskUsage {
                    mount: "/".into(),
                    total: 1000,
                    used: 400,
                    ..Default::default()
                }],
                interfaces: vec![InterfaceRate {
                    name: "eth0".into(),
                    rx_bytes_per_second: 10.0,
                    tx_bytes_per_second: 20.0,
                    ..Default::default()
                }],
                uptime: 3600,
                ..Default::default()
            }),
        }
    }

    #[test]
    fn 公开结构不包含任何地址与内部细节() {
        let value = serde_json::to_value(public_node(
            &node("C052", "C052 · 日本"),
            true,
            Some(&record(12.5)),
        ))
        .unwrap();
        let text = value.to_string();
        for secret in [
            "203.0.113.9",      // 公网地址
            "100.100.201.52",   // EasyTier 地址
            "65522",            // SSH 端口
            "docker",           // 容器细节
            "inventory",        // 原始采集数据
        ] {
            assert!(!text.contains(secret), "公开响应不应包含 {secret}");
        }
        // 白名单字段必须存在
        assert_eq!(value["id"], "C052");
        assert_eq!(value["region"], "日本");
        assert_eq!(value["cpu_usage"], 12.5);
        assert_eq!(value["disk_used"], 400);
        assert_eq!(value["net_rx_bytes_per_second"], 10);
    }

    #[test]
    fn 没有采样时指标字段为_null_而不是零() {
        let value =
            serde_json::to_value(public_node(&node("C052", "C052"), false, None)).unwrap();
        for field in [
            "cpu_usage",
            "memory_used",
            "disk_used",
            "load1",
            "net_rx_bytes_per_second",
            "uptime",
        ] {
            assert!(value[field].is_null(), "{field} 应为 null 而不是 0");
        }
        assert_eq!(value["online"], false);
    }

    #[test]
    fn prometheus_输出缺少采样时不写样本行() {
        let with = public_node(&node("C052", "东京 \"主力\" 节点"), true, Some(&record(12.5)));
        let without = public_node(&node("C061", "C061"), false, None);
        let text = render_prometheus(&[with, without]);
        // 帮助与类型行齐全
        for (name, _) in PROMETHEUS_METRICS {
            assert!(text.contains(&format!("# HELP {name} ")), "缺少 {name} 的 HELP");
            assert!(text.contains(&format!("# TYPE {name} gauge")));
        }
        // 在线状态一定有值
        assert!(text.contains("opsd_node_online{node=\"C052\",display_name=\"东京 \\\"主力\\\" 节点\"} 1"));
        assert!(text.contains("opsd_node_online{node=\"C061\",display_name=\"C061\"} 0"));
        // 有采样的节点输出 CPU，没有采样的节点不输出
        assert!(text.contains("opsd_node_cpu_usage_percent{node=\"C052\""));
        assert!(!text.contains("opsd_node_cpu_usage_percent{node=\"C061\""));
        // 标签里不能出现地址
        assert!(!text.contains("100.100.201.52"));
        assert!(!text.contains("203.0.113.9"));
    }

    #[test]
    fn 令牌过期与撤销后不再有效() {
        let mut token = ShareToken {
            id: "1".into(),
            label: "临时".into(),
            digest: digest("secret"),
            created_at: 0,
            expires_at: Some(100),
            nodes: None,
            revoked: false,
        };
        assert!(!token.expired(99));
        assert!(token.expired(100), "到期即失效");
        token.expires_at = None;
        assert!(!token.expired(999999));
        token.revoked = true;
        assert!(token.expired(1), "撤销后立即失效");
    }

    #[test]
    fn 令牌可限定可见节点() {
        let mut token = ShareToken {
            id: "1".into(),
            label: "子集".into(),
            digest: digest("secret"),
            created_at: 0,
            expires_at: None,
            nodes: Some(vec!["C052".into()]),
            revoked: false,
        };
        assert!(token.covers("C052"));
        assert!(!token.covers("C071"), "未列出的节点不可见");
        token.nodes = None;
        assert!(token.covers("C071"), "未限定时全部可见");
    }

    #[test]
    fn 索引用摘要查找且不保留明文() {
        let raw = "abcdef";
        let mut index = Index::default();
        index.tokens.insert(
            digest(raw),
            ShareToken {
                id: "1".into(),
                label: "x".into(),
                digest: digest(raw),
                created_at: 0,
                expires_at: None,
                nodes: None,
                revoked: false,
            },
        );
        assert!(index.find(raw).is_some(), "出示明文应能命中");
        assert!(index.find("wrong").is_none());
        // 索引里不应出现明文
        assert!(!index.tokens.contains_key(raw));
    }
}
