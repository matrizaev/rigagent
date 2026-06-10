# Specification

Last updated: 2026-06-10

## Product Definition

`rigagent` is an autonomous retail replenishment workflow for a simulated apparel
shop. It maintains durable state in SQLite, simulates deterministic daily demand,
and uses a Rig-backed decision agent to propose supplier restock orders. All
model proposals are validated by application and domain code before persistence.

The workflow is operated through CLI subcommands and has no chat loop.

## Goals

- Seed a deterministic retail scenario from YAML.
- Persist shop state, products, inventory, sales, restock orders, and decision
  runs in SQLite.
- Advance logical simulation time deterministically.
- Generate sales and lost-sales records from fixed-point demand rates.
- Score restock candidates using Rust business logic.
- Let the decision agent choose among ranked options and provide rationale.
- Reject invalid proposals before durable writes.
- Keep domain and application layers independent of frameworks and adapters.

## Non-Goals

- Stochastic demand forecasting.
- Holding costs, markdowns, supplier budgets, purchase approvals, or vendor
  calendars.
- Multi-store inventory.
- Human chat sessions or support-agent behavior.
- Persisting rejected proposals in v1.
- Network access during normal automated tests.

## CLI Contract

The binary supports these commands:

```bash
cargo run
cargo run -- seed [--reset]
cargo run -- simulate --days N
cargo run -- decide [--horizon-days N]
cargo run -- run-cycle --days N --decision-interval-days M [--horizon-days N]
```

Default behavior:

- Running with no subcommand prints help.
- Running with no subcommand must not load config, run migrations, open the
  database, or mutate state.

Common behavior:

- Mutating commands load `config.yaml` plus environment overrides.
- Commands that access retail state run embedded Diesel migrations before use.
- User-facing output is a concise one-line summary.
- Argument values that represent counts or intervals must be greater than zero.

Provider-key behavior:

- `seed` and `simulate` do not require `OPENAI_API_KEY`.
- `decide` and `run-cycle` require `OPENAI_API_KEY`.
- Missing provider key for decision commands exits as an interface argument
  failure.

## Configuration Contract

Required non-secret configuration:

```yaml
chat_model: gpt-5-nano
retail_db_path: data/retail.sqlite
retail_scenario_path: data/retail_scenario.yaml
decision_horizon_days: 14
max_restock_orders_per_decision: 2
```

Optional secret configuration:

```env
OPENAI_API_KEY=sk-...
```

Environment variables override matching YAML fields.

## Scenario YAML Contract

The scenario file contains shop state and product rows:

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

Validation requirements:

- `products` must not be empty.
- SKUs must be unique after canonicalization.
- Dates must parse as valid dates.
- Numeric values that represent money, space, stock, demand, or days must obey
  domain bounds.
- `max_order_quantity` must be greater than or equal to
  `min_order_quantity`.
- Initial inventory must fit within shop capacity.
- Demand rates are fixed-point decimal strings and are stored as milli-units.

## Domain Rules

### Product

- SKU and brand are non-empty.
- Unit price must be greater than or equal to unit cost.
- Minimum order quantity must be positive.
- Maximum order quantity must be greater than or equal to minimum order quantity.
- `bounded_order_quantity` rejects quantities outside min/max bounds.
- `restock_eta(current_date)` adds lead time with checked date arithmetic.
- Inactive products are not eligible for restock proposals.

### Inventory

- Inventory is tracked by SKU.
- On-hand stock and demand backlog are private state.
- Demand simulation is applied through intention-revealing domain behavior, not
  raw setters.
- Receiving restock increases on-hand units with checked arithmetic.
- Receiving with capacity enforcement rejects stock that would exceed total
  space capacity.
- Fulfilled demand is capped by on-hand stock.

### Demand

- Demand uses fixed-point milli-units.
- One unit equals `1000` milli-units.
- Daily demand plus backlog produces whole requested units and a fractional
  backlog for the next day.
- No floating-point arithmetic is used.

### Sales

- Fulfilled units must not exceed requested units.
- Revenue is fulfilled quantity times unit price.
- Cost is fulfilled quantity times unit cost.
- Lost units are requested minus fulfilled units.
- Gross profit is revenue minus cost.

