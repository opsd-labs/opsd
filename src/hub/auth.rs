use super::*;
use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use axum::{
    Extension, Json,
    extract::{Request, State as S},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub csrf: String,
    pub expires: i64,
}
pub fn hash_password(p: &str) -> Result<String> {
    Ok(Argon2::default()
        .hash_password(p.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .to_string())
}
pub fn cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|p| p.trim().strip_prefix("opsd_session=").map(str::to_owned))
}
pub async fn session(s: &State, h: &HeaderMap) -> Option<Session> {
    let key = cookie(h)?;
    let session = s.db.get::<Session>("sessions", &digest(key)).await.ok()??;
    (session.expires > now()).then_some(session)
}
pub async fn require(S(s): S<State>, mut req: Request, next: Next) -> Response {
    let Some(session) = session(&s, req.headers()).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"请先登录"})),
        )
            .into_response();
    };
    if !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) && (req.headers().get("origin").and_then(|v| v.to_str().ok()) != Some(s.origin.as_str())
        || req
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            != Some(session.csrf.as_str()))
    {
        return (StatusCode::FORBIDDEN, "请求来源或 CSRF 校验失败").into_response();
    }
    req.extensions_mut().insert(session);
    next.run(req).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub password: String,
}
pub async fn login(
    S(s): S<State>,
    Extension(peer): Extension<transport::Peer>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> ApiResult<Response> {
    if headers.get("origin").and_then(|v| v.to_str().ok()) != Some(s.origin.as_str()) {
        return Err(ApiError(StatusCode::FORBIDDEN, "请求来源不匹配".into()));
    }
    {
        let mut limits = s.login_limits.lock().await;
        limits.retain(|_, v| v.0 > now() - 300);
        if limits.len() > 4096 {
            return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "请稍后重试".into()));
        }
        let entry = limits
            .entry(peer.address.ip().to_string())
            .or_insert((now(), 0));
        entry.1 += 1;
        if entry.1 > 10 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "登录尝试过于频繁".into(),
            ));
        }
    }
    let hash =
        s.db.get::<String>("auth", "admin")
            .await?
            .ok_or_else(|| bad("未初始化"))?;
    let ok = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(input.password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .map_err(|_| bad("密码验证失败"))?;
    if !ok {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "密码错误".into()));
    }
    let key = format!("{}{}", id(), id());
    let session = Session {
        csrf: id(),
        expires: now() + 28800,
    };
    s.db.insert("sessions", &digest(&key), &session).await?;
    // Cookie 的 Path 限定在安全入口之下：浏览器**不会**把它发给分享面，
    // 这是"分享页拿不到控制台凭据"这条保证的落点，不能退化成 Path=/。
    let path = format!("/{}/", s.entrance_path());
    Ok((
        [(
            "set-cookie",
            format!(
                "opsd_session={key}; HttpOnly; Secure; SameSite=Strict; Path={path}; Max-Age=28800"
            ),
        )],
        Json(session),
    )
        .into_response())
}
pub async fn me(Extension(s): Extension<Session>) -> Json<Session> {
    Json(s)
}
pub async fn logout(S(s): S<State>, h: HeaderMap) -> ApiResult<Response> {
    if let Some(k) = cookie(&h) {
        s.db.delete("sessions", &digest(k)).await?;
    }
    let path = format!("/{}/", s.entrance_path());
    Ok((
        [(
            "set-cookie",
            format!("opsd_session=; HttpOnly; Secure; SameSite=Strict; Path={path}; Max-Age=0"),
        )],
        Json(serde_json::json!({"ok":true})),
    )
        .into_response())
}
