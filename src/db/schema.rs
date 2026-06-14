//! Database schema management for the unified `CozoDB` store.
//!
//! Provides [`ensure_schema`], which ensures the stored relations exist and
//! applies non-destructive structural migrations (v1→v3).  The v4 docline
//! pivot is intentionally *not* applied at open time to avoid pruning data
//! before a successful replacement ingest has occurred.  Write paths (`sync`,
//! `prewarm`) validate candidates, call [`prune_v4_data_for_rebuild`], run the
//! rebuild, and only then call [`mark_v4_migration_complete`] if the rebuild
//! finished cleanly.
//!
//! Read surfaces (`serve`, `status`) call [`needs_v4_migration`] to gate
//! deterministically on pre-v4 databases rather than silently exposing
//! stale content.
//!
//! # Schema versions
//!
//! | Version | Change |
//! |---------|--------|
//! | 1       | Initial schema: `doc_sources`, `doc_chunks`, `doc_edges`, `doc_code` |
//! | 2       | Added `doc_vectors` for embedding storage and semantic search |
//! | 3       | Merged `embedding: <F32; 384>?` into `doc_chunks`; removed `doc_vectors`; added HNSW index |
//! | 4       | Docline pivot: pruned all pre-pivot source/chunk/edge/code data; re-ingest required |

use std::collections::BTreeMap;

use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;

/// The current schema version stored in `doc_schema_ver`.
const SCHEMA_VERSION: i64 = 4;
/// Internal relation that records whether staged v4 retries must reuse the
/// persisted frozen snapshot established before the destructive prune.
const V4_MIGRATION_SNAPSHOT_LOCK_RELATION: &str = "doc_v4_migration_snapshot_lock";
/// Single-row identifier stored in [`V4_MIGRATION_SNAPSHOT_LOCK_RELATION`].
const V4_MIGRATION_SNAPSHOT_LOCK_ID: &str = "v4";

/// DDL string for the v3 `doc_chunks` relation (includes `embedding` column).
const DOC_CHUNKS_DDL: &str =
    ":create doc_chunks { chunk_id: String => source_id: String, path: String, \
     title: String?, position: Int, char_offset: Int, headings: String, content: String, \
     embedding: <F32; 384>? }";

/// Create all required stored relations and apply non-destructive structural
/// migrations (v1 → v3 only).
///
/// Safe to call on every startup — the operation is idempotent.
///
/// ## Migration safety
///
/// The **v4 data prune** (docline pivot) is intentionally **not** performed
/// here.  Pruning on open would destroy the existing index before a
/// successful replacement ingest has occurred.  Instead:
///
/// - Write paths (`sync`, `prewarm`) validate inputs, call
///   [`prune_v4_data_for_rebuild`] right before the rebuild starts, and only
///   call [`mark_v4_migration_complete`] after a clean rebuild.
/// - Query surfaces (`serve`, `status`) call [`needs_v4_migration`] to detect
///   pre-v4 databases and gate with an actionable error instead of silently
///   exposing stale data.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if any relation cannot be created or if
/// the non-destructive structural migration fails.
pub fn ensure_schema(store: &DataStore) -> Result<(), GraphtorError> {
    create_if_missing(
        store,
        "doc_schema_ver",
        ":create doc_schema_ver { ver: Int }",
    )?;
    let ver = get_schema_version(store)?;
    let snapshot_reuse_required = v4_migration_snapshot_locked(store)?;
    let has_doc_chunks = relation_exists(store, "doc_chunks")?;

    if ver < SCHEMA_VERSION {
        if has_doc_chunks || snapshot_reuse_required {
            // Existing pre-v4 database.
            //
            // Apply only non-destructive structural migrations. The v4 data
            // prune is intentionally skipped here — write paths run the staged
            // prune/complete flow only after validation.
            //
            // Interrupted staged prunes may temporarily drop `doc_chunks`
            // before recreating it. When the persisted snapshot lock is still
            // armed, treat that state as an in-progress migration rather than
            // as a fresh database.
            if has_doc_chunks && ver < 3 {
                migrate_to_v3(store, ver)?;
                // Record v3 so the structural migration is not repeated.
                // The version stays below 4 so `needs_v4_migration` can
                // detect that a full rebuild is still required.
                upsert_schema_version(store, 3)?;
            }
            // Leave the version at 3 (or wherever it is below 4); do NOT
            // stamp v4 here — that happens only after a clean rebuild.
        } else {
            // Fresh database — no existing data to migrate and no staged-prune
            // retry lock to honor; stamp v4 directly.
            upsert_schema_version(store, SCHEMA_VERSION)?;
        }
    }

    // Always create any missing base relations (idempotent self-heal).
    create_all_relations(store)?;
    create_internal_relations(store)?;
    // Always ensure the HNSW index is present (idempotent self-heal).
    create_hnsw_index_if_missing(store)?;

    debug!("database schema verified at version {SCHEMA_VERSION}");
    Ok(())
}

