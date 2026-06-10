# rigagent tutorial: 04 Diesel persistence

This branch is the fifth checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/03-application-layer` and adds Diesel-backed SQLite
persistence behind the application ports.

The command-line runtime still has placeholder command dispatch. Scenario YAML
loading, the Rig decision agent, and CLI-to-use-case wiring are still later
checkpoints.

## Current State

The repository now has infrastructure persistence:

```text
rigagent/
+-- config.yaml
+-- migrations/
|   +-- 2026-06-09-000001_create_retail_state/
|       +-- up.sql
|       +-- down.sql
+-- src/
    +-- domain/
    |   +-- retail/
    +-- application/
    |   +-- retail/
    +-- infrastructure/
    |   +-- mod.rs
    |   +-- persistence/
    |       +-- mod.rs
    |       +-- schema.rs
    +-- interfaces/
        +-- cli.rs
```

Dependency direction:

```text
infrastructure::persistence -> application -> domain
```

Diesel schema, row structs, insert structs, migrations, pools, and adapter error
mapping stay inside `src/infrastructure/persistence/`. Domain and application
APIs do not expose Diesel types.

## What This Branch Adds

### Migrations

`migrations/2026-06-09-000001_create_retail_state/up.sql` creates:

- `shop_state`
- `products`
- `inventory`
- `decision_runs`
- `sales_orders`
- `restock_orders`

The schema includes constraints that mirror important domain invariants:

- single-row shop state.
- non-negative money, stock, space, and demand values.
- positive lead times and order minimums.
- max order quantity greater than or equal to min order quantity.
- demand backlog below one fixed-point unit.
- known restock and decision-run statuses.
- fulfilled sales not exceeding requested sales.
- product and decision-run foreign keys.

`down.sql` drops those tables and indexes in reverse dependency order.

### Diesel Schema

`src/infrastructure/persistence/schema.rs` contains private Diesel `table!`
declarations and relationships. It is an infrastructure detail.

### Infrastructure Errors

`InfrastructureError` maps persistence failures into structured variants:

- not found.
- unique violation.
- foreign-key violation.
- migration failure.
- connection pool failure.
- serialization failure.
- invalid persisted data.

The adapter maps these into `ApplicationError` at the application boundary while
preserving source details.

### Pool And Migrations

The persistence module adds:

- `create_pool(database_url)`
- `run_migrations(pool)`

The pool is an r2d2-backed SQLite pool. Connections enable SQLite foreign-key
checks before use.

### Store Adapters

The branch adds:

- `DieselRetailStore`
- `DieselDecisionRunStore`

Implemented application ports:

- `RetailStore`
- `DecisionRunStore`

Supported behavior:

- state existence checks.
- snapshot loading.
- direct domain-state seeding through `seed_state`.
- due restock receipt.
- sales-day recording.
- restock order placement.
- open restock order queries.
- profit summary calculation.
- logical shop date advancement.
- decision-run start, complete, fail, and load.

Multi-write operations run in Diesel transactions.

### Seed Scenario Placeholder

`RetailStore::seed_scenario` is intentionally still a placeholder in this branch:

```text
scenario YAML loading is added in tutorial/05-scenario-seeding
```

This keeps branch 04 focused on persistence mechanics. The adapter already has a
`seed_state` method that tests can use with validated domain objects.

## Runtime Behavior

The CLI is still not wired to infrastructure:

```bash
cargo run
```

Expected behavior: command help is printed, no config is loaded, no database is
opened, and no state is mutated.

Real commands still return placeholder errors:

```bash
cargo run -- simulate --days 1
```

Expected behavior:

```text
simulate is parsed but not implemented until a later tutorial branch
```

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
- migrated SQLite state seeding through domain objects.
- shop date advancement.
- due restock receipt and inventory updates.
- sales-day recording and profit summary.
- transaction rollback on duplicate sale IDs.
- foreign-key violation mapping.
- unique violation mapping.
- invalid persisted product data mapping.
- explicit deferral of YAML scenario loading to the next branch.

## Goal For The Next Branch

The next branch is `tutorial/05-scenario-seeding`. It should add deterministic
YAML seed data and a scenario loader, then wire `DieselRetailStore::seed_scenario`
through that loader.

When you finish the next branch, the repository should contain:

```text
data/
+-- retail_scenario.yaml
src/infrastructure/
+-- scenario/
    +-- mod.rs
