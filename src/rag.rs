use std::fs;
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use rig::embeddings::EmbeddingsBuilder;
use rig::providers::openai;
use rig_sqlite::{SqliteVectorIndex, SqliteVectorStore};
use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;
use tokio_rusqlite::Connection;
use tracing::info;

use crate::config::AppConfig;
use crate::knowledge::SupportDoc;

const KNOWLEDGE_BASE_PATH: &str = "data/knowledge_base.json";

static SQLITE_VEC_EXTENSION: OnceLock<c_int> = OnceLock::new();

pub type SupportIndex = SqliteVectorIndex<openai::EmbeddingModel, SupportDoc>;

pub async fn prepare_index(
    config: &AppConfig,
    embedding_model: openai::EmbeddingModel,
    reindex: bool,
) -> Result<SupportIndex> {
    initialize_sqlite_vec()?;
    ensure_parent_dir(&config.rag_db_path)?;

    if reindex && config.rag_db_path.exists() {
        fs::remove_file(&config.rag_db_path).with_context(|| {
            format!(
                "failed to remove existing vector database {}",
                config.rag_db_path.display()
            )
        })?;
    }

    let conn = Connection::open(&config.rag_db_path)
        .await
        .with_context(|| format!("failed to open SQLite DB {}", config.rag_db_path.display()))?;
    let count_conn = conn.clone();
    let vector_store: SqliteVectorStore<_, SupportDoc> =
        SqliteVectorStore::new(conn, &embedding_model)
            .await
            .context("failed to initialize SQLite vector store")?;

    let existing_docs = support_doc_count(&count_conn).await?;
    if existing_docs == 0 {
        let docs = load_support_docs().context("failed to load support knowledge base")?;
        let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
            .documents(docs)
            .context("failed to stage support documents for embedding")?
            .build()
            .await
            .context("failed to embed support documents with OpenAI")?;

        let indexed = embeddings.len();
        vector_store
            .add_rows(embeddings)
            .await
            .context("failed to persist support embeddings in SQLite")?;
        info!(indexed, "indexed support documents");
    } else {
        info!(existing_docs, "using existing support document index");
    }

    Ok(vector_store.index(embedding_model))
}

fn initialize_sqlite_vec() -> Result<()> {
    let result = *SQLITE_VEC_EXTENSION.get_or_init(|| {
        // sqlite-vec requires auto-extension registration before SQLite connections open.
        unsafe {
            type SqliteExtensionInit = unsafe extern "C" fn(
                db: *mut rusqlite::ffi::sqlite3,
                pz_err_msg: *mut *mut c_char,
                api: *const rusqlite::ffi::sqlite3_api_routines,
            ) -> c_int;

            let init = std::mem::transmute::<*const (), SqliteExtensionInit>(
                sqlite3_vec_init as *const (),
            );
            sqlite3_auto_extension(Some(init))
        }
    });

    if result == rusqlite::ffi::SQLITE_OK {
        Ok(())
    } else {
        anyhow::bail!("failed to register sqlite-vec extension, SQLite code {result}")
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data directory {}", parent.display()))?;
    }

    Ok(())
}

fn load_support_docs() -> Result<Vec<SupportDoc>> {
    let json = fs::read_to_string(KNOWLEDGE_BASE_PATH)
        .with_context(|| format!("failed to read {KNOWLEDGE_BASE_PATH}"))?;
    let docs: Vec<SupportDoc> = serde_json::from_str(&json)?;
    if docs.is_empty() {
        anyhow::bail!("{KNOWLEDGE_BASE_PATH} must contain at least one support document");
    }

    Ok(docs)
}

async fn support_doc_count(conn: &Connection) -> Result<i64> {
    conn.call(|conn| {
        Ok(
            conn.query_row("SELECT COUNT(*) FROM support_docs", [], |row| {
                row.get::<_, i64>(0)
            })?,
        )
    })
    .await
    .context("failed to count indexed support documents")
}
