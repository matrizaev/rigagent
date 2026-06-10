//! Command-line adapter for the retail workflow.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::application::retail::{
    AcceptedRestockOrder, AdvanceSimulation, AdvanceSimulationResult, ApplicationError,
    DecisionResult, RejectedRestockProposal, RetailSnapshot, RunRestockDecision, RunWorkflowCycle,
    SeedRetailScenario, WorkflowCycleResult,
};
use crate::config::AppConfig;
use crate::domain::retail::{
    DecisionHorizonDays, DomainError, InventoryPosition, MoneyCents, Product, SpaceUnits,
    StockQuantity,
};
use crate::infrastructure::persistence::InfrastructureError;

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

pub(crate) fn required_openai_key(config: &AppConfig) -> Result<String, CliError> {
    config
        .openai_api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or(CliError::MissingOpenAiKey)
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

pub(crate) async fn write_seed_result<W>(
    output: &mut W,
    command: &SeedRetailScenario,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
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

pub(crate) async fn write_simulation_result<W>(
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
            result.received_restock_count.count(),
            result.sales_order_count.count(),
            result.lost_units.units()
        ),
    )
    .await
}

pub(crate) async fn write_decision_result<W>(
    output: &mut W,
    result: DecisionResult,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    write_line(
        output,
        format!(
            "decision {} accepted {} order(s), rejected {} proposal(s)",
            result.decision_run_id,
            result.accepted_orders.len(),
            result.rejected_proposals.len(),
        ),
    )
    .await?;

    if !result.accepted_orders.is_empty() {
        write_line(output, "accepted orders:".to_owned()).await?;
        for accepted in &result.accepted_orders {
            write_line(output, format_accepted_order(accepted)).await?;
        }
    }

    if !result.rejected_proposals.is_empty() {
        write_line(output, "rejected proposals:".to_owned()).await?;
        for rejected in &result.rejected_proposals {
            write_line(output, format_rejected_proposal(rejected)).await?;
        }
    }

    let summary = result.summary.trim();
    if !summary.is_empty() && result.rejected_proposals.is_empty() {
        write_line(output, format!("agent summary: {summary}")).await?;
    } else if !summary.is_empty() {
        write_line(
            output,
            "agent summary omitted because one or more proposals failed application validation"
                .to_owned(),
        )
        .await?;
    }

    Ok(())
}

fn format_accepted_order(accepted: &AcceptedRestockOrder) -> String {
    format!(
        "- {}: {} unit(s), eta {}, rationale: {}",
        accepted.order.sku(),
        accepted.order.quantity().units(),
        accepted.order.eta(),
        accepted.order.rationale()
    )
}

fn format_rejected_proposal(rejected: &RejectedRestockProposal) -> String {
    format!(
        "- {}: {} unit(s), reason: {}",
        rejected.sku,
        rejected.quantity.units(),
        rejected.reason
    )
}

pub(crate) async fn write_cycle_result<W>(
    output: &mut W,
    result: WorkflowCycleResult,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    write_line(
        output,
        format!(
            "advanced {} day(s), ran {} decision(s), final date {}",
            result.days_advanced,
            result.decision_runs.count(),
            result.final_date
        ),
    )
    .await
}

pub(crate) async fn write_status_result<W>(
    output: &mut W,
    snapshot: &RetailSnapshot,
) -> Result<(), CliError>
where
    W: AsyncWrite + Unpin,
{
    let occupied_space = occupied_space(snapshot)?;
    let profit = snapshot.profit_summary;
    write_line(output, format!("status date {}", snapshot.current_date)).await?;
    write_line(
        output,
        format!(
            "capacity: {}/{} space unit(s) used",
            occupied_space.units(),
            snapshot.capacity.units()
        ),
    )
    .await?;
    write_line(
        output,
        format!(
            "profit: revenue {}, cost {}, gross {}, lost {} unit(s)",
            format_money(profit.revenue)?,
            format_money(profit.cost)?,
            format_money(profit.gross_profit)?,
            profit.lost_units.units()
        ),
    )
    .await?;
    write_line(output, "inventory:".to_owned()).await?;
    for product in &snapshot.products {
        let inventory = inventory_for(&snapshot.inventory, product)?;
        let inbound = open_inbound_quantity(snapshot, product)?;
        write_line(
            output,
            format!(
                "- {} {} {} {}: on hand {}, cover {}, inbound {}, demand {}/day, backlog {}",
                product.sku(),
                product.brand(),
                product.kind(),
                product.size(),
                inventory.on_hand().units(),
                format_days_of_cover(inventory.on_hand(), product)?,
                inbound.units(),
                format_milli_units(product.demand_rate().milli_units())?,
                format_milli_units(inventory.demand_backlog().milli_units())?
            ),
        )
        .await?;
    }

    if snapshot.open_restocks.is_empty() {
        write_line(output, "open restocks: none".to_owned()).await?;
    } else {
        write_line(output, "open restocks:".to_owned()).await?;
        for order in &snapshot.open_restocks {
            write_line(
                output,
                format!(
                    "- {}: {} unit(s), ordered {}, eta {}, rationale: {}",
                    order.sku(),
                    order.quantity().units(),
                    order.ordered_at(),
                    order.eta(),
                    order.rationale()
                ),
            )
            .await?;
        }
    }

    write_line(
        output,
        format!("sales orders recorded: {}", snapshot.recent_sales.len()),
    )
    .await
}

