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

use clap::{CommandFactory, Parser, Subcommand};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tracing_subscriber::EnvFilter;

/// Application use cases for the retail workflow.
pub mod application;

/// Core business behavior for the retail workflow.
pub mod domain;

/// External adapters for the retail workflow.
pub mod infrastructure;

/// Command-line arguments for the retail replenishment workflow.
#[derive(Debug, Parser)]
#[command(author, version, about = "Retail replenishment workflow agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Retail workflow commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Seed retail state from the configured scenario.
    Seed,
    /// Advance the deterministic retail simulation.
    Simulate,
    /// Run one restock decision cycle.
    Decide,
    /// Run repeated simulation and decision cycles.
    RunCycle,
}

/// Errors returned by the runtime entrypoint.
#[derive(Debug, Error)]
pub enum RunError {
    /// The command-line interface could not render help.
    #[error("failed to render command help")]
    RenderHelp(#[source] std::io::Error),
}

/// Run the retail replenishment workflow binary.
///
/// # Errors
///
/// Returns an error when the command interface cannot render its default help.
pub async fn run() -> Result<(), RunError> {
    dotenvy::dotenv().ok();

    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Some(Command::Seed | Command::Simulate | Command::Decide | Command::RunCycle) | None => {
            render_default_help().await?;
        }
    }

    Ok(())
}

async fn render_default_help() -> Result<(), RunError> {
    let mut command = Cli::command();
    let mut output = Vec::new();
    command
        .write_help(&mut output)
        .map_err(RunError::RenderHelp)?;

    let mut stdout = tokio::io::stdout();
    stdout
        .write_all(&output)
        .await
        .map_err(RunError::RenderHelp)?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .ok();
}
