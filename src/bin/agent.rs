#[path = "../agent/mod.rs"]
mod agent;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    opsd::init_logging();
    agent::run().await
}
