# rigagent

`rigagent` is a build-along Rust tutorial for an autonomous retail
replenishment workflow agent. The finished application seeds a small apparel
shop, simulates deterministic demand, asks a Rig-backed decision agent for
restock proposals, validates those proposals in Rust, and persists accepted
orders in SQLite through Diesel.

This is intentionally not a chat assistant. It is a workflow runner with
durable state, clean application boundaries, deterministic business rules, and a
model-backed decision step.

## Teaching Model

The `master` branch is the complete implementation. The tutorial is designed to
also work as a branch ladder, where each branch adds one meaningful slice of the
system and leaves the repository in a working state.

Suggested cumulative learner branches:

| Branch | Learner outcome |
| --- | --- |
| `tutorial/00-start` | Minimal Cargo binary and the problem statement. |
| `tutorial/01-runtime-cutover` | Dependencies, strict lints, config shell, and runtime entrypoint. |
| `tutorial/02-domain-model` | Retail value objects, entities, domain errors, and deterministic services. |
| `tutorial/03-application-layer` | Use cases, commands, read models, ports, and in-memory fakes. |
| `tutorial/04-diesel-persistence` | Diesel migrations, SQLite adapters, row mapping, and transactions. |
| `tutorial/05-scenario-seeding` | YAML scenario loading and the `seed` workflow. |
| `tutorial/06-rig-decision-agent` | Rig-backed replenishment adapter and decision-session tools. |
| `tutorial/07-cli-workflow` | CLI commands, config wiring, migrations on startup, and workflow dispatch. |
| `master` | Finished implementation, documentation, and validation gates. |

When these branches exist, learners can inspect each stage with:

```bash
git switch tutorial/03-application-layer
git diff tutorial/02-domain-model..tutorial/03-application-layer
cargo test --all-features
```

The detailed phase plan that inspired this ladder lives in
[docs/plans/01 - retail-replenishment-agent.md](docs/plans/01%20-%20retail-replenishment-agent.md).

## Try The Finished App

Prerequisites:

- Rust toolchain compatible with edition 2024.
- SQLite development libraries if your platform needs them for Diesel.
- `OPENAI_API_KEY` only for commands that invoke the Rig decision agent.

Install dependencies through Cargo and show the command surface:

```bash
cargo run
```

Running with no subcommand prints help and performs no mutation.

The non-secret runtime defaults are in `config.yaml`:

```yaml
chat_model: gpt-5-nano
retail_db_path: data/retail.sqlite
retail_scenario_path: data/retail_scenario.yaml
decision_horizon_days: 14
max_restock_orders_per_decision: 2
```

Environment variables override matching YAML fields:

```bash
CHAT_MODEL=gpt-5-mini cargo run -- decide --horizon-days 14
```

Set a provider key only when running decision commands:

```bash
export OPENAI_API_KEY=sk-your-key
```

Seed the retail database from the scenario YAML:

```bash
cargo run -- seed --reset
```

Advance deterministic sales simulation without calling a model:

```bash
cargo run -- simulate --days 7
```

Run one replenishment decision:

```bash
cargo run -- decide --horizon-days 14
```

Run an autonomous cycle. The decision horizon comes from `config.yaml`.

