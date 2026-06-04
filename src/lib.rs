mod config;
mod rag;
mod repl;

pub mod knowledge;
pub mod tools;

use anyhow::Context;
use clap::Parser;
use rig::client::{CompletionClient, EmbeddingsClient};
use rig::providers::openai;
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::AppConfig;

#[derive(Debug, Parser)]
#[command(author, version, about = "Interactive Rig support agent demo")]
struct Cli {
    /// Rebuild the SQLite vector database from data/knowledge_base.json.
    #[arg(long)]
    reindex: bool,
}

pub async fn run() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    init_tracing();

    let cli = Cli::parse();
    let config = AppConfig::load()?;

    info!(
        chat_model = %config.chat_model,
        embedding_model = %config.embedding_model,
        db_path = %config.rag_db_path.display(),
        top_k = config.rag_top_k.get(),
        "starting support agent"
    );

    let openai_client = openai::Client::new(config.openai_api_key.clone())
        .context("failed to initialize OpenAI client")?;
    let embedding_model = openai_client.embedding_model(config.embedding_model.clone());
    let index = rag::prepare_index(&config, embedding_model, cli.reindex).await?;

    let agent = openai_client
        .agent(config.chat_model.clone())
        .preamble(repl::AGENT_PREAMBLE)
        .dynamic_context(config.rag_top_k.get(), index)
        .tool(tools::LookupOrderStatus)
        .tool(tools::ListOrders)
        .build();

    repl::run(agent).await?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
