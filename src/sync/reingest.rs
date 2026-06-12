//! Surgical database re-ingestion for changed files.
//!
//! Provides [`delete_file_data`] to remove all `CozoDB` records associated with
//! a source-root-relative path (chunks, edges, code snippets), and
//! [`reingest_file`] to re-parse and reload a single file after deletion.

use std::path::Path;

use tracing::{debug, info, warn};

use crate::db::chunks::delete_chunks_by_path;
use crate::db::edges::{delete_code_for_chunk, delete_edges_for_chunk};
use crate::db::vectors::upsert_vector;
use crate::db::{upsert_chunk, upsert_code_snippet, upsert_edge};
use crate::embed::{embed_text, EmbeddingModel};
use crate::error::GraphtorError;
use crate::parse::parse_file;
use crate::path::validate_path;
use crate::DataStore;

/// Remove all database records associated with `relative_path` from `CozoDB`.
///
/// Retrieves the chunk IDs for `relative_path`, deletes the chunks, then
/// removes all dependent edges (`doc_edges`) and code snippets (`doc_code`)
/// for each chunk ID.
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
/// Pass `model = None` to skip embedding.
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
    let safe_source_root = validate_path(source_root, root)?;

    // Derive the source-root-relative path used as the database key.
    let rel_path = safe_path
        .strip_prefix(&safe_source_root)
        .map_err(|_| GraphtorError::Pipeline {
            message: format!(
                "file '{}' is not within source root '{}'",
                safe_path.display(),
                safe_source_root.display()
            ),
            stage: "reingest".to_owned(),
        })?
        .to_string_lossy()
        .replace('\\', "/");

    // Delete stale records first.
    delete_file_data(store, &rel_path)?;

    // Re-parse using the same format dispatch as the full pipeline.
    let parsed = parse_file(&safe_path, &rel_path)?;

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

    // Optionally embed and persist vectors (chunks must already be in DB).
    if let Some(m) = model {
        for chunk in &parsed.chunks {
            match embed_text(m, &chunk.content) {
                Ok(embedding) => {
                    if let Err(e) = upsert_vector(store, &chunk.chunk_id, &embedding) {
                        warn!(
                            chunk_id = %chunk.chunk_id,
                            path = rel_path,
                            error = %e,
                            "vector upsert failed during reingest; continuing without embedding"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        chunk_id = %chunk.chunk_id,
                        path = rel_path,
                        error = %e,
                        "embedding failed during reingest; continuing without vector"
                    );
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::reingest_file;
    use crate::db::ensure_schema;
    use crate::error::GraphtorError;
    use crate::DataStore;

    #[test]
    fn reingest_pdf_routes_to_pdf_parser() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let file_path = source_root.join("bad.pdf");
        fs::write(&file_path, [0xFF, 0x00, 0xFE, 0x7F]).expect("write invalid pdf");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let error = reingest_file(&store, "pdf-source", &file_path, &source_root, root, None)
            .expect_err("invalid pdf should fail parsing");

        match error {
            GraphtorError::Parse { message, .. } => {
                let lower = message.to_ascii_lowercase();
                assert!(
                    lower.contains("pdf"),
                    "expected pdf parser error, got: {message}"
                );
                assert!(
                    !lower.contains("failed to read file"),
                    "pdf dispatch should not use markdown text reader: {message}"
                );
                assert!(
                    !lower.contains("markdown parse failed"),
                    "pdf dispatch should not report markdown parser errors: {message}"
                );
            }
            other => panic!("expected parse error, got: {other}"),
        }
    }
}
