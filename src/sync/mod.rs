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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::{info, info_span, warn};

use crate::acquire::filter_files;
use crate::config::Source;
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
use crate::parse::{is_supported_document_extension, normalized_document_extension};
use crate::DataStore;

/// Lifecycle state for a per-file sync progress event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncProgressStatus {
    /// A file is about to be re-ingested.
    Started,
    /// A file finished re-ingestion successfully.
    Completed {
        /// Wall-clock time spent re-ingesting the file.
        elapsed: Duration,
        /// Number of chunks created while processing the file.
        chunks_created: usize,
    },
    /// A file failed during re-ingestion.
    Failed {
        /// Wall-clock time spent before the failure occurred.
        elapsed: Duration,
        /// Human-readable error surfaced by the parser / loader pipeline.
        error: String,
    },
}

/// Structured per-file sync progress reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgressEvent {
    /// Source-relative path to the file being processed.
    pub path: PathBuf,
    /// One-based index of the current file.
    pub current: usize,
    /// Total number of files scheduled for re-ingestion.
    pub total: usize,
    /// Source file size, when metadata is available.
    pub size_bytes: Option<u64>,
    /// Lifecycle state for this file.
    pub status: SyncProgressStatus,
}

/// Callback type for per-file sync progress reporting.
pub type ProgressCallback<'a> = Option<&'a mut dyn FnMut(SyncProgressEvent)>;

/// Sync a single documentation source against its current on-disk state.
///
/// ## Behaviour
///
/// ### First run (no stored state for this source)
///
/// All tracked files in the source directory are treated as newly added.  The
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
/// When `Some`, the callback is invoked for per-file lifecycle events while a
/// file is re-ingested. The callback receives the source-relative file path,
/// the 1-based file index, total file count, optional file size metadata, and
/// whether the file started, completed, or failed.
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
            let current_mtimes = scan_tracked_source_mtimes(source, source_dir)?;
            mtime_diff::diff_mtimes(&current_mtimes, &stored_mtimes)
        }
        Source::Url(_) => {
            // URL sources: treat all crawled files as mtime-tracked, same as local.
            let stored_mtimes = stored
                .as_ref()
                .map(|s| &s.file_mtimes)
                .map_or_else(Default::default, Clone::clone);
            let current_mtimes = scan_tracked_source_mtimes(source, source_dir)?;
            mtime_diff::diff_mtimes(&current_mtimes, &stored_mtimes)
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
        let abs_path = source_dir.join(path);
        let current = idx + 1;
        let size_bytes = abs_path.metadata().ok().map(|metadata| metadata.len());

        if let Some(cb) = on_progress.as_mut() {
            cb(SyncProgressEvent {
                path: path.to_path_buf(),
                current,
                total: ingest_total,
                size_bytes,
                status: SyncProgressStatus::Started,
            });
        }

        let file_started_at = Instant::now();
        match reingest_file(store, source_id, &abs_path, source_dir, root, model) {
            Ok(n) => {
                metrics.files_synced += 1;
                metrics.chunks_created += n;
                if let Some(cb) = on_progress.as_mut() {
                    cb(SyncProgressEvent {
                        path: path.to_path_buf(),
                        current,
                        total: ingest_total,
                        size_bytes,
                        status: SyncProgressStatus::Completed {
                            elapsed: file_started_at.elapsed(),
                            chunks_created: n,
                        },
                    });
                }
            }
            Err(e) => {
                warn!(
                    path = %abs_path.display(),
                    error = %e,
                    "reingest failed; continuing"
                );
                metrics.errors += 1;
                if let Some(cb) = on_progress.as_mut() {
                    cb(SyncProgressEvent {
                        path: path.to_path_buf(),
                        current,
                        total: ingest_total,
                        size_bytes,
                        status: SyncProgressStatus::Failed {
                            elapsed: file_started_at.elapsed(),
                            error: e.to_string(),
                        },
                    });
                }
            }
        }
    }

    // ── Persist updated state ──────────────────────────────────────────────
    let new_source_state = build_new_state(source, source_dir, stored.as_ref())?;
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

