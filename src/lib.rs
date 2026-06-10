//! Runtime shell for the retail replenishment workflow tutorial.

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
#![warn(clippy::pedantic, clippy::nursery)]

use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::interfaces::cli::CliError;

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

/// Run the retail replenishment workflow binary.
///
/// # Errors
///
/// Returns an error when command parsing, configuration loading, or placeholder
/// command dispatch fails.
pub async fn run() -> Result<(), CliError> {
    load_dotenv();
    init_tracing();

    let cli = interfaces::cli::Cli::parse_args();
    let mut stdout = tokio::io::stdout();
    let Some(command) = cli.into_command() else {
        interfaces::cli::render_default_help(&mut stdout).await?;
        return Ok(());
    };

    let config = AppConfig::load()?;
    command.validate(&config)?;
    Err(CliError::PlaceholderCommand {
        command: command.name(),
    })
}

fn load_dotenv() {
    match dotenvy::dotenv() {
        Ok(_) | Err(_) => {}
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    if tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .is_err()
    {}
}
