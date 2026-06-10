//! Command-line adapter for the retail workflow shell.

use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::config::AppConfig;

/// Command-line failures.
#[derive(Debug, Error)]
pub enum CliError {
    /// Configuration loading failed.
    #[error("configuration error: {0}")]
    Config(#[from] ::config::ConfigError),
    /// A command argument failed interface validation.
    #[error("{field} must be greater than zero")]
    NonPositiveArgument {
        /// Invalid field.
        field: &'static str,
    },
    /// A parsed command has no implementation in this checkpoint branch.
    #[error("{command} is parsed but not implemented until a later tutorial branch")]
    PlaceholderCommand {
        /// Placeholder command name.
        command: &'static str,
    },
    /// Help rendering failed.
    #[error("failed to render command help")]
    RenderHelp(#[source] std::io::Error),
    /// User-facing output could not be written.
    #[error("failed to write command output")]
    WriteOutput(#[source] std::io::Error),
}

impl CliError {
    /// Return the process exit code for this interface failure.
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NonPositiveArgument { .. } => ExitCode::from(2),
            Self::Config(_)
            | Self::PlaceholderCommand { .. }
            | Self::RenderHelp(_)
            | Self::WriteOutput(_) => ExitCode::from(1),
        }
    }
}

pub(crate) async fn render_default_help<W>(output: &mut W) -> Result<(), CliError>
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

/// Command-line arguments for the retail replenishment workflow.
#[derive(Debug, Parser)]
#[command(author, version, about = "Retail replenishment workflow agent")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub(crate) fn parse_args() -> Self {
        Self::parse()
    }

    #[must_use]
    pub(crate) const fn into_command(self) -> Option<Command> {
        self.command
    }
}

/// Retail workflow subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Seed retail state from the configured scenario.
    Seed(SeedArgs),
    /// Advance the deterministic retail simulation.
    Simulate(SimulateArgs),
    /// Run one restock decision.
    Decide(DecideArgs),
    /// Run repeated simulation and decision cycles.
    RunCycle(RunCycleArgs),
}

impl Command {
    #[must_use]
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Seed(_) => "seed",
            Self::Simulate(_) => "simulate",
            Self::Decide(_) => "decide",
            Self::RunCycle(_) => "run-cycle",
        }
    }

    pub(crate) fn validate(&self, config: &AppConfig) -> Result<(), CliError> {
        match self {
            Self::Seed(_) => Ok(()),
            Self::Simulate(args) => (*args).validate(),
            Self::Decide(args) => (*args).validate(config),
            Self::RunCycle(args) => (*args).validate(config),
        }
    }
}

/// Seed command arguments.
#[derive(Debug, Clone, Copy, Parser)]
pub(crate) struct SeedArgs {
    /// Replace existing retail state before seeding.
    #[arg(long)]
    reset: bool,
}

/// Simulation command arguments.
#[derive(Debug, Clone, Copy, Parser)]
pub(crate) struct SimulateArgs {
    /// Number of simulated days to advance.
    #[arg(long)]
    days: u64,
}

impl SimulateArgs {
    pub(crate) const fn validate(self) -> Result<(), CliError> {
        require_positive("days", self.days)
    }
}

/// Decision command arguments.
#[derive(Debug, Clone, Copy, Parser)]
pub(crate) struct DecideArgs {
    /// Demand horizon in days for this decision.
    #[arg(long)]
    horizon_days: Option<u64>,
}

impl DecideArgs {
    pub(crate) fn validate(self, config: &AppConfig) -> Result<(), CliError> {
        let horizon_days = self
            .horizon_days
            .map_or(config.decision_horizon_days, |days| days);
        require_positive("horizon_days", horizon_days)?;
        require_positive(
            "max_restock_orders_per_decision",
            config.max_restock_orders_per_decision,
        )
    }
}

/// Workflow-cycle command arguments.
#[derive(Debug, Clone, Copy, Parser)]
pub(crate) struct RunCycleArgs {
    /// Total simulated days to advance.
    #[arg(long)]
    days: u64,
    /// Decision cadence in simulated days.
    #[arg(long)]
    decision_interval_days: u64,
}

impl RunCycleArgs {
    pub(crate) fn validate(self, config: &AppConfig) -> Result<(), CliError> {
        require_positive("days", self.days)?;
        require_positive("decision_interval_days", self.decision_interval_days)?;
        require_positive("decision_horizon_days", config.decision_horizon_days)?;
        require_positive(
            "max_restock_orders_per_decision",
            config.max_restock_orders_per_decision,
        )
    }
}

const fn require_positive(field: &'static str, value: u64) -> Result<(), CliError> {
    if value == 0 {
        return Err(CliError::NonPositiveArgument { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, CliError, Command, DecideArgs, RunCycleArgs, SimulateArgs, render_default_help,
    };
    use crate::config::AppConfig;

    #[tokio::test]
    async fn no_subcommand_prints_help_without_mutating_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();

        render_default_help(&mut output).await?;

        let help = String::from_utf8(output)?;
        assert!(help.contains("Retail replenishment workflow agent"));
        assert!(help.contains("seed"));
        Ok(())
    }

    #[test]
    fn simulate_rejects_zero_days() {
        let result = SimulateArgs { days: 0 }.validate();

        assert!(matches!(
            result,
            Err(CliError::NonPositiveArgument { field: "days" })
        ));
    }

    #[test]
    fn decide_rejects_zero_default_horizon() {
        let mut config = test_config();
        config.decision_horizon_days = 0;
        let result = DecideArgs { horizon_days: None }.validate(&config);

        assert!(matches!(
            result,
            Err(CliError::NonPositiveArgument {
                field: "horizon_days"
            })
        ));
    }

    #[test]
    fn run_cycle_rejects_zero_decision_interval() {
        let result = RunCycleArgs {
            days: 7,
            decision_interval_days: 0,
        }
        .validate(&test_config());

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

        assert!(matches!(cli.into_command(), Some(Command::RunCycle(_))));
        Ok(())
    }

    fn test_config() -> AppConfig {
        AppConfig {
            openai_api_key: None,
            chat_model: "gpt-test".to_owned(),
            retail_db_path: "data/test.sqlite".into(),
            retail_scenario_path: "data/retail_scenario.yaml".into(),
            decision_horizon_days: 14,
            max_restock_orders_per_decision: 2,
        }
    }
}
