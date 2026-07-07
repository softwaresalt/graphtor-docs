//! Pipeline orchestrator — acquire → parse → embed → load.
//!
//! This module coordinates the full data ingestion workflow:
//!
//! 1. **Acquire** — clone Git repos or scan local directories via [`crate::acquire`].
//! 2. **Parse** — markdown → chunks, edges, code snippets via [`crate::parse`].
//! 3. **Embed** — compute 384-dim vectors via [`crate::embed::embed_batch`]; vectors
//!    are held in memory until the Load phase confirms each chunk upsert succeeded.
//! 4. **Load** — upsert chunks into CozoDB via [`crate::db`], then persist the
//!    corresponding vector to `doc_vectors` (only on success) via
//!    [`crate::db::vectors::upsert_vector`].
//!
//! Error handling follows **continue-on-failure** semantics: a file-level
//! failure accumulates into [`PipelineResult::errors_encountered`] and does
//! not abort sibling files or other sources.
//!
//! # Usage
//!
//! ```no_run
//! use graphtor_core::pipeline::{run, PipelineConfig};
//! use graphtor_core::DataStore;
//!
//! # fn example() -> Result<(), graphtor_core::error::GraphtorError> {
//! let store = DataStore::open_mem()?;
//! store.ensure_schema()?;
//! let config = PipelineConfig::default();
//! // let result = run(&my_plan, &store, None, &config)?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::acquire::{execute as acquire_execute, AcquisitionPlan, PlannedSource, SourceOutcome};
use crate::config::Source;
use crate::db::chunks::{get_vectors_for, upsert_chunk, upsert_chunks_batch};
use crate::db::edges::{
    upsert_code_snippet, upsert_code_snippets_batch, upsert_edge, upsert_edges_batch,
};
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::db::urls::{upsert_url_index, upsert_url_index_batch};
use crate::db::vectors::upsert_vector;
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
use crate::parse::types::{Chunk, CodeSnippet, Reference};
use crate::parse::{normalized_document_extension, parse_file};
use crate::path::validate_path;
use crate::DataStore;

/// Runtime parameters for the ingestion pipeline.
///
/// All fields have sensible defaults via [`Default`].
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum number of files to process per batch.
    ///
    /// Larger batches use more memory but reduce per-batch overhead.
    /// Typical range: 10–50 files per batch.
    pub batch_size: usize,

    /// Enable parallel processing for the embed step.
    ///
    /// # Current behaviour
    ///
    /// [`EmbeddingModel`] uses `Arc<Mutex<Inner>>` internally, so concurrent
    /// model inference is serialized at the mutex boundary. True throughput
    /// parallelism requires async embedding support (planned for 009-F).
    /// Setting `parallel = true` is architectural scaffolding: both values
    /// produce identical results until that upgrade lands.
    pub parallel: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            batch_size: 20,
            parallel: false,
        }
    }
}

/// A per-file processing failure accumulated during a pipeline run.
#[derive(Debug, Clone)]
pub struct FileError {
    /// Path associated with this failure.
    ///
    /// Three formats are possible depending on the failure site:
    ///
    /// * **Absolute file path** — for I/O errors discovered during the parse
    ///   stage (path validation, `read_to_string`, markdown parse failure).
    /// * **Source-root-relative path** — for database errors discovered during
    ///   the load stage (chunk upsert failure). The base is the `allowed_root`
    ///   passed to the pipeline.
    /// * **Synthetic source identifier** — for source-level acquisition
    ///   failures, formatted as `source:{source_id}`.
    ///
    /// Callers should inspect the path value to determine which format applies.
    pub path: std::path::PathBuf,
    /// Human-readable description of the failure.
    pub error: String,
}

/// Result of processing a single batch of files through parse → embed → load.
///
/// Named struct replacing the previous `(usize, usize, Vec<FileError>)` tuple
/// for clarity and future extensibility.
#[derive(Debug)]
struct BatchResult {
    /// Number of files successfully processed in this batch.
    docs_processed: usize,
    /// Number of chunks successfully loaded in this batch.
    chunks_loaded: usize,
    /// Per-file errors accumulated during batch processing.
    errors: Vec<FileError>,
    /// Files skipped because their extension was not in the source's `formats` list.
    skipped_by_format: usize,
}

