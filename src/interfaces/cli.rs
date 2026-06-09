//! Command-line adapter for the retail workflow.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use diesel::SqliteConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::application::retail::{
    AdvanceSimulation, AdvanceSimulationResult, ApplicationError, DecisionAgentRequest,
    DecisionAgentResponse, ReplenishmentDecisionAgent, RetailWorkflow, RunRestockDecision,
    RunWorkflowCycle, SeedRetailScenario, WorkflowCycleResult,
};
use crate::config::AppConfig;
use crate::domain::retail::{DecisionHorizonDays, DomainError, StockQuantity};
use crate::infrastructure::agents::rig_replenishment::RigReplenishmentDecisionAgent;
use crate::infrastructure::ids::UuidRetailIdGenerator;
use crate::infrastructure::persistence::{
    DieselDecisionRunStore, DieselRetailStore, InfrastructureError, create_pool, run_migrations,
};

type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

/// Command-line failures.
#[derive(Debug, Error)]
pub enum CliError {
    /// Configuration loading failed.
    #[error("configuration error: {0}")]
    Config(#[from] ::config::ConfigError),
    /// Application use-case execution failed.
    #[error("{0}")]
    Application(#[from] ApplicationError),
    /// A command argument failed interface validation.
    #[error("{field} must be greater than zero")]
    NonPositiveArgument {
        /// Invalid field.
        field: &'static str,
    },
    /// A required provider key was missing for a decision command.
    #[error("OPENAI_API_KEY is required for decide and run-cycle commands")]
    MissingOpenAiKey,
    /// A configured path could not be used as a `SQLite` database URL.
    #[error("path {path} is not valid UTF-8")]
    NonUtf8Path {
        /// Invalid path.
        path: PathBuf,
    },
    /// Filesystem setup failed.
    #[error("filesystem error during {operation}")]
    Filesystem {
        /// Failed operation.
        operation: &'static str,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// Help rendering failed.
    #[error("failed to render command help")]
    RenderHelp(#[source] std::io::Error),
    /// User-facing output could not be written.
    #[error("failed to write command output")]
    WriteOutput(#[source] std::io::Error),
}

impl From<InfrastructureError> for CliError {
    fn from(error: InfrastructureError) -> Self {
        Self::Application(ApplicationError::from(error))
    }
}

impl From<DomainError> for CliError {
    fn from(error: DomainError) -> Self {
        Self::Application(ApplicationError::from(error))
    }
}

impl CliError {
    /// Return the process exit code for this interface failure.
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NonPositiveArgument { .. } | Self::MissingOpenAiKey => ExitCode::from(2),
            Self::Config(_)
            | Self::Application(_)
            | Self::NonUtf8Path { .. }
            | Self::Filesystem { .. }
            | Self::RenderHelp(_)
            | Self::WriteOutput(_) => ExitCode::from(1),
        }
    }
}

/// Run the command-line workflow from process arguments.
///
/// # Errors
///
/// Returns an error when configuration, adapter setup, command execution, or output fails.
pub async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let mut stdout = tokio::io::stdout();
    execute(cli, &mut stdout).await
}

async fn execute<W>(cli: Cli, output: &mut W) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let Some(command) = cli.command else {
        render_default_help(output).await?;
        return Ok(());
    };

    let config = AppConfig::load()?;
    match command {
        Command::Seed(args) => run_seed(args, &config, output).await,
        Command::Simulate(args) => run_simulate(args, &config, output).await,
        Command::Decide(args) => run_decide(args, &config, output).await,
        Command::RunCycle(args) => run_cycle(args, &config, output).await,
    }
}

async fn run_seed<W>(args: SeedArgs, config: &AppConfig, output: &mut W) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_without_agent(pool);
    let command = args.command(config);
    workflow.seed_scenario(&command)?;
    write_line(
        output,
        format!(
            "seeded retail state from {} (reset: {})",
            command.scenario_path.display(),
            command.reset
        ),
    )
    .await
}

async fn run_simulate<W>(
    args: SimulateArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let command = args.command()?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_without_agent(pool);
    let result = workflow.advance_simulation(command)?;
    write_simulation_result(output, result).await
}

