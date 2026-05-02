//! Incremental sync orchestration for `LocalDocRAG`.
//!
//! This module ties together sync state persistence, change detection, and
//! surgical re-ingestion to enable efficient incremental updates:
//!
//! - [`state`]: `.sync_state.json` read/write.
//! - [`git_diff`]: Tree-to-tree diff via `git2`.
//! - [`mtime_diff`]: Mtime-based change detection for local sources.
//! - [`reingest`]: Delete-then-reload pipeline for individual files.
//!
//! The top-level [`sync_source`] function drives a complete sync cycle for
//! one documentation source: detect changes, delete stale records, re-ingest
//! changed files, and persist updated state.

pub mod git_diff;
pub mod mtime_diff;
pub mod reingest;
pub mod state;

pub use git_diff::{compute_git_diff, ChangedFiles};
pub use mtime_diff::{compute_mtime_diff, scan_mtimes};
pub use reingest::{delete_file_data, reingest_file};
pub use state::{SourceSyncState, SyncState};

use std::path::Path;

use tracing::{info, warn};

use crate::config::Source;
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
use crate::DataStore;

/// Sync a single documentation source against its current on-disk state.
///
/// ## Behaviour
///
/// ### First run (no stored state for this source)
///
/// All Markdown files in the source directory are treated as newly added.  The
/// pipeline re-ingests every file and persists the resulting state.
///
/// ### Incremental run (stored state found)
///
/// - **Git sources**: tree-to-tree diff between the stored commit OID and
///   the current `HEAD` identifies added, modified, and deleted files.
/// - **Local sources**: mtime comparison against `stored.file_mtimes` identifies
///   added, modified, and deleted files.
///
/// Changed files are deleted from `CozoDB` then re-ingested.  Deleted files have
/// their records removed without reinsertion.  Unchanged files are skipped.
///
/// ## `state_path` and `root`
///
/// `state_path` is the workspace-relative path to `.sync_state.json`.  `root`
/// is the workspace root used for path security validation.  Both must refer to
/// paths within the same workspace.
///
/// ## Errors
///
/// Returns [`GraphtorError::Sync`] wrapping the underlying cause for any
/// non-fatal aggregate failure.  Returns the specific underlying error type for
/// fatal path-violation or database schema issues.
#[allow(clippy::too_many_arguments)]
pub fn sync_source(
    store: &DataStore,
    source: &Source,
    source_dir: &Path,
    state_path: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
) -> Result<SyncCycleResult, GraphtorError> {
    let source_id = source.id();
    let (source_kind, source_url, source_name) = match source {
        Source::Git(g) => ("git", g.url.as_str(), g.id.as_str()),
        Source::Local(l) => ("local", l.path.to_str().unwrap_or(""), l.id.as_str()),
        Source::Url(u) => ("url", u.url.as_str(), u.id.as_str()),
    };

    info!(source_id, source_kind, "starting sync cycle");

    // Load existing state (empty on first run).
    let mut sync_state = SyncState::load(state_path, root)?;
    let stored = sync_state.source(source_id).cloned();

    // ── Ensure the source record exists in the database ────────────────────
    let source_record = SourceRecord {
        source_id: source_id.to_string(),
        url: source_url.to_string(),
        kind: source_kind.to_string(),
        name: source_name.to_string(),
        synced_at: None,
    };
    upsert_source(store, &source_record)?;

    // ── Detect changes ─────────────────────────────────────────────────────
    let changes = match source {
        Source::Git(_) => {
            let last_commit = stored.as_ref().and_then(|s| s.last_commit.as_deref());
            compute_git_diff(source_dir, last_commit)?
        }
        Source::Local(_) => {
            let stored_mtimes = stored
                .as_ref()
                .map(|s| &s.file_mtimes)
                .map_or_else(Default::default, Clone::clone);
            compute_mtime_diff(source_dir, &stored_mtimes)?
        }
        Source::Url(_) => {
            // URL sources: treat all crawled files as mtime-tracked, same as local.
            let stored_mtimes = stored
                .as_ref()
                .map(|s| &s.file_mtimes)
                .map_or_else(Default::default, Clone::clone);
            compute_mtime_diff(source_dir, &stored_mtimes)?
        }
    };

    if changes.is_empty() {
        info!(source_id, "no changes detected; skipping re-ingestion");
        return Ok(SyncCycleResult {
            files_processed: 0,
            chunks_loaded: 0,
            files_deleted: 0,
            files_errored: 0,
        });
    }

    info!(
        source_id,
        added = changes.added.len(),
        modified = changes.modified.len(),
        deleted = changes.deleted.len(),
        "changes detected; beginning re-ingestion"
    );

    let mut files_processed: usize = 0;
    let mut chunks_loaded: usize = 0;
    let mut files_deleted: usize = 0;
    let mut files_errored: usize = 0;

    // ── Delete removed files ───────────────────────────────────────────────
    for path in &changes.deleted {
        let rel = path.to_string_lossy().replace('\\', "/");
        if let Err(e) = delete_file_data(store, &rel) {
            warn!(source_id, path = %path.display(), error = %e, "failed to delete stale records");
            files_errored += 1;
        } else {
            files_deleted += 1;
        }
    }

    // ── Re-ingest added and modified files ─────────────────────────────────
    for path in changes.added.iter().chain(changes.modified.iter()) {
        let abs_path = source_dir.join(path);
        match reingest_file(store, source_id, &abs_path, source_dir, root, model) {
            Ok(n) => {
                files_processed += 1;
                chunks_loaded += n;
            }
            Err(e) => {
                warn!(
                    source_id,
                    path = %abs_path.display(),
                    error = %e,
                    "reingest failed; continuing"
                );
                files_errored += 1;
            }
        }
    }

    // ── Persist updated state ──────────────────────────────────────────────
    let new_source_state = build_new_state(source, source_dir, stored.as_ref());
    *sync_state.source_mut(source_id) = new_source_state;
    sync_state.save(state_path, root)?;

    info!(
        source_id,
        files_processed, chunks_loaded, files_deleted, files_errored, "sync cycle complete"
    );

    Ok(SyncCycleResult {
        files_processed,
        chunks_loaded,
        files_deleted,
        files_errored,
    })
}

