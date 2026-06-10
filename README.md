# rigagent tutorial: 05 scenario seeding

This branch is the sixth checkpoint in the `rigagent` build-along tutorial. It
starts from `tutorial/04-diesel-persistence` and adds deterministic YAML seed
data plus an infrastructure scenario loader. `DieselRetailStore::seed_scenario`
now loads YAML, converts DTOs into validated domain objects, and seeds SQLite
through the persistence adapter.

The command-line runtime still has placeholder command dispatch. The Rig
decision agent and CLI-to-use-case wiring are still later checkpoints.

## Current State

The repository now includes scenario seeding:

```text
rigagent/
+-- config.yaml
+-- data/
|   +-- retail_scenario.yaml
+-- migrations/
+-- src/
    +-- domain/
    |   +-- retail/
    +-- application/
    |   +-- retail/
    +-- infrastructure/
    |   +-- mod.rs
    |   +-- persistence/
    |   +-- scenario/
    |       +-- mod.rs
    +-- interfaces/
        +-- cli.rs
```

Dependency direction:

```text
infrastructure::scenario -> infrastructure::persistence::SeedRetailState
infrastructure::scenario -> domain
infrastructure::persistence -> application -> domain
```

The scenario module owns YAML DTOs, file IO, and external seed-data shape.
Domain and application APIs still do not expose YAML DTOs or Diesel rows.

## What This Branch Adds

### Seed Data

`data/retail_scenario.yaml` defines:

- shop start date.
- stock-space capacity.
- product catalog.
- initial inventory.
- demand rates.
- restock lead times.
- min and max order quantities.

The bundled scenario includes four apparel products with different economics
and space constraints so later simulation and restock scoring have useful input.

Demand rates are quoted fixed-point decimal strings:

```yaml
daily_demand_rate: "2.750"
```

They are parsed into domain milli-units. The loader does not use float
arithmetic.

### Scenario Loader

`src/infrastructure/scenario/mod.rs` adds:

- private YAML DTOs.
- `ScenarioYamlLoader`.
- `ScenarioError`.
- checked DTO-to-domain conversion.

Validation rules:

- scenario file must be readable.
- YAML must parse.
- product list must not be empty.
- dates must be valid.
- numeric fields must be non-negative where appropriate.
- SKUs are canonicalized through `Sku`.
- duplicate SKUs are rejected after canonicalization.
- demand rates are parsed through `DemandRatePerDay`.
- product rows are validated through `Product::from_details`.
- initial inventory is converted into `InventoryPosition`.
- initial occupied space must fit within configured shop capacity.

### Persistence Wiring

`DieselRetailStore::seed_scenario` now:

1. Loads `SeedRetailState` through `ScenarioYamlLoader`.
2. Seeds SQLite through `seed_state`.
3. Runs the write in a persistence transaction.

Scenario failures map into `ApplicationError::StoreFailure` with source details
preserved.

## Runtime Behavior

The CLI is still not wired to the application layer:

```bash
cargo run
```

Expected behavior: command help is printed, no config is loaded, no database is
opened, and no state is mutated.

Real commands still return placeholder errors:

```bash
cargo run -- seed --reset
```

Expected behavior:

```text
seed is parsed but not implemented until a later tutorial branch
```

CLI wiring belongs to `tutorial/07-cli-workflow`.

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
- loading valid scenario YAML.
- duplicate SKU rejection.
- empty product list rejection.
- negative field rejection.
- initial capacity validation.
- seeding the bundled YAML through `DieselRetailStore::seed_scenario`.

## Goal For The Next Branch

The next branch is `tutorial/06-rig-decision-agent`. It should add the
Rig-backed implementation of the `ReplenishmentDecisionAgent` application port.

When you finish the next branch, the repository should contain:

```text
src/infrastructure/
+-- agents/
    +-- mod.rs
    +-- rig_replenishment/
        +-- mod.rs
```

The CLI may still return placeholder command errors. The goal is the decision
agent adapter, not runtime wiring.

## Step By Step: Reach `tutorial/06-rig-decision-agent`

### 1. Create The Agent Module

Create:

```text
src/infrastructure/agents/mod.rs
src/infrastructure/agents/rig_replenishment/mod.rs
```

Update `src/infrastructure/mod.rs`:

```rust
pub mod agents;
```

The agent adapter belongs in infrastructure because it owns provider SDK types,
prompt shape, Rig tools, and provider failure mapping.

### 2. Implement The Application Port

In `rig_replenishment/mod.rs`, define `RigReplenishmentDecisionAgent`.

It should implement:

```rust
ReplenishmentDecisionAgent
```

Constructor inputs:

- OpenAI API key.
- chat model name.

The adapter should construct provider clients only when a decision command needs
the agent in a later branch. `seed` and `simulate` must not require
`OPENAI_API_KEY`.

### 3. Add A Per-Decision Session

Create an in-memory decision session that contains:

- current `RetailSnapshot`.
- ranked deterministic `RestockOption` values.
- open restock orders.
- profit summary.
- max allowed proposals.
- proposals collected during the model turn.

Tools should operate on this session. They should not write directly to Diesel.

### 4. Add Tool Surface

Expose implementation-detail tools to the model:

- `get_inventory_snapshot`
- `list_open_restock_orders`
- `analyze_restock_options`
- `place_restock_order`
- `get_profit_summary`

Tool behavior:

- Snapshot tools return concise structured summaries.
- `analyze_restock_options` returns deterministic candidates scored by Rust.
- `place_restock_order` records a proposed SKU, quantity, and rationale in the
  in-memory session.
- Tools return validation feedback, but durable validation still happens in the
  application use case.

### 5. Write The Prompt

Use an autonomous workflow prompt, not a chat-support prompt.

The prompt should tell the model to:

- inspect inventory.
- inspect ranked restock options.
- inspect open inbound orders.
- inspect profit summary.
- place no more than the configured maximum number of restock orders.
- include SKU, quantity, and short rationale for each proposal.
- prefer high expected gross profit per occupied stock-space.
- avoid duplicate inbound orders.
- avoid capacity overflow.

Do not log provider prompts or sensitive payloads.

### 6. Return Application Models

After the model turn, the adapter should return:

```rust
DecisionAgentResponse {
    proposed_orders,
    summary,
}
```

The returned proposals are not trusted durable writes. `RetailWorkflow` already
validates them before persistence.

### 7. Map Provider Failures

Provider and tool failures should map into `ApplicationError::AgentFailure`.

Preserve the source error when available. Avoid collapsing failures into plain
strings except at the outermost display boundary.

### 8. Add Adapter Tests Without Network

Normal tests must not call OpenAI.

Good tests:

- tool argument parsing records a proposal.
- proposal quantity must be positive.
- max proposal count is enforced in the session.
- adapter failure mapping preserves an agent failure.
- formatting of inventory/options/profit summaries is deterministic enough for
  tests.

Use fakes or narrow seams for provider behavior. Keep real network calls out of
`cargo test`.

### 9. Keep Application Boundaries Clean

Do not put Rig types into:

- domain APIs.
- application commands.
- application read models.
- application ports.

The only public application-facing type should remain the
`ReplenishmentDecisionAgent` port and its request/response structs.

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
- Domain, application, persistence, and scenario tests still pass.
- Agent adapter tests pass without network access.
- The Rig adapter implements `ReplenishmentDecisionAgent`.
- Durable writes still happen only in the application/persistence path.

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
