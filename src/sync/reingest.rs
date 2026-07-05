//! Surgical database re-ingestion for changed files.
//!
//! Provides legacy [`delete_file_data`] / [`reingest_file`] wrappers plus the
//! source-scoped [`delete_file_data_for_source`] and contract-aware
//! [`reingest_file_with_old_contract_path`] entry points used by incremental
//! sync.

use std::collections::HashMap;
use std::path::Path;

use tracing::{debug, info, warn};

use crate::db::chunks::{delete_chunks_by_path, delete_chunks_by_source_and_path};
use crate::db::edges::{delete_code_for_chunk, delete_edges_for_chunk};
use crate::db::urls::{delete_url_index_for_chunks, register_document_url};
use crate::db::vectors::{get_vector, upsert_vector};
use crate::db::{upsert_chunk, upsert_code_snippet, upsert_edge};
use crate::embed::{embed_text, EmbeddingModel};
use crate::error::GraphtorError;
use crate::parse::parse_file;
use crate::path::validate_path;
use crate::DataStore;

fn delete_dependent_records(store: &DataStore, chunk_ids: &[String]) -> Result<(), GraphtorError> {
    for chunk_id in chunk_ids {
        delete_edges_for_chunk(store, chunk_id)?;
        delete_code_for_chunk(store, chunk_id)?;
    }
    // Remove any cross-source URL index entries that pointed at deleted chunks
    // so stale absolute-link targets do not resolve to missing documents.
    delete_url_index_for_chunks(store, chunk_ids)?;
    Ok(())
}

/// Remove all database records associated with `path`, regardless of source.
///
/// This preserves the legacy public API shape. New call sites that need
/// source-scoped deletes should prefer [`delete_file_data_for_source`].
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query or mutation failure.
pub fn delete_file_data(store: &DataStore, path: &str) -> Result<(), GraphtorError> {
    let chunk_ids = delete_chunks_by_path(store, path)?;
    delete_dependent_records(store, &chunk_ids)?;
    if !chunk_ids.is_empty() {
        debug!(
            path,
            count = chunk_ids.len(),
            "deleted stale database records for path"
        );
    }
    Ok(())
}

/// Remove all database records associated with `path` from `CozoDB`, scoped to
/// `source_id`.
///
/// Using `source_id` as a scope ensures that cross-source records at an
/// identical `path` value are not accidentally deleted.
///
/// Retrieves the chunk IDs for `(source_id, path)`, deletes the chunks, then
/// removes all dependent edges (`doc_edges`) and code snippets (`doc_code`)
/// for each chunk ID.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query or mutation failure.
pub fn delete_file_data_for_source(
    store: &DataStore,
    source_id: &str,
    path: &str,
) -> Result<(), GraphtorError> {
    let chunk_ids = delete_chunks_by_source_and_path(store, source_id, path)?;
    delete_dependent_records(store, &chunk_ids)?;
    if !chunk_ids.is_empty() {
        debug!(
            source_id,
            path,
            count = chunk_ids.len(),
            "deleted stale database records for (source, path)"
        );
    }
    Ok(())
}

/// Delete stale records for `file_path` then re-parse and reload the file.
///
/// This preserves the legacy public API shape and returns only the chunk count.
/// New call sites that need rename-aware cleanup and the validated contract
/// `source_path` should prefer [`reingest_file_with_old_contract_path`].
///
/// # Errors
///
/// Returns the same errors as [`reingest_file_with_old_contract_path`].
pub fn reingest_file(
    store: &DataStore,
    source_id: &str,
    file_path: &Path,
    source_root: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
) -> Result<usize, GraphtorError> {
    reingest_file_with_old_contract_path(
        store,
        source_id,
        file_path,
        source_root,
        root,
        None,
        model,
    )
    .map(|(chunks_loaded, _contract_path)| chunks_loaded)
}