/// Summary of a completed sync cycle for one source.
#[derive(Debug, Clone)]
pub struct SyncCycleResult {
    /// Number of files successfully re-ingested (added + modified).
    pub files_processed: usize,
    /// Total chunks written to the database in this cycle.
    pub chunks_loaded: usize,
    /// Number of deleted files whose records were removed from `CozoDB`.
    pub files_deleted: usize,
    /// Number of files that encountered errors (non-fatal).
    pub files_errored: usize,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build the new [`SourceSyncState`] to persist after a successful sync cycle.
fn build_new_state(
    source: &Source,
    source_dir: &Path,
    _stored: Option<&SourceSyncState>,
) -> SourceSyncState {
    let now = chrono_now();
    match source {
        Source::Git(_) => {
            // Read current HEAD commit from the repository.
            let last_commit = git2::Repository::open(source_dir).ok().and_then(|repo| {
                let target = repo.head().ok()?.target()?;
                Some(target.to_string())
            });
            SourceSyncState {
                last_commit,
                file_mtimes: std::collections::HashMap::new(),
                last_sync: Some(now),
            }
        }
        Source::Local(_) => {
            // Scan current mtimes to record the new baseline.
            let file_mtimes = scan_mtimes(source_dir).unwrap_or_default();
            SourceSyncState {
                last_commit: None,
                file_mtimes,
                last_sync: Some(now),
            }
        }
        Source::Url(_) => {
            // URL sources: track by mtime, same as local sources.
            let file_mtimes = scan_mtimes(source_dir).unwrap_or_default();
            SourceSyncState {
                last_commit: None,
                file_mtimes,
                last_sync: Some(now),
            }
        }
    }
}

/// Return the current UTC time as a Unix-epoch seconds string.
///
/// The returned string is used as the `last_sync` timestamp value in
/// [`SourceSyncState`].  It is NOT ISO-8601; it is the number of seconds
/// elapsed since the Unix epoch (1970-01-01T00:00:00Z) as a decimal string.
fn chrono_now() -> String {
    // Avoid pulling in the chrono crate — epoch seconds are sufficient for
    // sync state bookkeeping (ordering, not display).
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{secs}")
}
