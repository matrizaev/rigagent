# Repository Guidelines

## Project Structure & Module Organization

Rust application crate for `rigagent`. The repository is currently a small Cargo binary, so keep the structure simple until real boundaries appear. When the code grows, prefer explicit clean-architecture layers over a broad `utils` module:

- `src/main.rs`: process entrypoint only. Parse CLI/env, initialize telemetry/config, build adapters, run the app.
- `src/domain/`: framework-free business rules, aggregates, value objects, domain services, and domain errors.
- `src/application/`: use cases, commands, queries, ports/traits, orchestration, and transaction boundaries. Define outbound ports here when a use case needs persistence, clocks, IDs, model calls, process execution, or other side effects.
- `src/infrastructure/`: external adapters such as Diesel persistence, HTTP clients, filesystem, queues, clocks, IDs, and config loading.
- `src/interfaces/`: inbound adapters such as CLI, HTTP handlers, workers, or schedulers.
- `tests/`: integration tests that exercise public behavior through stable boundaries.
- `migrations/`: Diesel migrations for relational persistence once durable state is introduced.

Keep dependencies pointing inward: interfaces and infrastructure depend on application; application depends on domain; domain depends on the Rust standard library and deliberately chosen domain-only crates. Domain code must not know about HTTP, databases, environment variables, logging, or async runtimes unless that is the domain itself.

Do not let temporary structure harden into vague modules. When behavior appears, name the boundary by the business capability, for example `retail`, `runs`, `inventory`, `replenishment`, or `agents`, and keep APIs small enough that ownership is obvious.

## Build, Test, and Development Commands

- `cargo fmt --all`: format all Rust code.
- `cargo clippy --all-targets --all-features -- -D warnings`: lint all targets and deny warnings.
- `cargo test --all-features`: run unit, integration, and doc tests.
- `cargo run`: run the binary locally.
- `cargo doc --no-deps --all-features`: build crate docs when public APIs change.

Run `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` before handing back code changes. For docs-only changes such as `AGENTS.md`, do not run the full suite unless the docs include executable examples or build configuration changes; report `Not run (docs-only change)`.

## Rust Style & Naming

Use idiomatic Rust with small modules, explicit ownership, clear lifetimes, and narrow traits. Prefer `snake_case` for modules, files, functions, and fields; `PascalCase` for types, traits, and enum variants; `SCREAMING_SNAKE_CASE` for constants.

Model concepts by name instead of passing primitive strings and integers through the system. Prefer strong domain types such as `AgentId`, `RunId`, `WorkspacePath`, `Prompt`, `ModelName`, `ToolName`, `TokenBudget`, or `RetryPolicy` when they express invariants or prevent argument swaps. Keep fields private on domain aggregates and value objects; expose validated constructors, accessors, and behavior methods.

Avoid anemic domain models. Domain types should own the rules for their own valid state and transitions. Prefer behavior such as `run.cancel(reason)`, `inventory.receive_restock(order)`, or `product.restock_window(horizon)` over services that mutate public fields from the outside. Application services coordinate use cases; they should not become bags of business rules that belong on aggregates, entities, value objects, or domain services.

Make invalid states unrepresentable where practical:

- Use newtypes for IDs, names, paths, durations, counts, and externally supplied identifiers.
- Use enums for closed sets of states, actions, modes, providers, and outcomes.
- Use `NonZero*`, bounded constructors, and `TryFrom` for validated numeric values.
- Prefer typed request/command/query structs at boundaries over long parameter lists.
- Prefer `From`, `TryFrom`, `FromStr`, and `Display` for canonical conversions.
- Do not create ad hoc conversion helpers such as `to_domain`, `to_product`, `into_seed_state`, `try_into_seed_state`, `from_row`, or `as_model` when the conversion is exactly a `From`, `TryFrom`, `FromStr`, or `Display` implementation. For fallible DTO/row/reference conversions, implement `TryFrom<Dto>`, `TryFrom<&Dto>`, `TryFrom<Row>`, or `TryFrom<&Row>` as appropriate; callers should be able to use `.into()`, `.try_into()`, or `Type::try_from(...)`.
- Keep conversion implementations near the type or adapter that owns the boundary. Infrastructure DTOs and Diesel rows should convert into domain/application types through standard traits, not through Java-style `to_*` methods or private helper APIs that hide the conversion contract.
- Use private fields plus intention-revealing methods for mutation. Do not expose setters that bypass invariants.
- Keep DTOs, Diesel models, CLI structs, and provider payloads out of domain APIs. Map them at the layer boundary.

