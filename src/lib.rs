//! Retail replenishment workflow runtime.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    missing_docs
)]
#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]

use tracing_subscriber::EnvFilter;

use std::path::Path;

use diesel::SqliteConnection;
use diesel::r2d2::{ConnectionManager, Pool};

use crate::application::retail::{
    ApplicationError, DecisionAgentRequest, DecisionAgentResponse, ReplenishmentDecisionAgent,
    RetailWorkflow,
};
use crate::config::AppConfig;
use crate::infrastructure::agents::rig_replenishment::RigReplenishmentDecisionAgent;
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::ids::UuidRetailIdGenerator;
use crate::infrastructure::persistence::{
    DieselDecisionRunStore, DieselRetailStore, create_pool, run_migrations,
};
use crate::interfaces::cli::{CliError, Command};

/// Application use cases for the retail workflow.
pub mod application;

/// Runtime configuration for the retail workflow.
pub mod config;

/// Core business behavior for the retail workflow.
pub mod domain;

/// External adapters for the retail workflow.
pub mod infrastructure;

/// Inbound adapters for the retail workflow.
pub mod interfaces;

type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

/// Run the retail replenishment workflow binary.
///
/// # Errors
///
/// Returns an error when command parsing, configuration, adapter setup, or execution fails.
pub async fn run() -> Result<(), CliError> {
    dotenvy::dotenv().ok();

    init_tracing();

    let cli = interfaces::cli::Cli::parse_args();
    let mut stdout = tokio::io::stdout();
    let Some(command) = cli.into_command() else {
        interfaces::cli::render_default_help(&mut stdout).await?;
        return Ok(());
    };

    let config = AppConfig::load()?;
    match command {
        Command::Seed(args) => run_seed(args, &config, &mut stdout).await,
        Command::Status => run_status(&config, &mut stdout).await,
        Command::Simulate(args) => run_simulate(args, &config, &mut stdout).await,
        Command::Decide(args) => run_decide(args, &config, &mut stdout).await,
        Command::RunCycle(args) => run_cycle(args, &config, &mut stdout).await,
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();
}

async fn run_seed<W>(
    args: interfaces::cli::SeedArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_without_agent(pool);
    let command = args.command(config);
    workflow.seed_scenario(&command)?;
    interfaces::cli::write_seed_result(output, &command).await
}

async fn run_status<W>(config: &AppConfig, output: &mut W) -> Result<(), CliError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let pool = migrated_pool(config)?;
    let workflow = workflow_without_agent(pool);
    let snapshot = workflow.get_snapshot()?;
    interfaces::cli::write_status_result(output, &snapshot).await
}

async fn run_simulate<W>(
    args: interfaces::cli::SimulateArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let command = args.command()?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_without_agent(pool);
    let result = workflow.advance_simulation(command)?;
    interfaces::cli::write_simulation_result(output, result).await
}

async fn run_decide<W>(
    args: interfaces::cli::DecideArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let command = args.command(config)?;
    let openai_api_key = interfaces::cli::required_openai_key(config)?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_with_agent(pool, config, openai_api_key);
    let result = workflow.run_restock_decision(command).await?;
    interfaces::cli::write_decision_result(output, result).await
}

async fn run_cycle<W>(
    args: interfaces::cli::RunCycleArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let command = args.command(config)?;
    let openai_api_key = interfaces::cli::required_openai_key(config)?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_with_agent(pool, config, openai_api_key);
    let result = workflow.run_workflow_cycle(command).await?;
    interfaces::cli::write_cycle_result(output, result).await
}

fn workflow_without_agent(
    pool: SqlitePool,
) -> RetailWorkflow<
    DieselRetailStore,
    DieselDecisionRunStore,
    UnavailableDecisionAgent,
    UuidRetailIdGenerator,
    SystemClock,
> {
    RetailWorkflow::new(
        DieselRetailStore::from_pool(pool.clone()),
        DieselDecisionRunStore::from_pool(pool),
        UnavailableDecisionAgent,
        UuidRetailIdGenerator::new(),
        SystemClock::new(),
    )
}

fn workflow_with_agent(
    pool: SqlitePool,
    config: &AppConfig,
    openai_api_key: String,
) -> RetailWorkflow<
    DieselRetailStore,
    DieselDecisionRunStore,
    RigReplenishmentDecisionAgent,
    UuidRetailIdGenerator,
    SystemClock,
> {
    RetailWorkflow::new(
        DieselRetailStore::from_pool(pool.clone()),
        DieselDecisionRunStore::from_pool(pool),
        RigReplenishmentDecisionAgent::new(openai_api_key, config.chat_model.clone()),
        UuidRetailIdGenerator::new(),
        SystemClock::new(),
    )
}

fn migrated_pool(config: &AppConfig) -> Result<SqlitePool, CliError> {
    ensure_database_parent(config.retail_db_path.as_path())?;
    let pool = create_pool(database_url(config.retail_db_path.as_path())?)?;
    run_migrations(&pool)?;
    Ok(pool)
}

fn database_url(path: &Path) -> Result<String, CliError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| CliError::NonUtf8Path {
            path: path.to_path_buf(),
        })
}

fn ensure_database_parent(path: &Path) -> Result<(), CliError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    std::fs::create_dir_all(parent).map_err(|source| CliError::Filesystem {
        operation: "create database directory",
        source,
    })
}

#[derive(Debug, Clone, Copy)]
struct UnavailableDecisionAgent;

impl ReplenishmentDecisionAgent for UnavailableDecisionAgent {
    async fn decide(
        &self,
        _request: DecisionAgentRequest,
    ) -> Result<DecisionAgentResponse, ApplicationError> {
        Err(ApplicationError::agent_failure_message(
            "decision agent was not configured for this command",
        ))
    }
}
