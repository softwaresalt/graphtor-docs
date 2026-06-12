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
use crate::db::chunks::upsert_chunk;
use crate::db::edges::{upsert_code_snippet, upsert_edge};
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::db::vectors::upsert_vector;
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
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

                for batch in ffs.files.chunks(effective_batch_size) {
                    let result = process_batch(
                        batch,
                        &ffs.source_id,
                        store,
                        model,
                        &plan.allowed_root,
                        source_formats,
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
            SourceOutcome::Skipped { source_id } => {
                info!(source_id, "source skipped during acquisition");
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
) -> BatchResult {
    let mut docs_ok = 0_usize;
    let mut chunks_ok = 0_usize;
    let mut errors: Vec<FileError> = Vec::new();
    let mut skipped_by_format = 0_usize;

    // ── Parse ──────────────────────────────────────────────────────────────
    let mut parsed = Vec::new();
    for file in files {
        // `file` is the absolute path used for filesystem I/O and log context.
        // `path_str` is the source-root-relative path used for chunk IDs and DB
        // provenance — it must be portable across checkout locations.
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

        // Derive a source-root-relative path for stable chunk IDs.
        // Normalize to forward slashes so chunk IDs and MCP path-matching are
        // platform-independent; on Windows, strip_prefix produces backslash paths.
        let path_str = match file.strip_prefix(allowed_root) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(e) => {
                warn!(
                    path = %display_path,
                    root = %allowed_root.to_string_lossy(),
                    error = %e,
                    "failed to derive source-relative path; skipping file"
                );
                errors.push(FileError {
                    path: file.clone(),
                    error: format!("failed to derive source-relative path: {e}"),
                });
                continue;
            }
        };

        // Detect extension for format filtering and parse dispatch.
        // Canonicalise `.markdown` → `md` so the default allow-list
        // `["md", "pdf", "docx"]` correctly accepts both `.md` and `.markdown`
        // files without requiring users to enumerate both spellings.
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

        let parse_result = match ext.as_str() {
            "md" | "pdf" | "docx" => parse_file(file, &path_str),
            _ => {
                debug!(
                    path = %display_path,
                    extension = %ext,
                    "unsupported file extension; skipping"
                );
                continue;
            }
        };

        match parse_result {
            Ok(doc) => parsed.push((path_str, doc)),
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
    for (path_str, doc) in &parsed {
        if doc.chunks.is_empty() {
            debug!(path = %path_str, "document parsed with zero chunks; skipping load");
        }

        let mut file_loaded_ok = true;

        for chunk in &doc.chunks {
            match upsert_chunk(store, source_id, chunk) {
                Ok(()) => {
                    chunks_ok += 1;
                    // Persist vector only after chunk upsert succeeds, so
                    // `doc_vectors` never contains embeddings for absent chunks.
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
                        path: std::path::PathBuf::from(&path_str),
                        error: e.to_string(),
                    });
                    file_loaded_ok = false;
                }
            }
        }

        for reference in &doc.references {
            if let Err(e) = upsert_edge(store, reference) {
                warn!(error = %e, "edge upsert failed; continuing");
            }
        }

        for snippet in &doc.code_snippets {
            if let Err(e) = upsert_code_snippet(store, snippet) {
                warn!(error = %e, "snippet upsert failed; continuing");
            }
        }

        if file_loaded_ok {
            docs_ok += 1;
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
/// are accepted.  The comparison is case-insensitive.
fn is_format_allowed(formats: &[String], ext: &str) -> bool {
    if formats.is_empty() {
        return true;
    }
    formats.iter().any(|f| f.eq_ignore_ascii_case(ext))
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
    match &ps.source {
        Source::Git(git) => {
            let id = git.id.clone();
            SourceRecord {
                url: git.url.clone(),
                kind: "git".to_string(),
                name: id.clone(),
                source_id: id,
                synced_at: None,
            }
        }
        Source::Local(local) => {
            let id = local.id.clone();
            SourceRecord {
                url: local.path.to_string_lossy().into_owned(),
                kind: "local".to_string(),
                name: id.clone(),
                source_id: id,
                synced_at: None,
            }
        }
        Source::Url(url_src) => {
            let id = url_src.id.clone();
            SourceRecord {
                url: url_src.url.clone(),
                kind: "url".to_string(),
                name: id.clone(),
                source_id: id,
                synced_at: None,
            }
        }
    }
}