/// Build the new [`SourceSyncState`] to persist after a completed sync cycle,
/// including runs that reported non-fatal per-file errors.
fn build_new_state(
    source: &Source,
    source_dir: &Path,
    _stored: Option<&SourceSyncState>,
) -> Result<SourceSyncState, GraphtorError> {
    let now = chrono_now();
    match source {
        Source::Git(_) => {
            // Read current HEAD commit from the repository.
            let last_commit = git2::Repository::open(source_dir).ok().and_then(|repo| {
                let target = repo.head().ok()?.target()?;
                Some(target.to_string())
            });
            Ok(SourceSyncState {
                last_commit,
                file_mtimes: std::collections::HashMap::new(),
                last_sync: Some(now),
            })
        }
        Source::Local(_) => {
            // Scan current mtimes to record the new baseline.
            let file_mtimes = scan_tracked_source_mtimes(source, source_dir)?;
            Ok(SourceSyncState {
                last_commit: None,
                file_mtimes,
                last_sync: Some(now),
            })
        }
        Source::Url(_) => {
            // URL sources: track by mtime, same as local sources.
            let file_mtimes = scan_tracked_source_mtimes(source, source_dir)?;
            Ok(SourceSyncState {
                last_commit: None,
                file_mtimes,
                last_sync: Some(now),
            })
        }
    }
}

fn scan_tracked_source_mtimes(
    source: &Source,
    source_dir: &Path,
) -> Result<HashMap<String, u64>, GraphtorError> {
    let all_mtimes = scan_mtimes(source_dir)?;
    let relative_paths: Vec<PathBuf> = all_mtimes.keys().map(PathBuf::from).collect();
    let tracked_paths: HashSet<String> =
        filter_files(&relative_paths, source.include(), source.exclude())?
            .into_iter()
            .filter(|path| is_tracked_source_path(source, path))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();

    Ok(all_mtimes
        .into_iter()
        .filter(|(path, _)| tracked_paths.contains(path))
        .collect())
}

fn is_tracked_source_path(source: &Source, relative_path: &Path) -> bool {
    let Some(ext) = normalized_document_extension(relative_path) else {
        return false;
    };

    is_supported_document_extension(&ext)
        && (source.formats().is_empty()
            || source
                .formats()
                .iter()
                .any(|format| format.eq_ignore_ascii_case(&ext)))
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

    use super::{sync_source, SyncProgressStatus};
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
            database: None,
        });

        let mut progress_events = Vec::new();
        let mut cb = |event| {
            progress_events.push(event);
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
            progress_events.len(),
            6,
            "callback should fire start and completion events per file; got {progress_events:#?}"
        );
        let mut per_file = std::collections::BTreeMap::new();
        let mut observed_indices = std::collections::BTreeSet::new();
        for event in &progress_events {
            let file_name = event
                .path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .expect("progress event filename")
                .to_string();
            let entry = per_file.entry(file_name).or_insert_with(Vec::new);
            entry.push((event.current, &event.status));
            observed_indices.insert(event.current);
            assert_eq!(event.total, 3, "unexpected total in event: {event:#?}");
        }
        assert_eq!(
            observed_indices,
            std::collections::BTreeSet::from([1, 2, 3]),
            "expected one progress index per file; got {progress_events:#?}"
        );
        for index in 0..3 {
            let expected_name = format!("doc{index}.md");
            let Some(events) = per_file.get(expected_name.as_str()) else {
                panic!("missing progress events for {expected_name}: {progress_events:#?}");
            };
            assert_eq!(
                events.len(),
                2,
                "expected two events for {expected_name}: {events:#?}"
            );
            assert!(
                matches!(events[0].1, SyncProgressStatus::Started),
                "expected start event first for {expected_name}: {events:#?}"
            );
            assert!(
                matches!(events[1].1, SyncProgressStatus::Completed { .. }),
                "expected completion event second for {expected_name}: {events:#?}"
            );
            assert_eq!(
                events[0].0, events[1].0,
                "expected start/completion indices to match for {expected_name}: {events:#?}"
            );
        }
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
            database: None,
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