### Restock Orders

- New orders start as `Open`.
- Quantity must be positive.
- Rationale must be non-empty after trimming.
- `receive(on_date)` is valid only for open orders whose ETA has arrived.
- Received or cancelled orders cannot be received again.

### Decision Runs

- New decision runs start as `Started`.
- Started runs can transition to `Completed` with a summary and created order
  count.
- Started runs can transition to `Failed` with a summary.
- Completed or failed runs cannot transition again.

## Restock Scoring

The deterministic scorer produces ranked restock options before the model is
called.

Eligibility:

- Product must be active.
- ETA must fall within the decision horizon.
- Current on-hand plus open inbound stock must not already exceed capacity.
- Remaining capacity must support at least the product minimum order quantity.
- Expected incremental sold units must be positive.

Quantity selection:

- Available free space is converted into SKU units using product `space_units`.
- Candidate quantity is capped by product `max_order_quantity`.
- Candidate quantity must be at least product `min_order_quantity`.
- Candidate quantity is validated through product bounds.

Expected value:

- Expected incremental units are capped by expected demand within the horizon.
- Expected profit is incremental units times unit margin.
- Occupied space is candidate quantity times product space units.

Ranking:

- Primary sort: expected gross profit per occupied space.
- Tie-breaker: absolute expected gross profit.
- Final tie-breaker: SKU order.

## Decision Proposal Validation

The decision agent returns proposed restock orders. The application validates
each proposal before persistence.

Reject a proposal when:

- SKU is unknown.
- Product exists but is inactive.
- Quantity is zero.
- Quantity is below product minimum or above product maximum.
- The SKU already has an open inbound restock order in persisted or projected
  state.
- Projected inventory plus open inbound stock plus the proposal would exceed
  shop capacity.
- The proposal cannot be converted into a valid domain `RestockOrder`.

Accepted proposals become supplier restock orders with:

- A generated `RestockOrderId`.
- `ordered_at` set from the `Clock` port's current date for the decision.
- `eta` set from product lead time.
- A link to the creating `DecisionRunId`.
- The model-provided rationale.

## Use-Case Contracts

### SeedRetailScenario

Inputs:

- Scenario path.
- Reset flag.

Behavior:

- If durable state exists and reset is false, fail with `StateAlreadyExists`.
- If reset is true, replace retail state.
- Load and validate scenario YAML.
- Insert shop state, products, and inventory in one transaction.

### AdvanceSimulation

Inputs:

- Number of days to advance.

Behavior for each simulated day:

- Load current logical date from shop state.
- Receive due restock orders before sales.
- Load the updated snapshot.
- For each product, simulate daily demand from demand rate and backlog.
- Apply demand simulation to inventory.
- Record one sales order per product.
- Persist sales and updated inventory in one transaction.
- Advance `shop_state.current_date` by one day.

Outputs:

- Final current date.
- Days advanced.
- Received restock count.
- Sales order count.
- Lost units.

### RunRestockDecision

Inputs:

- Decision horizon.
- Maximum accepted restock orders.

Behavior:

- Reject zero maximum orders.
- Load retail snapshot.
- Get decision date from `Clock`.
- Generate decision-run ID.
- Persist started decision run.
- Score ranked restock options.
- Load current open restock orders.
- Call `ReplenishmentDecisionAgent`.
- Validate proposals in order until the maximum accepted count is reached.
- Persist accepted orders.
- Complete decision run with summary and created order count.
- If the agent or durable write fails, mark the decision run failed and return
  the original error.

Outputs:

- Decision run ID.
- Accepted restock orders.
- Rejected proposals with reasons.
- Decision summary.

### RunWorkflowCycle

Inputs:

- Total simulation days.
- Decision interval days.
- Decision horizon.
- Maximum accepted restock orders per decision.

Behavior:

- Reject zero total days or zero decision interval.
- Run an initial decision before advancing days.
- Advance simulation one day at a time.
- Run another decision whenever elapsed simulated days are divisible by the
  decision interval.

