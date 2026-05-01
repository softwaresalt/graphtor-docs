//! Database schema management for the unified `CozoDB` store.
//!
//! Provides [`ensure_schema`], which creates the five stored relations
//! (`doc_sources`, `doc_chunks`, `doc_edges`, `doc_code`, `doc_vectors`) if
//! they do not already exist. The operation is idempotent — safe to call on
//! every startup.
//!
//! # Schema versions
//!
//! | Version | Change |
//! |---------|--------|
//! | 1       | Initial schema: `doc_sources`, `doc_chunks`, `doc_edges`, `doc_code` |
//! | 2       | Added `doc_vectors` for embedding storage and semantic search |

use std::collections::BTreeMap;

use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;

/// The current schema version stored in `doc_schema_ver`.
const SCHEMA_VERSION: i64 = 2;

/// Create all required stored relations and record the schema version.
///
/// Each relation is checked for existence before creation, so calling
/// `ensure_schema` multiple times on the same database is safe.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if any relation cannot be created.
pub fn ensure_schema(store: &DataStore) -> Result<(), GraphtorError> {
    create_if_missing(
        store,
        "doc_schema_ver",
        ":create doc_schema_ver { ver: Int }",
    )?;
    create_if_missing(
        store,
        "doc_sources",
        ":create doc_sources { source_id: String => url: String, kind: String, name: String, synced_at: String? }",
    )?;
    create_if_missing(
        store,
        "doc_chunks",
        ":create doc_chunks { chunk_id: String => source_id: String, path: String, title: String?, position: Int, char_offset: Int, headings: String, content: String }",
    )?;
    create_if_missing(
        store,
        "doc_edges",
        ":create doc_edges { src_chunk_id: String, target_path: String => link_text: String, anchor: String? }",
    )?;
    create_if_missing(
        store,
        "doc_code",
        ":create doc_code { snippet_id: String => chunk_id: String, language: String?, content: String }",
    )?;
    create_if_missing(
        store,
        "doc_vectors",
        ":create doc_vectors { chunk_id: String => embedding: String }",
    )?;

    upsert_schema_version(store, SCHEMA_VERSION)?;
    debug!("database schema verified at version {SCHEMA_VERSION}");
    Ok(())
}

/// Create a stored relation only if it does not already exist.
fn create_if_missing(
    store: &DataStore,
    relation_name: &str,
    ddl: &str,
) -> Result<(), GraphtorError> {
    if relation_exists(store, relation_name)? {
        debug!(
            relation = relation_name,
            "schema relation already exists, skipping"
        );
        return Ok(());
    }
    store.mutate(ddl, BTreeMap::new())?;
    debug!(relation = relation_name, "schema relation created");
    Ok(())
}

/// Return `true` if the named stored relation already exists in the database.
fn relation_exists(store: &DataStore, name: &str) -> Result<bool, GraphtorError> {
    let rows = store.query("::relations", BTreeMap::new())?;
    Ok(rows
        .rows
        .iter()
        .any(|row| row.first().and_then(|v| v.get_str()) == Some(name)))
}

/// Record the current schema version (upsert).
fn upsert_schema_version(store: &DataStore, ver: i64) -> Result<(), GraphtorError> {
    let script = "?[ver] <- [[$ver]] :put doc_schema_ver { ver }";
    let mut params = BTreeMap::new();
    params.insert("ver".to_string(), cozo::DataValue::Num(cozo::Num::Int(ver)));
    store.mutate(script, params)?;
    Ok(())
}