fn occupied_space(snapshot: &RetailSnapshot) -> Result<SpaceUnits, CliError> {
    let mut total = SpaceUnits::new(0);
    for product in &snapshot.products {
        let inventory = inventory_for(&snapshot.inventory, product)?;
        total = total.checked_add(
            product
                .unit_space()
                .checked_mul_quantity(inventory.on_hand())?,
        )?;
    }
    Ok(total)
}

fn inventory_for<'a>(
    inventory: &'a [InventoryPosition],
    product: &Product,
) -> Result<&'a InventoryPosition, CliError> {
    inventory
        .iter()
        .find(|position| position.sku() == product.sku())
        .ok_or_else(|| {
            CliError::Application(ApplicationError::InventoryNotFound {
                sku: product.sku().clone(),
            })
        })
}

fn open_inbound_quantity(
    snapshot: &RetailSnapshot,
    product: &Product,
) -> Result<StockQuantity, CliError> {
    let mut total = StockQuantity::new(0);
    for order in &snapshot.open_restocks {
        if order.sku() == product.sku() {
            total = total.checked_add(order.quantity())?;
        }
    }
    Ok(total)
}

fn format_money(amount: MoneyCents) -> Result<String, CliError> {
    let cents = amount.cents();
    let dollars = checked_div(cents, 100, "money display dollars")?;
    let remainder = checked_rem(cents, 100, "money display cents")?;
    Ok(format!("${dollars}.{remainder:02}"))
}

fn format_milli_units(milli_units: u64) -> Result<String, CliError> {
    let whole = checked_div(milli_units, 1_000, "milli-unit display whole units")?;
    let fraction = checked_rem(milli_units, 1_000, "milli-unit display fraction")?;
    Ok(format!("{whole}.{fraction:03}"))
}

fn format_days_of_cover(quantity: StockQuantity, product: &Product) -> Result<String, CliError> {
    let demand = product.demand_rate().milli_units();
    if demand == 0 {
        return Ok("unbounded".to_owned());
    }

    let tenths = quantity
        .units()
        .checked_mul(10_000)
        .ok_or_else(|| count_overflow("days of cover numerator"))?
        .checked_div(demand)
        .ok_or_else(|| count_overflow("days of cover division"))?;
    let whole = checked_div(tenths, 10, "days of cover whole")?;
    let fraction = checked_rem(tenths, 10, "days of cover fraction")?;
    Ok(format!("{whole}.{fraction} day(s)"))
}

fn checked_div(dividend: u64, divisor: u64, operation: &'static str) -> Result<u64, CliError> {
    dividend
        .checked_div(divisor)
        .ok_or_else(|| count_overflow(operation))
}

fn checked_rem(dividend: u64, divisor: u64, operation: &'static str) -> Result<u64, CliError> {
    dividend
        .checked_rem(divisor)
        .ok_or_else(|| count_overflow(operation))
}

const fn count_overflow(operation: &'static str) -> CliError {
    CliError::Application(ApplicationError::CountOverflow { operation })
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

    pub(crate) const fn into_command(self) -> Option<Command> {
        self.command
    }
}

/// Retail workflow subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Seed retail state from the configured scenario.
    Seed(SeedArgs),
    /// Show current stock, inbound, and profit health.
    Status,
    /// Advance the deterministic retail simulation.
    Simulate(SimulateArgs),
    /// Run one restock decision.
    Decide(DecideArgs),
    /// Run repeated simulation and decision cycles.
    RunCycle(RunCycleArgs),
}

/// Seed command arguments.
#[derive(Debug, Clone, Parser)]
pub(crate) struct SeedArgs {
    /// Replace existing retail state before seeding.
    #[arg(long)]
    reset: bool,
}

impl SeedArgs {
    pub(crate) fn command(&self, config: &AppConfig) -> SeedRetailScenario {
        SeedRetailScenario {
            scenario_path: config.retail_scenario_path.clone(),
            reset: self.reset,
        }
    }
}

/// Simulation command arguments.
#[derive(Debug, Clone, Copy, Parser)]
pub(crate) struct SimulateArgs {
    /// Number of simulated days to advance.
    #[arg(long)]
    days: u64,
}