/// Return `true` when the database contains pre-v4 data that has not yet been
/// rebuilt through the docline ingestion pipeline.
///
/// Read-only — does not mutate the database.  Safe to call on any open store,
/// including read-only handles.
///
/// Query surfaces (`serve`, `status`) MUST call this and gate with an
/// actionable error before exposing index data to callers.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if the schema-version query fails.
pub fn needs_v4_migration(store: &DataStore) -> Result<bool, GraphtorError> {
    // If doc_schema_ver was never created the store has not been initialised;
    // treat as "no migration needed" — ensure_schema will set it up.
    if !relation_exists(store, "doc_schema_ver")? {
        return Ok(false);
    }
    let ver = get_schema_version(store)?;
    // ver == 0  → table exists but is empty (init in progress) — not pre-v4
    // ver 1..=3 → pre-v4 data present that has not been rebuilt yet
    Ok(ver > 0 && ver < 4)
}

/// Return `true` when a staged v4 retry must reuse the persisted frozen
/// snapshot instead of refreezing from live input.
///
/// This lock flips on immediately before the destructive staged prune and stays
/// set until [`mark_v4_migration_complete`] succeeds. While it is active, the
/// frozen snapshot is the authoritative rebuild input.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if the lock-state query fails.
pub fn v4_migration_snapshot_locked(store: &DataStore) -> Result<bool, GraphtorError> {
    if !relation_exists(store, V4_MIGRATION_SNAPSHOT_LOCK_RELATION)? {
        return Ok(false);
    }

    let rows = store.query(
        "?[lock_id] := *doc_v4_migration_snapshot_lock{ lock_id }",
        BTreeMap::new(),
    )?;
    Ok(rows.rows.iter().any(|row| {
        row.first().and_then(|value| value.get_str()) == Some(V4_MIGRATION_SNAPSHOT_LOCK_ID)
    }))
}

/// Prune all pre-v4 ingested data without marking the migration complete.
///
/// This is the destructive migration step for the docline pivot. It preserves
/// the pre-v4 schema gate so callers can rebuild the index while
/// [`needs_v4_migration`] continues to return `true`.
///
/// New write paths SHOULD pair this with [`mark_v4_migration_complete`] only
/// after a clean rebuild has finished.
/// Already-v4 databases are left untouched.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if any prune operation fails.
pub fn prune_v4_data_for_rebuild(store: &DataStore) -> Result<(), GraphtorError> {
    if !needs_v4_migration(store)? {
        return Ok(());
    }

    set_v4_migration_snapshot_locked(store, true)?;
    migrate_to_v4(store)?;
    Ok(())
}

/// Mark a previously pruned v4 rebuild as complete by stamping schema version
/// 4.
///
/// Callers MUST only invoke this after the replacement ingest succeeded
/// cleanly. Until this is called, [`needs_v4_migration`] continues to gate the
/// database as pre-v4.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if the version-stamp operation fails.
pub fn mark_v4_migration_complete(store: &DataStore) -> Result<(), GraphtorError> {
    upsert_schema_version(store, SCHEMA_VERSION)?;
    set_v4_migration_snapshot_locked(store, false)?;
    Ok(())
}

/// Prune all pre-v4 ingested data and stamp the schema version as 4.
///
/// This compatibility helper performs the full destructive migration in one
/// call. New write paths SHOULD prefer the staged
/// [`prune_v4_data_for_rebuild`] + [`mark_v4_migration_complete`] sequence so
/// the database remains gated until replacement ingest succeeds.
/// Already-v4 databases are left untouched.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if any prune or version-stamp
/// operation fails.
pub fn apply_v4_prune(store: &DataStore) -> Result<(), GraphtorError> {
    if !needs_v4_migration(store)? {
        return Ok(());
    }

    prune_v4_data_for_rebuild(store)?;
    mark_v4_migration_complete(store)?;
    Ok(())
}

/// Force-set the schema version stored in `doc_schema_ver`.
///
/// Intended only for test use — use to simulate pre-v4 database state for
/// migration regression tests.  Not part of the stable public API.
#[doc(hidden)]
pub fn set_schema_version_for_test(store: &DataStore, ver: i64) -> Result<(), GraphtorError> {
    upsert_schema_version(store, ver)
}

/// Create the permanent stored relations using `create_if_missing`.
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

/// Create internal metadata relations used by staged migrations.
fn create_internal_relations(store: &DataStore) -> Result<(), GraphtorError> {
    create_if_missing(
        store,
        V4_MIGRATION_SNAPSHOT_LOCK_RELATION,
        ":create doc_v4_migration_snapshot_lock { lock_id: String }",
    )?;
    Ok(())
}

