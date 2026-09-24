//! 安全入口：控制台只响应 `{ip}:{端口}/{安全入口}/...`，其余请求直接丢弃。
//!
//! 安全入口**不是认证**，它只降低被扫描到的概率。会话 Cookie、CSRF 与 Origin 校验
//! 仍然是认证主体，三者一个都不能省。
use anyhow::Result;
use axum::{
    body::Body,
    http::{Request, Response},
};
use std::{
    collections::HashMap,
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{Arc, RwLock},
    task::{Context as TaskContext, Poll},
};
use tokio::sync::Mutex;
use tower::Service;

/// 字符集为大小写字母、数字与 URL-unreserved 符号。
/// 只用 URL-unreserved 符号（`-` `_` `.` `~`），否则从日志复制到地址栏会触发百分号编码。
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
/// 首字符不能是 `.` 或 `-`：前者会被部分工具当成隐藏路径或相对引用，
/// 后者可能被误读为命令行选项。`validate()` 对此有同样约束，生成时必须一致。
const FIRST_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_~";
pub const LENGTH: usize = 16;
/// 每个来源地址每分钟允许的错误入口次数。
const FAILURE_LIMIT: u32 = 10;
const FAILURE_WINDOW: i64 = 60;
/// 超过错误上限后，关闭连接前的固定退避。
const SLOWDOWN: std::time::Duration = std::time::Duration::from_millis(500);

pub fn generate() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut value: String = (0..LENGTH)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect();
    // 首字符单独约束，保证 generate() 的产物一定能通过 validate()。
    let first = FIRST_ALPHABET[rng.gen_range(0..FIRST_ALPHABET.len())] as char;
    value.replace_range(0..1, &first.to_string());
    value
}

/// 校验管理员设置的安全入口。
pub fn validate(value: &str) -> Result<()> {
    anyhow::ensure!(value.len() == LENGTH, "安全入口必须是 {LENGTH} 位");
    anyhow::ensure!(
        value.bytes().all(|b| ALPHABET.contains(&b)),
        "安全入口只能使用字母、数字与 - . _ ~"
    );
    anyhow::ensure!(
        !value.starts_with('.') && !value.starts_with('-'),
        "安全入口不能以 . 或 - 开头"
    );
    Ok(())
}

/// 常量时间比较，避免逐字符比较泄漏时序信息。
pub fn matches(expected: &str, candidate: &str) -> bool {
    let a = expected.as_bytes();
    let b = candidate.as_bytes();
    let mut diff = (a.len() ^ b.len()) as u8;
    let shared = a.len().min(b.len());
    for i in 0..shared {
        diff |= a[i] ^ b[i];
    }
    // 长度不同时补齐剩余字节，保持总工作量不随匹配前缀变化
    for byte in &a[shared..] {
        diff |= *byte;
    }
    for byte in &b[shared..] {
        diff |= *byte;
    }
    diff == 0
}

/// 取出路径的第一段；`/` 与空路径返回 None。
pub fn first_segment(path: &str) -> Option<&str> {
    let rest = path.strip_prefix('/')?;
    let segment = rest.split('/').next().unwrap_or_default();
    (!segment.is_empty()).then_some(segment)
}

/// 分享面路径的首段。
pub const SHARE_PREFIX: &str = "share";

/// 取出 `/share/{令牌}` 中的令牌。
pub fn share_token(path: &str) -> Option<&str> {
    if first_segment(path)? != SHARE_PREFIX {
        return None;
    }
    let rest = path.strip_prefix('/')?.strip_prefix(SHARE_PREFIX)?;
    let token = rest.strip_prefix('/')?.split('/').next().unwrap_or_default();
    (!token.is_empty()).then_some(token)
}

/// 分享面校验：令牌无效时与错误入口一样被丢弃，不给出任何响应。
fn share_token_matches(path: &str, check: &Arc<dyn Fn(&str) -> bool + Send + Sync>) -> bool {
    share_token(path).is_some_and(|token| check(token))
}

/// 错误入口的按来源限速，防止暴力枚举。
#[derive(Clone, Default)]
pub struct Limits(Arc<Mutex<HashMap<IpAddr, (i64, u32)>>>);

