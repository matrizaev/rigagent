# rigagent tutorial: 06 Rig decision agent

This branch is the seventh checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/05-scenario-seeding` and adds the Rig/OpenAI-backed
implementation of the `ReplenishmentDecisionAgent` application port.

The command-line runtime still has placeholder command dispatch. This branch
adds the provider adapter only; runtime construction, `OPENAI_API_KEY` checks,
ID generation, clocks, and CLI-to-use-case wiring are the final checkpoint.

## Current State

The repository now includes a model-backed decision adapter:

```text
rigagent/
+-- data/
|   +-- retail_scenario.yaml
+-- migrations/
+-- src/
    +-- domain/
    |   +-- retail/
    +-- application/
    |   +-- retail/
    +-- infrastructure/
    |   +-- agents/
    |   |   +-- mod.rs
    |   |   +-- rig_replenishment/
    |   |       +-- mod.rs
    |   +-- persistence/
    |   +-- scenario/
    +-- interfaces/
        +-- cli.rs
```

Dependency direction:

```text
infrastructure::agents::rig_replenishment -> application -> domain
```

Rig and OpenAI provider types are isolated in the infrastructure adapter. Domain
and application APIs still do not expose provider payloads or Rig tool types.

## What This Branch Adds

### Rig Adapter

`RigReplenishmentDecisionAgent` implements:

```rust
ReplenishmentDecisionAgent
```

Constructor inputs:

- OpenAI API key.
- chat model name.

The adapter builds the provider client only when `decide` is called. Later CLI
wiring will ensure `seed` and `simulate` do not require `OPENAI_API_KEY`.

### Decision Session

Each decision turn uses an in-memory `DecisionSession` containing:

- current `RetailSnapshot`.
- ranked deterministic `RestockOption` values.
- open restock orders.
- profit summary.
- max allowed proposals.
- proposals collected by tool calls.

Tools write only to this session. They do not write directly to Diesel.

### Tool Surface

The adapter exposes implementation-detail tools:

- `get_inventory_snapshot`
- `list_open_restock_orders`
- `analyze_restock_options`
- `place_restock_order`
- `get_profit_summary`

`place_restock_order` validates session-level issues such as empty rationale,
zero quantity, duplicate session proposals, duplicate open inbound orders, and
quantities above the ranked option recommendation.

Application/domain validation still happens later in `RetailWorkflow` before
durable writes.

### Prompt Shape

The decision prompt is autonomous workflow guidance, not a chat-assistant
script. It asks the model to:

- inspect inventory.
- inspect ranked options.
- inspect open inbound orders.
- inspect profit summary.
- place no more than the configured maximum restock orders.
- include SKU, quantity, and rationale.
- prefer high expected gross profit per occupied stock-space.
- avoid duplicate inbound orders and capacity overflow.

The prompt also avoids exposing provider prompt details in outputs.

### Failure Mapping

Provider setup and provider decision failures map into
`ApplicationError::AgentFailure` with source errors preserved.

### Network-Free Tests

Normal tests do not call OpenAI.

The adapter tests cover:

- parsing `place_restock_order` arguments.
- recording session proposals.
- mapping completion failures without calling a provider.

## Runtime Behavior

The CLI is still not wired to application or infrastructure:

```bash
cargo run
```

Expected behavior: command help is printed, no config is loaded, no database is
opened, and no state is mutated.

Real commands still return placeholder errors:

```bash
cargo run -- decide --horizon-days 14
```

Expected behavior:

```text
decide is parsed but not implemented until a later tutorial branch
```

CLI wiring is the next and final tutorial checkpoint.

## Validate This Branch

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

The test suite now covers:

- runtime config and CLI shell behavior.
- domain invariants and deterministic services.
- application use cases with fakes.
- Diesel persistence against migrated SQLite databases.
- scenario YAML loading and seed-state conversion.
- Rig adapter session and failure behavior without network access.

## Goal For The Next Branch

The next branch is `tutorial/07-cli-workflow`. It should wire the runtime shell
to the application service and infrastructure adapters so the commands actually
run.

When you finish the next branch, the repository should contain:

```text
src/infrastructure/
+-- clock.rs
+-- ids.rs
src/interfaces/
+-- cli.rs
src/lib.rs
src/main.rs
```

The final tutorial branch should support:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

## Step By Step: Reach `tutorial/07-cli-workflow`

### 1. Add Clock And ID Adapters

Create:

```text
src/infrastructure/clock.rs
src/infrastructure/ids.rs
```

Update `src/infrastructure/mod.rs`:

```rust
pub mod clock;
pub mod ids;
```

`SystemClock` should implement `Clock`.

`UuidRetailIdGenerator` should implement `IdGenerator` and create typed:

- `SalesOrderId`
- `RestockOrderId`
- `DecisionRunId`

Keep ID generation in infrastructure. Domain types validate IDs; they do not
generate them.

### 2. Map CLI Args Into Application Commands

Update `src/interfaces/cli.rs` so parser structs build application commands:

- `SeedArgs -> SeedRetailScenario`
- `SimulateArgs -> AdvanceSimulation`
- `DecideArgs -> RunRestockDecision`
- `RunCycleArgs -> RunWorkflowCycle`

Keep validation at the interface boundary for CLI-specific argument shape. Use
domain constructors for typed horizons and quantities.

### 3. Require OpenAI Key Only For Decision Commands

Config should load without `OPENAI_API_KEY`.

Add a helper such as:

```rust
required_openai_key(config: &AppConfig) -> Result<String, CliError>
```

Use it only for:

- `decide`
- `run-cycle`

Do not require a provider key for:

- no subcommand help.
- `seed`
- `simulate`.

### 4. Build Runtime Adapter Assembly

Update `src/lib.rs::run()`:

1. Load `.env`.
2. Initialize tracing.
3. Parse CLI.
4. Render help and return if no subcommand is present.
5. Load `AppConfig`.
6. Create the SQLite pool from `retail_db_path`.
7. Run embedded migrations.
8. Build `DieselRetailStore`.
9. Build `DieselDecisionRunStore`.
10. Build `UuidRetailIdGenerator`.
11. Build `SystemClock`.
12. Build `RigReplenishmentDecisionAgent` only for decision commands.
13. Dispatch to `RetailWorkflow`.

Use an unavailable decision-agent placeholder for `seed` and `simulate` if the
generic workflow type needs an agent but the command will not call it.

### 5. Keep `cargo run` No-Mutation

Running with no subcommand must:

- print help.
- not load config.
- not create a database pool.
- not run migrations.
- not require `OPENAI_API_KEY`.
- not mutate state.

This is a core interface contract.

### 6. Add User-Facing Output

Write concise command summaries:

- seed:
  - scenario path.
  - reset flag.
- simulate:
  - days advanced.
  - final date.
  - received restock count.
  - sales count.
  - lost units.
- decide:
  - decision run ID.
  - accepted order count.
  - rejected proposal count.
  - summary.
- run-cycle:
  - days advanced.
  - decision count.
  - final date.

Direct stdout/stderr should stay in interface code or the binary edge.

### 7. Add Interface Tests

Good tests:

- `no_subcommand_prints_help_without_mutating_state`
- `seed_maps_reset_flag_to_application_command`
- `simulate_rejects_zero_days`
- `decide_requires_openai_key`
- `run_cycle_rejects_zero_decision_interval`
- command output formatting for each result type.

Use fakes where possible. Do not call OpenAI in tests.

### 8. Run Local Workflow Checks

Without a provider key:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 3
```

With a valid provider key:

```bash
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
OPENAI_API_KEY=sk-your-key cargo run -- run-cycle --days 14 --decision-interval-days 7
```

Provider-backed commands are manual checks; normal automated tests should not
need network access.

### 9. Validate The Next Checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 3
```

Expected state at the end:

- Help prints with no mutation.
- Seed creates SQLite state from YAML.
- Simulate advances deterministic state.
- Decision commands require `OPENAI_API_KEY`.
- All normal tests pass without OpenAI or network access.

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
