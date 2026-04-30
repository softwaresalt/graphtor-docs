//! Surgical database re-ingestion for changed files.
//!
//! Provides [`delete_file_data`] to remove all `CozoDB` records associated with
//! a source-root-relative path (chunks, edges, code snippets), and
//! [`reingest_file`] to re-parse and reload a single file after deletion.

use std::path::Path;

use tracing::{debug, info, warn};

use crate::db::chunks::delete_chunks_by_path;
use crate::db::edges::{delete_code_for_chunk, delete_edges_for_chunk};
use crate::db::{upsert_chunk, upsert_code_snippet, upsert_edge};
use crate::embed::{embed_text, EmbeddingModel};
use crate::error::GraphtorError;
use crate::parse::parse_document;
use crate::path::validate_path;
use crate::DataStore;

/// Remove all database records associated with `relative_path` from `CozoDB`.
///
/// Deletes in dependency order:
/// 1. Outgoing edges (`doc_edges`) for every chunk at this path.
/// 2. Code snippets (`doc_code`) for every chunk at this path.
/// 3. The chunks themselves (`doc_chunks`).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query or mutation failure.
pub fn delete_file_data(store: &DataStore, relative_path: &str) -> Result<(), GraphtorError> {
    let chunk_ids = delete_chunks_by_path(store, relative_path)?;
    for chunk_id in &chunk_ids {
        delete_edges_for_chunk(store, chunk_id)?;
        delete_code_for_chunk(store, chunk_id)?;
    }
    if !chunk_ids.is_empty() {
        debug!(
            path = relative_path,
            count = chunk_ids.len(),
            "deleted stale database records for path"
        );
    }
    Ok(())
}

/// Delete stale records for `file_path` then re-parse and reload the file.
///
/// `source_id` is the identifier of the parent source.  `file_path` is the
/// absolute path to the file on disk.  `source_root` is the root directory of
/// the source — used to derive the relative path stored in the database and to
/// satisfy the path security check.  `root` is the workspace root for path
/// validation.
///
/// Pass `model = None` to skip embedding (vectors are not persisted in this
/// release).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if `CozoDB` operations fail.
/// Returns [`GraphtorError::Parse`] if the file cannot be read or parsed.
/// Returns [`GraphtorError::PathViolation`] if `file_path` escapes `root`.
pub fn reingest_file(
    store: &DataStore,
    source_id: &str,
    file_path: &Path,
    source_root: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
) -> Result<usize, GraphtorError> {
    let safe_path = validate_path(file_path, root)?;

    // Derive the source-root-relative path used as the database key.
    let rel_path = safe_path
        .strip_prefix(source_root)
        .unwrap_or(&safe_path)
        .to_string_lossy()
        .replace('\\', "/");

    // Delete stale records first.
    delete_file_data(store, &rel_path)?;

    // Re-parse.
    let content = std::fs::read_to_string(&safe_path).map_err(|e| GraphtorError::Parse {
        message: format!("failed to read file: {e}"),
        path: Some(safe_path.clone()),
    })?;

    let parsed = parse_document(&content, &rel_path).map_err(|e| GraphtorError::Parse {
        message: format!("markdown parse failed: {e}"),
        path: Some(safe_path.clone()),
    })?;

    // Optionally embed (vectors not persisted — no-op on model absence).
    if let Some(m) = model {
        for chunk in &parsed.chunks {
            if let Err(e) = embed_text(m, &chunk.content) {
                warn!(
                    chunk_id = %chunk.chunk_id,
                    path = rel_path,
                    error = %e,
                    "embedding failed during reingest; continuing without vector"
                );
            }
        }
    }

    // Reload into CozoDB.
    let mut chunks_loaded: usize = 0;
    for chunk in &parsed.chunks {
        upsert_chunk(store, source_id, chunk)?;
        chunks_loaded += 1;
    }
    for reference in &parsed.references {
        upsert_edge(store, reference)?;
    }
    for snippet in &parsed.code_snippets {
        upsert_code_snippet(store, snippet)?;
    }

    info!(
        path = rel_path,
        chunks = chunks_loaded,
        edges = parsed.references.len(),
        snippets = parsed.code_snippets.len(),
        "re-ingested file"
    );
    Ok(chunks_loaded)
}
