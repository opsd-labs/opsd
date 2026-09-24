//! 安全入口的行为约束。这些断言对应入口最核心的承诺：
//! 不匹配时不产生任何 HTTP 响应，匹配时才进入正常处理。
use axum::{Router, body::Body, http::Request, routing::get};
use opsd::entrance::{self, Gate, Limits};
use std::sync::{Arc, RwLock};
use tower::ServiceExt;

const ENTRANCE: &str = "aB3-._~aB3-._~aB";

fn gate(value: Option<&str>) -> Gate<Router> {
    let router = Router::new()
        .route("/", get(|| async { "console" }))
        .route("/{*rest}", get(|| async { "console" }));
    let shared = value.map(|v| Arc::new(RwLock::new(v.to_owned())));
    Gate::new(
        router,
        shared,
        Limits::default(),
        "203.0.113.7:40000".parse().unwrap(),
    )
}

async fn status(value: Option<&str>, path: &str) -> Result<u16, std::io::Error> {
    // Gate 不匹配时返回 Err，调用方（hyper）据此直接关闭连接而不写响应。
    let response = gate(value)
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await?;
    Ok(response.status().as_u16())
}

#[tokio::test]
async fn 不匹配的入口不产生任何响应() {
    for path in [
        "/",
        "/wrongentrance00/",
        "/wrongentrance00/api/v1/nodes",
        // 正确入口的前缀截断：不能因为前缀相同就放行
        "/aB3-._~aB3-._~a/",
        "/aB3-._~aB3-._~aC/",
        "/aB3-._~aB3-._~aBB/",
        // 大小写不同也不放行
        "/AB3-._~aB3-._~aB/",
        // 协议相对地址：首段为空，必须丢弃
        "//aB3-._~aB3-._~aB/",
    ] {
        let result = status(Some(ENTRANCE), path).await;
        assert!(
            result.is_err(),
            "{path} 必须被丢弃，实际返回了 {}",
            result.unwrap_or_default()
        );
    }
}

#[tokio::test]
async fn 匹配的入口正常进入处理() {
    for path in [
        "/aB3-._~aB3-._~aB",
        "/aB3-._~aB3-._~aB/",
        "/aB3-._~aB3-._~aB/api/v1/nodes",
        "/aB3-._~aB3-._~aB/assets/index.js",
    ] {
        assert_eq!(
            status(Some(ENTRANCE), path).await.unwrap(),
            200,
            "{path} 应当正常处理"
        );
    }
}

#[tokio::test]
async fn 未配置入口时不拦截() {
    // Agent 通道等不设入口的监听必须完全不受影响。
    assert_eq!(status(None, "/").await.unwrap(), 200);
    assert_eq!(status(None, "/agent").await.unwrap(), 200);
}

#[tokio::test]
async fn 运行期修改入口后旧地址立即失效() {
    let shared: entrance::Shared = Arc::new(RwLock::new(ENTRANCE.to_owned()));
    let router = Router::new().route("/{*rest}", get(|| async { "console" }));
    let build = || Gate::new(router.clone(), Some(shared.clone()), Limits::default(), "203.0.113.7:40000".parse().unwrap());
    let request = |path: &str| {
        Request::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap()
    };
    assert!(build().oneshot(request(&format!("/{ENTRANCE}/"))).await.is_ok());
    let next = entrance::generate();
    *shared.write().unwrap() = next.clone();
    // 旧地址失效
    assert!(build().oneshot(request(&format!("/{ENTRANCE}/"))).await.is_err());
    // 新地址生效，不需要重启
    assert!(build().oneshot(request(&format!("/{next}/"))).await.is_ok());
}