/// Migrate a v3 database to v4 — docline pivot.
///
/// Prunes all pre-pivot ingested data: source records, chunks, edges, and
/// code snippets are cleared so the docline pipeline can re-ingest from scratch.
/// The schema relations themselves are preserved — only the data is removed.
fn migrate_to_v4(store: &DataStore) -> Result<(), GraphtorError> {
    debug!("migrating database schema to v4: pruning pre-pivot ingested data");

    // Remove all edge data.
    if relation_exists(store, "doc_edges")? {
        store.mutate(
            "?[src_chunk_id, target_path] := *doc_edges{ src_chunk_id, target_path }
             :rm doc_edges { src_chunk_id, target_path }",
            BTreeMap::new(),
        )?;
    }
    // Remove all code snippets.
    if relation_exists(store, "doc_code")? {
        store.mutate(
            "?[snippet_id] := *doc_code{ snippet_id }
             :rm doc_code { snippet_id }",
            BTreeMap::new(),
        )?;
    }
    // Remove all chunks (drop and recreate to also reset the HNSW index).
    if relation_exists(store, "doc_chunks:embedding_idx")? {
        store.mutate("::hnsw drop doc_chunks:embedding_idx", BTreeMap::new())?;
    }
    if relation_exists(store, "doc_chunks")? {
        store.mutate("::remove doc_chunks", BTreeMap::new())?;
        store.mutate(DOC_CHUNKS_DDL, BTreeMap::new())?;
    }
    // Remove all source records.
    if relation_exists(store, "doc_sources")? {
        store.mutate(
            "?[source_id] := *doc_sources{ source_id }
             :rm doc_sources { source_id }",
            BTreeMap::new(),
        )?;
    }

    debug!("migration to v4 complete; all ingested data pruned — re-ingest required");
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

/// Record the current schema version (replace — exactly one row is kept).
fn upsert_schema_version(store: &DataStore, ver: i64) -> Result<(), GraphtorError> {
    // Remove every existing row first so that the table always holds exactly
    // one version record.  Without this a second call with a different version
    // value would insert a new key (since `ver` is the primary key) and leave
    // two rows, making `get_schema_version` non-deterministic.
    store.mutate(
        "?[ver] := *doc_schema_ver{ ver } :rm doc_schema_ver { ver }",
        BTreeMap::new(),
    )?;
    let script = "?[ver] <- [[$ver]] :put doc_schema_ver { ver }";
    let mut params = BTreeMap::new();
    params.insert("ver".to_string(), cozo::DataValue::Num(cozo::Num::Int(ver)));
    store.mutate(script, params)?;
    Ok(())
}

/// Set or clear the staged v4 snapshot-reuse lock.
fn set_v4_migration_snapshot_locked(store: &DataStore, locked: bool) -> Result<(), GraphtorError> {
    if relation_exists(store, V4_MIGRATION_SNAPSHOT_LOCK_RELATION)? {
        store.mutate(
            "?[lock_id] := *doc_v4_migration_snapshot_lock{ lock_id } \
             :rm doc_v4_migration_snapshot_lock { lock_id }",
            BTreeMap::new(),
        )?;
    }

    if locked {
        let mut params = BTreeMap::new();
        params.insert(
            "lock_id".to_string(),
            cozo::DataValue::Str(V4_MIGRATION_SNAPSHOT_LOCK_ID.into()),
        );
        store.mutate(
            "?[lock_id] <- [[$lock_id]] :put doc_v4_migration_snapshot_lock { lock_id }",
            params,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn ensure_schema_keeps_pre_v4_gate_for_interrupted_staged_prune() {
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");
        set_schema_version_for_test(&store, 3).expect("set schema version to 3");
        set_v4_migration_snapshot_locked(&store, true)
            .expect("lock persisted snapshot reuse for staged retry");

        if relation_exists(&store, "doc_chunks:embedding_idx").expect("check HNSW index") {
            store
                .mutate("::hnsw drop doc_chunks:embedding_idx", BTreeMap::new())
                .expect("drop HNSW index before simulating interrupted prune");
        }
        store
            .mutate("::remove doc_chunks", BTreeMap::new())
            .expect("simulate crash after dropping doc_chunks");

        ensure_schema(&store).expect("ensure_schema should self-heal interrupted prune state");

        assert_eq!(
            get_schema_version(&store).expect("read schema version after self-heal"),
            3,
            "ensure_schema must not stamp v4 while the staged-prune snapshot lock is active"
        );
        assert!(
            needs_v4_migration(&store).expect("check v4 migration gate"),
            "interrupted staged prune must remain gated for rebuild"
        );
        assert!(
            v4_migration_snapshot_locked(&store).expect("check snapshot lock after self-heal"),
            "ensure_schema must preserve the staged-prune snapshot lock"
        );
        assert!(
            relation_exists(&store, "doc_chunks").expect("check recreated doc_chunks relation"),
            "ensure_schema should recreate doc_chunks after an interrupted staged prune"
        );
        assert!(
            relation_exists(&store, "doc_chunks:embedding_idx")
                .expect("check recreated HNSW index"),
            "ensure_schema should recreate the HNSW index after an interrupted staged prune"
        );
    }
}
