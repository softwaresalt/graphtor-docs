//! Database schema management for the unified `CozoDB` store.
//!
//! Provides [`ensure_schema`], which creates or migrates the stored relations
//! to the current v3 schema and ensures the HNSW vector index is present.
//! The operation is idempotent — safe to call on every startup.
//!
//! # Schema versions
//!
//! | Version | Change |
//! |---------|--------|
//! | 1       | Initial schema: `doc_sources`, `doc_chunks`, `doc_edges`, `doc_code` |
//! | 2       | Added `doc_vectors` for embedding storage and semantic search |
//! | 3       | Merged `embedding: <F32; 384>?` into `doc_chunks`; removed `doc_vectors`; added HNSW index |

use std::collections::BTreeMap;

use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;

/// The current schema version stored in `doc_schema_ver`.
const SCHEMA_VERSION: i64 = 3;

/// DDL string for the v3 `doc_chunks` relation (includes `embedding` column).
const DOC_CHUNKS_DDL: &str =
    ":create doc_chunks { chunk_id: String => source_id: String, path: String, \
     title: String?, position: Int, char_offset: Int, headings: String, content: String, \
     embedding: <F32; 384>? }";

/// Create all required stored relations, migrate existing data if needed, and
/// ensure the HNSW vector index is present.
///
/// Safe to call on every startup — the operation is idempotent.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if any relation cannot be created or if
/// the migration fails.
pub fn ensure_schema(store: &DataStore) -> Result<(), GraphtorError> {
    create_if_missing(
        store,
        "doc_schema_ver",
        ":create doc_schema_ver { ver: Int }",
    )?;
    let ver = get_schema_version(store)?;

    if ver < SCHEMA_VERSION {
        // Fresh installation or pre-v3 database: run migration if needed.
        if relation_exists(store, "doc_chunks")? {
            // Existing database: migrate doc_chunks to the v3 schema.
            migrate_to_v3(store, ver)?;
        }
        upsert_schema_version(store, SCHEMA_VERSION)?;
    }

    // Always create any missing base relations (idempotent self-heal).
    create_all_relations(store)?;
    // Always ensure the HNSW index is present (idempotent self-heal).
    create_hnsw_index_if_missing(store)?;

    debug!("database schema verified at version {SCHEMA_VERSION}");
    Ok(())
}

/// Create the four permanent stored relations using `create_if_missing`.
///
/// Each relation is checked for existence before creation so this function
/// is safe to call multiple times.
fn create_all_relations(store: &DataStore) -> Result<(), GraphtorError> {
    create_if_missing(
        store,
        "doc_sources",
        ":create doc_sources { source_id: String => url: String, kind: String, \
         name: String, synced_at: String? }",
    )?;
    create_if_missing(store, "doc_chunks", DOC_CHUNKS_DDL)?;
    create_if_missing(
        store,
        "doc_edges",
        ":create doc_edges { src_chunk_id: String, target_path: String => \
         link_text: String, anchor: String? }",
    )?;
    create_if_missing(
        store,
        "doc_code",
        ":create doc_code { snippet_id: String => chunk_id: String, \
         language: String?, content: String }",
    )?;
    Ok(())
}

/// Migrate an existing pre-v3 database to the v3 schema.
///
/// Exports all `doc_chunks` rows, drops the old relation (and `doc_vectors`
/// for v2 databases), recreates `doc_chunks` with the `embedding: <F32; 384>?`
/// column, and re-inserts the exported rows with `null` embeddings.  Embeddings
/// must be repopulated by re-running the sync pipeline.
fn migrate_to_v3(store: &DataStore, from_ver: i64) -> Result<(), GraphtorError> {
    debug!(from_ver, "migrating database schema to v3");

    // Export all chunk metadata rows (old schema has no embedding column).
    let saved = store.query(
        r"?[chunk_id, source_id, path, title, position, char_offset, headings, content]
               := *doc_chunks{ chunk_id, source_id, path, title,
                               position, char_offset, headings, content }",
        BTreeMap::new(),
    )?;
    let rows = saved.rows;

    // Drop the HNSW index before removing the base relation (defensive guard).
    if relation_exists(store, "doc_chunks:embedding_idx")? {
        store.mutate("::hnsw drop doc_chunks:embedding_idx", BTreeMap::new())?;
    }
    store.mutate("::remove doc_chunks", BTreeMap::new())?;

    // For v2 databases, also remove the now-superseded doc_vectors relation.
    if from_ver >= 2 && relation_exists(store, "doc_vectors")? {
        store.mutate("::remove doc_vectors", BTreeMap::new())?;
    }

    // Recreate doc_chunks with the v3 schema (embedding column added).
    store.mutate(DOC_CHUNKS_DDL, BTreeMap::new())?;

    // Re-insert all saved rows with null embeddings (re-sync required).
    for row in &rows {
        if row.len() < 8 {
            continue;
        }
        let script = r"
            ?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
                <- [[$c, $s, $p, $t, $pos, $off, $h, $con, null]]
            :put doc_chunks {
                chunk_id => source_id, path, title, position, char_offset, headings, content, embedding
            }
        ";
        let mut params = BTreeMap::new();
        params.insert("c".to_string(), row[0].clone());
        params.insert("s".to_string(), row[1].clone());
        params.insert("p".to_string(), row[2].clone());
        params.insert("t".to_string(), row[3].clone());
        params.insert("pos".to_string(), row[4].clone());
        params.insert("off".to_string(), row[5].clone());
        params.insert("h".to_string(), row[6].clone());
        params.insert("con".to_string(), row[7].clone());
        store.mutate(script, params)?;
    }

    debug!(
        migrated = rows.len(),
        "migration to v3 complete; embeddings require re-sync"
    );
    Ok(())
}

/// Create the HNSW vector index on `doc_chunks.embedding` if it does not
/// already exist.
fn create_hnsw_index_if_missing(store: &DataStore) -> Result<(), GraphtorError> {
    if relation_exists(store, "doc_chunks:embedding_idx")? {
        debug!("HNSW index doc_chunks:embedding_idx already exists, skipping");
        return Ok(());
    }
    store.mutate(
        "::hnsw create doc_chunks:embedding_idx { \
         dim: 384, m: 16, dtype: F32, fields: [embedding], \
         distance: Cosine, ef_construction: 200 }",
        BTreeMap::new(),
    )?;
    debug!("created HNSW index doc_chunks:embedding_idx");
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

/// Read the current schema version from `doc_schema_ver`.
///
/// Returns `0` when the table is empty (schema not yet applied).
fn get_schema_version(store: &DataStore) -> Result<i64, GraphtorError> {
    let rows = store.query("?[ver] := *doc_schema_ver{ ver }", BTreeMap::new())?;
    Ok(rows
        .rows
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.get_int())
        .unwrap_or(0))
}

/// Record the current schema version (upsert).
fn upsert_schema_version(store: &DataStore, ver: i64) -> Result<(), GraphtorError> {
    let script = "?[ver] <- [[$ver]] :put doc_schema_ver { ver }";
    let mut params = BTreeMap::new();
    params.insert("ver".to_string(), cozo::DataValue::Num(cozo::Num::Int(ver)));
    store.mutate(script, params)?;
    Ok(())
}
