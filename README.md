# rigagent tutorial: 03 application layer

This branch is the fourth checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/02-domain-model` and adds the application layer around the
retail domain: typed commands, read models, application errors, outbound ports,
use-case orchestration, proposal validation, and deterministic fakes for tests.

The command-line runtime still has placeholder command dispatch. There is no
Diesel adapter, scenario loader, Rig agent, or CLI-to-use-case wiring yet.

## Current State

The repository now has both the domain center and the application ring:

```text
rigagent/
+-- config.yaml
+-- src/
    +-- main.rs
    +-- lib.rs
    +-- config.rs
    +-- domain/
    |   +-- retail/
    |       +-- errors.rs
    |       +-- value_objects.rs
    |       +-- entities.rs
    |       +-- services.rs
    +-- application/
    |   +-- mod.rs
    |   +-- retail/
    |       +-- mod.rs
    |       +-- commands.rs
    |       +-- errors.rs
    |       +-- ports.rs
    |       +-- read_models.rs
    |       +-- use_cases.rs
    +-- infrastructure/
    |   +-- mod.rs
    +-- interfaces/
        +-- mod.rs
        +-- cli.rs
```

Dependency direction:

```text
application -> domain
interfaces -> runtime shell
```

The application layer does not import Diesel, Rig, Clap, config loader internals,
YAML DTOs, provider payloads, or infrastructure modules. It defines ports for
those side effects instead.

## What This Branch Adds

### Commands

`src/application/retail/commands.rs` defines typed use-case inputs:

- `SeedRetailScenario`
- `AdvanceSimulation`
- `RunRestockDecision`
- `RunWorkflowCycle`

These are application commands, not CLI parser structs. The CLI will map into
them in a later branch.

### Read Models

`src/application/retail/read_models.rs` defines stable outputs:

- `RetailSnapshot`
- `ProfitSummary`
- `AdvanceSimulationResult`
- `DecisionResult`
- `WorkflowCycleResult`
- `EventCount`
- accepted restock order models.
- rejected restock proposal models.

Read models may contain domain types such as `Sku`, `Product`,
`InventoryPosition`, `RestockOrder`, `SimulationDate`, and `MoneyCents`.
They do not contain database rows, provider payloads, or Clap types.

### Application Errors

`src/application/retail/errors.rs` defines `ApplicationError`.

It covers:

- domain failures.
- state already exists.
- SKU not found.
- inactive products.
- missing inventory.
- duplicate open restock orders.
- capacity overflow proposals.
- proposal quantities outside product bounds.
- non-positive command fields.
- count overflow.
- missing decision runs.
- port failures for stores, clocks, and the decision agent.

Port failures preserve source errors without coupling application code to
concrete infrastructure error types.

### Ports

`src/application/retail/ports.rs` defines outbound boundaries:

- `RetailStore`
- `DecisionRunStore`
- `ReplenishmentDecisionAgent`
- `Clock`
- `IdGenerator`

The ports are behavior-oriented. They do not expose generic table access or
adapter-specific types.

### Use Cases

`src/application/retail/use_cases.rs` adds `RetailWorkflow`, a generic
application service constructed from port implementations.

Implemented use cases:

- `seed_scenario`
  - rejects existing state unless reset is requested.
  - delegates durable seeding to the store port.
- `get_snapshot`
  - loads a read snapshot through the store port.
- `advance_simulation`
  - receives due restocks before demand.
  - simulates deterministic demand per product.
  - records sales and lost units.
  - updates inventory through domain behavior.
  - advances the logical shop date.
- `run_restock_decision`
  - starts a decision run.
  - scores deterministic restock options.
  - calls the decision-agent port.
  - validates every proposal in application/domain code.
  - persists accepted restock orders.
  - completes or fails the decision run.
- `run_workflow_cycle`
  - decides on day zero.
  - simulates one day at a time.
  - decides again on each configured interval.

### Proposal Validation

The application layer does not trust model proposals.

It rejects proposals when:

- SKU is unknown.
- product is inactive.
- quantity is zero.
- quantity is outside product min/max bounds.
- SKU already has an open restock order.
- projected inventory would exceed stock-space capacity.

Rejected proposal details are returned in `DecisionResult`. They are not
persisted in this checkpoint.

### Fake-Backed Tests

Application tests use hand-written fakes for:

- `RetailStore`
- `DecisionRunStore`
- `ReplenishmentDecisionAgent`
- `Clock`
- `IdGenerator`

The fakes store domain types, not database rows. They also support failure
injection for important paths.

## Runtime Behavior

The CLI is still intentionally not wired to the application layer:

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

CLI wiring belongs to a later checkpoint after infrastructure adapters exist.

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
- seeding requiring reset when state already exists.
- simulation receiving due restocks before sales.
- simulation recording lost sales.
- application propagation of store failures.
- ranked restock options by expected profit per occupied space.
- accepted decision-agent proposals being persisted.
- capacity overflow proposals being rejected.
- inactive product proposals being rejected.
- decision runs being marked failed on agent and transaction failures.
- workflow cycles deciding on day zero and on each interval.

## Goal For The Next Branch

The next branch is `tutorial/04-diesel-persistence`. It should add relational
persistence behind the application ports while keeping Diesel private to
infrastructure.

When you finish the next branch, the repository should contain:

```text
migrations/
+-- 2026-06-09-000001_create_retail_state/
    +-- up.sql
    +-- down.sql