```bash
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

Generated local state is ignored by Git:

```gitignore
data/retail.sqlite*
.env
```

## Architecture You Build

The repository follows clean-architecture dependency direction:

```text
interfaces -> application -> domain
infrastructure -> application -> domain
```

The completed module map is:

```text
rigagent/
+-- config.yaml
+-- data/
|   +-- retail_scenario.yaml
+-- migrations/
+-- src/
|   +-- main.rs
|   +-- lib.rs
|   +-- config.rs
|   +-- domain/
|   |   +-- retail/
|   +-- application/
|   |   +-- retail/
|   +-- infrastructure/
|   |   +-- agents/
|   |   |   +-- rig_replenishment/
|   |   +-- clock.rs
|   |   +-- ids.rs
|   |   +-- persistence/
|   |   +-- scenario/
|   +-- interfaces/
|       +-- cli.rs
```

Layer responsibilities:

- `domain/retail`: framework-free business language, invariants, state
  transitions, value objects, and deterministic scoring rules.
- `application/retail`: commands, use cases, read models, ports, validation, and
  transaction-oriented orchestration.
- `infrastructure/persistence`: Diesel migrations, SQLite pools, row structs,
  checked row/domain conversion, and adapter error mapping.
- `infrastructure/scenario`: YAML DTO parsing and conversion into validated
  retail state.
- `infrastructure/agents/rig_replenishment`: Rig/OpenAI decision adapter and
  per-decision tools.
- `interfaces/cli`: Clap parsing, config loading, adapter assembly, and
  user-facing command summaries.

The domain layer does not depend on Diesel, Rig, Clap, config loading,
environment variables, async runtimes, provider payloads, or tracing.

## Build Path

Each stage below should end with a small, working checkpoint. Prefer one focused
commit per stage. If you are creating the branch ladder from scratch, create the
branch after the checkpoint passes and before starting the next stage.

### Stage 00: Start From The Problem

Goal: establish the product and constraints before writing implementation code.

Create or read:

- `SPEC.md` for the user-facing contract.
- `ARCHITECTURE.md` for dependency direction and module ownership.
- `docs/features/01 - retail-replenishment-agent.md` for feature scope.
- `docs/plans/01 - retail-replenishment-agent.md` for the original phase plan.

Decisions to make up front:

- The app is a workflow agent, not a REPL or support chatbot.
- Retail state is durable and lives in SQLite.
- Demand simulation is deterministic so tutorial output is repeatable.
- The model proposes restocks, but Rust application/domain code validates every
  durable write.
- Clean layers are part of the teaching goal, not incidental structure.

Checkpoint:

```bash
cargo run
```

At this point a starting branch may only show a stub binary or help text.

### Stage 01: Runtime Cutover, Dependencies, And Lints

Goal: make the crate ready for strict Rust application development.

Update `Cargo.toml` with focused dependencies:

- `thiserror` for layer-owned errors.
- `chrono` for dates behind domain value objects.
- `diesel` and `diesel_migrations` for SQLite persistence.
- `serde`, `serde_yaml`, and `config` for scenario and runtime config.
- `uuid` for infrastructure-generated IDs.
- `rig-core` for the decision-agent adapter.
- `clap`, `tokio`, `tracing`, and `tracing-subscriber` for the binary edge.

Add strict crate-level lints in `src/lib.rs` and `src/main.rs`:

- Forbid unsafe code.
- Deny unwraps, expects, panics, todos, debug macros, direct stdout/stderr prints
  outside the interface boundary, unchecked casts, unchecked arithmetic, and
  missing docs for public APIs.
- Warn on `clippy::pedantic`, `clippy::nursery`, and `clippy::cargo`.

Keep `src/main.rs` tiny:

```rust
#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Delegate to rigagent::run() and translate the final error into an exit code.
}
```

Build `src/lib.rs::run()` as the binary coordinator:

- Load `.env` for local development.
- Initialize tracing.
- Parse CLI arguments.
- Print help and exit if no subcommand is supplied.
- Load config only for commands that need it.
- Dispatch to dedicated command runners.

Checkpoint:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

### Stage 02: Domain Model

Goal: encode retail rules in framework-free types.

Create `src/domain/retail/` and keep it independent from adapters and runtime
concerns.

Start with value objects:

- `Sku`: trim, reject empty values, canonicalize to uppercase.
- `Brand`: trim and reject empty display names.
- `ApparelKind` and `SizeLabel`: closed or validated product descriptors.
- `MoneyCents`, `SpaceUnits`, and `StockQuantity`: non-negative quantities with
  checked operations.
- `DemandRatePerDay` and `DemandBacklog`: fixed-point milli-units, never floats.
- `LeadTimeDays`, `DecisionHorizonDays`, and `SimulationDate`: bounded date and
  duration types.
- `SalesOrderId`, `RestockOrderId`, and `DecisionRunId`: typed identifiers.

Then add entities:

- `Product`: SKU, economics, space usage, demand rate, lead time, order bounds,
  and active state.
- `InventoryPosition`: on-hand units and carried fractional demand backlog.
- `RestockOrder`: supplier order with `Open`, `Received`, or `Cancelled`
  status.
- `SalesOrder`: simulated sale with requested, fulfilled, lost, revenue, and
  cost fields.
- `DecisionRun`: decision lifecycle with `Started`, `Completed`, or `Failed`
  status.

Put business behavior on the owning type:

- `product.unit_margin()`
- `product.restock_eta(current_date)`
- `product.bounded_order_quantity(quantity)`
- `inventory.receive_restock(quantity)`
- `inventory.fulfill_demand(requested_units)`
- `restock_order.receive(on_date)`
- `decision_run.complete(summary, created_count)`
- `decision_run.fail(summary)`

Add domain services only when behavior spans multiple entities:

- `DemandSimulator` converts demand rate plus backlog into whole requested units
  and a remaining backlog.
- `RestockOptionScorer` ranks replenishment candidates by expected gross profit
  per occupied space while respecting lead time, capacity, demand, order bounds,
  and inbound stock.

Tests to write in this stage:

- `rejects_empty_sku`
- `parses_decimal_demand_rate_without_float_arithmetic`
- `carries_fractional_demand_backlog`
- `rejects_restock_quantity_outside_product_bounds`
- `receives_open_restock_order`
- `rejects_receiving_cancelled_restock_order`
- `prevents_inventory_from_exceeding_capacity`
- `computes_profit_with_checked_cents_arithmetic`

Checkpoint:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### Stage 03: Application Layer

Goal: define use cases and ports without committing to Diesel, Rig, YAML, or
Clap.

Create `src/application/retail/` with:

- `commands.rs` for typed command inputs.
- `read_models.rs` for stable outputs.
- `ports.rs` for side-effect boundaries.
- `errors.rs` for `ApplicationError`.
- `use_cases.rs` for `RetailWorkflow`.

Commands:

- `SeedRetailScenario`: scenario path and reset flag.
- `AdvanceSimulation`: number of days to simulate.
- `RunRestockDecision`: horizon and max restock orders.
- `RunWorkflowCycle`: total days, decision interval, horizon, and max restock
  orders.

Ports:

- `RetailStore`: snapshots, scenario seeding, due restocks, sales-day writes,
  restock placement, open restocks, profit summary, and logical date
  advancement.
- `DecisionRunStore`: start, complete, fail, and load decision runs.
- `ReplenishmentDecisionAgent`: model/provider boundary for restock proposals.
- `Clock`: real dates when needed for decision/order timestamps.
- `IdGenerator`: typed IDs for sales orders, restock orders, and decision runs.

Use-case behavior:

- `seed_scenario` rejects existing state unless `reset` is true.
- `advance_simulation` receives due restocks before generating that day's sales.
- `run_restock_decision` starts a decision run, loads state, scores options,
  calls the agent port, validates proposals, persists accepted orders, and marks
  the run completed or failed.
- `run_workflow_cycle` decides on day zero, simulates one day at a time, and
  decides again on each configured interval.

Write hand-made fakes for application tests. Fakes should store domain types,
enforce the same uniqueness and transition expectations the use cases depend on,
and support explicit failure injection.

Tests to write in this stage:

- `seed_requires_reset_when_state_exists`
- `advance_simulation_receives_due_restock_before_sales`
- `advance_simulation_records_lost_sales`
- `run_decision_persists_accepted_agent_proposals`
- `run_decision_rejects_capacity_overflow_proposal`
- `run_decision_marks_run_failed_on_agent_error`
- `run_cycle_decides_on_day_zero_and_each_interval`

Checkpoint:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### Stage 04: Diesel Persistence

Goal: persist retail state behind application ports while keeping Diesel private
to infrastructure.

Create migrations under `migrations/`:

- `shop_state`: single logical shop row with current date and capacity.
- `products`: catalog rows and product economics.
- `inventory`: on-hand units and demand backlog per SKU.
- `sales_orders`: simulated customer demand and fulfillment records.
- `restock_orders`: supplier orders with ETA, status, decision run, and
  rationale.
- `decision_runs`: durable decision lifecycle records.

Create `src/infrastructure/persistence/`:

- `create_pool` builds an r2d2-backed SQLite pool.
- `run_migrations` runs embedded Diesel migrations.
- `DieselRetailStore` implements `RetailStore`.
- `DieselDecisionRunStore` implements `DecisionRunStore`.
- Diesel schema, row structs, and insert structs remain private to this module.

Mapping rules:

- Convert rows into domain/application types with `TryFrom`.
- Preserve invalid persisted data as `InfrastructureError::InvalidPersistedData`.
- Map Diesel not-found, unique violation, foreign-key violation, migration,
  pool, and serialization failures into structured errors.
- Wrap multi-write operations in Diesel transactions.

Infrastructure tests should use isolated temporary SQLite databases and real
migrations.

Checkpoint:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### Stage 05: Scenario YAML And Seeding

Goal: load a deterministic retail shop from human-readable data.

Create `data/retail_scenario.yaml`:

```yaml
shop:
  start_date: "2026-06-09"
  capacity_space_units: 240

