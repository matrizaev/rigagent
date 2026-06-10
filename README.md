# rigagent tutorial: 00 start

This branch is the first checkpoint in the `rigagent` build-along tutorial. It
contains only a minimal Cargo binary and the problem statement. The next branch,
`tutorial/01-runtime-cutover`, starts after you add the runtime shell,
dependencies, strict lint posture, and configuration skeleton described below.

The finished application on `master` is an autonomous retail replenishment
workflow agent. It seeds a small apparel shop, simulates deterministic demand,
asks a Rig-backed decision agent for supplier restock proposals, validates those
proposals in Rust, and persists accepted restock orders in SQLite through
Diesel.

This is not a chat assistant. The target system is a command-line workflow
runner with durable state, deterministic business rules, clean architecture
boundaries, and one model-backed decision step.

## Current State

The code in this branch is intentionally small:

```text
rigagent/
+-- AGENTS.md
+-- Cargo.lock
+-- Cargo.toml
+-- .gitignore
+-- README.md
+-- src/
    +-- main.rs
```

`src/main.rs` is only a compiling binary stub:

```rust
fn main() {}
```

Run the baseline:

```bash
cargo run
cargo test
```

Both commands should succeed. There is no application behavior yet.

## Product Target

You are building a Rust application with these final user-facing commands:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

Final behavior:

- `cargo run` prints help and does not mutate state.
- `seed` creates retail state from a YAML scenario.
- `simulate` advances deterministic sales and inventory state without a model.
- `decide` runs one replenishment decision through a Rig-backed agent.
- `run-cycle` repeats simulation and decision turns on a configured cadence.

Important constraints:

- Domain code owns retail rules and must not depend on Diesel, Rig, Clap,
  environment variables, async runtimes, provider payloads, or tracing.
- Application code coordinates use cases and defines ports for side effects.
- Infrastructure implements ports for Diesel persistence, scenario loading,
  clocks, ID generation, and the Rig decision agent.
- Interfaces adapt command-line input and output.
- Model proposals are never written directly to the database. Rust validates
  every proposal before persistence.

## Goal For The Next Branch

The next branch, `tutorial/01-runtime-cutover`, should still have no retail
domain behavior. It should establish the runtime foundation that all later
branches build on.

When you finish this step, the repository should contain:

- Focused dependencies in `Cargo.toml`.
- Strict crate-level lints in `src/main.rs` and `src/lib.rs`.
- A tiny binary entrypoint that delegates to `rigagent::run()`.
- A library runtime function that initializes tracing, parses the CLI shell, and
  renders help when no subcommand is supplied.
- A config type that can load common non-secret settings without requiring
  `OPENAI_API_KEY`.
- Empty module boundaries for `domain`, `application`, `infrastructure`, and
  `interfaces`.
- A README update explaining the new runtime shell.

## Step By Step: Reach `tutorial/01-runtime-cutover`

### 1. Add focused dependencies

Update `Cargo.toml` with the crates the completed project will need:

```toml
[dependencies]
chrono = { version = "0.4", default-features = false, features = ["serde"] }
clap = { version = "4", features = ["derive"] }
config = { version = "0.15", default-features = false, features = ["yaml"] }
diesel = { version = "2.2", default-features = false, features = ["chrono", "r2d2", "sqlite"] }
diesel_migrations = { version = "2.2", default-features = false, features = ["sqlite"] }
dotenvy = "0.15"
rig = { package = "rig-core", version = "0.38.1", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
thiserror = "2"
tokio = { version = "1", features = ["io-std", "io-util", "macros", "rt-multi-thread"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
uuid = { version = "1", features = ["serde", "v4"] }
```

Why these belong in the runtime branch:

- `clap`, `tokio`, `dotenvy`, and `tracing` shape the binary edge.
- `config`, `serde`, and `serde_yaml` support typed configuration and scenario
  loading in later branches.