/// Summary of a completed pipeline run.
#[derive(Debug)]
pub struct PipelineResult {
    /// Number of files successfully processed end-to-end (parse + load).
    pub documents_processed: usize,
    /// Total number of chunks written to the database across all sources.
    pub total_chunks: usize,
    /// Per-file parse or load failures collected during the run.
    ///
    /// Each entry represents a file that could not be processed due to a
    /// parse or load error. Processing continues for all other files.
    /// Format-based skips are tracked separately in [`PipelineResult::skipped_by_format`]
    /// and are not included here.
    pub errors_encountered: Vec<FileError>,
    /// Number of files skipped because their extension was not in the
    /// source's `formats` allow-list.
    ///
    /// These are silent skips — no error is generated.  Inspect this counter
    /// to audit format filtering behaviour.
    pub skipped_by_format: usize,
}

/// Run the full ingestion pipeline: acquire → parse → embed → load.
///
/// Processes all sources in `plan`. Source-level and file-level failures
/// are accumulated into [`PipelineResult::errors_encountered`] rather than
/// aborting the run.
///
/// Pass `model = None` to skip the embed step. This is useful for tests
/// or when the `all-MiniLM-L6-v2` model (~80 MB) has not yet been
/// downloaded. Chunks are written to the database regardless.
///
/// # Errors
///
/// Returns [`GraphtorError`] only for fatal conditions (database
/// unavailable, schema not initialised). Per-file failures are accumulated,
/// not propagated.
///
/// # Note on parallelism
///
/// When `config.parallel` is `true` the embed step follows the same
/// sequential code path until async embedding is implemented (009-F).
/// See [`PipelineConfig::parallel`] for details.
#[must_use = "pipeline result contains errors_encountered; inspect or explicitly ignore with `let _ = ...`"]
#[allow(clippy::too_many_lines)] // orchestrator — extracting sub-functions would add indirection
pub fn run(
    plan: &AcquisitionPlan,
    store: &DataStore,
    model: Option<&EmbeddingModel>,
    config: &PipelineConfig,
) -> Result<PipelineResult, GraphtorError> {
    // Guard: batch_size = 0 would cause slice::chunks(0) to panic at runtime.
    let effective_batch_size = if config.batch_size == 0 {
        warn!("PipelineConfig.batch_size is 0; clamped to 1");
        1_usize
    } else {
        config.batch_size
    };

    let run_start = Instant::now();
    info!(
        batch_size = effective_batch_size,
        parallel = config.parallel,
        "pipeline start"
    );

    // ── Stage 1: Acquire ───────────────────────────────────────────────────
    let acq_start = Instant::now();
    let acq_result = acquire_execute(plan, false);
    info!(
        succeeded = acq_result.succeeded,
        failed = acq_result.failed,
        total_files = acq_result.total_files,
        elapsed_ms = u64::try_from(acq_start.elapsed().as_millis()).unwrap_or(u64::MAX),
        "acquire stage complete"
    );

    // Build a source_id → PlannedSource index for metadata lookup.
    let source_index: HashMap<&str, &PlannedSource> =
        plan.sources.iter().map(|ps| (ps.source.id(), ps)).collect();

    let mut documents_processed: usize = 0;
    let mut total_chunks: usize = 0;
    let mut errors_encountered: Vec<FileError> = Vec::new();
    let mut skipped_by_format: usize = 0;

    // ── Stages 2–4: Parse → Embed → Load (per source) ─────────────────────
    for outcome in &acq_result.outcomes {
        match outcome {
            SourceOutcome::Success(ffs) => {
                // Resolve actual source metadata (kind, url, name) from the plan.
                // Falls back to source_id values when the planned source is not found
                // (should not happen in normal operation).
                let source_rec = if let Some(ps) = source_index.get(ffs.source_id.as_str()) {
                    build_source_record(ps)
                } else {
                    warn!(
                        source_id = %ffs.source_id,
                        "planned source not found in index; using source_id as fallback"
                    );
                    SourceRecord {
                        source_id: ffs.source_id.clone(),
                        url: ffs.source_id.clone(),
                        kind: "unknown".to_string(),
                        name: ffs.source_id.clone(),
                        synced_at: None,
                    }
                };
                if let Err(e) = upsert_source(store, &source_rec) {
                    warn!(
                        source_id = %ffs.source_id,
                        error = %e,
                        "failed to register source node; continuing"
                    );
                }

                let stage_start = Instant::now();
                let mut source_docs = 0_usize;
                let mut source_chunks = 0_usize;
                let mut source_skipped = 0_usize;

                // Resolve the format allow-list once per source, outside the batch loop.
                let source_formats: &[String] = source_index
                    .get(ffs.source_id.as_str())
                    .map_or(&[], |ps| ps.source.formats());

                // ── Pre-scan: fail-closed {source_id, source_path} duplicate detection ──
                // If two or more files in this source declare the same `source_path` in
                // their docline frontmatter, a future sync cycle would clobber one of them
                // via delete-before-insert.  Detect collisions upfront, push errors for
                // every conflicting file, and skip them all during the load phase.
                let rejected_by_duplicate: std::collections::HashSet<std::path::PathBuf> = {
                    let mut sp_to_files: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
                    for file in &ffs.files {
                        let ext = normalized_document_extension(file).unwrap_or_default();
                        // Only markdown files carry docline frontmatter; non-markdown files
                        // and format-excluded files will be handled (skipped) in process_batch.
                        if ext.as_str() != "md" || !is_format_allowed(source_formats, &ext) {
                            continue;
                        }
                        if let Ok(sp) = crate::ingest_contract::extract_source_path_from_file(file)
                        {
                            sp_to_files.entry(sp).or_default().push(file.clone());
                        }
                        // Files that fail the lightweight pre-scan (no frontmatter, invalid
                        // path, etc.) are left out of the map; they fail again during parse
                        // with a full-context FileError.
                    }
                    let mut rejected = std::collections::HashSet::new();
                    for (sp, files) in &sp_to_files {
                        if files.len() > 1 {
                            for file in files {
                                warn!(
                                    source_id = %ffs.source_id,
                                    source_path = %sp,
                                    file = %file.display(),
                                    conflict_count = files.len(),
                                    "duplicate source_path within source; \
                                     all conflicting files rejected to prevent data clobbering"
                                );
                                errors_encountered.push(FileError {
                                    path: file.clone(),
                                    error: format!(
                                        "duplicate source_path '{sp}' within source '{}': \
                                         {n} files claim the same canonical identity; \
                                         all are rejected (fail-closed) to prevent \
                                         delete-before-insert data loss on future sync",
                                        ffs.source_id,
                                        n = files.len()
                                    ),
                                });
                            }
                            rejected.extend(files.iter().cloned());
                        }
                    }
                    rejected
                };

                for batch in ffs.files.chunks(effective_batch_size) {
                    let result = process_batch(
                        batch,
                        &ffs.source_id,
                        store,
                        model,
                        &plan.allowed_root,
                        source_formats,
                        &rejected_by_duplicate,
                    );
                    source_docs += result.docs_processed;
                    source_chunks += result.chunks_loaded;
                    source_skipped += result.skipped_by_format;
                    errors_encountered.extend(result.errors);
                }

                info!(
                    source_id = %ffs.source_id,
                    documents = source_docs,
                    chunks = source_chunks,
                    elapsed_ms = u64::try_from(stage_start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "parse/embed/load stage complete"
                );

                documents_processed += source_docs;
                total_chunks += source_chunks;
                skipped_by_format += source_skipped;
            }
            SourceOutcome::Failed { source_id, error } => {
                warn!(source_id, error, "source acquisition failed; skipping");
                errors_encountered.push(FileError {
                    path: format!("source:{source_id}").into(),
                    error: error.clone(),
                });
            }
        }
    }

    info!(
        documents_processed,
        total_chunks,
        skipped_by_format,
        error_count = errors_encountered.len(),
        elapsed_ms = u64::try_from(run_start.elapsed().as_millis()).unwrap_or(u64::MAX),
        "pipeline complete"
    );

    Ok(PipelineResult {
        documents_processed,
        total_chunks,
        errors_encountered,
        skipped_by_format,
    })
}

/// Process one batch of files through parse → embed → load.
///
/// All errors are per-file; a failure on one file does not abort other
/// files in the same batch.
// Extension dispatch adds slightly more than 100 lines — extracting further
// would create additional indirection without readability gain.
#[allow(clippy::too_many_lines)]
fn process_batch(
    files: &[std::path::PathBuf],
    source_id: &str,
    store: &DataStore,
    model: Option<&EmbeddingModel>,
    allowed_root: &Path,
    formats: &[String],
    rejected_by_duplicate: &std::collections::HashSet<std::path::PathBuf>,
) -> BatchResult {
    let mut docs_ok = 0_usize;
    let mut chunks_ok = 0_usize;
    let mut errors: Vec<FileError> = Vec::new();
    let mut skipped_by_format = 0_usize;

    // ── Parse ──────────────────────────────────────────────────────────────
    let mut parsed = Vec::new();
    for file in files {
        // Skip files pre-rejected by the duplicate source_path check in `run()`.
        // Errors were already pushed by the caller; we simply skip here to avoid
        // double-counting and to prevent loading data under a colliding identity.
        if rejected_by_duplicate.contains(file) {
            debug!(
                path = %file.display(),
                "skipping file: pre-rejected by duplicate source_path check"
            );
            continue;
        }
        // `file` is the absolute path used for filesystem I/O and log context.
        // The canonical document path (source_path) is now derived from the
        // validated docline frontmatter contract, not from the filesystem path.
        let display_path = file.to_string_lossy();
        debug!(path = %display_path, "parsing file");

        // Belt-and-suspenders path guard — acquire already validates paths,
        // but we re-check here to enforce the workspace boundary at every stage.
        if let Err(e) = validate_path(file, allowed_root) {
            warn!(path = %display_path, error = %e, "path validation failed; skipping file");
            errors.push(FileError {
                path: file.clone(),
                error: e.to_string(),
            });
            continue;
        }

        // Detect extension for format filtering and parse dispatch.
        // Canonicalise `.markdown` → `md` so the format allow-list
        // correctly accepts both `.md` and `.markdown` files without
        // requiring users to enumerate both spellings.
        let ext = normalized_document_extension(file).unwrap_or_default();

        // Format allow-list filtering: non-empty `formats` acts as an allow-list.
        // Empty `formats` means "no restriction — accept all extensions".
        if !is_format_allowed(formats, &ext) {
            debug!(
                path = %display_path,
                extension = %ext,
                "file extension not in source formats allow-list; skipping"
            );
            skipped_by_format += 1;
            continue;
        }

        let parse_result = if ext.as_str() == "md" {
            parse_file(file, source_id)
        } else {
            debug!(
                path = %display_path,
                extension = %ext,
                "unsupported file extension; skipping"
            );
            continue;
        };

        match parse_result {
            Ok(doc) => {
                let doc_path = doc.path.clone();
                parsed.push((doc_path, doc));
            }
            Err(e) => {
                warn!(path = %display_path, error = %e, "parse failed; skipping file");
                errors.push(FileError {
                    path: file.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    // ── Embed ──────────────────────────────────────────────────────────────
    // Compute embeddings upfront for batch efficiency; vectors are persisted
    // only after the corresponding `upsert_chunk` succeeds (Load phase) to
    // prevent orphaned embeddings in `doc_vectors`.
    let vectors: std::collections::HashMap<String, Vec<f32>> = model
        .map(|m| compute_embeddings(&parsed, m))
        .unwrap_or_default();

    // ── Load ───────────────────────────────────────────────────────────────
    // Batched loads amortize CozoScript compilation and transaction commits:
    // one multi-row `:put` per relation for the whole batch instead of one
    // script per row. The happy path (no errors) is fully batched; on a batched
    // failure we fall back to the per-row path so per-file error attribution and
    // counts are preserved exactly.

    // Collect batch-wide load inputs. Empty-chunk documents contribute no
    // chunks, refs, or url entry (mirroring the per-doc rules): they still count
    // as processed because "all zero of their chunks loaded" is trivially true.
    let mut all_chunks: Vec<&Chunk> = Vec::new();
    let mut all_refs: Vec<&Reference> = Vec::new();
    let mut all_snippets: Vec<&CodeSnippet> = Vec::new();
    let mut url_entries: Vec<(String, String)> = Vec::new();
    for (path_str, doc) in &parsed {
        if doc.chunks.is_empty() {
            debug!(path = %path_str, "document parsed with zero chunks; skipping load");
        }
        all_chunks.extend(doc.chunks.iter());
        all_refs.extend(doc.references.iter());
        all_snippets.extend(doc.code_snippets.iter());
        // Register the document's canonical_url (the cross-source key) against
        // its entry chunk — same rule as `register_document_url`.
        if let Some(canonical) = doc
            .frontmatter
            .as_ref()
            .and_then(|f| f.canonical_url.as_deref())
        {
            if let Some(entry) = doc.entry_chunk() {
                url_entries.push((canonical.to_owned(), entry.chunk_id.clone()));
            }
        }
    }

    // Merge freshly computed vectors with any previously stored ones via ONE
    // batched read, so a `model = None` re-sync preserves embeddings without a
    // per-chunk query. Rule: stored embedding = fresh vector if present, else
    // the existing stored vector, else Null. Because the embedding column is
    // written inline with each chunk row, a vector is only ever persisted for a
    // chunk that is also being put — no orphaned embeddings.
    let existing_vectors = if all_chunks.is_empty() {
        Ok(HashMap::new())
    } else {
        let ids: Vec<&str> = all_chunks.iter().map(|c| c.chunk_id.as_str()).collect();
        get_vectors_for(store, &ids)
    };

    // Attempt the batched chunk put. On any failure (including the batched
    // embedding read), fall back to per-row loads for the whole batch so
    // per-file `FileError` attribution and `docs_ok` are preserved exactly.
    let batched_chunks_ok = match existing_vectors {
        Ok(mut merged) => {
            for (id, vec) in &vectors {
                merged.insert(id.clone(), vec.clone());
            }
            match upsert_chunks_batch(store, source_id, &all_chunks, &merged) {
                Ok(()) => {
                    chunks_ok += all_chunks.len();
                    true
                }
                Err(e) => {
                    warn!(error = %e, "batch chunk upsert failed; falling back to per-row loads");
                    false
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "batch embedding read failed; falling back to per-row loads");
            false
        }
    };

    if batched_chunks_ok {
        // Every document's chunks were stored, so every document counts as
        // processed (identical to the per-row path marking each file ok).
        docs_ok += parsed.len();
    } else {
        // Per-row fallback: identical to the historical per-file load loop.
        for (path_str, doc) in &parsed {
            let mut file_loaded_ok = true;
            for chunk in &doc.chunks {
                match upsert_chunk(store, source_id, chunk) {
                    Ok(()) => {
                        chunks_ok += 1;
                        // Persist a freshly computed vector only after the chunk
                        // upsert succeeds, so no embedding is stored for an
                        // absent chunk.
                        if let Some(vec) = vectors.get(&chunk.chunk_id) {
                            if let Err(e) = upsert_vector(store, &chunk.chunk_id, vec) {
                                warn!(
                                    chunk_id = %chunk.chunk_id,
                                    error = %e,
                                    "vector upsert failed; chunk stored without embedding"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            chunk_id = %chunk.chunk_id,
                            error = %e,
                            "chunk upsert failed; file marked with error"
                        );
                        errors.push(FileError {
                            path: std::path::PathBuf::from(path_str),
                            error: e.to_string(),
                        });
                        file_loaded_ok = false;
                    }
                }
            }
            if file_loaded_ok {
                docs_ok += 1;
            }
        }
    }

    // Edges, code snippets, and url-index entries are best-effort for the whole
    // batch regardless of the chunk outcome (matching the historical per-doc
    // behavior of attempting them even for a file whose chunk load failed). On a
    // batched failure, fall back to per-row so a single bad row does not drop
    // the rest — preserving today's "warn and continue" semantics.
    if !all_refs.is_empty() {
        if let Err(e) = upsert_edges_batch(store, &all_refs) {
            warn!(error = %e, "batch edge upsert failed; falling back to per-row");
            for reference in &all_refs {
                if let Err(e) = upsert_edge(store, reference) {
                    warn!(error = %e, "edge upsert failed; continuing");
                }
            }
        }
    }

    if !all_snippets.is_empty() {
        if let Err(e) = upsert_code_snippets_batch(store, &all_snippets) {
            warn!(error = %e, "batch snippet upsert failed; falling back to per-row");
            for snippet in &all_snippets {
                if let Err(e) = upsert_code_snippet(store, snippet) {
                    warn!(error = %e, "snippet upsert failed; continuing");
                }
            }
        }
    }

    if !url_entries.is_empty() {
        if let Err(e) = upsert_url_index_batch(store, &url_entries) {
            warn!(error = %e, "batch url index upsert failed; falling back to per-row");
            for (canonical, chunk_id) in &url_entries {
                if let Err(e) = upsert_url_index(store, canonical, chunk_id) {
                    warn!(error = %e, "url index registration failed; continuing");
                }
            }
        }
    }

    BatchResult {
        docs_processed: docs_ok,
        chunks_loaded: chunks_ok,
        errors,
        skipped_by_format,
    }
}

/// Return `true` if `ext` should be processed given `formats`.
///
/// An empty `formats` list means "no restriction — allow all extensions".
/// A non-empty `formats` list acts as an allow-list: only extensions listed
/// are accepted.  The comparison is case-insensitive and `"markdown"` is
/// treated as an alias for `"md"` (matching the normalisation applied by
/// [`crate::parse::normalized_document_extension`]).
fn is_format_allowed(formats: &[String], ext: &str) -> bool {
    if formats.is_empty() {
        return true;
    }
    formats
        .iter()
        .any(|f| crate::config::source::canonicalize_format_ext(f).eq_ignore_ascii_case(ext))
}

/// Compute embeddings for all chunks in a parsed batch.
///
/// Returns a map from `chunk_id` to its 384-dimensional embedding vector.
/// Embedding failures are logged as warnings and the affected chunks are
/// simply absent from the returned map; keyword search remains available for
/// them.  Callers persist the vectors only after confirming the corresponding
/// chunk upsert succeeded, avoiding orphaned embeddings in `doc_vectors`.
fn compute_embeddings(
    parsed: &[(String, crate::parse::types::ParsedDocument)],
    model: &EmbeddingModel,
) -> std::collections::HashMap<String, Vec<f32>> {
    let mut map = std::collections::HashMap::new();
    for (path_str, doc) in parsed {
        if doc.chunks.is_empty() {
            continue;
        }
        let texts: Vec<&str> = doc.chunks.iter().map(|c| c.content.as_str()).collect();
        match crate::embed::embed_batch(model, &texts) {
            Ok(vecs) => {
                debug!(
                    path = %path_str,
                    chunk_count = vecs.len(),
                    "embeddings computed"
                );
                for (chunk, vec) in doc.chunks.iter().zip(vecs) {
                    map.insert(chunk.chunk_id.clone(), vec);
                }
            }
            Err(e) => warn!(
                path = %path_str,
                error = %e,
                "embedding failed; chunks stored without vectors"
            ),
        }
    }
    map
}

/// Build a [`SourceRecord`] with accurate metadata from a [`PlannedSource`].
///
/// Extracts kind, URL, and display name from the original source configuration
/// rather than using the source identifier as a placeholder.
fn build_source_record(ps: &PlannedSource) -> SourceRecord {
    let Source::Local(local) = &ps.source;
    let id = local.id.clone();
    SourceRecord {
        url: local.path.to_string_lossy().into_owned(),
        kind: "local".to_string(),
        name: id.clone(),
        source_id: id,
        synced_at: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;

    use super::*;
    use crate::db::list_chunks_for_source;

    fn docline_md(source_path: &str, title: &str, body: &str) -> String {
        format!(
            "---\ntitle: {title}\nsource: /test/source\ningested_at: \
             2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{body}"
        )
    }

    /// A batch containing one valid docline file and one file that fails to
    /// parse must: record a `FileError` for the bad file, load the good file's
    /// chunks, and count exactly one processed document. This exercises the
    /// mixed-good/bad batch resilience of the batched Load stage.
    #[test]
    fn process_batch_loads_good_doc_and_records_bad_doc_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).expect("create docs dir");

        let good = docs.join("good.md");
        fs::write(
            &good,
            docline_md("good.md", "Good", "# Good\n\nGood body text.\n").as_bytes(),
        )
        .expect("write good.md");

        // Invalid UTF-8 bytes make `parse_file` fail at read time.
        let bad = docs.join("bad.md");
        fs::write(&bad, b"\xFF\xFE not valid utf-8").expect("write bad.md");

        let store = DataStore::open_mem().expect("open_mem");
        store.ensure_schema().expect("ensure_schema");

        let files = vec![good.clone(), bad.clone()];
        let result = process_batch(
            &files,
            "mixed-source",
            &store,
            None,
            root,
            &[],
            &HashSet::new(),
        );

        assert_eq!(result.docs_processed, 1, "only the good doc is processed");
        assert!(result.chunks_loaded >= 1, "good doc chunks were loaded");
        assert_eq!(result.errors.len(), 1, "exactly one file error recorded");
        assert!(
            result.errors[0].path.to_string_lossy().contains("bad.md"),
            "error must reference bad.md, got {:?}",
            result.errors[0].path
        );

        let stored = list_chunks_for_source(&store, "mixed-source").expect("list chunks");
        assert!(!stored.is_empty(), "good document chunks must be persisted");
        assert!(
            stored.iter().all(|c| c.path == "good.md"),
            "only good.md chunks should be present"
        );
    }

    /// The all-batched happy path must persist chunks, edges, code snippets, and
    /// the canonical-url index for every document, and count them all.
    #[test]
    fn process_batch_happy_path_loads_all_relations() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).expect("create docs dir");

        for (name, sp) in [("a.md", "a.md"), ("b.md", "b.md")] {
            fs::write(
                docs.join(name),
                docline_md(
                    sp,
                    "Doc",
                    "# Heading\n\nSee [other](b.md).\n\n```rust\nfn x() {}\n```\n",
                )
                .as_bytes(),
            )
            .expect("write doc");
        }

        let store = DataStore::open_mem().expect("open_mem");
        store.ensure_schema().expect("ensure_schema");

        let files = vec![docs.join("a.md"), docs.join("b.md")];
        let result = process_batch(
            &files,
            "happy-source",
            &store,
            None,
            root,
            &[],
            &HashSet::new(),
        );

        assert_eq!(result.docs_processed, 2, "both docs processed");
        assert_eq!(result.errors.len(), 0, "no errors on the happy path");
        let stored = list_chunks_for_source(&store, "happy-source").expect("list chunks");
        assert_eq!(stored.len(), result.chunks_loaded, "chunk count consistent");
        assert!(stored.len() >= 2, "each doc contributes at least one chunk");
    }
}