products:
  - sku: "TSH-ACME-M-BLK"
    item_type: "shirt"
    brand: "Acme"
    size: "M"
    unit_cost_cents: 1200
    unit_price_cents: 2999
    space_units: 2
    initial_on_hand: 18
    daily_demand_rate: "2.750"
    restock_lead_time_days: 4
    min_order_quantity: 6
    max_order_quantity: 36
```

Create `src/infrastructure/scenario/`:

- Deserialize YAML into infrastructure DTOs only.
- Convert DTOs into domain types through checked constructors and `TryFrom`.
- Parse `daily_demand_rate` as a quoted fixed-point decimal string.
- Reject duplicate SKUs, empty product lists, invalid dates, invalid quantities,
  and initial inventory that exceeds capacity.

Wire scenario loading into the seed workflow:

- `seed --reset` recreates only the configured retail database state.
- `seed` without `--reset` fails when durable state already exists.
- Products, initial inventory, and shop state are inserted in one transaction.

Checkpoint:

```bash
cargo run -- seed --reset
cargo run -- simulate --days 3
cargo test --all-features
```

### Stage 06: Rig-Backed Decision Agent

Goal: let a model choose restocks while Rust keeps authority over durable state.

Create `src/infrastructure/agents/rig_replenishment/` and implement the
application `ReplenishmentDecisionAgent` port.

Important design rule: Rig tools operate on an in-memory `DecisionSession`.
Tools do not write directly to Diesel.

Decision request contents:

- Current retail snapshot.
- Ranked restock options from `RestockOptionScorer`.
- Open inbound restock orders.
- Profit summary.
- Decision horizon.
- Max accepted restock orders.

Suggested tool surface:

- `get_inventory_snapshot`
- `list_open_restock_orders`
- `analyze_restock_options`
- `place_restock_order`
- `get_profit_summary`

Prompt requirements:

- Inspect inventory, ranked options, open inbound orders, and profit summary.
- Place no more than the configured maximum restock orders.
- Include SKU, quantity, and short rationale for each proposal.
- Prefer high expected gross profit per space unit.
- Avoid duplicate inbound orders and capacity overflow.

After the model turn:

- Return proposals and summary text to the application use case.
- Let `RunRestockDecision` validate proposals again.
- Persist only accepted restock orders in an application transaction.
- Mark the decision run completed or failed.

Normal tests should not require OpenAI network access. Unit test tool argument
parsing, decision-session proposal recording, and adapter failure mapping with
fakes or narrow seams.

Checkpoint:

```bash
cargo test --all-features
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
```

### Stage 07: CLI, Config, And Workflow Runner

Goal: expose the workflow through a small command surface.

Implement `src/config.rs`:

- Load `config.yaml`.
- Apply environment overrides.
- Keep `OPENAI_API_KEY` optional at config-load time.
- Require `OPENAI_API_KEY` only when constructing the Rig adapter for `decide`
  or `run-cycle`.

Implement `src/interfaces/cli.rs`:

- `seed --reset`
- `simulate --days N`
- `decide --horizon-days N`
- `run-cycle --days N --decision-interval-days M`

Interface behavior:

- `cargo run` prints help and does not load config or mutate state.
- Count and interval arguments must be greater than zero.
- User-facing output is a concise one-line summary.
- Interface errors map application failures into clear exit codes.

Wire `src/lib.rs::run()`:

- Parse CLI.
- Load config for mutating commands.
- Create the SQLite pool.
- Run embedded migrations before commands that access retail state.
- Construct stores, ID generators, clocks, and the decision agent as needed.
- Dispatch to `RetailWorkflow`.

Checkpoint:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
OPENAI_API_KEY=sk-your-key cargo run -- run-cycle --days 14 --decision-interval-days 7
```