/// Delete stale records for `file_path` then re-parse and reload the file.
///
/// ## Identity model
///
/// The document's canonical identity is `{source_id, contract.source_path}`,
/// where `contract.source_path` is the `source_path` field from the validated
/// docline v1 frontmatter.
///
/// - `old_contract_path` is the `source_path` last recorded in sync state for
///   this file.  When `Some`, stale records are deleted by this identity before
///   re-ingesting.  Pass `None` for newly added files (nothing to delete).
///
/// ## Parameters
///
/// - `source_id` — source registry identifier.
/// - `file_path` — absolute on-disk path to the file.
/// - `source_root` — root directory of the source (for path validation).
/// - `root` — workspace root for security boundary checks.
/// - `old_contract_path` — previous `source_path` from sync state; `None` when
///   the file has never been ingested before.
/// - `model` — optional embedding model; pass `None` to skip embedding.
///
/// ## Return value
///
/// Returns `(chunks_loaded, new_contract_source_path)` on success.
/// `new_contract_source_path` is the `source_path` from the validated contract
/// and should be stored in sync state for the next incremental cycle.
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] if the file's frontmatter fails contract
/// validation (malformed YAML, missing required fields, unsupported schema
/// version, invalid `source_path`, or `content_sha256` mismatch).
/// Returns [`GraphtorError::Database`] if `CozoDB` operations fail.
/// Returns [`GraphtorError::Parse`] if the file cannot be read.
/// Returns [`GraphtorError::PathViolation`] if `file_path` escapes `root`.
pub fn reingest_file_with_old_contract_path(
    store: &DataStore,
    source_id: &str,
    file_path: &Path,
    source_root: &Path,
    root: &Path,
    old_contract_path: Option<&str>,
    model: Option<&EmbeddingModel>,
) -> Result<(usize, String), GraphtorError> {
    let safe_path = validate_path(file_path, root)?;
    // Validate source_root is within the workspace boundary.
    let _ = validate_path(source_root, root)?;

    // Re-parse using the contract-enforced parser. The canonical `source_path`
    // comes from the validated frontmatter, not from the filesystem path.
    let parsed = parse_file(&safe_path, source_id)?;
    let new_contract_path = parsed.path.clone();

    // ── Preserve embeddings before deletion (--no-embed reingest) ─────────
    // When model is None, no new embeddings will be computed.  Preserve any
    // existing embeddings for chunk_ids that survive the reparse unchanged so
    // that a `--no-embed` sync cycle does not silently erase previously
    // computed vectors.  Read them BEFORE deletion because `delete_file_data`
    // removes the underlying `doc_chunks` rows along with their embeddings.
    let saved_embeddings: HashMap<String, Vec<f32>> = if model.is_none() {
        let mut map = HashMap::with_capacity(parsed.chunks.len());
        for chunk in &parsed.chunks {
            match get_vector(store, &chunk.chunk_id) {
                Ok(Some(vec)) => {
                    map.insert(chunk.chunk_id.clone(), vec);
                }
                Ok(None) => {}
                Err(e) => {
                    debug!(
                        chunk_id = %chunk.chunk_id,
                        error = %e,
                        "failed to read existing embedding before reingest; \
                         chunk will be re-stored without vector"
                    );
                }
            }
        }
        map
    } else {
        HashMap::new()
    };

    // Determine which identity to delete stale records for.
    // - If old_contract_path is provided, delete by that (handles renames).
    // - Otherwise delete by new_contract_path (handles first-time and stable path).
    let delete_path = old_contract_path.unwrap_or(&new_contract_path);
    delete_file_data_for_source(store, source_id, delete_path)?;

    // If the contract path changed, also clean up records under the new path
    // (from any prior full-sync run that stored the fs-relative path).
    if old_contract_path.is_some() && old_contract_path != Some(new_contract_path.as_str()) {
        if let Err(e) = delete_file_data_for_source(store, source_id, &new_contract_path) {
            debug!(
                source_id,
                new_path = new_contract_path,
                error = %e,
                "failed to clean up records under new contract path (non-fatal)"
            );
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

    // Re-register the document's canonical_url against its entry chunk. The
    // delete above removed the prior index entry, so cross-source resolution
    // would regress on every incremental sync unless it is repopulated here —
    // mirroring the full-sync pipeline. Best-effort: index failure does not
    // fail the reingest.
    if let Err(e) = register_document_url(store, &parsed) {
        warn!(error = %e, "url index registration failed during reingest; continuing");
    }

    // Optionally embed and persist vectors (chunks must already be in DB).
    if let Some(m) = model {
        for chunk in &parsed.chunks {
            match embed_text(m, &chunk.content) {
                Ok(embedding) => {
                    if let Err(e) = upsert_vector(store, &chunk.chunk_id, &embedding) {
                        warn!(
                            chunk_id = %chunk.chunk_id,
                            path = new_contract_path,
                            error = %e,
                            "vector upsert failed during reingest; continuing without embedding"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        chunk_id = %chunk.chunk_id,
                        path = new_contract_path,
                        error = %e,
                        "embedding failed during reingest; continuing without vector"
                    );
                }
            }
        }
    } else if !saved_embeddings.is_empty() {
        // --no-embed path: restore any embeddings we saved before deletion.
        // Only chunk_ids whose content (and therefore chunk_id hash) is
        // unchanged will have a saved embedding; new or modified chunks will
        // simply remain without a vector until the next embed pass.
        for chunk in &parsed.chunks {
            if let Some(vec) = saved_embeddings.get(&chunk.chunk_id) {
                if let Err(e) = upsert_vector(store, &chunk.chunk_id, vec) {
                    warn!(
                        chunk_id = %chunk.chunk_id,
                        path = new_contract_path,
                        error = %e,
                        "failed to restore preserved embedding during --no-embed reingest"
                    );
                }
            }
        }
    }

    info!(
        path = new_contract_path,
        chunks = chunks_loaded,
        edges = parsed.references.len(),
        snippets = parsed.code_snippets.len(),
        "re-ingested file"
    );
    Ok((chunks_loaded, new_contract_path))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{delete_file_data, reingest_file, reingest_file_with_old_contract_path};
    use crate::db::ensure_schema;
    use crate::error::GraphtorError;
    use crate::DataStore;

    /// Build a docline-conformant markdown string for test fixtures.
    pub(super) fn docline_md(source_path: &str, title: &str, content: &str) -> String {
        format!(
            "---\ntitle: {title}\nsource: /test/source\ningested_at: \
             2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
        )
    }

    #[test]
    fn reingest_unsupported_extension_returns_parse_error() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let file_path = source_root.join("bad.xyz");
        fs::write(&file_path, b"some content").expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let error = reingest_file(&store, "test-source", &file_path, &source_root, root, None)
            .expect_err("unsupported extension should fail parsing");

        assert!(
            matches!(error, GraphtorError::Parse { .. }),
            "expected Parse error, got: {error}"
        );
    }

    #[test]
    fn reingest_file_without_frontmatter_fails_contract() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let file_path = source_root.join("no-fm.md");
        fs::write(&file_path, b"# Heading\n\nNo frontmatter here.\n").expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let error = reingest_file(&store, "test-source", &file_path, &source_root, root, None)
            .expect_err("missing frontmatter must fail contract");

        assert!(
            matches!(error, GraphtorError::Parse { .. }),
            "expected Parse (wrapping Contract) error, got: {error}"
        );
        assert!(
            error.to_string().contains("contract"),
            "error should mention contract: {error}"
        );
    }

    #[test]
    fn reingest_valid_docline_file_returns_chunks_and_contract_path() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let md = docline_md(
            "api/guide.md",
            "API Guide",
            "# API Guide\n\nContent here.\n",
        );
        let file_path = source_root.join("api-guide.md");
        fs::write(&file_path, md.as_bytes()).expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let (chunks, contract_path) = reingest_file_with_old_contract_path(
            &store,
            "test-source",
            &file_path,
            &source_root,
            root,
            None,
            None,
        )
        .expect("valid file must reingest");

        assert!(chunks > 0, "should produce at least one chunk");
        assert_eq!(
            contract_path, "api/guide.md",
            "contract_path must come from the frontmatter source_path"
        );
    }

    #[test]
    fn reingest_preserves_canonical_url_index_across_incremental_sync() {
        use crate::db::urls::resolve_canonical_url;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        // Docline frontmatter carrying a canonical_url.
        let md = "---\ntitle: Admin\nsource: /test/source\ningested_at: \
                  2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: admin/foo.md\n\
                  canonical_url: /fabric/admin/foo\n---\n# Admin\n\nBody.\n";
        let file_path = source_root.join("admin-foo.md");
        fs::write(&file_path, md.as_bytes()).expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        // First ingest registers the canonical_url.
        reingest_file_with_old_contract_path(
            &store,
            "src",
            &file_path,
            &source_root,
            root,
            None,
            None,
        )
        .expect("first reingest");
        let first = resolve_canonical_url(&store, "/fabric/admin/foo").expect("resolve");
        assert!(first.is_some(), "canonical_url indexed on first ingest");

        // Incremental re-sync of the same file must NOT drop the index entry.
        reingest_file_with_old_contract_path(
            &store,
            "src",
            &file_path,
            &source_root,
            root,
            None,
            None,
        )
        .expect("second reingest");
        let second = resolve_canonical_url(&store, "/fabric/admin/foo").expect("resolve");
        assert_eq!(
            second, first,
            "canonical_url index must survive an incremental re-sync"
        );
    }

    #[test]
    fn reingest_idempotent_no_duplicate_chunks() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let md = docline_md("guide.md", "Guide", "# Guide\n\nParagraph.\n");
        let file_path = source_root.join("guide.md");
        fs::write(&file_path, md.as_bytes()).expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let source_id = "idempotent-source";

        let (n1, path1) = reingest_file_with_old_contract_path(
            &store,
            source_id,
            &file_path,
            &source_root,
            root,
            None,
            None,
        )
        .expect("first reingest");
        // Second reingest passes the previous contract path as old_contract_path.
        let (n2, path2) = reingest_file_with_old_contract_path(
            &store,
            source_id,
            &file_path,
            &source_root,
            root,
            Some(&path1),
            None,
        )
        .expect("second reingest");

        assert_eq!(path1, path2, "contract path should not change");
        assert_eq!(n1, n2, "chunk count should be stable across reingests");

        // Verify no duplicate chunks exist in the DB.
        let chunks =
            crate::db::chunks::list_chunks_for_source(&store, source_id).expect("list chunks");
        assert_eq!(
            chunks.len(),
            n1,
            "DB must contain exactly one copy of each chunk after idempotent reingest"
        );
    }

    #[test]
    fn legacy_reingest_file_api_returns_chunk_count() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let md = docline_md(
            "legacy/guide.md",
            "Legacy Guide",
            "# Legacy Guide\n\nContent.\n",
        );
        let file_path = source_root.join("guide.md");
        fs::write(&file_path, md.as_bytes()).expect("write file");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        let chunks = reingest_file(
            &store,
            "legacy-source",
            &file_path,
            &source_root,
            root,
            None,
        )
        .expect("legacy API must reingest successfully");

        assert!(chunks > 0, "legacy API must return the chunk count");
    }

    #[test]
    fn legacy_delete_file_data_api_deletes_all_sources_for_path() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root).expect("create source root");

        let contract_path = "shared/guide.md";
        let file_a = source_root.join("a.md");
        let file_b = source_root.join("b.md");
        fs::write(
            &file_a,
            docline_md(contract_path, "Guide A", "# Guide A\n\nContent A.\n").as_bytes(),
        )
        .expect("write a.md");
        fs::write(
            &file_b,
            docline_md(contract_path, "Guide B", "# Guide B\n\nContent B.\n").as_bytes(),
        )
        .expect("write b.md");

        let store = DataStore::open_mem().expect("open mem db");
        ensure_schema(&store).expect("ensure schema");

        reingest_file_with_old_contract_path(
            &store,
            "source-a",
            &file_a,
            &source_root,
            root,
            None,
            None,
        )
        .expect("ingest source-a");
        reingest_file_with_old_contract_path(
            &store,
            "source-b",
            &file_b,
            &source_root,
            root,
            None,
            None,
        )
        .expect("ingest source-b");

        delete_file_data(&store, contract_path).expect("legacy delete must succeed");

        assert!(
            crate::db::chunks::list_chunks_for_source(&store, "source-a")
                .expect("list chunks for source-a")
                .is_empty(),
            "legacy delete must remove chunks for source-a"
        );
        assert!(
            crate::db::chunks::list_chunks_for_source(&store, "source-b")
                .expect("list chunks for source-b")
                .is_empty(),
            "legacy delete must remove chunks for source-b"
        );
    }
}