- `thiserror` supports layer-owned errors from the start.
- `chrono`, `diesel`, `diesel_migrations`, `uuid`, and `rig-core` are not used
  deeply yet, but adding them here creates one dependency cutover before domain,
  persistence, and agent code arrive.

Run:

```bash
cargo check
```

### 2. Add strict lint posture

At the top of both `src/main.rs` and the new `src/lib.rs`, add the crate-level
lint posture that later branches must satisfy:

```rust
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
```

This is deliberately strict. Later code should model valid states with types and
return typed errors instead of relying on runtime assertions.

### 3. Split binary entrypoint from library runtime

Change `src/main.rs` so it only starts Tokio and translates the final error into
an exit code:

```rust
use std::process::ExitCode;

use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> ExitCode {
    match rigagent::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut stderr = tokio::io::stderr();
            let message = format!("{error}\n");
            if stderr.write_all(message.as_bytes()).await.is_err() {
                return ExitCode::FAILURE;
            }
            error.exit_code()
        }
    }
}
```

Keep all user-facing output at the interface boundary.

### 4. Create empty module boundaries

Create these files:

```text
src/lib.rs
src/config.rs
src/domain/mod.rs
src/application/mod.rs
src/infrastructure/mod.rs
src/interfaces/mod.rs
src/interfaces/cli.rs
```

For now, most module files can contain only module documentation. The point is
to reserve the dependency boundaries before behavior exists.

`src/lib.rs` should publicly expose the top-level layers:

```rust
pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod interfaces;
```

### 5. Add a config shell

Create `config.yaml` with final non-secret defaults:

```yaml
chat_model: gpt-5-nano
retail_db_path: data/retail.sqlite
retail_scenario_path: data/retail_scenario.yaml
decision_horizon_days: 14
max_restock_orders_per_decision: 2
```

In `src/config.rs`, define an `AppConfig` that loads this YAML plus environment
overrides. Include `openai_api_key: Option<String>`, but do not require it just
to load common config. `seed` and `simulate` will not need the key later.

### 6. Add the CLI shell

In `src/interfaces/cli.rs`, create the command surface with Clap:

```text
seed --reset
simulate --days N
decide --horizon-days N
run-cycle --days N --decision-interval-days M
```

At this stage, the subcommands can be parsed but do not need real use-case
behavior. The one behavior that should exist now: running with no subcommand
prints help and performs no mutation.

### 7. Implement `rigagent::run()`

In `src/lib.rs`, build the early runtime flow:

1. Load `.env` for local development.
2. Initialize tracing with a default `warn` filter.
3. Parse CLI args.
4. If no subcommand is present, render Clap help to stdout and return `Ok(())`.
5. Load `AppConfig` only after a real subcommand is present.
6. Return a typed interface error for unsupported placeholder commands.

Do not create database pools, run migrations, call Rig, or add retail domain
types in this branch. Those belong to later checkpoints.

### 8. Add tests for the shell

Add focused tests around behavior that exists in this branch:

- `no_subcommand_prints_help_without_mutating_state`
- `missing_openai_key_still_loads_common_config`
- `environment_overrides_yaml`
- `simulate_rejects_zero_days`
- `run_cycle_rejects_zero_decision_interval`

Use temporary in-memory config sources in tests instead of reading developer
machine state.

### 9. Validate the checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

Expected state at the end:

- `cargo run` shows command help.
- No retail state exists yet.
- No database is opened.
- No provider key is required for help or common config loading.
- The codebase matches the intended `tutorial/01-runtime-cutover` checkpoint.

## Branch Ladder

The intended cumulative tutorial branches are:

```text
tutorial/00-start
tutorial/01-runtime-cutover
tutorial/02-domain-model
tutorial/03-application-layer
tutorial/04-diesel-persistence
tutorial/05-scenario-seeding
tutorial/06-rig-decision-agent
tutorial/07-cli-workflow
master
```

Use `master` as the full reference implementation when you need to compare a
stage with the finished design.