src/infrastructure/
+-- mod.rs
+-- persistence/
    +-- mod.rs
    +-- schema.rs
```

The application layer should not change shape unless a missing port behavior is
discovered. The CLI may still return placeholder command errors in this branch.

## Step By Step: Reach `tutorial/04-diesel-persistence`

### 1. Add Diesel Migrations

Create:

```text
migrations/2026-06-09-000001_create_retail_state/up.sql
migrations/2026-06-09-000001_create_retail_state/down.sql
```

The schema should include:

- `shop_state`
  - one logical row.
  - current simulation date.
  - stock-space capacity.
- `products`
  - SKU primary key.
  - apparel type, brand, and size text.
  - money, space, demand, lead time, order bounds, and active fields.
- `inventory`
  - SKU primary key and product foreign key.
  - on-hand units.
  - fixed-point demand backlog.
- `sales_orders`
  - ID primary key.
  - sale date.
  - SKU foreign key.
  - requested, fulfilled, and lost units.
  - revenue and cost.
- `restock_orders`
  - ID primary key.
  - SKU foreign key.
  - quantity.
  - order date.
  - ETA.
  - status constrained to `open`, `received`, or `cancelled`.
  - decision run ID.
  - rationale.
- `decision_runs`
  - ID primary key.
  - decision date.
  - horizon days.
  - status constrained to `started`, `completed`, or `failed`.
  - summary.
  - created restock count.

Add database constraints that mirror important domain invariants. The domain
prevents invalid construction; the database protects persisted integrity.

### 2. Create The Persistence Module

Create:

```text
src/infrastructure/persistence/mod.rs
src/infrastructure/persistence/schema.rs
```

Update `src/infrastructure/mod.rs`:

```rust
pub mod persistence;
```

`schema.rs` should contain Diesel `table!` declarations. Keep it inside
infrastructure.

### 3. Add Infrastructure Errors

In `persistence/mod.rs`, define `InfrastructureError`.

Recommended variants:

- connection pool failure.
- migration failure.
- Diesel query failure.
- invalid persisted data.
- unique violation.
- foreign-key violation.
- serialization or transaction failure.

Preserve source errors where possible. Map infrastructure errors into
`ApplicationError` at the adapter boundary so application APIs do not expose
Diesel types.

### 4. Add Pool And Migration Helpers

Add:

- `create_pool(database_url: String)`
- `run_migrations(pool)`

Use an r2d2-backed SQLite pool. The pool is useful later because Rig tools and
workflow adapters need `Send + Sync` boundaries without sharing raw SQLite
connections.

Run embedded migrations before persistence-backed commands in a later branch.

### 5. Add Row And Insert Types

Inside the persistence module, define private row and insert structs for:

- shop state.
- products.
- inventory.
- sales orders.
- restock orders.
- decision runs.

Keep Diesel structs private where practical. They are adapter details, not
application or domain APIs.

### 6. Use Checked Row/Domain Conversion

Map rows into domain/application types through `TryFrom`.

Examples:

- product row -> `Product`
- inventory row -> `InventoryPosition`
- restock order row -> `RestockOrder`
- decision run row -> `DecisionRun`
- sales order row -> `SalesOrder`

When persisted data violates domain constructors, return
`InfrastructureError::InvalidPersistedData`.

Do not add ad hoc conversion helpers such as `to_domain`, `from_row`, or
`as_model`. Use `From`, `TryFrom`, `FromStr`, and `Display`.

### 7. Implement Store Adapters

Add:

- `DieselRetailStore`
- `DieselDecisionRunStore`

Implement the application ports:

- `RetailStore` for `DieselRetailStore`.
- `DecisionRunStore` for `DieselDecisionRunStore`.

Behavior to support:

- state existence checks.
- snapshot loading.
- receiving due restocks.
- recording one sales day.
- placing accepted restock orders.
- open restock queries.
- profit summary.
- logical shop date advancement.
- starting, completing, failing, and loading decision runs.

Wrap multi-write operations in Diesel transactions.

### 8. Keep Scenario Loading Out If Needed

The next branch after persistence is `tutorial/05-scenario-seeding`. If you want
to keep branch 04 focused, `RetailStore::seed_scenario` may temporarily return a
structured store failure such as "scenario loading is added in the next branch".

The important checkpoint for branch 04 is persistence mechanics: migrations,
row mapping, adapter errors, transactions, and read/write behavior against an
isolated SQLite database.

### 9. Add Infrastructure Tests

Use isolated SQLite databases for adapter tests. Prefer temporary files or
in-memory connections that run real embedded migrations before each test.

Good tests:

- migrations create the expected tables.
- decision runs can start, complete, fail, and reload.
- open restock orders query by status.
- received restocks update inventory and order status.
- recording a sales day writes sales and inventory in one transaction.
- failed multi-write operations roll back.
- invalid row data maps to invalid persisted data.
- uniqueness and foreign-key failures preserve useful source errors.

Normal tests should not call OpenAI and should not rely on developer machine
state.

### 10. Validate The Next Checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

Expected state at the end:

- Runtime help still works with no mutation.
- Domain and application tests still pass.
- Diesel adapter tests pass against isolated migrated SQLite databases.
- Diesel schema, row structs, and migrations stay inside infrastructure.
- Domain and application APIs do not expose Diesel types.

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