### Stage 08: Documentation, Cleanup, And Master

Goal: make the completed branch usable as both a reference implementation and a
teaching target.

Cleanup tasks:

- Remove obsolete support-chat, RAG, vector-store, and REPL runtime paths.
- Remove unused demo data from previous versions.
- Keep `Cargo.lock` committed.
- Keep generated databases and secrets ignored.
- Update `SPEC.md`, `ARCHITECTURE.md`, and this `README.md` when behavior
  changes.

Final validation:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run -- --help
cargo run -- seed --reset
cargo run -- simulate --days 3
```

With a valid provider key:

```bash
OPENAI_API_KEY=sk-your-key cargo run -- decide --horizon-days 14
OPENAI_API_KEY=sk-your-key cargo run -- run-cycle --days 14 --decision-interval-days 7
```

## Quality Bar

Before considering a stage complete, check these points:

- Business rules live in domain behavior, not in CLI handlers or persistence
  adapters.
- Application code coordinates use cases and depends on ports for side effects.
- Diesel, Rig, Clap, provider payloads, and config types do not leak into domain
  APIs.
- Model proposals are validated by application/domain code before persistence.
- Arithmetic is checked, saturating, or otherwise explicit.
- Errors are typed and preserve source failures.
- Tests use deterministic fakes for application ports and real migrations for
  Diesel adapter coverage.
- No `unsafe`, `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, `dbg`, or
  suppressed lints are used to bypass design problems.

## Current Command Reference

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

`seed` and `simulate` do not require `OPENAI_API_KEY`. `decide` and `run-cycle`
do require it because they construct the Rig-backed replenishment adapter.