Do not use `unwrap` or `expect` in non-test code. If a value is statically known to be present, encode that fact in the type system (newtype, enum, `NonZero*`, infallible constructor) instead of asserting it at runtime. In tests, an `expect` message must explain the invariant being asserted.

Use `derive` intentionally. `Debug`, `Clone`, `Copy`, `Eq`, `Ord`, `Hash`, `Serialize`, and `Deserialize` are API commitments when exposed publicly; derive them only when they make sense for the type's role and privacy boundary.

## Lints and Forbidden Patterns

Treat lints as a correctness gate, not advisory noise. Configure maximum clippy strictness at the crate root and keep it green at every commit.

- Enable the strictest reasonable clippy profile in `Cargo.toml` or `clippy.toml`/`src/lib.rs`/`src/main.rs` via crate-level attributes:

  ```rust
  #![deny(
      unsafe_code,
      clippy::unwrap_used,
      clippy::expect_used,
      clippy::panic,
      clippy::todo,
      clippy::unimplemented,
      clippy::dbg_macro,
      clippy::print_stdout,
      clippy::print_stderr,
      clippy::indexing_slicing,
      clippy::integer_arithmetic,
      clippy::float_arithmetic,
      clippy::as_conversions,
      clippy::cast_possible_truncation,
      clippy::cast_possible_wrap,
      clippy::cast_sign_loss,
      clippy::missing_errors_doc,
      clippy::missing_panics_doc,
      missing_docs,
  )]
  #![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
  ```

- `unsafe` code is forbidden. Add `#![forbid(unsafe_code)]` at the crate root. If a specific dependency genuinely requires unsafe glue, isolate it in a clearly named module behind a reviewed exception and document the invariant.
- Do not silence clippy with `#[allow(...)]`, `#[expect(...)]`, or `--allow` flags to make code compile. Lints are signals — fix the underlying design instead. The only acceptable allows are:
  - Test-only modules where a specific lint conflicts with deliberate test ergonomics, scoped as narrowly as possible (`#[cfg_attr(test, allow(...))]` on the smallest item).
  - Generated code (Diesel `table!` macros, `prost`, `serde_derive` output) where the lint targets machinery outside our control; scope to the generated module only.

  Every allow must include a comment explaining why the rule does not apply and what alternative was rejected. Drive-by `#[allow]` on production code is a review blocker.
- Replace patterns that clippy flags rather than suppressing them: use `?`, `match`, `Option::ok_or`, `TryFrom`, `let else`, `get`/`get_mut`, checked arithmetic (`checked_*`, `saturating_*`, `wrapping_*`), and `From`/`TryFrom` instead of `as` casts.
- No `panic!`, `todo!`, `unimplemented!`, `unreachable!` (without proof), `dbg!`, or `println!`/`eprintln!` for diagnostics in non-test code. Use `tracing` for observability and typed errors for failures.
- Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check` in CI. A failing lint is a failing build.

## Error Handling

Use `thiserror` for crate-owned error enums. Keep errors close to the layer that owns the failure and convert across boundaries with `#[from]` or deliberate mapping.

- Domain errors describe violated business invariants or impossible transitions.
- Application errors describe use-case failures and wrap domain or port errors.
- Infrastructure errors preserve external failure detail from databases, HTTP, IO, serialization, or provider SDKs.
- Interface errors translate application failures into CLI exit codes, HTTP responses, worker retries, or user-facing messages.

Return `Result<T, LayerError>` from fallible functions. Do not collapse internal failures into `String` until crossing an external boundary. Use `?` for straight propagation through `From`/`#[from]` conversions, and implement `From<SourceError> for LayerError` whenever a source error can be wrapped without extra runtime context. Avoid routine `map_err` closures; they are a signal that an error conversion is missing. Use `map_err` only when the target variant must include contextual data that is unavailable to a `From` implementation, such as a field path, operation name, file path, SKU, migration version, or provider name. Prefer a small named helper over repeated inline `map_err` closures when that contextual mapping recurs. Use `tracing` spans/events for observability rather than burying diagnostics in error strings.

Preserve error information. When wrapping an external failure, keep the source error with `#[source]` or `#[from]`, and include typed context such as IDs, paths, provider names, migration versions, or operation names. Avoid `Box<dyn Error>`, `anyhow::Error`, or stringly typed errors in domain, application, and infrastructure code. `anyhow` is acceptable only at the binary edge for final process-level reporting during early bootstrapping.

