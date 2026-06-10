# rigagent tutorial: 02 domain model

This branch is the third checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/01-runtime-cutover` and adds the framework-free retail
domain model: value objects, entities, domain errors, deterministic demand
simulation, and restock option scoring.

The command-line runtime still has placeholder command dispatch. There is no
application service, persistence adapter, scenario loader, or Rig agent yet.

## Current State

The repository now has a real domain center:

```text
rigagent/
+-- config.yaml
+-- src/
    +-- main.rs
    +-- lib.rs
    +-- config.rs
    +-- domain/
    |   +-- mod.rs
    |   +-- retail/
    |       +-- mod.rs
    |       +-- errors.rs
    |       +-- value_objects.rs
    |       +-- entities.rs
    |       +-- services.rs
    +-- application/
    |   +-- mod.rs
    +-- infrastructure/
    |   +-- mod.rs
    +-- interfaces/
        +-- mod.rs
        +-- cli.rs
```

The dependency direction is still simple:

```text
interfaces -> runtime shell
domain -> standard library + chrono + thiserror
```

The domain does not import application, infrastructure, interfaces, Diesel, Rig,
Clap, config loading, tracing, `tokio`, environment variables, provider DTOs, or
serde.

## What This Branch Adds

### Domain Errors

`src/domain/retail/errors.rs` defines `DomainError`, including variants for:

- Empty or invalid text fields.
- Non-positive or out-of-range numeric values.
- Arithmetic overflow and insufficient values.
- Restock quantities outside product bounds.
- Capacity overflow.
- Invalid restock-order transitions.
- Receiving restock orders before ETA.
- Invalid decision-run transitions.
- Invalid fixed-point demand backlog.
- Date arithmetic overflow.
- Fulfilled sales exceeding requested sales.
- Unit price below unit cost.

These errors describe business failures, not transport or adapter failures.

### Value Objects

`src/domain/retail/value_objects.rs` adds strong types for retail concepts:

- `Sku`
- `Brand`
- `ApparelKind`
- `SizeLabel`
- `MoneyCents`
- `SpaceUnits`
- `StockQuantity`
- `DemandRatePerDay`
- `DemandBacklog`
- `LeadTimeDays`
- `DecisionHorizonDays`
- `SimulationDate`
- `SalesOrderId`
- `RestockOrderId`
- `DecisionRunId`

Important rules:

- SKUs are trimmed, non-empty, and canonical uppercase.
- Demand uses fixed-point milli-units, not floats.
- Lead times and decision horizons are positive and bounded.
- Dates use checked arithmetic through `SimulationDate`.
- Money, space, and stock operations use checked arithmetic where overflow is
  possible.
- Standard traits such as `FromStr`, `TryFrom`, and `Display` define canonical
  conversions.

### Entities

`src/domain/retail/entities.rs` adds:

- `Product`
- `InventoryPosition`
- `SalesOrder`
- `RestockOrder`
- `DecisionRun`

Business behavior lives on the owning type:

- `product.unit_margin()`
- `product.restock_eta(current_date)`
- `product.bounded_order_quantity(quantity)`
- `inventory.receive_restock(quantity)`
- `inventory.apply_demand_simulation(simulation)`
- `inventory.occupied_space(product)`
- `restock_order.receive(on_date)`
- `decision_run.complete(summary, created_count)`
- `decision_run.fail(summary)`

Fields are private on domain entities. Construction and mutation go through
validated constructors and intention-revealing behavior methods.

### Domain Services

`src/domain/retail/services.rs` adds:

- `DemandSimulator`: converts daily demand plus carried backlog into whole
  requested units and a remaining backlog.
- `RestockOptionScorer`: scores deterministic restock candidates by expected
  gross profit and occupied stock-space.

The scorer keeps arithmetic in Rust. A later Rig adapter may choose among ranked
options, but the model will not own the business math.

## Runtime Behavior

The runtime shell still behaves as it did in the previous branch:

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

The test suite now covers both the runtime shell and domain behavior:

- Config loads without requiring `OPENAI_API_KEY`.
- CLI help renders with no subcommand.
- Zero day and interval arguments are rejected.
- Empty SKUs are rejected.
- Decimal demand rates parse without float arithmetic.
- Fractional demand backlog carries across days.
- Product prices below cost are rejected.
- Restock quantities are checked against product bounds.
- Inventory capacity is enforced.
- Restock orders receive only through valid transitions.
- Sales profit is computed with checked cents arithmetic.
- Restock options are scored and capped by product and capacity rules.

## Goal For The Next Branch

The next branch is `tutorial/03-application-layer`. It should add use cases and
ports around the domain model without introducing Diesel, Rig, YAML DTOs, or CLI
business logic.

When you finish the next branch, the repository should contain:

```text
src/application/
+-- mod.rs
+-- retail/
    +-- mod.rs
    +-- commands.rs
    +-- errors.rs
    +-- ports.rs
    +-- read_models.rs
    +-- use_cases.rs
