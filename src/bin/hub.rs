#[path = "../hub/mod.rs"]
mod hub;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    opsd::init_logging();
    hub::run().await
}
