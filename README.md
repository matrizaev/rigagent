# rigagent tutorial: 07 CLI workflow

This branch is the final checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/06-rig-decision-agent` and wires the runtime shell to the
application service and infrastructure adapters.

The workflow is now runnable from the CLI:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

`master` is the polished full implementation. This branch is the final teaching
checkpoint before that reference branch.

## Current State

The repository now includes the complete vertical slice:

```text
rigagent/
+-- config.yaml
+-- clippy.toml
+-- data/
|   +-- retail_scenario.yaml
+-- migrations/
+-- src/
    +-- main.rs
    +-- lib.rs
    +-- config.rs
    +-- domain/
    |   +-- retail/
    +-- application/
    |   +-- retail/
    +-- infrastructure/
    |   +-- agents/
    |   +-- clock.rs
    |   +-- ids.rs
    |   +-- persistence/
    |   +-- scenario/
    +-- interfaces/
        +-- cli.rs
```

Dependency direction:

```text
interfaces -> application -> domain
infrastructure -> application -> domain
```

The domain remains free of Diesel, Rig, Clap, config loading, environment
variables, async runtimes, provider payloads, and tracing.

## What This Branch Adds

### Final Lint Configuration

Branch 07 matches the final reference lint posture by enabling
`clippy::cargo` at the crate root and adding `clippy.toml` with the explicit
duplicate-crate allowlist needed by the current dependency graph.

This is not part of the retail workflow behavior. It keeps the documented
validation command strict and reproducible:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Final Source Alignment

This checkpoint also removes a few tutorial-only rough edges so `src/` matches
the final reference implementation:

- `src/config.rs` uses final-state documentation and keeps test-only config
  builders inside tests.
- `src/main.rs` uses the final binary wording and preserves the application
  error exit code after writing to stderr.
- `src/interfaces/mod.rs` and `src/infrastructure/persistence/mod.rs` match the
  final module documentation and formatting.

These changes do not add new workflow behavior; they keep the last tutorial
checkpoint aligned with `master`.

### Runtime Wiring

`src/lib.rs::run()` now:

1. Loads `.env` for local development.
2. Initializes tracing.
3. Parses CLI arguments.
4. Prints help and exits when no subcommand is supplied.
5. Loads `config.yaml` plus environment overrides for real commands.
6. Creates the SQLite pool from `retail_db_path`.
7. Runs embedded Diesel migrations.
8. Builds `DieselRetailStore`.
9. Builds `DieselDecisionRunStore`.
10. Builds `UuidRetailIdGenerator`.
11. Builds `SystemClock`.
12. Builds `RigReplenishmentDecisionAgent` only for decision commands.
13. Dispatches to `RetailWorkflow`.

Running with no subcommand does not load config, open a database, run
migrations, require `OPENAI_API_KEY`, or mutate state.

### Clock And ID Adapters

`src/infrastructure/clock.rs` adds `SystemClock`, an implementation of the
application `Clock` port.

`src/infrastructure/ids.rs` adds `UuidRetailIdGenerator`, an implementation of
the `IdGenerator` port for:

- `SalesOrderId`
- `RestockOrderId`
- `DecisionRunId`

### CLI Mapping

`src/interfaces/cli.rs` now maps Clap parser structs into application commands:

- `SeedArgs -> SeedRetailScenario`
- `SimulateArgs -> AdvanceSimulation`
- `DecideArgs -> RunRestockDecision`
- `RunCycleArgs -> RunWorkflowCycle`

It also writes user-facing command summaries.

### Provider Key Behavior

`OPENAI_API_KEY` is required only for:

- `decide`
- `run-cycle`

It is not required for:

- `cargo run`
- `seed`
- `simulate`

This keeps non-model workflows usable without provider configuration.

### Command Output

Successful command output is intentionally concise:

```text
seeded retail state from data/retail_scenario.yaml (reset: true)
advanced 3 day(s) to 2026-06-12; received 0 restock order(s), recorded 12 sale(s), lost 0 unit(s)
decision decision-... accepted 2 order(s), rejected 0 proposal(s): ...
advanced 14 day(s), ran 3 decision(s), final date 2026-06-23
```

## Try It

Show help:

```bash
cargo run
```

Seed the local SQLite database:

```bash
cargo run -- seed --reset
```

Advance deterministic simulation:

```bash
cargo run -- simulate --days 3
```

Run one provider-backed decision:

```bash
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
```

Run a repeated workflow cycle:

```bash
OPENAI_API_KEY=sk-your-key cargo run -- run-cycle --days 14 --decision-interval-days 7
```

Generated local state is ignored by Git:

```gitignore
data/retail.sqlite*
.env
```

## Validate This Branch

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 3
```

Decision commands require provider access and are manual checks:

```bash
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
OPENAI_API_KEY=sk-your-key cargo run -- run-cycle --days 14 --decision-interval-days 7
```

Normal automated tests do not call OpenAI.

## Completed Tutorial Path

The cumulative tutorial branches are:

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

Use `master` as the full reference implementation. Use the tutorial branches to
study how each layer is introduced:

- `00-start`: minimal Cargo binary and problem statement.
- `01-runtime-cutover`: dependencies, lints, config, and CLI shell.
- `02-domain-model`: retail value objects, entities, and deterministic services.
- `03-application-layer`: use cases, ports, read models, and fakes.
- `04-diesel-persistence`: migrations and SQLite adapters.
- `05-scenario-seeding`: YAML loader and seed data.
- `06-rig-decision-agent`: Rig/OpenAI decision adapter.
- `07-cli-workflow`: runtime adapter assembly and runnable commands.

## Quality Bar

Before treating the workflow as complete, verify:

- Business rules live in domain behavior.
- Application code coordinates workflows through ports.
- Diesel and Rig stay in infrastructure.
- Clap and process output stay in interfaces or the binary edge.
- Config and environment reads happen at startup only.
- Model proposals are validated by application/domain code before persistence.
- No normal test requires OpenAI network access.
- `cargo fmt`, `cargo clippy`, and `cargo test` pass.
