use crate::{entrance, pki};
use anyhow::Result;
use axum::{Extension, Router};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use std::{net::SocketAddr, path::Path, sync::Arc};

#[derive(Clone)]
pub struct Peer {
    pub certificate: Option<String>,
    pub address: SocketAddr,
}

/// 浏览器面的入口门禁配置。`None` 表示该监听不做入口校验（例如 Agent 通道）。
#[derive(Clone)]
pub struct Gate {
    pub entrance: entrance::Shared,
    pub limits: entrance::Limits,
    /// 精确匹配的豁免路径，见 [`entrance::AGENT_PATHS`]。
    pub allow: Vec<String>,
    /// 分享令牌校验回调；返回 false 即与错误入口一样丢弃连接。
    pub share: Option<std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

pub async fn serve(
    address: SocketAddr,
    dir: &Path,
    mtls: bool,
    router: Router,
    gate: Option<Gate>,
) -> Result<()> {
    let builder = rustls::ServerConfig::builder();
    let builder = if mtls {
        let mut roots = rustls::RootCertStore::empty();
        for c in pki::certs(&dir.join("ca.pem"))? {
            roots.add(c)?;
        }
        builder.with_client_cert_verifier(
            rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build()?,
        )
    } else {
        builder.with_no_client_auth()
    };
    let config = builder.with_single_cert(
        pki::certs(&dir.join("server.pem"))?,
        pki::key(&dir.join("server-key.pem"))?,
    )?;
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address,mtls,gated=gate.is_some(),"监听已启动");
    loop {
        let (stream, address) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let router = router.clone();
        let gate = gate.clone();
        tokio::spawn(async move {
            let Ok(Ok(stream)) =
                tokio::time::timeout(std::time::Duration::from_secs(10), acceptor.accept(stream))
                    .await
            else {
                return;
            };
            let certificate = stream
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|c| c.first())
                .map(|c| crate::protocol::digest(c.as_ref()));
            let router = router.layer(Extension(Peer {
                certificate,
                address,
            }));
            let service = match gate {
                Some(gate) => {
                    let allow: Vec<&str> = gate.allow.iter().map(String::as_str).collect();
                    let service = entrance::Gate::new(
                        router,
                        Some(gate.entrance),
                        gate.limits,
                        address,
                    )
                    .allow(&allow);
                    match gate.share {
                        Some(check) => service.share(check),
                        None => service,
                    }
                }
                None => entrance::Gate::new(router, None, Default::default(), address),
            };
            if let Err(e) = Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(
                    TokioIo::new(stream),
                    TowerToHyperService::new(service),
                )
                .await
            {
                tracing::debug!(error=%e,"连接结束");
            }
        });
    }
}

/// 健康检查专用的明文监听。只应绑定回环地址：它不经过入口门禁，
/// 因此既不能被门禁遮蔽，也不能成为绕过门禁的探针。
pub async fn serve_plain(address: SocketAddr, router: Router) -> Result<()> {
    anyhow::ensure!(
        address.ip().is_loopback(),
        "健康检查监听必须绑定回环地址，避免成为绕过入口的门路"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address,"健康检查监听已启动（仅回环）");
    loop {
        let (stream, _) = listener.accept().await?;
        let router = router.clone();
        tokio::spawn(async move {
            if let Err(e) = Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), TowerToHyperService::new(router))
                .await
            {
                tracing::debug!(error=%e,"健康检查连接结束");
            }
        });
    }
}
