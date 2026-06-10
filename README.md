# rigagent tutorial: 01 runtime cutover

This branch is the second checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/00-start` and adds the runtime foundation: dependencies,
strict lint posture, typed configuration loading, a Clap command shell, empty
clean-architecture module boundaries, and tests for the behavior that exists so
far.

There is still no retail domain model, no database adapter, no scenario loader,
and no Rig agent implementation in this branch. Real workflow behavior begins
in later checkpoints.

## Current State

The repository now has the shape that later branches will fill in:

```text
rigagent/
+-- AGENTS.md
+-- Cargo.lock
+-- Cargo.toml
+-- config.yaml
+-- README.md
+-- src/
    +-- main.rs
    +-- lib.rs
    +-- config.rs
    +-- application/
    |   +-- mod.rs
    +-- domain/
    |   +-- mod.rs
    +-- infrastructure/
    |   +-- mod.rs
    +-- interfaces/
        +-- mod.rs
        +-- cli.rs
```

The binary entrypoint is intentionally thin. `src/main.rs` starts Tokio, calls
`rigagent::run()`, writes the final process error to stderr, and returns the
mapped exit code.

`src/lib.rs::run()` owns the early runtime flow:

1. Load `.env` for local development.
2. Initialize tracing.
3. Parse CLI arguments.
4. If no subcommand is present, render help and exit successfully.
5. Load common config only after a real subcommand is present.
6. Validate basic command arguments.
7. Return a placeholder error for commands that later branches will implement.

Run the branch:

```bash
cargo run
```

Expected behavior: command help is printed, config is not loaded, no database is
opened, and no state is mutated.

Try a parsed-but-unimplemented command:

```bash
cargo run -- simulate --days 1
```

Expected behavior: the CLI validates the command and reports that `simulate` is
not implemented until a later tutorial branch.

## Runtime Contracts

The branch introduces the final command surface:

```bash
cargo run
cargo run -- seed --reset
cargo run -- simulate --days 7
cargo run -- decide --horizon-days 14
cargo run -- run-cycle --days 30 --decision-interval-days 7
```

Only the shell exists in this checkpoint. These commands parse, but the real
use cases do not exist yet.

`config.yaml` contains the non-secret settings that later branches will use:

```yaml
chat_model: gpt-5-nano
retail_db_path: data/retail.sqlite
retail_scenario_path: data/retail_scenario.yaml
decision_horizon_days: 14
max_restock_orders_per_decision: 2
```

`OPENAI_API_KEY` is represented as an optional config field but is not required
for common config loading. That matters because `seed` and `simulate` will not
need provider access later.

Environment variables override YAML fields:

```bash
CHAT_MODEL=gpt-5-mini cargo run -- seed
```

This branch keeps the strict correctness lint posture: no unsafe code, unwraps,
expects, panics, todos, debug macros, unchecked casts, unchecked arithmetic, or
direct stdout/stderr printing outside the interface boundary. It enables
`clippy::pedantic` and `clippy::nursery`.

`clippy::cargo` is not enabled in this checkpoint because the planned dependency
set pulls duplicate transitive crates before this tutorial owns any adapter code
that can reduce that graph. Keep the correctness lints green first; revisit
dependency-tree cleanup when the infrastructure branches are in place.

## Validate This Branch

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

The test suite covers:

- Help rendering with no subcommand.
- Environment variables overriding YAML config.
- Loading common config without `OPENAI_API_KEY`.
- Rejecting zero simulation days.
- Rejecting a zero decision interval.
- Rejecting a zero default decision horizon.
- Parsing the final subcommand surface.

## Goal For The Next Branch

The next branch is `tutorial/02-domain-model`. It should add the retail domain
language and deterministic business rules, while still avoiding Diesel, Rig,
Clap, config, tracing, environment variables, async runtimes, and provider
payloads in the domain layer.

When you finish the next branch, the repository should contain:

- `src/domain/retail/mod.rs`
- `src/domain/retail/errors.rs`
- `src/domain/retail/value_objects.rs`
- `src/domain/retail/entities.rs`
- `src/domain/retail/services.rs`
- Domain tests for validation, state transitions, demand simulation, and
  restock scoring.

The CLI should still return placeholder errors for real commands. The goal of
the next branch is not to wire the application yet. It is to make the retail
rules explicit and testable in isolation.

## Step By Step: Reach `tutorial/02-domain-model`

### 1. Create The Retail Domain Module

Create the module tree:

```text
src/domain/
+-- mod.rs
+-- retail/
    +-- mod.rs
    +-- errors.rs
    +-- value_objects.rs
    +-- entities.rs
    +-- services.rs
