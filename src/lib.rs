pub mod entrance;
pub mod pki;
pub mod protocol;
pub mod store;
pub mod transport;

pub fn init_logging() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opsd=info".into()),
        )
        .init();
}