```

The CLI may still return placeholder command errors after the next branch. The
goal is seed-data loading and validation, not runtime command dispatch.

## Step By Step: Reach `tutorial/05-scenario-seeding`

### 1. Add The Scenario File

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

Add several products so simulation and restock scoring have meaningful variety:

- a fast-moving low-space item.
- a high-margin larger item.
- a moderate-demand footwear item.
- an accessory with high demand and low space cost.

Keep demand rates quoted strings. They are fixed-point decimal inputs, not
floats.

### 2. Create The Scenario Module

Create:

```text
src/infrastructure/scenario/mod.rs
```

Update `src/infrastructure/mod.rs`:

```rust
pub mod scenario;
```

The scenario module belongs in infrastructure because it owns file IO, YAML
DTOs, and external data shape.

### 3. Define YAML DTOs

In `scenario/mod.rs`, define private DTO structs:

- `ScenarioDocument`
- `ShopDto`
- `ProductDto`

Use `serde::Deserialize` on DTOs only. Do not derive serde traits on domain
types just to parse YAML.

Expected YAML fields:

- `shop.start_date`
- `shop.capacity_space_units`
- `products[].sku`
- `products[].item_type`
- `products[].brand`
- `products[].size`
- `products[].unit_cost_cents`
- `products[].unit_price_cents`
- `products[].space_units`
- `products[].initial_on_hand`
- `products[].daily_demand_rate`
- `products[].restock_lead_time_days`
- `products[].min_order_quantity`
- `products[].max_order_quantity`

### 4. Add Scenario Errors

Define `ScenarioError` with `thiserror`.

Recommended variants:

- read failure with path context.
- YAML parse failure with path context.
- domain conversion failure.
- duplicate SKU.
- empty product list.
- initial inventory exceeding capacity.

Preserve IO and YAML source errors.

### 5. Convert DTOs Into Domain State

Add `ScenarioYamlLoader::load(path) -> Result<SeedRetailState, ScenarioError>`.

Conversion rules:

- Parse `shop.start_date` into `SimulationDate`.
- Convert capacity into `SpaceUnits`.
- Convert each product row into `Product`.
- Convert initial inventory into `InventoryPosition`.
- Parse `daily_demand_rate` as a fixed-point decimal string using
  `DemandRatePerDay`.
- Canonicalize SKUs through `Sku`.
- Reject duplicate SKUs after canonicalization.
- Reject empty product lists.
- Reject capacity that cannot hold initial stock.

Use domain constructors and `TryFrom`/`FromStr`. Do not bypass invariants.

### 6. Wire Diesel Seeding

In `src/infrastructure/persistence/mod.rs`, replace the placeholder
`seed_scenario` method:

```rust
fn seed_scenario(&mut self, scenario_path: &Path, reset: bool) -> Result<(), ApplicationError> {
    let state = ScenarioYamlLoader::load(scenario_path)?;
    self.seed_state(&state, reset)?;
    Ok(())
}
```

Map `ScenarioError` into `ApplicationError::StoreFailure` with operation
`"load scenario"` or similarly explicit context.

### 7. Add Scenario Tests

Good tests:

- loads the bundled scenario file.
- rejects duplicate SKUs after canonicalization.
- rejects empty product lists.
- rejects invalid dates.
- rejects invalid demand decimal strings.
- rejects initial stock that exceeds shop capacity.

Use temporary files for negative YAML cases.

### 8. Add Persistence Integration For Scenario Seeding

Add or restore a persistence test:

```text
seeds_scenario_yaml_through_store_port
```

It should:

- create an isolated SQLite database.
- run real migrations.
- call `store.seed_scenario("data/retail_scenario.yaml", true)`.
- load a snapshot.
- assert the expected date, product count, and inventory count.

### 9. Validate The Next Checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

Expected state at the end:

- Runtime help still works with no mutation.
- Domain, application, and persistence tests still pass.
- Scenario loader tests pass without OpenAI or network access.
- `DieselRetailStore::seed_scenario` works through the YAML loader.
- Domain and application APIs still do not expose YAML DTOs or Diesel rows.

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
