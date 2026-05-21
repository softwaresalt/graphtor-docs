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
use std::time::Instant;

use serde::Serialize;
use tracing::{info, info_span, warn};

use crate::config::Source;
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
use crate::DataStore;

/// Callback type for per-file sync progress reporting.
///
/// Called once per file during re-ingestion with `(path, current, total)`.
pub type ProgressCallback<'a> = Option<&'a mut dyn FnMut(&Path, usize, usize)>;

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
/// ## `on_progress`
///
/// When `Some`, the callback is invoked once for each file that is re-ingested
/// (added or modified).  The callback receives `(path, current, total)` where
/// `path` is the source-relative file path, `current` is the 1-based index of
/// the current file, and `total` is the total number of files to re-ingest.
///
/// ## Errors
///
/// Returns [`GraphtorError::Sync`] wrapping the underlying cause for any
/// non-fatal aggregate failure.  Returns the specific underlying error type for
/// fatal path-violation or database schema issues.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn sync_source(
    store: &DataStore,
    source: &Source,
    source_dir: &Path,
    state_path: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
    mut on_progress: ProgressCallback<'_>,
) -> Result<SyncMetrics, GraphtorError> {
    let started_at = Instant::now();
    let source_id = source.id();
    let (source_kind, source_url, source_name) = match source {
        Source::Git(g) => ("git", g.url.as_str(), g.id.as_str()),
        Source::Local(l) => ("local", l.path.to_str().unwrap_or(""), l.id.as_str()),
        Source::Url(u) => ("url", u.url.as_str(), u.id.as_str()),
    };

    let sync_span = info_span!("sync_source", source_id, source_kind);
    let _sync_span = sync_span.enter();

    info!(source_name, source_url, "starting sync cycle");

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

    let files_total = changes.added.len() + changes.modified.len() + changes.deleted.len();

    if changes.is_empty() {
        info!("no changes detected; skipping re-ingestion");

        let metrics = SyncMetrics {
            files_total,
            files_synced: 0,
            files_deleted: 0,
            chunks_created: 0,
            chunks_deleted: 0,
            duration_ms: elapsed_millis(started_at),
            errors: 0,
        };

        info!(
            files_total = metrics.files_total,
            files_synced = metrics.files_synced,
            files_deleted = metrics.files_deleted,
            chunks_created = metrics.chunks_created,
            chunks_deleted = metrics.chunks_deleted,
            duration_ms = metrics.duration_ms,
            errors = metrics.errors,
            "sync cycle complete"
        );

        return Ok(metrics);
    }

    info!(
        files_total,
        added = changes.added.len(),
        modified = changes.modified.len(),
        deleted = changes.deleted.len(),
        "changes detected; beginning re-ingestion"
    );

    let mut metrics = SyncMetrics {
        files_total,
        files_synced: 0,
        files_deleted: 0,
        chunks_created: 0,
        chunks_deleted: 0,
        duration_ms: 0,
        errors: 0,
    };

    // ── Delete removed files ───────────────────────────────────────────────
    for path in &changes.deleted {
        let rel = path.to_string_lossy().replace('\\', "/");
        if let Err(e) = delete_file_data(store, &rel) {
            warn!(path = %path.display(), error = %e, "failed to delete stale records");
            metrics.errors += 1;
        } else {
            metrics.files_deleted += 1;
        }
    }

    // ── Re-ingest added and modified files ─────────────────────────────────
    let ingest_total = changes.added.len() + changes.modified.len();
    for (idx, path) in changes
        .added
        .iter()
        .chain(changes.modified.iter())
        .map(std::path::PathBuf::as_path)
        .enumerate()
    {
        if let Some(cb) = on_progress.as_mut() {
            cb(path, idx + 1, ingest_total);
        }
        let abs_path = source_dir.join(path);
        match reingest_file(store, source_id, &abs_path, source_dir, root, model) {
            Ok(n) => {
                metrics.files_synced += 1;
                metrics.chunks_created += n;
            }
            Err(e) => {
                warn!(
                    path = %abs_path.display(),
                    error = %e,
                    "reingest failed; continuing"
                );
                metrics.errors += 1;
            }
        }
    }

    // ── Persist updated state ──────────────────────────────────────────────
    let new_source_state = build_new_state(source, source_dir, stored.as_ref());
    *sync_state.source_mut(source_id) = new_source_state;
    sync_state.save(state_path, root)?;

    metrics.duration_ms = elapsed_millis(started_at);

    info!(
        files_total = metrics.files_total,
        files_synced = metrics.files_synced,
        files_deleted = metrics.files_deleted,
        chunks_created = metrics.chunks_created,
        chunks_deleted = metrics.chunks_deleted,
        duration_ms = metrics.duration_ms,
        errors = metrics.errors,
        "sync cycle complete"
    );

    Ok(metrics)
}

