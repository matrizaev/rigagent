//! Binary entrypoint for the retail replenishment workflow.

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rigagent::run().await?;
    Ok(())
}
