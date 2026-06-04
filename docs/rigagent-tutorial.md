# Build a Concise RigAgent Demo in Rust

This tutorial builds a terminal support agent with:

- OpenAI chat and embeddings through Rig.
- RAG over support documents stored in SQLite.
- Deterministic tools for listing orders and looking up order status.
- YAML configuration with environment-variable overrides.
- A small interactive terminal REPL.

The design deliberately supports one provider. Removing runtime provider
switching keeps configuration, agent construction, tests, and documentation
small. Add more providers only when the application actually needs them.

## Architecture

```text
rigagent/
├── config.yaml
├── data/knowledge_base.json
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── knowledge.rs
│   ├── rag.rs
│   ├── tools.rs
│   └── repl.rs
└── .env
```

Responsibilities:

- `config.rs`: deserialize YAML, then apply environment overrides.
- `knowledge.rs`: define searchable documents and their SQLite schema.
- `rag.rs`: embed documents and expose a SQLite vector index.
- `tools.rs`: deterministic order listing and status lookup.
- `repl.rs`: retain chat history and process terminal commands.
- `lib.rs`: construct and run the agent.

## 1. Create the Project

```bash
cargo new rigagent
cd rigagent
mkdir -p data
```

Use these dependencies:

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
config = { version = "0.15", default-features = false, features = ["yaml"] }
dotenvy = "0.15"
rig = { package = "rig-core", version = "0.38.1", features = ["derive"] }
rig-sqlite = "0.38.1"
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlite-vec = "0.1"
tokio = { version = "1", features = ["io-std", "io-util", "macros", "rt-multi-thread"] }
tokio-rusqlite = { version = "0.6", features = ["bundled"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

Key choices:

- `config` replaces manual environment parsing.
- Rig's `derive` feature enables `#[tool_macro]`, which generates tool schemas
  and `Tool` implementations.
- `rig-sqlite` is a companion crate; Rig core has no SQLite feature.

## 2. Configure the Application

Create `config.yaml` for non-secret defaults:

```yaml
chat_model: gpt-4o-mini
embedding_model: text-embedding-3-small
rag_db_path: data/agent.sqlite
rag_top_k: 3
```

Create `.env` for secrets:

```env
OPENAI_API_KEY=sk-your-key
```

Any environment variable can override the matching YAML field:

```bash
CHAT_MODEL=gpt-4o RAG_TOP_K=5 cargo run
```

Ignore secrets and generated DB files:

```gitignore
/target
.env
data/agent.sqlite*
```

### Typed Config Loader

Create `src/config.rs`:

```rust
use std::{num::NonZeroUsize, path::PathBuf};

use ::config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct AppConfig {
    pub openai_api_key: String,
    pub chat_model: String,
    pub embedding_model: String,
    pub rag_db_path: PathBuf,
    pub rag_top_k: NonZeroUsize,
}

impl AppConfig {
    pub fn load() -> Result<Self, ::config::ConfigError> {
        Config::builder()
            .add_source(File::with_name("config"))
            .add_source(
                Environment::default()
                    .ignore_empty(true)
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()
    }
}
```

Sources are merged in order. YAML loads first; environment variables override
it. Because `openai_api_key` is absent from YAML, startup fails when
`OPENAI_API_KEY` is missing.

`NonZeroUsize` also rejects `RAG_TOP_K=0` during deserialization.

## 3. Add Support Documents

Create `data/knowledge_base.json`:

```json
[
  {
    "id": "returns-accessories",
    "title": "Accessory Return Policy",
    "source": "support/policies/returns.md",
    "category": "policy",
    "content": "Accessories can be returned within 45 days of purchase."
  },
  {
    "id": "shipping-timeline",
    "title": "Shipping Timeline",
    "source": "support/fulfillment/shipping.md",
    "category": "shipping",
    "content": "Standard shipping usually leaves the warehouse within two business days."
  }
]
```

These are static support documents for RAG. Order information comes from a
tool, not this knowledge base.

## 4. Define Searchable Documents

Create `src/knowledge.rs`:

```rust
use rig::Embed;
use rig_sqlite::{Column, ColumnValue, SqliteVectorStoreTable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Embed, Serialize)]
pub struct SupportDoc {
    pub id: String,
    pub title: String,
    pub source: String,
    pub category: String,
    #[embed]
    pub content: String,
}

impl SqliteVectorStoreTable for SupportDoc {
    fn name() -> &'static str {
        "support_docs"
    }

    fn schema() -> Vec<Column> {
        vec![
            Column::new("id", "TEXT PRIMARY KEY"),
            Column::new("title", "TEXT"),
            Column::new("source", "TEXT"),
            Column::new("category", "TEXT").indexed(),
            Column::new("content", "TEXT"),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, Box<dyn ColumnValue>)> {
        vec![
            ("id", Box::new(self.id.clone())),
            ("title", Box::new(self.title.clone())),
            ("source", Box::new(self.source.clone())),
            ("category", Box::new(self.category.clone())),
            ("content", Box::new(self.content.clone())),
        ]
    }
}
```

Only `content` has `#[embed]`. Remaining fields are metadata available to the
model after retrieval.

## 5. Build the SQLite Vector Index

The RAG startup flow is:

1. Register the `sqlite-vec` extension before opening SQLite.
2. Open `data/agent.sqlite`.
3. Create the document and vector tables.
4. Embed seed docs when the table is empty.
5. Return a vector index for agent dynamic context.

Core code from `src/rag.rs`:

```rust
pub async fn prepare_index(
    config: &AppConfig,
    embedding_model: openai::EmbeddingModel,
    reindex: bool,
) -> anyhow::Result<SupportIndex> {
    initialize_sqlite_vec()?;

    if reindex && config.rag_db_path.exists() {
        std::fs::remove_file(&config.rag_db_path)?;
    }

    let conn = tokio_rusqlite::Connection::open(&config.rag_db_path).await?;
    let count_conn = conn.clone();
    let store: SqliteVectorStore<_, SupportDoc> =
        SqliteVectorStore::new(conn, &embedding_model).await?;

    if support_doc_count(&count_conn).await? == 0 {
        let docs: Vec<SupportDoc> =
            serde_json::from_str(&std::fs::read_to_string("data/knowledge_base.json")?)?;

        let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
            .documents(docs)?
            .build()
            .await?;

        store.add_rows(embeddings).await?;
    }

    Ok(store.index(embedding_model))
}
```

`sqlite-vec` requires a small unsafe registration wrapper because it exposes a C
extension initializer. Keep that isolated in `initialize_sqlite_vec`; see the
complete implementation in [`src/rag.rs`](../src/rag.rs).

Use `--reindex` after changing seed docs:

```bash
cargo run -- --reindex
```

## 6. Add Deterministic Tools

Tools perform operations where model guesses are unacceptable:

- Listing valid demo order IDs.
- Order status lookup.
- Live API or database reads.

### Generate Tools with `#[tool_macro]`

Rig 0.38.1 can generate the argument schema, tool type, and `Tool`
implementation from a normal function:

```rust
use rig::{tool::ToolError, tool_macro};
use serde::Serialize;

#[derive(Serialize)]
pub struct OrderStatus {
    pub order_id: String,
    pub found: bool,
    pub status: Option<String>,
}

#[tool_macro(description = "List existing order IDs.")]
pub fn list_orders() -> Result<Vec<String>, ToolError> {
    Ok(vec![
        "RIG-1001".to_string(),
        "RIG-1002".to_string(),
        "RIG-1003".to_string(),
    ])
}

#[tool_macro(
    description = "Look up a demo order and return its current fulfillment status.",
    params(order_id = "Demo order id such as RIG-1001, RIG-1002, or RIG-1003.")
)]
pub fn lookup_order_status(order_id: String) -> Result<OrderStatus, ToolError> {
    // Match the order id or call a real order service.
    todo!()
}
```

The macros generate `ListOrders` and `LookupOrderStatus` tool types. Register
both with the agent:

```rust
.tool(tools::LookupOrderStatus)
.tool(tools::ListOrders)
```

The model can call `list_orders` when the user does not know a valid demo order
ID, then pass one of those IDs to `lookup_order_status`. The lookup matches
`RIG-1001`, `RIG-1002`, and `RIG-1003`. Replace the tools' internals with a real
database or HTTP client later without changing their model-facing contracts.

See the complete tools in [`src/tools.rs`](../src/tools.rs).

## 7. Build the Agent

Create `src/lib.rs`. The central wiring is short because chat and embeddings
both use OpenAI:

```rust
let config = AppConfig::load()?;

let openai = openai::Client::new(config.openai_api_key.clone())?;
let embedding_model = openai.embedding_model(config.embedding_model.clone());
let index = rag::prepare_index(&config, embedding_model, cli.reindex).await?;

let agent = openai
    .agent(config.chat_model.clone())
    .preamble(repl::AGENT_PREAMBLE)
    .dynamic_context(config.rag_top_k.get(), index)
    .tool(tools::LookupOrderStatus)
    .tool(tools::ListOrders)
    .build();

repl::run(agent).await?;
```

The preamble tells the model when to retrieve docs and when to call tools:

```text
Use retrieved support documents when relevant.
Call lookup_order_status for order status, tracking, carrier, or fulfillment questions.
Call list_orders to see valid order ids for lookup_order_status.
Cite support document title and source when retrieved context informs an answer.
If context or tools do not contain the answer, say what is missing.
```

## 8. Add the Terminal REPL

Rig's `Chat` trait appends user, assistant, and tool messages to a mutable chat
history:

```rust
pub async fn run<A>(agent: A) -> anyhow::Result<()>
where
    A: rig::completion::Chat,
{
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut history = Vec::new();

    while let Some(line) = lines.next_line().await? {
        match line.trim() {
            "/quit" => break,
            "" => continue,
            prompt => println!("{}", agent.chat(prompt, &mut history).await?),
        }
    }

    Ok(())
}
```

The complete REPL adds `/help`, async stdout writes, and
user-friendly error handling. See [`src/repl.rs`](../src/repl.rs).

Keep `src/main.rs` minimal:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rigagent::run().await
}
```

## 9. Run the Demo

Create `.env`:

```bash
cp .env.example .env
```

Add a real OpenAI key, then build the initial vector index:

```bash
RUST_LOG=info cargo run -- --reindex
```

Try:

```text
What is the return policy for accessories?
Which orders can I look up?
Where is order RIG-1001?
```

Expected behavior:

- Policy questions retrieve support docs from SQLite.
- Questions about available demo orders call `list_orders`.
- Order questions call `lookup_order_status`.

## 10. Test and Validate

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo run -- --help
```

High-value tests:

- Environment variables override YAML.
- Missing `OPENAI_API_KEY` fails configuration.
- Listing orders returns the valid demo order IDs.
- Known and unknown order IDs return correct results.

## Where to Simplify Further

This demo still contains some unavoidable integration code:

- `SqliteVectorStoreTable` requires explicit columns and values.
- `sqlite-vec` requires extension registration.

Do not hide those boundaries behind broad utilities. They are useful places to
see what crosses into SQLite or the model.

For a production app, replace hardcoded order data with an infrastructure
adapter while keeping the model-facing tool contract stable.

## References

- Rig core: https://docs.rs/rig-core/latest/rig/
- Rig tool macro: https://docs.rs/rig-core/latest/rig/attr.tool_macro.html
- Rig tools: https://docs.rs/rig-core/latest/rig/tool/trait.Tool.html
- Rig dynamic context: https://docs.rs/rig-core/latest/rig/agent/index.html
- Rig-SQLite: https://docs.rs/rig-sqlite/latest/rig_sqlite/
- config-rs: https://docs.rs/config/latest/config/
