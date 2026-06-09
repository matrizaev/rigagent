# Retail Replenishment Decision Agent

Date: 2026-06-09
Status: Accepted

## Summary

Pivot the current Rig support-agent tutorial into a stateful autonomous retail
inventory workflow demo. The simulator maintains durable retail state in SQLite
through Diesel migrations and adapters: customer sales orders, current
inventory, and supplier restock orders with ETAs. A workflow agent runs scheduled
or CLI-triggered decision cycles, uses application ports to inspect state and
place restock orders, then writes restock orders that maximize expected gross
profit under limited stock-space capacity. This is not an assistant-style chat
agent and does not need a REPL.

## Key Changes

- Add CLI subcommands: `seed`, `simulate`, `decide`, and `run-cycle`. Default
  `cargo run` should show command help to avoid accidental database mutation.
- Introduce clean layers once the demo outgrows the current flat structure:
  - Domain: `Sku`, `Brand`, `ApparelKind`, `SizeLabel`, `MoneyCents`,
    `SpaceUnits`, `DemandRatePerDay`, `LeadTimeDays`, `StockQuantity`,
    `SimulationDate`, and behavior methods such as
    `inventory.receive_restock(order)` and `product.restock_window(horizon)`.
  - Application commands: `SeedRetailScenario`, `AdvanceSimulation`,
    `RunRestockDecision`, `GetRetailSnapshot`.
  - Application ports: `RetailStore`, `DecisionRunStore`,
    `ReplenishmentDecisionAgent`, `Clock`, and `IdGenerator`.
  - Infrastructure: `DieselRetailStore`, Diesel migrations,
    `ScenarioYamlLoader`, and a Rig-backed implementation of
    `ReplenishmentDecisionAgent`.
  - Interfaces: Clap CLI and the autonomous workflow runner.
- Add config keys: `retail_db_path`, `retail_scenario_path`,
  `decision_horizon_days`, `max_restock_orders_per_decision`.
- Add `data/retail_scenario.yaml` with shop capacity/start date and product
  rows: `sku`, `item_type`, `brand`, `size`, `unit_cost_cents`,
  `unit_price_cents`, `space_units`, `initial_on_hand`, `daily_demand_rate`,
  `restock_lead_time_days`, `min_order_quantity`, `max_order_quantity`.
- Add focused dependencies: `chrono` for dates, `thiserror` for layer-owned
  errors, `diesel` and `diesel_migrations` for persistence, and a Diesel
  connection-pool crate only if the workflow needs shared pooled connections.
- Add strict crate-level lint configuration required by `AGENTS.md`, including
  `forbid(unsafe_code)` and denies for unwraps, panics, todos, debug prints, and
  unchecked arithmetic/casts.
- Keep domain APIs free of Diesel, Rig, CLI, config, async runtime, and provider
  payload types. `SimulationDate` should be a domain value object backed by
  `chrono::NaiveDate` through checked constructors and canonical conversions.

## Data And Workflow

Diesel migrations define the SQLite schema:

- `shop_state(id, current_date, capacity_space_units)`.
- `products(sku, item_type, brand, size, unit_cost_cents, unit_price_cents,
  space_units, daily_demand_rate, restock_lead_time_days, min_order_quantity,
  max_order_quantity, active)`.
- `inventory(sku, on_hand, demand_backlog_fraction)`.
- `sales_orders(id, sale_date, sku, quantity_requested, quantity_fulfilled,
  revenue_cents, cost_cents, lost_units)`.
- `restock_orders(id, sku, quantity, ordered_at, eta_date, status,
  decision_run_id, rationale)`.
- `decision_runs(id, decision_date, horizon_days, status, summary,
  created_restock_count)`.

The Diesel adapter keeps generated schema, `Queryable`, `Insertable`,
`Selectable`, SQL constraints, and migration details private to
`src/infrastructure/`. Diesel rows map into domain types through checked
`TryFrom` conversions, and multi-write use cases run inside explicit Diesel
transactions.

Workflow:

- `seed --reset` recreates the retail DB from the scenario YAML.
- `simulate --days N` receives due restocks, generates deterministic demand from
  each SKU's `daily_demand_rate`, records fulfilled and lost sales, updates
  inventory, then advances the logical date.
- `decide --horizon-days N` creates a `decision_run`, invokes the
  `ReplenishmentDecisionAgent` application port, and lets the agent place
  restock orders without human chat input.
- `run-cycle --days N --decision-interval-days M` repeatedly advances the
  simulation and invokes the decision workflow on the configured interval.
- Rig tool names are implementation details of the Rig-backed
  `ReplenishmentDecisionAgent`: `get_inventory_snapshot`,
  `list_open_restock_orders`, `analyze_restock_options`,
  `place_restock_order`, `get_profit_summary`.
- `analyze_restock_options` performs deterministic scoring: expected
  incremental units sold within the horizon after ETA times unit margin, ranked
  by expected profit per occupied space and capped by capacity, lead time,
  min/max order quantities, and open inbound stock. The LLM chooses from ranked
  candidates and writes rationale; arithmetic stays in Rust.

## Errors And Boundaries

- Use `thiserror` for `DomainError`, `ApplicationError`, `InfrastructureError`,
  and interface errors. Preserve source errors with `#[source]` or `#[from]`.
- Represent absence and conflicts explicitly, for example
  `SkuNotFound { sku }`, `DuplicateRestockOrder { sku }`, and
  `DecisionRunNotFound { run_id }`.
- Map Diesel `NotFound`, unique violations, foreign-key violations, migration
  failures, pool failures, and serialization failures into structured
  infrastructure/application errors without leaking Diesel types across ports.
- Use constructor injection for stores, clocks, ID generators, scenario loaders,
  and decision-agent implementations. Do not use globals or hidden environment
  reads outside startup/config loading.

## Test Plan

- Domain tests for SKU/product validation, capacity constraints, ETA
  calculation, demand accrual, and restock status transitions.
- Application tests for seeding, simulation-day advancement, due-restock
  receipt, lost-sale recording, and no-over-capacity restock placement using
  hand-written in-memory fakes for `RetailStore`, `DecisionRunStore`, clocks,
  IDs, and decision agents.
- Decision tests using a fake `ReplenishmentDecisionAgent` to verify
  `RunRestockDecision` creates a decision run, persists rationale, and links
  placed restock orders.
- Diesel adapter tests against an isolated SQLite database using real migrations
  for scenario loading, sales writes, inventory updates, transactions,
  constraint violations, error-source preservation, and restock queries.
- Interface tests for CLI parsing, default help behavior, command-to-application
  mapping, exit codes, and error presentation.
- Final validation for implementation: `cargo fmt --all`,
  `cargo clippy --all-targets --all-features -- -D warnings`, and
  `cargo test --all-features`.

## Assumptions

- "Purchase orders" means customer sales orders; supplier replenishment orders
  are modeled separately as `restock_orders`.
- Profit is fulfilled revenue minus unit cost; v1 excludes holding cost,
  markdowns, supplier budget limits, and stochastic demand.
- Demand is deterministic for repeatable tutorial output.
- The agent is allowed to create restock orders, but all writes are validated by
  application/domain code.
- The existing support-order/RAG demo is not part of v1 retail behavior; the
  tutorial should pivot to workflow tools and SQLite state.