Make absence explicit. Use dedicated variants such as `RunNotFound { run_id: RunId }`, `SkuNotFound { sku: Sku }`, or `DuplicateRestockOrder { sku: Sku }` instead of treating all storage failures as the same database error. Map infrastructure details into application errors without losing the original source.

## Clean Architecture Boundaries

Treat the domain as the center of the application. It owns business language and rules. Application code coordinates workflows but does not smuggle framework concerns into domain types. Infrastructure implements ports. Interfaces adapt external inputs to application commands and queries.

Use traits as ports only where they protect a meaningful boundary: persistence, clock, ID generation, model/provider calls, filesystem, queues, and process execution. Keep ports narrow and behavior-oriented. Avoid generic repository abstractions that merely mirror database CRUD if the use case needs richer behavior.

Prefer dependency injection through constructors and structs over globals. Inbound adapters should validate transport shape, build typed commands or queries, call a use case, and translate the result. They should not contain business rules.

Public APIs should communicate ownership, failure, and side effects clearly:

- Accept typed commands, queries, and value objects instead of loose parameter lists.
- Return domain entities, application DTOs, or read models that belong to the current layer.
- Keep async and transactions visible at application and infrastructure boundaries.
- Avoid leaking Diesel, HTTP, CLI, serialization, provider SDK, or runtime types into domain or application ports unless the port is explicitly about that technology.
- Prefer constructor injection with concrete structs over global state, singletons, service locators, or hidden environment reads.

## Persistence With Diesel

Use Diesel for relational persistence when durable application state is introduced. Diesel belongs in `src/infrastructure/` and `migrations/`; domain and application code must not depend on Diesel schemas, query builders, connection types, or generated models.

Persistence implementation guidelines:

- Define repository or store traits in `src/application/` around use-case behavior, for example `RetailStore`, `RunRepository`, or `DecisionRunStore`.
- Implement those traits in infrastructure with Diesel-backed adapters, for example `DieselRetailStore`.
- Keep Diesel table structs, `Queryable`, `Insertable`, `Selectable`, and migration concerns private to the infrastructure module where practical.
- Map Diesel rows to domain types through checked conversions. Use `TryFrom` when database data can violate domain invariants.
- Run multi-write use cases inside explicit Diesel transactions, and keep transaction boundaries in the application/infrastructure seam rather than scattered through domain methods.
- Map `diesel::result::Error::NotFound`, unique violations, foreign-key violations, migration failures, pool failures, and serialization failures into structured layer errors while preserving the original source.
- Prefer query methods that match business needs over generic CRUD. For example, use `open_restock_orders_for_sku`, `record_sales_day`, or `pending_decision_runs` instead of exposing arbitrary table access.
- Use Diesel migrations as the source of truth for schema changes. Keep schema evolution compatible with existing data or include deliberate migration steps.

Do not put SQL strings, table names, persistence IDs, or ORM assumptions into domain logic. If a database constraint mirrors a domain invariant, keep both: the domain prevents invalid construction, and the database protects persisted integrity.

## Test Doubles And Mock Repositories

Every application port should have a deterministic test double for use-case tests. Prefer hand-written in-memory fakes that model behavior and failure modes over brittle call-order mocks. Use a mocking crate only when interaction assertions are the behavior being tested.

For persistence ports:

- Provide an in-memory mock or fake repository that implements the same application trait as the Diesel adapter.
- Store domain types, not Diesel rows, inside the fake repository.
- Support explicit failure injection for important paths such as not found, duplicate, transaction failure, provider failure, and invalid persisted data.
- Keep fake behavior honest: enforce the same uniqueness, ordering, and state-transition expectations that application tests rely on.
- Cover the Diesel adapter separately with infrastructure tests against an isolated temporary database and real migrations.

## CQRS

Separate writes from reads when the distinction clarifies responsibility:

- Commands mutate state or trigger side effects. Name them imperatively, for example `StartRun`, `CancelRun`, `RecordToolResult`, or `UpdateAgentConfig`.
- Command handlers enforce invariants, coordinate transactions, publish events when needed, and return only what the caller needs to continue.
- Queries read state without mutation. Name them by the information they return, for example `GetRunStatus`, `ListAgentRuns`, or `FindPendingWork`.
- Query handlers may use read-optimized projections or DTOs, but those DTOs must not leak back into domain logic.

Do not force CQRS ceremony into trivial code. A simple function is better than a handler stack until there is a real read/write boundary, transaction boundary, or adapter boundary.