```

The application layer should coordinate workflows and define outbound ports for
side effects. It should not know about database rows, Rig tools, Clap structs,
config loader internals, or provider payloads.

## Step By Step: Reach `tutorial/03-application-layer`

### 1. Create The Retail Application Module

Create:

```text
src/application/retail/mod.rs
src/application/retail/commands.rs
src/application/retail/errors.rs
src/application/retail/ports.rs
src/application/retail/read_models.rs
src/application/retail/use_cases.rs
```

Update `src/application/mod.rs`:

```rust
pub mod retail;
```

`mod.rs` should re-export the public application API that later infrastructure
and interface code will use.

### 2. Add Commands

In `commands.rs`, define typed command structs:

- `SeedRetailScenario`
  - scenario path.
  - reset flag.
- `AdvanceSimulation`
  - number of days to advance.
- `RunRestockDecision`
  - `DecisionHorizonDays`.
  - max restock orders.
- `RunWorkflowCycle`
  - total days.
  - decision interval days.
  - decision horizon.
  - max restock orders.

Keep CLI-specific argument structs out of the application layer. The interface
branch will map CLI args into these typed commands.

### 3. Add Read Models

In `read_models.rs`, define stable application outputs:

- `RetailSnapshot`
- `ProfitSummary`
- `AdvanceSimulationResult`
- `DecisionResult`
- `WorkflowCycleResult`
- `EventCount`
- accepted and rejected restock proposal models.

Read models may contain domain types such as `Sku`, `Product`,
`InventoryPosition`, `RestockOrder`, `SimulationDate`, and `MoneyCents`.
They should not contain Diesel rows, provider payloads, or Clap types.

### 4. Add Application Errors

In `errors.rs`, define `ApplicationError`.

It should include:

- `Domain(#[from] DomainError)`
- state already exists.
- SKU not found.
- inactive product.
- inventory not found.
- duplicate open restock order.
- capacity overflow proposal.
- proposal outside product bounds.
- missing decision run.
- non-positive command fields.
- count overflow.
- port failures for store, decision-run store, clock, and decision agent.

Application errors should preserve source failures from ports without depending
on concrete infrastructure error types. A small shared source wrapper is
acceptable when you need cloneable errors for tests and read models.

### 5. Define Ports

In `ports.rs`, define narrow traits around behavior, not generic CRUD.

Recommended ports:

- `RetailStore`
  - state exists.
  - load snapshot.
  - seed scenario.
  - receive due restocks.
  - record one sales day.
  - place accepted restock orders.
  - open restock orders.
  - profit summary.
  - advance logical shop date.
- `DecisionRunStore`
  - start decision run.
  - complete decision run.
  - fail decision run.
  - load decision run.
- `ReplenishmentDecisionAgent`
  - accept a typed decision request.
  - return proposed orders and a summary.
- `Clock`
  - return today's date where real calendar time is needed.
- `IdGenerator`
  - generate sales, restock, and decision IDs.

Use constructor injection later. Do not use globals or hidden environment reads.

### 6. Implement `RetailWorkflow`

In `use_cases.rs`, create a generic application service:

```text
RetailWorkflow<RetailStore, DecisionRunStore, ReplenishmentDecisionAgent, IdGenerator, Clock>
```

Use cases to implement:

- `seed_scenario`
  - fail if state exists and reset is false.
  - delegate durable seeding to the store port.
- `advance_simulation`
  - load current snapshot.
  - receive due restocks before sales.
  - run deterministic demand per active product.
  - create sales orders through generated IDs.
  - update inventory through domain behavior.
  - record the sales day.
  - advance the logical shop date.
- `run_restock_decision`
  - start a decision run.
  - load snapshot.
  - score deterministic restock options.
  - call the decision agent port.
  - validate every proposal against domain/application rules.
  - persist accepted orders.
  - complete or fail the decision run.
- `run_workflow_cycle`
  - run an initial decision on day zero.
  - simulate one day at a time.
  - run another decision each configured interval.

Keep transactions visible at the application/infrastructure boundary. The
application decides which operations must be atomic; the infrastructure branch
will implement that with Diesel.

### 7. Validate Agent Proposals In Application Code

The decision agent will not be trusted with durable writes.

Reject proposals when:

- SKU is unknown.
- product is inactive.
- quantity is zero.
- quantity is outside product min/max bounds.
- SKU already has an open restock order.
- projected inventory would exceed stock-space capacity.

Return rejected proposal details in `DecisionResult`. Do not persist rejected
proposals in this tutorial stage.

### 8. Add Deterministic Fakes For Tests

Application tests should not use Diesel, Rig, network, or filesystem state.

Add hand-written fakes for:

- `RetailStore`
- `DecisionRunStore`
- `ReplenishmentDecisionAgent`
- `Clock`
- `IdGenerator`

Fakes should store domain types, not database rows. They should enforce enough
behavior to keep tests honest and support explicit failure injection for the
important paths.

### 9. Add Application Tests

Good first tests:

- `seed_requires_reset_when_state_exists`
- `advance_simulation_receives_due_restock_before_sales`
- `advance_simulation_records_lost_sales`
- `run_decision_persists_accepted_agent_proposals`
- `run_decision_rejects_capacity_overflow_proposal`
- `run_decision_marks_run_failed_on_agent_error`
- `run_cycle_decides_on_day_zero_and_each_interval`

Test through public use-case behavior. Avoid tests that only restate private
helper logic.

### 10. Keep The Runtime Placeholder

The next branch may add the application layer without wiring it into the CLI.
It is acceptable for:

```bash
cargo run -- simulate --days 1
```

to still return the placeholder command error. CLI-to-application wiring belongs
to a later branch.

### 11. Validate The Next Checkpoint

Run:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run
```

Expected state at the end:

- Runtime help still works with no mutation.
- Domain tests still pass.
- Application tests pass using fakes only.
- Application APIs expose commands, read models, use cases, and ports.
- No application API exposes Diesel, Rig, Clap, YAML DTOs, or provider payloads.

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