impl Limits {
    /// 记录一次错误入口，并返回该来源当前是否已被限速。
    pub async fn record_failure(&self, ip: IpAddr) -> bool {
        let mut limits = self.0.lock().await;
        limits.retain(|_, v| v.0 > crate::protocol::now() - FAILURE_WINDOW);
        if limits.len() > 4096 {
            limits.clear();
        }
        let entry = limits.entry(ip).or_insert((crate::protocol::now(), 0));
        entry.1 += 1;
        entry.1 > FAILURE_LIMIT
    }
    /// 入口匹配成功时清空该来源的失败计数。
    pub async fn clear(&self, ip: IpAddr) {
        self.0.lock().await.remove(&ip);
    }
}

/// 运行期可改的安全入口；设置页修改后立即生效，无需重启。
pub type Shared = Arc<RwLock<String>>;

/// 明确豁免入口校验的路径白名单。
///
/// Agent 的首次注册必须发生在它拿到客户端证书之前，因此不可能走 mTLS 通道；
/// 这些路径挂在入口前缀**之外**的根路径上，凭据是一次性、短期、随机的注册令牌
/// 与 CA 指纹核对，不依赖入口的隐蔽性。
///
/// 代价必须写明：这些路径会暴露自身存在（返回 401/400 而不是直接断连），
/// 使扫描者能据此得知这是一个 opsd 主控。因此白名单必须**逐条精确匹配**，
/// 绝不能写成前缀，否则会变成绕过门禁的通道。
pub const AGENT_PATHS: &[&str] = &["/api/v1/enroll"];