/// Structured telemetry for one completed sync cycle.
///
/// `chunks_deleted` is currently reported as `0` because the delete path does
/// not expose per-chunk delete counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncMetrics {
    /// Total files considered by the cycle (added + modified + deleted).
    pub files_total: usize,
    /// Number of files successfully re-ingested.
    pub files_synced: usize,
    /// Number of deleted files whose records were removed from `CozoDB`.
    pub files_deleted: usize,
    /// Total chunks written to the database in this cycle.
    pub chunks_created: usize,
    /// Total chunks deleted from the database in this cycle.
    pub chunks_deleted: usize,
    /// Wall-clock duration of the sync cycle in milliseconds.
    pub duration_ms: u64,
    /// Number of files that encountered errors (non-fatal).
    pub errors: usize,
}

/// Backward-compatible alias for [`SyncMetrics`].
pub type SyncCycleResult = SyncMetrics;

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

/// Convert a captured [`Instant`] to elapsed milliseconds, clamped to at least 1.
///
/// Exported so callers that measure the same sync cycle (e.g. the binary entry
/// point) do not need to duplicate this logic.
#[must_use]
pub fn elapsed_millis(started_at: Instant) -> u64 {
    let elapsed_ms = started_at.elapsed().as_millis();
    u64::try_from(elapsed_ms).unwrap_or(u64::MAX).max(1)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::sync_source;
    use crate::config::source::LocalSource;
    use crate::db::ensure_schema;
    use crate::{DataStore, Source};

    #[test]
    fn sync_source_progress_callback_invoked_per_file() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        for i in 0..3_u8 {
            fs::write(
                source_dir.join(format!("doc{i}.md")),
                format!("# Doc {i}\n\nContent.\n"),
            )
            .expect("write markdown");
        }

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "progress-test".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
        });

        let mut call_count: usize = 0;
        let mut cb = |_path: &std::path::Path, _idx: usize, _total: usize| {
            call_count += 1;
        };

        let metrics = sync_source(
            &store,
            &source,
            &source_dir,
            &state_path,
            root,
            None,
            Some(&mut cb),
        )
        .expect("sync with progress callback");

        assert_eq!(
            call_count, 3,
            "callback should fire once per file; got {call_count}"
        );
        assert_eq!(metrics.files_synced, 3, "metrics: {metrics:?}");
    }

    #[test]
    fn sync_metrics_returned_for_local_source() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        fs::write(source_dir.join("guide.md"), "# Guide\n\nHello world.\n")
            .expect("write markdown");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "local-sync".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
        });

        let metrics =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("sync");

        assert_eq!(metrics.files_total, 1, "metrics: {metrics:?}");
        assert_eq!(metrics.files_synced, 1, "metrics: {metrics:?}");
        assert_eq!(metrics.files_deleted, 0, "metrics: {metrics:?}");
        assert!(metrics.chunks_created > 0, "metrics: {metrics:?}");
        assert_eq!(metrics.chunks_deleted, 0, "metrics: {metrics:?}");
        assert!(metrics.duration_ms > 0, "metrics: {metrics:?}");
        assert_eq!(metrics.errors, 0, "metrics: {metrics:?}");
    }
}