async fn run_decide<W>(args: DecideArgs, config: &AppConfig, output: &mut W) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let command = args.command(config)?;
    let openai_api_key = required_openai_key(config)?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_with_agent(pool, config, openai_api_key);
    let result = workflow.run_restock_decision(command).await?;
    write_line(
        output,
        format!(
            "decision {} accepted {} order(s), rejected {} proposal(s): {}",
            result.decision_run_id,
            result.accepted_orders.len(),
            result.rejected_proposals.len(),
            result.summary
        ),
    )
    .await
}

async fn run_cycle<W>(
    args: RunCycleArgs,
    config: &AppConfig,
    output: &mut W,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let command = args.command(config)?;
    let openai_api_key = required_openai_key(config)?;
    let pool = migrated_pool(config)?;
    let mut workflow = workflow_with_agent(pool, config, openai_api_key);
    let result = workflow.run_workflow_cycle(command).await?;
    write_cycle_result(output, result).await
}

fn workflow_without_agent(
    pool: SqlitePool,
) -> RetailWorkflow<
    DieselRetailStore,
    DieselDecisionRunStore,
    UnavailableDecisionAgent,
    UuidRetailIdGenerator,
    (),
> {
    RetailWorkflow::new(
        DieselRetailStore::from_pool(pool.clone()),
        DieselDecisionRunStore::from_pool(pool),
        UnavailableDecisionAgent,
        UuidRetailIdGenerator::new(),
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
    (),
> {
    RetailWorkflow::new(
        DieselRetailStore::from_pool(pool.clone()),
        DieselDecisionRunStore::from_pool(pool),
        RigReplenishmentDecisionAgent::new(openai_api_key, config.chat_model.clone()),
        UuidRetailIdGenerator::new(),
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

fn required_openai_key(config: &AppConfig) -> Result<String, CliError> {
    config
        .openai_api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or(CliError::MissingOpenAiKey)
}

async fn render_default_help<W>(output: &mut W) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let mut command = Cli::command();
    let mut help = Vec::new();
    command
        .write_help(&mut help)
        .map_err(CliError::RenderHelp)?;
    output.write_all(&help).await.map_err(CliError::WriteOutput)
}

async fn write_line<W>(output: &mut W, line: String) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    output
        .write_all(line.as_bytes())
        .await
        .map_err(CliError::WriteOutput)?;
    output.write_all(b"\n").await.map_err(CliError::WriteOutput)
}

async fn write_simulation_result<W>(
    output: &mut W,
    result: AdvanceSimulationResult,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    write_line(
        output,
        format!(
            "advanced {} day(s) to {}; received {} restock order(s), recorded {} sale(s), lost {} unit(s)",
            result.days_advanced,
            result.current_date,
            result.received_restock_count.units(),
            result.sales_order_count.units(),
            result.lost_units.units()
        ),
    )
    .await
}

async fn write_cycle_result<W>(output: &mut W, result: WorkflowCycleResult) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    write_line(
        output,
        format!(
            "advanced {} day(s), ran {} decision(s), final date {}",
            result.days_advanced,
            result.decision_runs.units(),
            result.final_date
        ),
    )
    .await
}

/// Command-line arguments for the retail replenishment workflow.
#[derive(Debug, Parser)]
#[command(author, version, about = "Retail replenishment workflow agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Retail workflow subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Seed retail state from the configured scenario.
    Seed(SeedArgs),
    /// Advance the deterministic retail simulation.
    Simulate(SimulateArgs),
    /// Run one restock decision.
    Decide(DecideArgs),
    /// Run repeated simulation and decision cycles.
    RunCycle(RunCycleArgs),
}

/// Seed command arguments.
#[derive(Debug, Clone, Parser)]
struct SeedArgs {
    /// Replace existing retail state before seeding.
    #[arg(long)]
    reset: bool,
}

impl SeedArgs {
    fn command(&self, config: &AppConfig) -> SeedRetailScenario {
        SeedRetailScenario {
            scenario_path: config.retail_scenario_path.clone(),
            reset: self.reset,
        }
    }
}

/// Simulation command arguments.
#[derive(Debug, Clone, Copy, Parser)]
struct SimulateArgs {
    /// Number of simulated days to advance.
    #[arg(long)]
    days: u64,
}

impl SimulateArgs {
    const fn command(self) -> Result<AdvanceSimulation, CliError> {
        if self.days == 0 {
            return Err(CliError::NonPositiveArgument { field: "days" });
        }
        Ok(AdvanceSimulation { days: self.days })
    }
}