/// 连接的入口门禁。
///
/// 放行规则有两类：
///
/// 1. **安全入口**：控制台，首段路径必须等于安全入口。
/// 2. **分享令牌**：分享面，路径形如 `/share/{令牌}/...`，且令牌必须有效。
///    令牌的校验由调用方通过 `share` 回调注入，门禁只负责"不通过就丢弃"。
///
/// 其余请求**不产生任何 HTTP 响应**：返回错误让 hyper 直接关闭连接，
/// 从而不留下 404/400 之类可区分的存在性信号。
#[derive(Clone)]
pub struct Gate<S> {
    inner: S,
    entrance: Option<Shared>,
    limits: Limits,
    peer: SocketAddr,
    allow: Arc<Vec<String>>,
    /// 分享令牌校验：给定路径首段之后的第二个路径段，返回是否有效。
    share: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl<S> Gate<S> {
    pub fn new(inner: S, entrance: Option<Shared>, limits: Limits, peer: SocketAddr) -> Self {
        Self {
            inner,
            entrance,
            limits,
            peer,
            allow: Arc::new(Vec::new()),
            share: None,
        }
    }
    /// 设置精确匹配的白名单路径。见 [`AGENT_PATHS`]。
    pub fn allow(mut self, paths: &[&str]) -> Self {
        self.allow = Arc::new(paths.iter().map(|p| (*p).to_owned()).collect());
        self
    }
    /// 注入分享令牌校验。分享面与安全入口是两条独立的放行路径。
    pub fn share(mut self, check: Arc<dyn Fn(&str) -> bool + Send + Sync>) -> Self {
        self.share = Some(check);
        self
    }
}

impl<S, B> Service<Request<B>> for Gate<S>
where
    S: Service<Request<B>, Response = Response<Body>, Error = std::convert::Infallible>
        + Send
        + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response<Body>;
    type Error = std::io::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(|e| match e {})
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let Some(entrance) = self.entrance.clone() else {
            let future = self.inner.call(req);
            return Box::pin(async move { future.await.map_err(|e| match e {}) });
        };
        let path = req.uri().path().to_owned();
        // 白名单逐条精确匹配：绝不能用前缀，否则等于给门禁开口子。
        if self.allow.iter().any(|allowed| allowed == &path) {
            let future = self.inner.call(req);
            return Box::pin(async move { future.await.map_err(|e| match e {}) });
        }
        let candidate = first_segment(&path).unwrap_or_default().to_owned();
        // 分享面：首段是 share 时必须出示有效令牌，否则与错误入口一样直接丢弃。
        let shared = candidate == crate::entrance::SHARE_PREFIX
            && self
                .share
                .as_ref()
                .is_some_and(|check| share_token_matches(&path, check));
        let expected = entrance
            .read()
            .map(|value| value.clone())
            .unwrap_or_default();
        let accepted = shared || matches(&expected, &candidate);
        if !accepted {
            let limits = self.limits.clone();
            let ip = self.peer.ip();
            return Box::pin(async move {
                // 无论是否超限都必须给出完全相同的失败表现：直接关闭连接、不产生任何响应。
                // 因此入口本身不存在可供枚举的判别信号；这里的退避只是拖慢连接洪泛。
                if limits.record_failure(ip).await {
                    tokio::time::sleep(SLOWDOWN).await;
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "入口不匹配",
                ))
            });
        }
        let limits = self.limits.clone();
        let ip = self.peer.ip();
        let future = self.inner.call(req);
        Box::pin(async move {
            limits.clear(ip).await;
            future.await.map_err(|e| match e {})
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 生成的安全入口满足字符集与长度约束() {
        // 必须多次采样：首字符的约束只在部分样本上才会暴露问题
        for _ in 0..512 {
            let value = generate();
            assert_eq!(value.len(), LENGTH);
            validate(&value).unwrap_or_else(|e| panic!("生成的入口 {value:?} 未通过校验：{e}"));
            assert!(value.bytes().all(|b| ALPHABET.contains(&b)));
        }
        // 随机性：连续两次生成不应相同
        assert_ne!(generate(), generate());
    }

    #[test]
    fn 拒绝不合规的安全入口() {
        for bad in [
            "",
            "short",
            "0123456789abcdefg",                     // 17 位
            "0123456789abcde/",                      // 非法字符
            "0123456789abcde ",                      // 空格
            "0123456789abcde中",                     // 非 ASCII 字节长度不符
            ".0123456789abcde",                      // 以点开头
            "-0123456789abcde",                      // 以连字符开头
        ] {
            assert!(validate(bad).is_err(), "应拒绝 {bad:?}");
        }
        validate("aB3-._~aB3-._~aB").unwrap();
    }

    #[test]
    fn 常量时间比较只认可完全相等() {
        let value = "aB3-._~aB3-._~aB";
        assert_eq!(value.len(), LENGTH);
        assert!(matches(value, value));
        for bad in [
            "",
            "aB3-._~aB3-._~aC",
            "aB3-._~aB3-._~a",
            "aB3-._~aB3-._~aBB",
            "AB3-._~aB3-._~aB",
        ] {
            assert!(!matches(value, bad), "应拒绝 {bad:?}");
        }
    }
    #[test]
    fn 分享路径按前缀与令牌段解析() {
        assert_eq!(share_token("/share/abc/"), Some("abc"));
        assert_eq!(share_token("/share/abc"), Some("abc"));
        assert_eq!(share_token("/share/abc/data/nodes"), Some("abc"));
        assert_eq!(share_token("/share/abc/api/v1/public/metrics"), Some("abc"));
        // 缺少令牌段
        assert_eq!(share_token("/share/"), None);
        assert_eq!(share_token("/share"), None);
        // 其他首段一概不认，安全入口不会被当成分享令牌
        assert_eq!(share_token("/other/abc"), None);
        assert_eq!(share_token("/abc/api/v1/nodes"), None);
        assert_eq!(share_token("/"), None);
    }

    #[test]
    fn 首段提取忽略查询串与多余层级() {
        assert_eq!(first_segment("/abc"), Some("abc"));
        assert_eq!(first_segment("/abc/"), Some("abc"));
        assert_eq!(first_segment("/abc/api/v1/nodes"), Some("abc"));
        assert_eq!(first_segment("/"), None);
        assert_eq!(first_segment(""), None);
        // 前导双斜杠按协议相对地址处理时，首段为空
        assert_eq!(first_segment("//abc"), None);
    }

    #[tokio::test]
    async fn 错误入口按来源限速() {
        let limits = Limits::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        for _ in 0..FAILURE_LIMIT {
            assert!(!limits.record_failure(ip).await);
        }
        assert!(limits.record_failure(ip).await);
        limits.clear(ip).await;
        assert!(!limits.record_failure(ip).await);
    }
}
