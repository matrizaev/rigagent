#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rigagent::run().await
}