/// Decision command arguments.
#[derive(Debug, Clone, Copy, Parser)]
struct DecideArgs {
    /// Demand horizon in days for this decision.
    #[arg(long)]
    horizon_days: Option<u64>,
}

impl DecideArgs {
    fn command(self, config: &AppConfig) -> Result<RunRestockDecision, CliError> {
        let horizon_days = self.horizon_days.unwrap_or(config.decision_horizon_days);
        Ok(RunRestockDecision {
            horizon: DecisionHorizonDays::try_from(horizon_days)?,
            max_restock_orders: StockQuantity::new(config.max_restock_orders_per_decision),
        })
    }
}

/// Workflow-cycle command arguments.
#[derive(Debug, Clone, Copy, Parser)]
struct RunCycleArgs {
    /// Total simulated days to advance.
    #[arg(long)]
    days: u64,
    /// Decision cadence in simulated days.
    #[arg(long)]
    decision_interval_days: u64,
}

impl RunCycleArgs {
    fn command(self, config: &AppConfig) -> Result<RunWorkflowCycle, CliError> {
        if self.days == 0 {
            return Err(CliError::NonPositiveArgument { field: "days" });
        }
        if self.decision_interval_days == 0 {
            return Err(CliError::NonPositiveArgument {
                field: "decision_interval_days",
            });
        }
        Ok(RunWorkflowCycle {
            total_days: self.days,
            decision_interval_days: self.decision_interval_days,
            horizon: DecisionHorizonDays::try_from(config.decision_horizon_days)?,
            max_restock_orders: StockQuantity::new(config.max_restock_orders_per_decision),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct UnavailableDecisionAgent;

impl ReplenishmentDecisionAgent for UnavailableDecisionAgent {
    async fn decide(
        &self,
        _request: DecisionAgentRequest,
    ) -> Result<DecisionAgentResponse, ApplicationError> {
        Err(ApplicationError::AgentFailure {
            message: "decision agent was not configured for this command".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, CliError, Command, RunCycleArgs, SeedArgs, SimulateArgs, execute, required_openai_key,
    };
    use crate::config::AppConfig;

    #[tokio::test]
    async fn no_subcommand_prints_help_without_mutating_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli { command: None };
        let mut output = Vec::new();

        execute(cli, &mut output).await?;

        let help = String::from_utf8(output)?;
        assert!(help.contains("Retail replenishment workflow agent"));
        assert!(help.contains("seed"));
        Ok(())
    }

    #[test]
    fn seed_maps_reset_flag_to_application_command() {
        let config = test_config(None);
        let command = SeedArgs { reset: true }.command(&config);

        assert_eq!(command.scenario_path, config.retail_scenario_path);
        assert!(command.reset);
    }

    #[test]
    fn simulate_rejects_zero_days() {
        let result = SimulateArgs { days: 0 }.command();

        assert!(matches!(
            result,
            Err(CliError::NonPositiveArgument { field: "days" })
        ));
    }

    #[test]
    fn decide_requires_openai_key() {
        let config = test_config(None);

        assert!(matches!(
            required_openai_key(&config),
            Err(CliError::MissingOpenAiKey)
        ));
    }

    #[test]
    fn run_cycle_rejects_zero_decision_interval() {
        let config = test_config(Some("sk-test".to_owned()));
        let result = RunCycleArgs {
            days: 7,
            decision_interval_days: 0,
        }
        .command(&config);

        assert!(matches!(
            result,
            Err(CliError::NonPositiveArgument {
                field: "decision_interval_days"
            })
        ));
    }

    #[test]
    fn parses_expected_subcommands() -> Result<(), Box<dyn std::error::Error>> {
        let cli = <Cli as clap::Parser>::try_parse_from([
            "rigagent",
            "run-cycle",
            "--days",
            "14",
            "--decision-interval-days",
            "7",
        ])?;

        assert!(matches!(cli.command, Some(Command::RunCycle(_))));
        Ok(())
    }

    fn test_config(openai_api_key: Option<String>) -> AppConfig {
        AppConfig {
            openai_api_key,
            chat_model: "gpt-test".to_owned(),
            retail_db_path: "data/test.sqlite".into(),
            retail_scenario_path: "data/retail_scenario.yaml".into(),
            decision_horizon_days: 14,
            max_restock_orders_per_decision: 2,
        }
    }
}