## Twelve-Factor App Practices

Configuration comes from the environment, CLI flags, or explicitly loaded config files at the process boundary. Keep config parsing in infrastructure or the entrypoint, then pass typed settings inward.

- Store deploy-specific values in env vars, not source code.
- Keep logs on stdout/stderr with structured `tracing`; do not write application logs to ad hoc files by default.
- Treat backing services as attached resources behind ports.
- Make startup deterministic and fail fast on invalid configuration.
- Keep processes stateless where practical; persist durable state through explicit adapters.
- Use graceful shutdown for long-running work and external connections.

Secrets must never be committed. Redact tokens, API keys, prompts containing sensitive data, and provider responses where logs could escape the local machine.

## Async, Concurrency, and Side Effects

Keep async at the edges unless the domain genuinely needs asynchronous behavior. Domain methods should usually be synchronous and deterministic. Use async in application and infrastructure for IO-bound work.

Use bounded concurrency and explicit cancellation for external calls, long-running agent work, and process execution. Make retry behavior typed and visible; retries should be idempotent or guarded by idempotency keys.

Abstract clocks, randomness, filesystem access, process execution, network calls, and ID generation behind ports when they affect business behavior or tests.

## Testing Guidelines

Favor fast, deterministic tests close to the behavior being protected.

- Domain tests cover invariants, state transitions, and value-object validation.
- Application tests use in-memory fakes for ports and verify command/query behavior.
- Infrastructure tests cover Diesel migrations, adapter mappings, serialization, persistence, transactions, constraint violations, and provider edge cases.
- Interface tests cover request parsing, response mapping, CLI exits, and error presentation.

Name tests by behavior, such as `rejects_empty_prompt`, `records_failed_tool_result`, or `query_does_not_mutate_run_state`. Add regression tests for every bug fix that changes behavior.

Test through public behavior. Avoid tests that only restate private implementation details. Use builders or fixtures when they clarify setup, but keep them typed and domain-valid by default. Include negative tests for invalid constructors, impossible transitions, persistence constraint mappings, and error-source preservation when a bug would otherwise be easy to hide.

## Dependency Guidelines

Prefer standard library types unless a crate materially improves correctness or clarity. Accept focused crates such as `thiserror`, `serde`, `tracing`, `tokio`, `uuid`, `time`, `url`, `camino`, `diesel`, `diesel_migrations`, `r2d2` or `deadpool-diesel`, and focused test-helper crates when they match the boundary being implemented.

Before adding a dependency, check whether it belongs in domain, application, infrastructure, or interfaces. Avoid pulling runtime, HTTP, database, or serialization dependencies into `src/domain/` unless the domain explicitly owns that format.

Commit `Cargo.lock` for this application crate. Keep dependency features narrow and explicit.

Choose crates for long-term maintainability, not convenience alone. Avoid broad framework dependencies when a small crate or explicit code would make boundaries clearer. Keep feature flags minimal, document non-obvious feature choices in `Cargo.toml`, and avoid duplicate libraries that solve the same problem unless there is a migration plan.

## Quality Bar

Before handing back Rust application changes, check for these failure modes:

- Business rules live in domain behavior, not in CLI handlers, database adapters, or loose helper functions.
- Invalid state is blocked by types, constructors, enums, and aggregate methods before it reaches persistence.
- Errors are typed, actionable, and source-preserving from the failing layer to the interface boundary.
- Application code depends on traits for external resources and can be tested with deterministic fakes.
- Diesel persistence is isolated behind application ports and verified with real migration-backed tests.
- Public APIs expose meaningful domain/application concepts rather than transport, ORM, or provider internals.
- Side effects, retries, transactions, concurrency limits, and cancellation are explicit.
- No `unsafe`, no `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` in non-test code, no `#[allow(...)]` on clippy or compiler lints to bypass design problems.
- `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` pass unless the change is docs-only.

## Commit & Pull Request Guidelines

Use concise imperative commit messages, for example `Add typed run commands` or `Validate workspace paths`. PRs should include summary, tests run, and any changes to public behavior, configuration, persistence, or external integrations.

## Agent-Specific Instructions

Before editing, inspect the relevant modules and current boundaries. Preserve clear responsibility splits and avoid broad refactors unless they are necessary for the requested change.

When adding new behavior, start from the domain language, then expose it through application commands or queries, then wire adapters. Prefer strong types and layer-owned errors from the first implementation rather than retrofitting them later.