Outputs:

- Days advanced.
- Decision run count.
- Final shop date.

## Persistence Contract

The SQLite schema is defined by Diesel migrations.

Tables:

- `shop_state(id, current_date, capacity_space_units)`
- `products(sku, item_type, brand, size, unit_cost_cents, unit_price_cents,
  space_units, daily_demand_milli_units, restock_lead_time_days,
  min_order_quantity, max_order_quantity, active)`
- `inventory(sku, on_hand, demand_backlog_milli_units)`
- `decision_runs(id, decision_date, horizon_days, status, summary,
  created_restock_count)`
- `sales_orders(id, sale_date, sku, quantity_requested, quantity_fulfilled,
  revenue_cents, cost_cents, lost_units)`
- `restock_orders(id, sku, quantity, ordered_at, eta_date, status,
  decision_run_id, rationale)`

Constraints:

- `shop_state` is a singleton with `id = 1`.
- Non-negative numeric fields are checked by the database.
- Positive day counts and positive restock quantities are checked by the
  database.
- Restock and decision statuses are constrained to known enum strings.
- Foreign keys protect SKU references and decision-run links.
- Open restock orders are indexed by SKU and ETA.

Adapter requirements:

- Diesel schema and row types stay inside infrastructure.
- Row conversions into domain/read-model types use checked conversions.
- Invalid persisted data is reported as structured infrastructure failure.
- Unique violations, foreign-key violations, not-found errors, pool failures,
  migration failures, and serialization failures preserve source errors.
- Multi-write operations use explicit transactions.

## Agent Adapter Contract

`RigReplenishmentDecisionAgent` implements `ReplenishmentDecisionAgent`.

The adapter:

- Builds a Rig/OpenAI agent only for decision commands.
- Uses the configured `chat_model` and `OPENAI_API_KEY`.
- Uses temperature `0.0` for deterministic behavior where the provider allows
  it.
- Exposes tools as adapter internals.
- Stores proposals in an in-memory `DecisionSession`.
- Returns proposals and summary to the application use case.

The adapter must not:

- Write restock orders directly to Diesel.
- Expose provider payloads to domain or application APIs.
- Require network access in normal unit tests.
- Log secrets or provider prompts as diagnostics.

## Error Contract

Errors are typed by layer:

- Domain failures describe invariant, arithmetic, and transition problems.
- Application failures describe use-case and port-level problems.
- Infrastructure failures preserve external source errors and adapter context.
- Interface failures map command/config/output problems to process exit codes.

Source preservation is required whenever an external failure is wrapped.

## Security And Operational Constraints

- Secrets are supplied through environment variables, not source files.
- `OPENAI_API_KEY` must not be logged.
- Provider prompts and responses should not be logged by default.
- Application logs use `tracing`.
- User-facing command summaries may be written to stdout by the interface layer.
- No `unsafe` code is allowed in crate-owned source.

## Acceptance Criteria

The implementation is acceptable when:

- `cargo run` prints help without mutating state.
- `cargo run -- seed --reset` creates the SQLite retail database from
  `data/retail_scenario.yaml`.
- `cargo run -- simulate --days 3` advances logical shop date, records sales,
  updates inventory, and reports lost units.
- `cargo run -- decide --horizon-days 14` creates a decision run, validates
  proposals, and persists accepted orders when `OPENAI_API_KEY` is valid.
- `cargo run -- run-cycle --days 14 --decision-interval-days 7` runs the
  autonomous workflow when `OPENAI_API_KEY` is valid.
- Domain code remains independent of infrastructure and interface concerns.
- Application ports have deterministic fakes in use-case tests.
- Diesel adapters are tested against real migrations.
- Rejected agent proposals are returned with reasons.
- Error chains preserve adapter sources.

Required local validation:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Additional manual validation:

```bash
cargo run -- --help
cargo run -- seed --reset
cargo run -- simulate --days 3
OPENAI_API_KEY=sk-... cargo run -- decide --horizon-days 14
OPENAI_API_KEY=sk-... cargo run -- run-cycle --days 14 --decision-interval-days 7
```
