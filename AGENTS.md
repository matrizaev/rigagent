# Repository Guidelines

## Project Structure & Module Organization

Rust application crate for `rigagent`. The repository is currently a small Cargo binary, so keep the structure simple until real boundaries appear. When the code grows, prefer explicit clean-architecture layers over a broad `utils` module:

- `src/main.rs`: process entrypoint only. Parse CLI/env, initialize telemetry/config, build adapters, run the app.
- `src/domain/`: framework-free business rules, aggregates, value objects, domain services, and domain errors.
- `src/application/`: use cases, commands, queries, ports/traits, orchestration, and transaction boundaries.
- `src/infrastructure/`: external adapters such as persistence, HTTP clients, filesystem, queues, clocks, IDs, and config loading.
- `src/interfaces/`: inbound adapters such as CLI, HTTP handlers, workers, or schedulers.
- `tests/`: integration tests that exercise public behavior through stable boundaries.

Keep dependencies pointing inward: interfaces and infrastructure depend on application; application depends on domain; domain depends on the Rust standard library and deliberately chosen domain-only crates. Domain code must not know about HTTP, databases, environment variables, logging, or async runtimes unless that is the domain itself.

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

Make invalid states unrepresentable where practical:

- Use newtypes for IDs, names, paths, durations, counts, and externally supplied identifiers.
- Use enums for closed sets of states, actions, modes, providers, and outcomes.
- Use `NonZero*`, bounded constructors, and `TryFrom` for validated numeric values.
- Prefer typed request/command/query structs at boundaries over long parameter lists.
- Prefer `From`, `TryFrom`, `FromStr`, and `Display` for canonical conversions.

Avoid `unwrap` and `expect` in production paths. In tests, an `expect` message should explain the invariant being asserted.

## Error Handling

Use `thiserror` for crate-owned error enums. Keep errors close to the layer that owns the failure and convert across boundaries with `#[from]` or deliberate mapping.

- Domain errors describe violated business invariants or impossible transitions.
- Application errors describe use-case failures and wrap domain or port errors.
- Infrastructure errors preserve external failure detail from databases, HTTP, IO, serialization, or provider SDKs.
- Interface errors translate application failures into CLI exit codes, HTTP responses, worker retries, or user-facing messages.

Return `Result<T, LayerError>` from fallible functions. Do not collapse internal failures into `String` until crossing an external boundary. Use `?` for straight propagation, `map_err` when adding semantic context, and `tracing` spans/events for observability rather than burying diagnostics in error strings.

## Clean Architecture Boundaries

Treat the domain as the center of the application. It owns business language and rules. Application code coordinates workflows but does not smuggle framework concerns into domain types. Infrastructure implements ports. Interfaces adapt external inputs to application commands and queries.

Use traits as ports only where they protect a meaningful boundary: persistence, clock, ID generation, model/provider calls, filesystem, queues, and process execution. Keep ports narrow and behavior-oriented. Avoid generic repository abstractions that merely mirror database CRUD if the use case needs richer behavior.

Prefer dependency injection through constructors and structs over globals. Inbound adapters should validate transport shape, build typed commands or queries, call a use case, and translate the result. They should not contain business rules.

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
- Infrastructure tests cover adapter mappings, serialization, persistence, and provider edge cases.
- Interface tests cover request parsing, response mapping, CLI exits, and error presentation.

Name tests by behavior, such as `rejects_empty_prompt`, `records_failed_tool_result`, or `query_does_not_mutate_run_state`. Add regression tests for every bug fix that changes behavior.

## Dependency Guidelines

Prefer standard library types unless a crate materially improves correctness or clarity. Accept focused crates such as `thiserror`, `serde`, `tracing`, `tokio`, `uuid`, `time`, `url`, or `camino` when they match the boundary being implemented.

Before adding a dependency, check whether it belongs in domain, application, infrastructure, or interfaces. Avoid pulling runtime, HTTP, database, or serialization dependencies into `src/domain/` unless the domain explicitly owns that format.

Commit `Cargo.lock` for this application crate. Keep dependency features narrow and explicit.

## Commit & Pull Request Guidelines

Use concise imperative commit messages, for example `Add typed run commands` or `Validate workspace paths`. PRs should include summary, tests run, and any changes to public behavior, configuration, persistence, or external integrations.

## Agent-Specific Instructions

Before editing, inspect the relevant modules and current boundaries. Preserve clear responsibility splits and avoid broad refactors unless they are necessary for the requested change.

When adding new behavior, start from the domain language, then expose it through application commands or queries, then wire adapters. Prefer strong types and layer-owned errors from the first implementation rather than retrofitting them later.