impl SimulateArgs {
    pub(crate) const fn command(self) -> Result<AdvanceSimulation, CliError> {
        if self.days == 0 {
            return Err(CliError::NonPositiveArgument { field: "days" });
        }
        Ok(AdvanceSimulation { days: self.days })
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
    pub(crate) fn command(self, config: &AppConfig) -> Result<RunRestockDecision, CliError> {
        let horizon_days = self.horizon_days.unwrap_or(config.decision_horizon_days);
        Ok(RunRestockDecision {
            horizon: DecisionHorizonDays::try_from(horizon_days)?,
            max_restock_orders: StockQuantity::new(config.max_restock_orders_per_decision),
        })
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
    pub(crate) fn command(self, config: &AppConfig) -> Result<RunWorkflowCycle, CliError> {
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

#[cfg(test)]
mod tests {
    use super::{
        Cli, CliError, Command, RunCycleArgs, SeedArgs, SimulateArgs, render_default_help,
        required_openai_key, write_decision_result, write_status_result,
    };
    use chrono::NaiveDate;

    use crate::application::retail::{ProfitSummary, RetailSnapshot};
    use crate::config::AppConfig;
    use crate::domain::retail::{
        ApparelKind, Brand, DecisionRunId, DemandBacklog, DemandRatePerDay, InventoryPosition,
        LeadTimeDays, MoneyCents, Product, ProductDetails, SimulationDate, SizeLabel, Sku,
        SpaceUnits, StockQuantity,
    };

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

    #[test]
    fn parses_status_subcommand() -> Result<(), Box<dyn std::error::Error>> {
        let cli = <Cli as clap::Parser>::try_parse_from(["rigagent", "status"])?;

        assert!(matches!(cli.command, Some(Command::Status)));
        Ok(())
    }

    #[tokio::test]
    async fn status_renders_stock_and_profit_health() -> Result<(), Box<dyn std::error::Error>> {
        let sku = Sku::new("tsh-test")?;
        let product = Product::from_details(ProductDetails {
            sku: sku.clone(),
            kind: ApparelKind::Shirt,
            brand: Brand::new("Acme")?,
            size: SizeLabel::M,
            unit_cost: MoneyCents::new(1_000),
            unit_price: MoneyCents::new(2_500),
            unit_space: SpaceUnits::new(2),
            demand_rate: DemandRatePerDay::from_milli_units(2_500),
            lead_time: LeadTimeDays::new(3)?,
            min_order_quantity: StockQuantity::new(1),
            max_order_quantity: StockQuantity::new(10),
            active: true,
        })?;
        let snapshot = RetailSnapshot {
            current_date: test_date()?,
            capacity: SpaceUnits::new(20),
            products: vec![product],
            inventory: vec![InventoryPosition::new(
                sku,
                StockQuantity::new(5),
                DemandBacklog::from_milli_units(250)?,
            )],
            open_restocks: Vec::new(),
            recent_sales: Vec::new(),
            profit_summary: ProfitSummary {
                revenue: MoneyCents::new(12_345),
                cost: MoneyCents::new(5_000),
                gross_profit: MoneyCents::new(7_345),
                lost_units: StockQuantity::new(2),
            },
        };
        let mut output = Vec::new();

        write_status_result(&mut output, &snapshot).await?;

        let rendered = String::from_utf8(output)?;
        assert!(rendered.contains("status date 2026-06-09"));
        assert!(rendered.contains("capacity: 10/20 space unit(s) used"));
        assert!(
            rendered.contains("profit: revenue $123.45, cost $50.00, gross $73.45, lost 2 unit(s)")
        );
        assert!(rendered.contains(
            "- TSH-TEST Acme shirt M: on hand 5, cover 2.0 day(s), inbound 0, demand 2.500/day, backlog 0.250"
        ));
        assert!(rendered.contains("open restocks: none"));
        assert!(rendered.contains("sales orders recorded: 0"));
        Ok(())
    }

    #[tokio::test]
    async fn decision_result_renders_rejected_proposal_reasons()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = crate::application::retail::DecisionResult {
            decision_run_id: DecisionRunId::new("decision-1")?,
            accepted_orders: Vec::new(),
            rejected_proposals: vec![crate::application::retail::RejectedRestockProposal {
                sku: Sku::new("sho-urbn-9-wht")?,
                quantity: StockQuantity::new(24),
                reason:
                    "proposal for SKU SHO-URBN-9-WHT exceeds available capacity: requires 96, available 46, overflow 50"
                        .to_owned(),
            }],
            summary: "Placed 1 validated restock order".to_owned(),
        };
        let mut output = Vec::new();

        write_decision_result(&mut output, result).await?;

        let rendered = String::from_utf8(output)?;
        assert!(rendered.contains("accepted 0 order(s), rejected 1 proposal(s)"));
        assert!(rendered.contains("rejected proposals:"));
        assert!(rendered.contains(
            "- SHO-URBN-9-WHT: 24 unit(s), reason: proposal for SKU SHO-URBN-9-WHT exceeds available capacity: requires 96, available 46, overflow 50"
        ));
        assert!(rendered.contains(
            "agent summary omitted because one or more proposals failed application validation"
        ));
        assert!(!rendered.contains("Placed 1 validated restock order"));
        Ok(())
    }

    fn test_date() -> Result<SimulationDate, Box<dyn std::error::Error>> {
        let date = NaiveDate::from_ymd_opt(2026, 6, 9)
            .ok_or_else(|| std::io::Error::other("test date must be valid"))?;
        Ok(SimulationDate::new(date))
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
