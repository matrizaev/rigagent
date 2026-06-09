# Run the Retail Replenishment Agent

This tutorial walks through the autonomous retail replenishment workflow in this
repository. The binary seeds a small apparel shop, advances deterministic sales
simulation, asks a Rig-backed decision agent for replenishment proposals, and
persists accepted restock orders through Diesel.

The first screen is the workflow itself:

```bash
cargo run
```

With no subcommand, the binary prints command help and performs no mutation.

## Architecture

```text
rigagent/
├── config.yaml
├── data/
│   └── retail_scenario.yaml
├── migrations/
└── src/
    ├── application/retail/
    ├── domain/retail/
    ├── infrastructure/
    │   ├── agents/rig_replenishment/
    │   ├── persistence/
    │   ├── scenario/
    │   └── ids.rs
    └── interfaces/cli.rs
```

The layers are intentionally narrow:

- `domain/retail`: products, inventory, sales, restock orders, decision runs,
  value objects, and deterministic scoring rules.
- `application/retail`: workflow commands, use cases, ports, validation, and
  transaction-oriented orchestration.
- `infrastructure/persistence`: Diesel adapters and embedded migrations.
- `infrastructure/scenario`: YAML seed-data loader.
- `infrastructure/agents/rig_replenishment`: Rig/OpenAI decision adapter and
  per-decision tools.
- `interfaces/cli`: Clap command parsing, config loading, adapter assembly, and
  user-facing command summaries.

Domain code does not depend on Diesel, Rig, Clap, environment variables, async
runtimes, or provider payloads.

## Configure the Demo

`config.yaml` contains non-secret defaults:

```yaml
chat_model: gpt-5-nano
retail_db_path: data/retail.sqlite
retail_scenario_path: data/retail_scenario.yaml
decision_horizon_days: 14
max_restock_orders_per_decision: 2
```

Set an OpenAI key only for commands that invoke the decision agent:

```env
OPENAI_API_KEY=sk-your-key
```

`seed` and `simulate` do not need `OPENAI_API_KEY`. `decide` and `run-cycle`
require it because they construct the Rig-backed decision adapter.

Any matching environment variable can override a YAML field:

```bash
CHAT_MODEL=gpt-5-mini cargo run -- decide --horizon-days 14
```

Generated local state is ignored by Git:

```gitignore
data/retail.sqlite*
.env
```

## Seed Retail State

Create or replace durable retail state from the scenario YAML:

```bash
cargo run -- seed --reset
```

The seed command runs embedded Diesel migrations, clears prior retail state when
`--reset` is passed, then loads `data/retail_scenario.yaml`. The scenario defines
the shop start date, stock-space capacity, product catalog, initial inventory,
demand rates, restock lead times, and order bounds.

Expected output looks like:

```text
seeded retail state from data/retail_scenario.yaml (reset: true)
```

## Simulate Sales

Advance the deterministic simulation without calling a model:

```bash
cargo run -- simulate --days 7
```

For each simulated day, the application service:

- Receives supplier restocks whose ETA is due.
- Computes deterministic demand from each product demand rate.
- Records fulfilled and lost sales.
- Updates inventory and carried fractional demand backlog.
- Advances the logical shop date.

The command prints a concise operational summary:

```text
advanced 7 day(s) to 2026-06-16; received 0 restock order(s), recorded 28 sale(s), lost 0 unit(s)
```

## Run One Replenishment Decision

Run one autonomous decision turn:

```bash
cargo run -- decide --horizon-days 14
```

This command requires `OPENAI_API_KEY`. It creates a decision run, gathers the
current retail snapshot, scores deterministic restock options, and invokes the
Rig-backed adapter. The adapter exposes implementation-detail tools to the
model:

- `get_inventory_snapshot`
- `list_open_restock_orders`
- `analyze_restock_options`
- `place_restock_order`
- `get_profit_summary`

The tools operate on an in-memory `DecisionSession`. They do not write directly
to Diesel. `place_restock_order` records proposed orders in the session after
basic validation. The application use case then validates proposals again
against domain rules, persists accepted orders in one transaction, and completes
or fails the decision run.

The decision prompt asks the model to inspect inventory, ranked options, open
inbound orders, and profit summary; propose no more than the configured maximum;
include SKU, quantity, and rationale for each proposal; and prefer high expected
gross profit per space unit while avoiding duplicate inbound orders and capacity
overflow.

## Run an Autonomous Cycle

Run repeated simulation and decision steps:

```bash
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

This command also requires `OPENAI_API_KEY`. It runs an initial restock decision,
then advances the simulation one day at a time and runs another decision each
time the simulated day count reaches the configured interval.

The workflow is autonomous: it does not open a chat session and does not require
operator input between decisions.

## Useful Checks

Run these before handing back changes:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run -- --help
cargo run -- seed --reset
cargo run -- simulate --days 3
```

With a valid `OPENAI_API_KEY`, also run:

```bash
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 14 --decision-interval-days 7
```

For local experimentation, reseed with `--reset` whenever you want to return the
database to the scenario baseline.