```

Update `src/domain/mod.rs`:

```rust
pub mod retail;
```

Keep all domain files free of runtime and adapter concerns. Do not import
Diesel, Rig, Clap, config, tracing, `tokio`, provider DTOs, or environment
helpers.

### 2. Add `DomainError`

In `src/domain/retail/errors.rs`, define a crate-owned error enum with
`thiserror`.

Start with variants for:

- Empty or invalid text fields.
- Invalid dates.
- Invalid money, quantity, capacity, or demand values.
- Arithmetic overflow.
- Quantity outside a product's order bounds.
- Invalid restock-order status transitions.
- Invalid decision-run status transitions.

Errors should describe domain failures, not transport failures. For example,
prefer `QuantityOutsideBounds { requested, min, max }` over a generic string.

### 3. Add Strong Value Objects

In `value_objects.rs`, add types that make invalid state hard to construct.

Recommended first set:

- `Sku`: trim input, reject empty values, canonicalize to uppercase.
- `Brand`: trim input and reject empty display names.
- `ApparelKind`: closed enum such as `Shirt`, `Pants`, `Jacket`, `Dress`,
  `Shoes`, and `Accessory`.
- `SizeLabel`: validated size label.
- `MoneyCents`: non-negative cents.
- `SpaceUnits`: non-negative stock-space units.
- `StockQuantity`: non-negative item quantity.
- `DemandRatePerDay`: fixed-point milli-units per day.
- `DemandBacklog`: carried fixed-point milli-units.
- `LeadTimeDays`: positive bounded day count.
- `DecisionHorizonDays`: positive bounded day count.
- `SimulationDate`: checked date wrapper around `chrono::NaiveDate`.
- `SalesOrderId`, `RestockOrderId`, and `DecisionRunId`: typed IDs backed by
  validated strings.

Implement standard conversions where they fit:

- `TryFrom<&str>` or `FromStr` for parsed value objects.
- `TryFrom<u64>` or `TryFrom<i64>` for bounded numeric inputs.
- `Display` for canonical output.
- `From<T>` only when construction is infallible.

Do not add ad hoc helpers such as `to_domain`, `from_row`, or `as_model`.
Boundary conversions in later branches should use `From`, `TryFrom`,
`FromStr`, and `Display`.

### 4. Keep Arithmetic Explicit

The domain must not use floats for demand or scoring.

Use fixed-point milli-units:

```text
1 unit/day = 1000 milli-units/day
2.750 units/day = 2750 milli-units/day
```

Add methods that use checked or saturating arithmetic where overflow is
possible:

- checked money addition and multiplication by quantity.
- checked stock and space addition.
- checked stock subtraction for fulfillment.
- checked date addition for ETA calculations.

Avoid `as` casts. Use `TryFrom`, `From`, or typed constructors.

### 5. Add Product And Inventory Entities

In `entities.rs`, start with `Product` and `InventoryPosition`.

`Product` should own:

- SKU.
- apparel kind.
- brand.
- size.
- unit cost.
- unit price.
- space units.
- demand rate.
- restock lead time.
- min and max order quantities.
- active state.

Product behavior:

- `unit_margin()`
- `restock_eta(current_date)`
- `bounded_order_quantity(requested_quantity)`
- `is_active()`

`InventoryPosition` should own:

- SKU.
- on-hand quantity.
- demand backlog.

Inventory behavior:

- `receive_restock(quantity)`
- `fulfill_demand(requested_units)`
- `occupied_space(product)`
- backlog access and update through intention-revealing methods.

Keep fields private. Expose constructors, accessors, and behavior methods that
preserve invariants.

### 6. Add Sales, Restock Orders, And Decision Runs

Still in `entities.rs`, add the lifecycle entities.

`SalesOrder` records:

- ID.
- sale date.
- SKU.
- requested units.
- fulfilled units.
- lost units.
- revenue.
- cost.

Rules:

- Fulfilled units cannot exceed requested units.
- Lost units are requested minus fulfilled units.
- Revenue is fulfilled quantity times unit price.
- Cost is fulfilled quantity times unit cost.
- Gross profit is revenue minus cost.

`RestockOrder` records:

- ID.
- SKU.
- quantity.
- order date.
- ETA.
- status: `Open`, `Received`, or `Cancelled`.
- decision run ID.
- rationale.

Rules:

- New orders start as `Open`.
- Quantity must be positive.
- Rationale must be non-empty.
- `receive(on_date)` only succeeds for open orders whose ETA has arrived.
- Received or cancelled orders cannot be received again.

`DecisionRun` records:

- ID.
- decision date.
- horizon.
- status: `Started`, `Completed`, or `Failed`.
- summary.
- created restock count.

Rules:

- New runs start as `Started`.
- Started runs can complete or fail.
- Completed and failed runs cannot transition again.

### 7. Add Demand Simulation

In `services.rs`, add `DemandSimulator`.

It should convert:

```text
DemandRatePerDay + DemandBacklog -> requested whole units + next backlog
```

Example:

```text
rate = 1250 milli-units
backlog = 500 milli-units
total = 1750 milli-units
requested = 1 unit
next backlog = 750 milli-units
```

This service should be deterministic and synchronous. It should not know about
databases, clocks, CLI arguments, or model calls.

### 8. Add Restock Option Scoring

In `services.rs`, add `RestockOptionScorer`.

Inputs should be domain objects or small domain read models:

- products.
- current inventory positions.
- open inbound restock quantities.
- current simulation date.
- decision horizon.
- total stock-space capacity.

Eligibility rules:

- Product must be active.
- ETA must fall within the decision horizon.
- Remaining capacity must support at least the product minimum order quantity.
- Expected incremental sold units must be positive.

Quantity rules:

- Convert remaining free space into SKU units using product `space_units`.
- Cap by product `max_order_quantity`.
- Require at least product `min_order_quantity`.
- Validate through `product.bounded_order_quantity`.

Ranking rules:

- Primary: expected gross profit per occupied space.
- Tie-breaker: absolute expected gross profit.
- Final tie-breaker: SKU order.

Keep scoring deterministic. The LLM will choose among ranked options later, but
the arithmetic belongs in Rust.

### 9. Add Domain Tests

Write tests close to the domain behavior. Good first tests:

- `rejects_empty_sku`
- `canonicalizes_sku`
- `parses_decimal_demand_rate_without_float_arithmetic`
- `carries_fractional_demand_backlog`
- `rejects_product_price_below_cost`
- `rejects_restock_quantity_outside_product_bounds`
- `receives_open_restock_order`
- `rejects_receiving_cancelled_restock_order`
- `rejects_receiving_before_eta`
- `prevents_inventory_from_exceeding_capacity`
- `computes_profit_with_checked_cents_arithmetic`
- `ranks_restock_options_by_profit_per_space`

Test through public constructors and behavior methods. Avoid tests that only
repeat private implementation details.

### 10. Validate The Next Checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

Expected state at the end:

- `cargo run` still shows help with no mutation.
- The CLI still returns placeholder errors for real commands.
- Domain tests pass without requiring a database, OpenAI key, network, or async
  runtime.
- Domain APIs expose retail concepts and not adapter concerns.

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
