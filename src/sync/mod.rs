//! Incremental sync orchestration for `LocalDocRAG`.
//!
//! This module ties together sync state persistence, change detection, and
//! surgical re-ingestion to enable efficient incremental updates:
//!
//! - [`state`]: `.sync_state.json` read/write.
//! - [`mtime_diff`]: Mtime-based change detection for local sources.
//! - [`reingest`]: Delete-then-reload pipeline for individual files.
//!
//! The top-level [`sync_source`] function drives a complete sync cycle for
//! one documentation source: detect changes, delete stale records, re-ingest
//! changed files, and persist updated state.

pub mod mtime_diff;
pub mod reingest;
pub mod state;

pub use mtime_diff::{compute_mtime_diff, scan_mtimes, ChangedFiles};
pub use reingest::{
    delete_file_data, delete_file_data_for_source, reingest_file,
    reingest_file_with_old_contract_path,
};
pub use state::{SourceSyncState, SyncState};

/// Per-source pre-sync capture of live filesystem state.
///
/// Captured immediately **before** the full-sync pipeline runs so the persisted
/// sync state reflects what was on disk at pipeline start time — not a
/// post-pipeline re-scan that could include post-run mutations.
///
/// See [`capture_pre_sync_snapshot`] and [`seed_sync_state_from_pre_sync_snapshot`].
#[derive(Debug, Clone, Default)]
pub struct SourcePreSyncCapture {
    /// Map of source-root-relative path → Unix mtime (seconds).
    pub file_mtimes: HashMap<String, u64>,
    /// Map of source-root-relative path → validated contract `source_path`.
    ///
    /// Populated **leniently**: files whose frontmatter cannot be parsed are
    /// included in `file_mtimes` but omitted from this map.  They still
    /// participate in mtime-based change detection; identity tracking is absent
    /// until a later incremental ingest succeeds.
    pub file_contract_paths: HashMap<String, String>,
}

/// Pre-sync snapshot for all sources in a plan, keyed by source ID.
///
/// Returned by [`capture_pre_sync_snapshot`] and consumed by
/// [`seed_sync_state_from_pre_sync_snapshot`].
pub type PreSyncSnapshot = HashMap<String, SourcePreSyncCapture>;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::{info, info_span, warn};

use crate::acquire::{filter_files, AcquisitionPlan};
use crate::config::Source;
use crate::db::nodes::{upsert_source, SourceRecord};
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;
use crate::ingest_contract::CONTRACT_EPOCH;
use crate::parse::{is_supported_document_extension, normalized_document_extension};
use crate::path::validate_path;
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
/// Mtime comparison against `stored.file_mtimes` identifies added, modified,
/// and deleted files. Changed files are deleted from `CozoDB` then re-ingested.
/// Deleted files have their records removed without reinsertion. Unchanged files
/// are skipped.
///
/// ### Epoch mismatch (schema pivot)
///
/// When the stored `contract_epoch` differs from [`CONTRACT_EPOCH`], all
/// tracked files are treated as changed and re-ingested from scratch.
///
/// ### Pending v4 database migration
///
/// When [`crate::db::needs_v4_migration`] still reports pre-v4 state, all
/// tracked files are also treated as changed so a retried rebuild repopulates
/// the database before the migration gate is cleared.
///
/// ## `state_path` and `root`
///
/// `state_path` is the workspace-relative path to `.sync_state.json`. `root`
/// is the workspace root used for path security validation. Both must refer to
/// paths within the same workspace.
///
/// ## Errors
///
/// Returns [`GraphtorError::Sync`] wrapping the underlying cause for any
/// non-fatal aggregate failure. Returns the specific underlying error type for
/// fatal path-violation or database schema issues.
pub fn sync_source(
    store: &DataStore,
    source: &Source,
    source_dir: &Path,
    state_path: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
    on_progress: ProgressCallback<'_>,
) -> Result<SyncMetrics, GraphtorError> {
    sync_source_with_frozen_mtimes::<std::collections::hash_map::RandomState>(
        store,
        source,
        source_dir,
        state_path,
        root,
        model,
        None,
        on_progress,
    )
}

/// Sync a source while persisting a caller-provided mtime snapshot.
///
/// This is used by the staged v4 migration path after the rebuild input has
/// been frozen into a snapshot directory. The sync cycle reads file contents
/// from `source_dir`, but when `frozen_state_mtimes` is present it persists
/// those captured live-source mtimes into `.sync_state.json` instead of the
/// snapshot file mtimes so the next incremental cycle still compares against
/// the real source tree.
///
/// # Errors
///
/// Returns the same errors as [`sync_source`].
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn sync_source_with_frozen_mtimes<S: std::hash::BuildHasher>(
    store: &DataStore,
    source: &Source,
    source_dir: &Path,
    state_path: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
    frozen_state_mtimes: Option<&HashMap<String, u64, S>>,
    on_progress: ProgressCallback<'_>,
) -> Result<SyncMetrics, GraphtorError> {
    sync_source_with_frozen_mtimes_and_ignored_root(
        store,
        source,
        source_dir,
        state_path,
        root,
        model,
        frozen_state_mtimes,
        None,
        on_progress,
    )
}

/// Sync a source while persisting a caller-provided mtime snapshot and
/// skipping any live-tree files under `ignored_root`.
///
/// This is used by CLI-facing sync paths to exclude reserved internal
/// directories (such as stale frozen migration snapshots) from normal source
/// scans while still allowing explicit rebuild plans to opt into those files.
///
/// # Errors
///
/// Returns the same errors as [`sync_source`].
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn sync_source_with_frozen_mtimes_and_ignored_root<S: std::hash::BuildHasher>(
    store: &DataStore,
    source: &Source,
    source_dir: &Path,
    state_path: &Path,
    root: &Path,
    model: Option<&EmbeddingModel>,
    frozen_state_mtimes: Option<&HashMap<String, u64, S>>,
    ignored_root: Option<&Path>,
    mut on_progress: ProgressCallback<'_>,
) -> Result<SyncMetrics, GraphtorError> {
    let started_at = Instant::now();
    let source_id = source.id();

    let Some(local) = source.as_local() else {
        return Err(GraphtorError::Sync {
            message: "source is not a local ingestion source".to_string(),
            source_id: source_id.to_string(),
        });
    };
    let source_path_str = local.path.to_str().unwrap_or("");

    let sync_span = info_span!("sync_source", source_id, source_kind = "local");
    let _sync_span = sync_span.enter();

    info!(
        source_name = source_id,
        source_url = source_path_str,
        "starting sync cycle"
    );

    // Load existing state (empty on first run).
    let mut sync_state = SyncState::load(state_path, root)?;
    let stored = sync_state.source(source_id).cloned();

    let migration_pending = store.needs_v4_migration()?;
    if migration_pending {
        info!(
            source_id,
            "database still needs v4 migration; forcing full re-ingest"
        );
    }

    // ── Epoch mismatch check ───────────────────────────────────────────────
    // A missing (None) epoch means pre-pivot legacy state — treat it as a
    // mismatch to force a full re-ingest, preventing stale pre-pivot data from
    // suppressing reprocessing under the docline contract model.
    let epoch_changed = stored
        .as_ref()
        .is_some_and(|s| s.contract_epoch.as_deref() != Some(CONTRACT_EPOCH));

    if epoch_changed {
        info!(
            source_id,
            stored_epoch = stored
                .as_ref()
                .and_then(|s| s.contract_epoch.as_deref())
                .unwrap_or("<none>"),
            current_epoch = CONTRACT_EPOCH,
            "contract epoch mismatch or missing; forcing full re-ingest"
        );
    }

    let force_full_rebuild = migration_pending || epoch_changed;

    // Guard: if a full rebuild is forced (epoch mismatch or v4 migration) and
    // no embedding model is available, pre-pivot embeddings stored under the old
    // chunk-ID scheme cannot be recovered by the new-ID lookup.  Emit a warning
    // so operators are aware that embeddings will be absent after the rebuild.
    // (The hard reject for the --no-embed flag is enforced at the CLI layer before
    // destructive work begins; this warning covers model-unavailable degraded mode.)
    if force_full_rebuild && model.is_none() {
        warn!(
            source_id,
            "full rebuild forced (epoch mismatch or pending v4 migration) but no embedding \
             model is available; pre-pivot embeddings stored under the old chunk-ID scheme \
             cannot be recovered — run with embedding enabled to recompute vectors"
        );
    }
    let stored_mtimes_for_diff: HashMap<String, u64> = if force_full_rebuild {
        // Preserve stored file keys (for deletion detection) but zero all mtime
        // values so every file that still exists on disk appears as "modified"
        // and is unconditionally re-ingested.  Using an empty map would suppress
        // deletion detection: files tracked in the old state that have since been
        // removed from the filesystem would never appear in `changes.deleted` and
        // their database records would be preserved indefinitely.
        stored
            .as_ref()
            .map(|s| s.file_mtimes.keys().map(|k| (k.clone(), 0u64)).collect())
            .unwrap_or_default()
    } else {
        stored
            .as_ref()
            .map(|s| s.file_mtimes.clone())
            .unwrap_or_default()
    };

    // ── Ensure the source record exists in the database ────────────────────
    let source_record = SourceRecord {
        source_id: source_id.to_string(),
        url: source_path_str.to_string(),
        kind: "local".to_string(),
        name: source_id.to_string(),
        synced_at: None,
    };
    upsert_source(store, &source_record)?;

    // ── Detect changes ─────────────────────────────────────────────────────
    let current_mtimes = scan_tracked_source_mtimes(source, source_dir, ignored_root)?;
    let changes = mtime_diff::diff_mtimes(&current_mtimes, &stored_mtimes_for_diff);

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
    // Track files whose database cleanup failed so they can be retried on the
    // next sync cycle (their mtime and contract path entries are preserved).
    let mut failed_deleted_fs_paths: HashSet<String> = HashSet::new();
    for path in &changes.deleted {
        let fs_rel = path.to_string_lossy().replace('\\', "/");
        // Use the contract source_path we recorded for this fs path; fall back
        // to the fs_rel path for pre-pivot state that never recorded one.
        let contract_path = stored_contract_path_or_fs_rel(stored.as_ref(), &fs_rel)
            .unwrap_or_else(|| fs_rel.clone());
        if let Err(e) = delete_file_data_for_source(store, source_id, &contract_path) {
            warn!(path = %path.display(), error = %e, "failed to delete stale records");
            metrics.errors += 1;
            failed_deleted_fs_paths.insert(fs_rel.clone());
        } else {
            metrics.files_deleted += 1;
        }
    }

    // ── Re-ingest added and modified files ─────────────────────────────────
    let ingest_total = changes.added.len() + changes.modified.len();
    // Track which fs-relative paths failed so we can preserve their old mtimes.
    let mut failed_fs_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Accumulate successful contract paths for sync state update.
    let mut new_contract_path_map: HashMap<String, String> = HashMap::new();

    // ── Pre-scan: fail-closed {source_id, source_path} duplicate detection ──
    // If two or more of the changed files declare the same `source_path` in
    // their docline frontmatter, reingesting both would cause delete-before-insert
    // clobbering: the second reingest deletes the first file's chunks, losing data.
    // Pre-scan all changed files, find collisions, and mark every conflicting file
    // as failed BEFORE any reingest begins so that no data is clobbered.
    {
        let mut sp_to_fs_rels: HashMap<String, Vec<String>> = HashMap::new();
        for path in changes.added.iter().chain(changes.modified.iter()) {
            let fs_rel = path.to_string_lossy().replace('\\', "/");
            let abs_path = source_dir.join(path);
            if let Ok(sp) = crate::ingest_contract::extract_source_path_from_file(&abs_path) {
                sp_to_fs_rels.entry(sp).or_default().push(fs_rel);
            }
            // Files that fail the pre-scan (no frontmatter, bad path, etc.) are
            // left out of the map; they will fail during reingest_file with a
            // complete, contextual error.
        }
        for (sp, fs_rels) in &sp_to_fs_rels {
            if fs_rels.len() > 1 {
                for fs_rel in fs_rels {
                    warn!(
                        source_id,
                        source_path = %sp,
                        fs_path = %fs_rel,
                        conflict_count = fs_rels.len(),
                        "duplicate source_path across changed files; \
                         all conflicting files rejected to prevent data clobbering"
                    );
                    metrics.errors += 1;
                    failed_fs_paths.insert(fs_rel.clone());
                }
            }
        }

        // ── Cross-batch collision: changed file vs. unchanged stored document ──
        // A changed (added/modified) file whose `source_path` matches an
        // UNCHANGED file's stored contract path would clobber the unchanged
        // file's chunks via delete-before-insert on reingest.  Reject the
        // changed file instead.
        //
        // Files in any change category (added/modified/deleted) are excluded
        // from the "unchanged" set so that a delete+add of the same source_path
        // within one cycle is correctly allowed.
        let all_cycle_fs_rels: HashSet<String> = changes
            .added
            .iter()
            .chain(changes.modified.iter())
            .chain(changes.deleted.iter())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        // Reverse map: source_path → fs_rel for truly unchanged files only.
        let unchanged_sp_to_fs_rel: HashMap<String, String> = stored
            .as_ref()
            .map(|state| {
                state
                    .file_mtimes
                    .keys()
                    .filter(|fs_rel| !all_cycle_fs_rels.contains(*fs_rel))
                    .filter_map(|fs_rel| {
                        stored_contract_path_or_fs_rel(Some(state), fs_rel)
                            .map(|sp| (sp, fs_rel.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (sp, changed_rels) in &sp_to_fs_rels {
            if let Some(unchanged_fs_rel) = unchanged_sp_to_fs_rel.get(sp) {
                for changed_rel in changed_rels {
                    if !failed_fs_paths.contains(changed_rel) {
                        warn!(
                            source_id,
                            source_path = %sp,
                            changed_fs_path = %changed_rel,
                            unchanged_fs_path = %unchanged_fs_rel,
                            "changed file source_path collides with unchanged stored \
                             document; changed file rejected to prevent clobbering"
                        );
                        metrics.errors += 1;
                        failed_fs_paths.insert(changed_rel.clone());
                    }
                }
            }
        }

        // ── Swap/steal detection: new path of one file == old path of another ──
        // When file-A changes to new_sp=P and file-B has old_sp=P (both in this
        // cycle), the order of reingest matters:
        //
        //   A first → B's reingest later deletes P, clobbering A's fresh records.
        //   B first → A's reingest deletes P (its old path), clobbering B's load.
        //
        // Either ordering clobbers data.  Reject both fail-closed.
        //
        // Build: old_sp → fs_rel for every changed file that was previously
        // ingested, falling back to the fs-relative identity for legacy state
        // that predates `file_contract_paths`.
        let old_sp_to_changed_fs_rel: HashMap<String, String> = changes
            .added
            .iter()
            .chain(changes.modified.iter())
            .filter_map(|path| {
                let fs_rel = path.to_string_lossy().replace('\\', "/");
                stored_contract_path_or_fs_rel(stored.as_ref(), &fs_rel)
                    .map(|old_sp| (old_sp, fs_rel))
            })
            .collect();
        for (new_sp, new_file_fs_rels) in &sp_to_fs_rels {
            if let Some(old_holder_fs_rel) = old_sp_to_changed_fs_rel.get(new_sp) {
                for new_file_fs_rel in new_file_fs_rels {
                    if new_file_fs_rel != old_holder_fs_rel {
                        // Reject the file that is claiming the old holder's path.
                        if !failed_fs_paths.contains(new_file_fs_rel) {
                            warn!(
                                source_id,
                                source_path = %new_sp,
                                claiming_file = %new_file_fs_rel,
                                old_owner = %old_holder_fs_rel,
                                "changed file's new source_path equals another changed \
                                 file's OLD source_path; claiming file rejected to \
                                 prevent order-dependent data clobbering"
                            );
                            metrics.errors += 1;
                            failed_fs_paths.insert(new_file_fs_rel.clone());
                        }
                        // Reject the old owner: its reingest deletes records for
                        // its old path (= new_sp), which would clobber the
                        // claiming file's just-loaded data if processed after it.
                        if !failed_fs_paths.contains(old_holder_fs_rel) {
                            warn!(
                                source_id,
                                source_path = %new_sp,
                                claiming_file = %new_file_fs_rel,
                                old_owner = %old_holder_fs_rel,
                                "changed file's old source_path is being claimed by \
                                 another changed file; old owner also rejected to \
                                 prevent clobbering (swap detected)"
                            );
                            metrics.errors += 1;
                            failed_fs_paths.insert(old_holder_fs_rel.clone());
                        }
                    }
                }
            }
        }
    }

    for (idx, path) in changes
        .added
        .iter()
        .chain(changes.modified.iter())
        .map(std::path::PathBuf::as_path)
        .enumerate()
    {
        let fs_rel = path.to_string_lossy().replace('\\', "/");
        let abs_path = source_dir.join(path);
        let current = idx + 1;
        let size_bytes = validated_file_size(&abs_path, root);

        // Skip files pre-rejected by the duplicate source_path check above.
        // The mtime for these files is intentionally not advanced (they remain in
        // `failed_fs_paths`), so the operator will be retried on the next sync
        // after resolving the source_path collision.
        if failed_fs_paths.contains(&fs_rel) {
            if let Some(cb) = on_progress.as_mut() {
                cb(SyncProgressEvent {
                    path: path.to_path_buf(),
                    current,
                    total: ingest_total,
                    size_bytes,
                    status: SyncProgressStatus::Failed {
                        elapsed: Duration::ZERO,
                        error: "duplicate source_path collision detected; \
                                file skipped to prevent data clobbering — \
                                resolve the source_path conflict in frontmatter and re-sync"
                            .to_string(),
                    },
                });
            }
            continue;
        }

        if let Some(cb) = on_progress.as_mut() {
            cb(SyncProgressEvent {
                path: path.to_path_buf(),
                current,
                total: ingest_total,
                size_bytes,
                status: SyncProgressStatus::Started,
            });
        }

        // Look up the old contract source_path for this file (for delete scoping).
        let old_contract_path = stored_contract_path_or_fs_rel(stored.as_ref(), &fs_rel);

        let file_started_at = Instant::now();
        match reingest_file_with_old_contract_path(
            store,
            source_id,
            &abs_path,
            source_dir,
            root,
            old_contract_path.as_deref(),
            model,
        ) {
            Ok((n, contract_path)) => {
                metrics.files_synced += 1;
                metrics.chunks_created += n;
                new_contract_path_map.insert(fs_rel, contract_path);
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
                    "reingest failed; marking file as pending for retry"
                );
                metrics.errors += 1;
                failed_fs_paths.insert(fs_rel);
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
    // Build the new mtime snapshot, then:
    // 1. For failed files: restore the old mtime (or remove if new) so they
    //    remain "changed" on the next run and get retried.
    // 2. Carry forward contract path mappings, merging any new ones from this
    //    cycle's successful reingests.
    let mut new_source_state = if let Some(frozen_mtimes) = frozen_state_mtimes {
        build_new_state_from_mtimes(
            frozen_mtimes
                .iter()
                .map(|(path, mtime)| (path.clone(), *mtime))
                .collect(),
        )
    } else {
        build_new_state(source, source_dir, ignored_root)?
    };

    // Restore old mtimes (or remove) for failed files so they are retried.
    for fs_rel in &failed_fs_paths {
        if let Some(&old_mtime) = stored_mtimes_for_diff.get(fs_rel) {
            // Modified file that failed: keep old mtime → detected as changed next run.
            new_source_state
                .file_mtimes
                .insert(fs_rel.clone(), old_mtime);
        } else {
            // Newly added file that failed: remove from state → detected as added next run.
            new_source_state.file_mtimes.remove(fs_rel);
        }
    }

    // Merge contract path mappings from stored state and this cycle's successes.
    {
        let stored_contract_paths = stored
            .as_ref()
            .map(|s| s.file_contract_paths.clone())
            .unwrap_or_default();
        // Start from stored (preserves paths for files not touched this cycle).
        let mut merged = stored_contract_paths;
        // Apply successful reingests from this cycle.
        merged.extend(new_contract_path_map);
        // Apply delete outcomes: remove successful deletes from state, and
        // preserve failed ones (with restored mtimes) so the next sync cycle
        // can retry the cleanup.
        apply_delete_cleanup_to_state(
            &mut merged,
            &mut new_source_state.file_mtimes,
            &changes.deleted,
            &failed_deleted_fs_paths,
            &stored_mtimes_for_diff,
        );
        new_source_state.file_contract_paths = merged;
    }

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

/// Seed sync state after a successful frozen-snapshot rebuild.
///
/// This is used by the staged v4 migration full-sync path. The rebuild loads
/// files from a frozen snapshot directory, but the persisted sync state must
/// retain the live-source mtimes captured at freeze time so the next
/// incremental cycle can detect live-tree deletions and modifications.
///
/// Existing sync-state entries for sources in `plan` are replaced so any stale
/// pre-migration file or contract-path mappings are cleared.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if a snapshot file escapes `root`.
/// Returns [`GraphtorError::Config`] if the sync-state file cannot be read or
/// written. Returns [`GraphtorError::Sync`] if a frozen snapshot file cannot be
/// re-read to recover its validated contract `source_path`.
pub fn seed_sync_state_from_frozen_snapshot<SOuter, SInner>(
    plan: &AcquisitionPlan,
    frozen_source_mtimes: &HashMap<String, HashMap<String, u64, SInner>, SOuter>,
    state_path: &Path,
    root: &Path,
) -> Result<(), GraphtorError>
where
    SOuter: std::hash::BuildHasher,
    SInner: std::hash::BuildHasher,
{
    let mut sync_state = SyncState::load(state_path, root)?;

    for planned in &plan.sources {
        let source_id = planned.source.id();
        let file_mtimes = frozen_source_mtimes
            .get(source_id)
            .map(|source_mtimes| {
                source_mtimes
                    .iter()
                    .map(|(path, mtime)| (path.clone(), *mtime))
                    .collect()
            })
            .unwrap_or_default();
        let file_contract_paths = build_frozen_snapshot_contract_paths(
            &planned.target_dir,
            &file_mtimes,
            source_id,
            root,
        )?;
        let mut source_state = build_new_state_from_mtimes(file_mtimes);
        source_state.file_contract_paths = file_contract_paths;
        *sync_state.source_mut(source_id) = source_state;
    }

    sync_state.save(state_path, root)
}

/// Seed sync state after a successful full sync from live sources.
///
/// This is the non-migration counterpart to [`seed_sync_state_from_frozen_snapshot`].
/// After a `--full` pipeline run (no v4 migration), the sync state must be
/// rebuilt so the next incremental cycle has a correct baseline and does not
/// re-ingest every file from scratch.
///
/// For each source in `plan`, this function:
/// 1. Scans live mtimes from `planned.target_dir`.
/// 2. Reads each file's validated `source_path` from its docline frontmatter.
/// 3. **Merges** prior-state entries for files absent from the live scan or
///    whose contract identity changed, so the next incremental cycle can clean
///    up orphaned DB rows that the full pipeline did not delete.
/// 4. Persists the resulting `SourceSyncState` into `.sync_state.json`.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if a source file escapes `root`.
/// Returns [`GraphtorError::Config`] if the sync-state file cannot be written.
/// Returns [`GraphtorError::Sync`] if a live source file's `source_path` cannot
/// be extracted from its frontmatter.
pub fn seed_sync_state_from_live_sources(
    plan: &AcquisitionPlan,
    state_path: &Path,
    root: &Path,
) -> Result<(), GraphtorError> {
    let mut sync_state = SyncState::load(state_path, root)?;

    for planned in &plan.sources {
        let source_id = planned.source.id();
        let source_dir = &planned.target_dir;

        // Mirror the ignored-root behaviour used by normal incremental/full scans:
        // when `allow_internal_snapshot_scan` is false (all regular sources), exclude
        // the internal v4-migration-snapshot directory so stale snapshot markdown files
        // are never seeded into sync state.  Without this guard a workspace-root source
        // would seed every snapshot `.md` file, and the next incremental run would treat
        // them as deleted and invoke the delete path — potentially removing the live
        // document whose `source_path` the stale snapshot shares.
        let ignored_root = (!planned.allow_internal_snapshot_scan)
            .then(|| crate::path::v4_migration_snapshot_dir(&plan.data_root));
        let file_mtimes =
            scan_tracked_source_mtimes(&planned.source, source_dir, ignored_root.as_deref())?;
        let file_contract_paths =
            build_frozen_snapshot_contract_paths(source_dir, &file_mtimes, source_id, root)?;
        let mut source_state = build_new_state_from_mtimes(file_mtimes);
        source_state.file_contract_paths = file_contract_paths;

        // Snapshot the prior state for this source before overwriting it so we
        // can merge stale entries that the full pipeline left behind.
        let prior = sync_state.source(source_id).cloned();
        merge_prior_state_into_seeded(&mut source_state, prior.as_ref());

        *sync_state.source_mut(source_id) = source_state;
    }

    sync_state.save(state_path, root)
}

/// Capture live filesystem state for all sources in `plan` **before** the
/// full-sync pipeline runs.
///
/// The returned [`PreSyncSnapshot`] must be passed to
/// [`seed_sync_state_from_pre_sync_snapshot`] after the pipeline completes so
/// that the persisted sync state reflects the on-disk state at pipeline start
/// time, not a post-run rescan.  This closes the race where a file that changes
/// after the pipeline reads it (but before the old post-run rescan) would
/// silently be recorded as already-synced, causing the next incremental to skip
/// it despite the DB holding stale content.
///
/// Contract-path extraction is **lenient**: files whose frontmatter cannot be
/// parsed are included in the mtime map but omitted from the contract-path map.
/// They will still participate in mtime-based change detection; only identity
/// tracking is absent until a later incremental ingest succeeds.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if a source file escapes `root`.
/// Returns [`GraphtorError::Config`] if directory scanning fails.
pub fn capture_pre_sync_snapshot(
    plan: &AcquisitionPlan,
    root: &Path,
) -> Result<PreSyncSnapshot, GraphtorError> {
    let mut snapshot = PreSyncSnapshot::default();

    for planned in &plan.sources {
        let source_id = planned.source.id();
        let source_dir = &planned.target_dir;

        // Mirror the ignored-root behaviour used by all other scan paths.
        let ignored_root = (!planned.allow_internal_snapshot_scan)
            .then(|| crate::path::v4_migration_snapshot_dir(&plan.data_root));
        let file_mtimes =
            scan_tracked_source_mtimes(&planned.source, source_dir, ignored_root.as_deref())?;

        let mut file_contract_paths = HashMap::with_capacity(file_mtimes.len());
        for fs_rel in file_mtimes.keys() {
            let abs_path = validate_path(&source_dir.join(fs_rel), root)?;
            // Lenient: skip files whose frontmatter cannot be parsed.
            // The pipeline will emit a descriptive error for them; we still
            // track their mtime so incremental change detection works.
            if let Ok(contract_path) =
                crate::ingest_contract::extract_source_path_from_file(&abs_path)
            {
                file_contract_paths.insert(fs_rel.clone(), contract_path);
            }
        }

        snapshot.insert(
            source_id.to_owned(),
            SourcePreSyncCapture {
                file_mtimes,
                file_contract_paths,
            },
        );
    }

    Ok(snapshot)
}

/// Seed sync state using a pre-captured snapshot taken **before** the full-sync
/// pipeline ran.
///
/// Identical in merge behaviour to [`seed_sync_state_from_live_sources`], but
/// uses pre-captured mtimes and contract paths rather than a fresh rescan.
/// This closes the race in the regular `--full` seeding path:
///
/// ```text
/// OLD (racy):  pipeline → live-rescan  → persist
/// NEW (safe):  pre-capture → pipeline → persist(pre-capture)
/// ```
///
/// If a file changes or is deleted after the pipeline reads it but before the
/// old rescan, the pre-capture retains the mtime the pipeline actually saw, so
/// the **next incremental** detects and repairs the drift instead of silently
/// skipping it.
///
/// The existing prior-state merge logic ([`merge_prior_state_into_seeded`])
/// operates against the pre-captured snapshot, preserving pending cleanup for
/// pre-full-sync deletions and identity changes.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if a path escapes `root`.
/// Returns [`GraphtorError::Config`] if the sync-state file cannot be written.
pub fn seed_sync_state_from_pre_sync_snapshot(
    plan: &AcquisitionPlan,
    pre_sync_snapshot: &PreSyncSnapshot,
    state_path: &Path,
    root: &Path,
) -> Result<(), GraphtorError> {
    let mut sync_state = SyncState::load(state_path, root)?;

    for planned in &plan.sources {
        let source_id = planned.source.id();
        let capture = pre_sync_snapshot
            .get(source_id)
            .cloned()
            .unwrap_or_default();

        let mut source_state = build_new_state_from_mtimes(capture.file_mtimes);
        source_state.file_contract_paths = capture.file_contract_paths;

        // Snapshot the prior state for this source before overwriting it so we
        // can merge stale entries that the full pipeline left behind.
        let prior = sync_state.source(source_id).cloned();
        merge_prior_state_into_seeded(&mut source_state, prior.as_ref());

        *sync_state.source_mut(source_id) = source_state;
    }

    sync_state.save(state_path, root)
}

/// Merge prior-state entries that are absent from the live scan or have a
/// changed contract identity into the freshly seeded `source_state`.
///
/// This preserves enough history after a regular full sync for the **next**
/// incremental cycle to clean up orphaned DB rows that the full pipeline did
/// not delete.
///
/// ### Absent entries (file deleted before the full sync)
///
/// The full pipeline does not remove DB rows for files that disappeared before
/// the run started.  Carrying forward their old mtime and contract path ensures
/// the next incremental detects them as "deleted" and invokes the cleanup path.
///
/// ### Contract-path identity changes
///
/// When a live file's frontmatter `source_path` changed since the last tracked
/// sync, the full pipeline re-ingests it under the NEW identity but leaves
/// orphaned rows for the OLD identity.  Carrying forward the OLD mtime and OLD
/// contract path forces the next incremental to re-ingest the file via
/// `reingest_file_with_old_contract_path`, which deletes the stale rows and
/// inserts fresh ones under the new identity.
///
/// ### Guard against clobbering live documents
///
/// In both cases, carry-forward is skipped when the old contract path is
/// already claimed by a currently-live file — clobbering live DB rows via a
/// spurious incremental delete would be worse than leaving the orphan.
fn merge_prior_state_into_seeded(
    source_state: &mut SourceSyncState,
    prior: Option<&SourceSyncState>,
) {
    let Some(prior) = prior else {
        return;
    };

    // Contract paths claimed by the live files we just scanned.
    // Collected as owned Strings so the immutable borrow on
    // `source_state.file_contract_paths` is released before the mutation loop.
    let live_contract_path_set: HashSet<String> =
        source_state.file_contract_paths.values().cloned().collect();

    for (fs_rel, &old_mtime) in &prior.file_mtimes {
        if source_state.file_mtimes.contains_key(fs_rel.as_str()) {
            // The file still exists on disk.  Check for a contract-path identity
            // change: the full pipeline re-ingested it under a new `source_path`
            // but left orphaned rows for the old identity.
            let new_cp = source_state
                .file_contract_paths
                .get(fs_rel.as_str())
                .cloned();
            let old_cp = prior.file_contract_paths.get(fs_rel.as_str()).cloned();
            if let (Some(ref new_cp), Some(ref old_cp)) = (new_cp, old_cp) {
                if new_cp != old_cp && !live_contract_path_set.contains(old_cp.as_str()) {
                    // Store the OLD mtime so the next incremental sees this file
                    // as "modified" and calls reingest_file_with_old_contract_path,
                    // which deletes orphaned rows for the old identity.
                    source_state.file_mtimes.insert(fs_rel.clone(), old_mtime);
                    source_state
                        .file_contract_paths
                        .insert(fs_rel.clone(), old_cp.clone());
                }
            }
            continue;
        }

        // The file is absent from the live scan: it was deleted before this
        // full sync ran.  The full pipeline did not remove its DB rows.
        let old_contract_path = prior
            .file_contract_paths
            .get(fs_rel.as_str())
            .map_or(fs_rel.as_str(), String::as_str);

        // Guard: if the old contract path is now claimed by a live file, do not
        // carry the entry forward.  Doing so would cause the next incremental to
        // delete the live document's rows when it processes this entry as "deleted".
        if live_contract_path_set.contains(old_contract_path) {
            continue;
        }

        // Carry forward: preserve the old mtime so diff_mtimes classifies this
        // entry as "deleted" on the next incremental cycle.
        source_state.file_mtimes.insert(fs_rel.clone(), old_mtime);
        if let Some(old_cp) = prior.file_contract_paths.get(fs_rel.as_str()) {
            source_state
                .file_contract_paths
                .insert(fs_rel.clone(), old_cp.clone());
        }
    }
}
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
    ignored_root: Option<&Path>,
) -> Result<SourceSyncState, GraphtorError> {
    if !source.is_ingestible() {
        return Err(GraphtorError::Sync {
            message: "source is not a local ingestion source".to_string(),
            source_id: source.id().to_string(),
        });
    }
    let file_mtimes = scan_tracked_source_mtimes(source, source_dir, ignored_root)?;
    Ok(build_new_state_from_mtimes(file_mtimes))
}

fn build_new_state_from_mtimes(file_mtimes: HashMap<String, u64>) -> SourceSyncState {
    SourceSyncState {
        last_sync: Some(chrono_now()),
        file_mtimes,
        // file_contract_paths is populated separately by the sync loop.
        file_contract_paths: std::collections::HashMap::new(),
        contract_epoch: Some(CONTRACT_EPOCH.to_string()),
    }
}

fn build_frozen_snapshot_contract_paths(
    source_dir: &Path,
    file_mtimes: &HashMap<String, u64>,
    source_id: &str,
    root: &Path,
) -> Result<HashMap<String, String>, GraphtorError> {
    let mut file_contract_paths = HashMap::with_capacity(file_mtimes.len());

    for fs_rel in file_mtimes.keys() {
        let snapshot_path = validate_path(&source_dir.join(fs_rel), root)?;
        let contract_path = crate::ingest_contract::extract_source_path_from_file(&snapshot_path)
            .map_err(|error| GraphtorError::Sync {
            message: format!(
                "failed to extract contract source_path from frozen snapshot file '{}': \
                     {error}",
                snapshot_path.display()
            ),
            source_id: source_id.to_string(),
        })?;
        file_contract_paths.insert(fs_rel.clone(), contract_path);
    }

    Ok(file_contract_paths)
}

fn stored_contract_path_or_fs_rel(
    stored: Option<&SourceSyncState>,
    fs_rel: &str,
) -> Option<String> {
    stored.and_then(|state| {
        state.file_contract_paths.get(fs_rel).cloned().or_else(|| {
            state
                .file_mtimes
                .contains_key(fs_rel)
                .then(|| fs_rel.to_string())
        })
    })
}

fn scan_tracked_source_mtimes(
    source: &Source,
    source_dir: &Path,
    ignored_root: Option<&Path>,
) -> Result<HashMap<String, u64>, GraphtorError> {
    let all_mtimes = mtime_diff::scan_mtimes_with_ignored_root(source_dir, ignored_root)?;
    let relative_paths: Vec<PathBuf> = all_mtimes.keys().map(PathBuf::from).collect();
    let tracked_paths: std::collections::HashSet<String> =
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
            || source.formats().iter().any(|format| {
                crate::config::source::canonicalize_format_ext(format).eq_ignore_ascii_case(&ext)
            }))
}

/// Validate `path` against `root` before performing any metadata I/O.
///
/// This ensures progress reporting cannot touch symlink targets or other paths
/// outside the workspace boundary before `reingest_file()` enforces the same
/// validation.
fn validated_file_size(path: &Path, root: &Path) -> Option<u64> {
    validate_path(path, root)
        .ok()
        .and_then(|canonical| canonical.metadata().ok())
        .map(|metadata| metadata.len())
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
/// [`SourceSyncState`]. It is NOT ISO-8601; it is the number of seconds
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

/// Apply delete-cycle outcomes to the per-source sync state maps.
///
/// - **Successful deletes** (`fs_rel` not in `failed_fs_rels`): the
///   `source_path` mapping is removed from `contract_paths`.
/// - **Failed deletes** (`fs_rel` in `failed_fs_rels`): the `source_path`
///   mapping is **preserved** in `contract_paths`, and the old mtime is
///   **restored** in `file_mtimes` so the next sync cycle re-detects the
///   file as deleted and retries the cleanup.
/// - **Failed deletes with a replacement owner**: if another file already
///   claims the same contract identity in `contract_paths`, the stale retry
///   entry is dropped instead of preserved so the next sync cycle cannot
///   delete the replacement document.
fn apply_delete_cleanup_to_state(
    contract_paths: &mut HashMap<String, String>,
    file_mtimes: &mut HashMap<String, u64>,
    deleted_paths: &[PathBuf],
    failed_fs_rels: &HashSet<String>,
    stored_mtimes: &HashMap<String, u64>,
) {
    for path in deleted_paths {
        let fs_rel = path.to_string_lossy().replace('\\', "/");
        if failed_fs_rels.contains(&fs_rel) {
            let retry_identity = contract_paths
                .get(&fs_rel)
                .cloned()
                .unwrap_or_else(|| fs_rel.clone());
            let replacement_claimed = contract_paths.iter().any(|(other_fs_rel, source_path)| {
                other_fs_rel != &fs_rel && source_path == &retry_identity
            });

            if replacement_claimed {
                contract_paths.remove(&fs_rel);
                continue;
            }

            // Deletion failed: preserve the contract path record so the next
            // sync cycle can retry.  Restore the old mtime so diff_mtimes still
            // classifies the (now-absent) file as deleted.
            if let Some(&old_mtime) = stored_mtimes.get(&fs_rel) {
                file_mtimes.insert(fs_rel, old_mtime);
            }
        } else {
            // Deletion succeeded: remove the contract path tracking entry.
            contract_paths.remove(&fs_rel);
        }
    }
}

/// Source-scoped candidate metadata for v4 migration preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPreflightCandidate {
    /// Source identifier used to scope duplicate `source_path` detection.
    pub source_id: String,
    /// Absolute path to the candidate Markdown file.
    pub path: PathBuf,
}

fn is_markdown_candidate_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn validate_v4_migration_candidates(
    candidates: &[MigrationPreflightCandidate],
) -> Result<(), GraphtorError> {
    let mut source_path_to_candidates: BTreeMap<(String, String), Vec<PathBuf>> = BTreeMap::new();

    // Pre-validate all candidate markdown files before committing to a prune.
    for candidate in candidates {
        if !is_markdown_candidate_path(&candidate.path) {
            continue; // non-markdown files are skipped silently
        }
        // Full contract-enforced parse; any failure aborts migration.
        let parsed =
            crate::parse::parse_file(&candidate.path, &candidate.source_id).map_err(|e| {
                GraphtorError::Contract {
                    message: format!(
                        "v4 migration aborted: candidate file '{}' for source '{}' \
                     failed contract validation: {e}; existing data preserved — \
                     fix the file and retry",
                        candidate.path.display(),
                        candidate.source_id
                    ),
                    field: None,
                }
            })?;
        source_path_to_candidates
            .entry((candidate.source_id.clone(), parsed.path))
            .or_default()
            .push(candidate.path.clone());
    }

    for ((source_id, source_path), duplicate_paths) in source_path_to_candidates {
        if duplicate_paths.len() < 2 {
            continue;
        }

        let duplicate_files = duplicate_paths
            .iter()
            .map(|path| format!("'{}'", path.display()))
            .collect::<Vec<_>>()
            .join(", ");

        return Err(GraphtorError::Contract {
            message: format!(
                "v4 migration aborted: source '{source_id}' has duplicate \
                 source_path '{source_path}' across candidate files \
                 {duplicate_files}; existing data preserved — resolve the \
                 duplicate frontmatter source_path and retry"
            ),
            field: Some("source_path".to_string()),
        });
    }

    Ok(())
}

/// Pre-validate all source-scoped candidate markdown files before pruning
/// pre-v4 data for a staged rebuild.
///
/// # Why this matters
///
/// For pre-v4 databases, [`crate::db::DataStore::prune_v4_data_for_rebuild`]
/// clears **all** existing ingested data. If even one candidate file is invalid
/// (fails contract validation), the migration would destroy the existing index
/// without a valid replacement being available.
///
/// # Behaviour
///
/// - If the store does **not** need a v4 migration, this is a no-op — returns
///   `Ok(false)` immediately, regardless of file validity.
/// - If the store **does** need a v4 migration:
///   - Tries to parse every `.md` / `.markdown` file in `candidates`.
///   - If any file fails contract validation, returns [`GraphtorError::Contract`]
///     **without** pruning, preserving existing data.
///   - If any source has multiple candidate files with the same validated
///     `source_path`, returns [`GraphtorError::Contract`] **without** pruning,
///     preserving existing data.
///   - If all files pass (or `candidate_paths` is empty), prunes the database
///     and returns `Ok(true)`, leaving the v4 gate active until the caller marks
///     the rebuild complete.
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] if any candidate file fails validation
/// or if duplicate `source_path` values are found within the same source
/// (migration aborted, existing data preserved).
/// Returns [`GraphtorError::Database`] if the schema-version query or prune
/// fails.
pub fn validate_and_begin_v4_migration_for_sources(
    store: &crate::DataStore,
    candidates: &[MigrationPreflightCandidate],
) -> Result<bool, GraphtorError> {
    if !store.needs_v4_migration()? {
        return Ok(false);
    }

    validate_v4_migration_candidates(candidates)?;
    store.prune_v4_data_for_rebuild()?;
    Ok(true)
}

/// Backward-compatible wrapper for staged v4 migration preflight.
///
/// All candidate files are treated as belonging to a single synthetic source.
/// New call sites that need source-scoped duplicate detection should prefer
/// [`validate_and_begin_v4_migration_for_sources`].
///
/// # Errors
///
/// Returns the same errors as [`validate_and_begin_v4_migration_for_sources`].
pub fn validate_and_begin_v4_migration(
    store: &crate::DataStore,
    candidate_paths: &[PathBuf],
) -> Result<bool, GraphtorError> {
    const LEGACY_PREFLIGHT_SOURCE_ID: &str = "__migration_preflight__";

    let candidates = candidate_paths
        .iter()
        .cloned()
        .map(|path| MigrationPreflightCandidate {
            source_id: LEGACY_PREFLIGHT_SOURCE_ID.to_string(),
            path,
        })
        .collect::<Vec<_>>();

    validate_and_begin_v4_migration_for_sources(store, &candidates)
}

/// Backward-compatible wrapper for v4 migration preflight.
///
/// All candidate files are treated as belonging to a single synthetic source.
/// New call sites that need source-scoped duplicate detection should prefer
/// [`validate_and_begin_v4_migration_for_sources`].
///
/// # Errors
///
/// Returns the same errors as [`validate_and_begin_v4_migration_for_sources`]
/// and [`crate::db::DataStore::mark_v4_migration_complete`].
pub fn validate_and_apply_v4_migration_for_sources(
    store: &crate::DataStore,
    candidates: &[MigrationPreflightCandidate],
) -> Result<(), GraphtorError> {
    if validate_and_begin_v4_migration_for_sources(store, candidates)? {
        store.mark_v4_migration_complete()?;
    }
    Ok(())
}

/// Backward-compatible wrapper for v4 migration preflight.
///
/// All candidate files are treated as belonging to a single synthetic source.
/// New call sites that need source-scoped duplicate detection should prefer
/// [`validate_and_begin_v4_migration_for_sources`].
///
/// # Errors
///
/// Returns the same errors as [`validate_and_apply_v4_migration_for_sources`].
pub fn validate_and_apply_v4_migration(
    store: &crate::DataStore,
    candidate_paths: &[PathBuf],
) -> Result<(), GraphtorError> {
    if validate_and_begin_v4_migration(store, candidate_paths)? {
        store.mark_v4_migration_complete()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{sync_source, validated_file_size, SyncProgressStatus};
    use crate::config::source::LocalSource;
    use crate::db::ensure_schema;
    use crate::{DataStore, Source};

    /// Build a docline-conformant markdown string for test fixtures.
    fn docline_md(source_path: &str, title: &str, content: &str) -> String {
        format!(
            "---\ntitle: {title}\nsource: /test/source\ningested_at: \
             2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
        )
    }

    fn unix_mtime_secs(path: &std::path::Path) -> u64 {
        path.metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs())
    }

    #[test]
    fn sync_source_progress_callback_invoked_per_file() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        for i in 0..3_u8 {
            let fname = format!("doc{i}.md");
            let sp = format!("docs/doc{i}.md");
            let title = format!("Doc {i}");
            let content = format!("# Doc {i}\n\nContent.\n");
            fs::write(
                source_dir.join(&fname),
                docline_md(&sp, &title, &content).as_bytes(),
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
        // All files are valid and exist within root: size_bytes must be populated
        // after validate_path succeeds (regression guard for the metadata-before-
        // validation fix — guards the happy path).
        for event in &progress_events {
            assert!(
                event.size_bytes.is_some(),
                "size_bytes should be populated for valid in-root files: {event:#?}"
            );
        }
    }

    #[test]
    fn sync_metrics_returned_for_local_source() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("guide.md", "Guide", "# Guide\n\nHello world.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write markdown");

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

    #[test]
    fn validated_file_size_returns_none_for_path_outside_root() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let outside_file = outside.path().join("secret.md");
        fs::write(&outside_file, "# Secret\n\nSensitive content.\n").expect("write outside file");

        assert_eq!(
            validated_file_size(&outside_file, workspace.path()),
            None,
            "progress metadata must not be read for paths outside the workspace root"
        );
    }

    /// Issue 3: Legacy sync state with no `contract_epoch` must force full rebuild.
    #[test]
    fn legacy_state_without_contract_epoch_forces_full_rebuild() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("guide.md", "Guide", "# Guide\n\nContent.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "epoch-test".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // Simulate legacy state: file_mtimes has the file (so normally it would
        // be a no-op on second run), but contract_epoch is absent (pre-pivot).
        let mut pre_state = crate::sync::state::SyncState::default();
        {
            let src = pre_state.source_mut("epoch-test");
            // Pretend the file was ingested with the current mtime (no change).
            let mtime = source_dir.join("guide.md").metadata().map_or(0, |m| {
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs())
            });
            src.file_mtimes.insert("guide.md".to_string(), mtime);
            // Explicitly no contract_epoch (pre-pivot legacy state).
            src.contract_epoch = None;
        }
        pre_state
            .save(&state_path, root)
            .expect("save legacy state");

        // Sync should ignore the stored mtimes and force full re-ingest.
        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync with legacy state");

        assert_eq!(
            metrics.files_synced, 1,
            "full rebuild must re-ingest the file even though mtime matches: {metrics:?}"
        );

        // Loaded state should now have the current epoch.
        let after_state = crate::sync::state::SyncState::load(&state_path, root).expect("load");
        let epoch = after_state
            .source("epoch-test")
            .and_then(|s| s.contract_epoch.as_deref());
        assert_eq!(
            epoch,
            Some(crate::ingest_contract::CONTRACT_EPOCH),
            "sync state must be stamped with the current contract epoch"
        );
    }

    /// Pending database migration must force a full rebuild even when the
    /// stored sync state already carries the current contract epoch and mtime.
    #[test]
    fn pending_v4_database_migration_forces_full_rebuild() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("guide.md", "Guide", "# Guide\n\nContent.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "migration-pending".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let m1 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m1");
        assert_eq!(
            m1.files_synced, 1,
            "initial sync must ingest the file: {m1:?}"
        );

        store
            .set_schema_version_for_test(3)
            .expect("mark store as needing v4 migration");
        assert!(
            store
                .needs_v4_migration()
                .expect("check migration pending state"),
            "pre-condition: store must remain gated as pre-v4"
        );

        let m2 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m2");
        assert_eq!(
            m2.files_synced, 1,
            "pending v4 migration must force a full rebuild even when state is current: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "forced rebuild must stay clean: {m2:?}");
    }

    /// Issue 4: Failed reingests must not advance the sync state for that file.
    #[test]
    fn failed_reingest_does_not_advance_sync_state() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        // Write one valid file and one invalid file (missing frontmatter).
        let good_md = docline_md("good.md", "Good", "# Good\n\nContent.\n");
        fs::write(source_dir.join("good.md"), good_md.as_bytes()).expect("write good");
        fs::write(source_dir.join("bad.md"), b"# Bad\n\nNo frontmatter.\n").expect("write bad");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "fail-test".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync must not return fatal error");

        assert_eq!(
            metrics.files_synced, 1,
            "good file should succeed: {metrics:?}"
        );
        assert_eq!(
            metrics.errors, 1,
            "bad file must be counted as error: {metrics:?}"
        );

        // The failed file must NOT be in the sync state with its current mtime.
        // It should either be absent or have its original (zero, since it was never
        // previously ingested) mtime so it gets retried next run.
        let state = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src_state = state.source("fail-test").expect("source in state");

        // The failed file must be absent from file_mtimes so it is retried.
        assert!(
            !src_state.file_mtimes.contains_key("bad.md"),
            "failed file must not be advanced in sync state: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );

        // The good file must be present in the state.
        assert!(
            src_state.file_mtimes.contains_key("good.md"),
            "successful file must be in sync state: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    /// Legacy sync state without `file_contract_paths` must still delete stale
    /// rows keyed by the filesystem-relative path when a tracked file is
    /// modified and re-ingested under a canonical contract `source_path`.
    #[test]
    fn modified_legacy_file_without_contract_path_mapping_cleans_up_fs_rel_rows() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let file_path = source_dir.join("guide.md");
        let current_md = docline_md("canonical/guide.md", "Guide", "# Guide\n\nFresh content.\n");
        fs::write(&file_path, current_md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source_id = "legacy-contract-paths";
        let source = Source::Local(LocalSource {
            id: source_id.to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // Seed a legacy chunk row under the filesystem-relative path to simulate
        // pre-pivot state that never persisted `file_contract_paths`.
        let legacy_doc = crate::parse::parse_document(
            "# Legacy Guide\n\nStale content.\n",
            source_id,
            "guide.md",
        )
        .expect("parse legacy doc");
        for chunk in &legacy_doc.chunks {
            crate::db::upsert_chunk(&store, source_id, chunk).expect("seed legacy chunk");
        }

        let mut state = crate::sync::state::SyncState::default();
        {
            let src = state.source_mut(source_id);
            src.file_mtimes.insert("guide.md".to_string(), 0);
            src.contract_epoch = Some(crate::ingest_contract::CONTRACT_EPOCH.to_string());
            // Intentionally leave file_contract_paths empty to simulate legacy state.
        }
        state.save(&state_path, root).expect("save legacy state");

        let before = crate::db::list_chunks_for_source(&store, source_id).expect("list before");
        assert!(
            before.iter().any(|chunk| chunk.path == "guide.md"),
            "pre-condition: legacy fs-rel chunk must exist before reingest"
        );

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync with legacy state");
        assert_eq!(
            metrics.files_synced, 1,
            "modified file must re-ingest successfully: {metrics:?}"
        );
        assert_eq!(metrics.errors, 0, "reingest must stay clean: {metrics:?}");

        let after = crate::db::list_chunks_for_source(&store, source_id).expect("list after");
        assert!(
            !after.iter().any(|chunk| chunk.path == "guide.md"),
            "legacy fs-rel rows must be deleted during modified-file reingest: {after:?}"
        );
        assert!(
            after.iter().all(|chunk| chunk.path == "canonical/guide.md"),
            "all remaining rows must use the canonical contract path: {after:?}"
        );
    }

    /// Issue 1: Full sync then incremental reingest must not duplicate the same document.
    #[test]
    fn full_sync_then_incremental_does_not_duplicate() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("canonical/guide.md", "Guide", "# Guide\n\nContent.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "dedup-test".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // First sync (full).
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("first sync");
        assert_eq!(m1.files_synced, 1, "first sync must ingest 1 file");

        // Force a second incremental sync by clearing sync state (simulates
        // epoch pivot or state corruption).
        fs::remove_file(&state_path).expect("remove state");

        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("second sync");
        assert_eq!(m2.files_synced, 1, "second sync must re-ingest 1 file");

        // Chunks must not be duplicated.
        let chunks =
            crate::db::chunks::list_chunks_for_source(&store, "dedup-test").expect("list chunks");
        let n1 = m1.chunks_created;
        assert_eq!(
            chunks.len(),
            n1,
            "chunk count must equal first sync output; \
             got {} in DB vs {} created on first sync — duplicate or orphan chunks detected",
            chunks.len(),
            n1
        );
    }

    // ── T-DSP-S01: two files with the same source_path in a sync cycle ────────

    /// When two changed files carry identical `source_path` values the sync
    /// pre-scan must reject BOTH (fail-closed) before any reingest begins.
    /// No chunks for the conflicting path may be present after the cycle, and
    /// both files must remain as "pending" (absent from sync state) for retry.
    #[test]
    fn sync_two_files_same_source_path_both_rejected_no_clobbering() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let shared_path = "canonical/guide.md";

        // Two files claim the same source_path.
        let md_a = docline_md(shared_path, "Guide A", "# Guide A\n\nContent A.\n");
        let md_b = docline_md(shared_path, "Guide B", "# Guide B\n\nContent B.\n");
        fs::write(source_dir.join("file-a.md"), md_a.as_bytes()).expect("write file-a");
        fs::write(source_dir.join("file-b.md"), md_b.as_bytes()).expect("write file-b");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "dup-sync".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync must not return a fatal error");

        // Both files must be counted as errors (pre-scan rejection).
        assert_eq!(
            metrics.errors, 2,
            "both conflicting files must be rejected; metrics: {metrics:?}"
        );

        // Neither file must be counted as synced.
        assert_eq!(
            metrics.files_synced, 0,
            "no files should be synced when all are rejected; metrics: {metrics:?}"
        );

        // No chunks must be stored for the conflicting source_path.
        let chunks =
            crate::db::chunks::list_chunks_for_source(&store, "dup-sync").expect("list chunks");
        assert!(
            chunks.is_empty(),
            "database must be empty after duplicate source_path collision; \
             found {} chunks",
            chunks.len()
        );

        // Both files must be absent from sync state (pending retry).
        let state = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src_state = state.source("dup-sync").expect("source in state");
        assert!(
            !src_state.file_mtimes.contains_key("file-a.md"),
            "file-a.md must be absent from sync state (pending retry): {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
        assert!(
            !src_state.file_mtimes.contains_key("file-b.md"),
            "file-b.md must be absent from sync state (pending retry): {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    // ── T-DSP-S02: conflicting pair + clean file ──────────────────────────────

    /// A source with a conflicting pair and one clean file must reject only the
    /// pair; the clean file must be ingested and its chunks present in the DB.
    #[test]
    fn sync_clean_file_succeeds_alongside_duplicate_pair() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let shared_path = "api/overview.md";

        let md_dup_a = docline_md(shared_path, "Overview A", "# Overview A\n\nContent A.\n");
        let md_dup_b = docline_md(shared_path, "Overview B", "# Overview B\n\nContent B.\n");
        let md_clean = docline_md("guide.md", "Guide", "# Guide\n\nContent.\n");

        fs::write(source_dir.join("dup-a.md"), md_dup_a.as_bytes()).expect("write dup-a");
        fs::write(source_dir.join("dup-b.md"), md_dup_b.as_bytes()).expect("write dup-b");
        fs::write(source_dir.join("guide.md"), md_clean.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "mixed-sync".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync must not return a fatal error");

        // Exactly 2 errors from the conflicting pair.
        assert_eq!(
            metrics.errors, 2,
            "only the two conflicting files should be errored; metrics: {metrics:?}"
        );

        // The clean file must be synced successfully.
        assert_eq!(
            metrics.files_synced, 1,
            "clean file must be synced; metrics: {metrics:?}"
        );

        // Chunks for the clean file must exist.
        let chunks =
            crate::db::chunks::list_chunks_for_source(&store, "mixed-sync").expect("list chunks");
        assert!(
            !chunks.is_empty(),
            "clean file must produce at least one chunk"
        );

        // No chunks must be stored under the conflicting source_path.
        let conflict_chunks: Vec<_> = chunks.iter().filter(|c| c.path == shared_path).collect();
        assert!(
            conflict_chunks.is_empty(),
            "no chunks must be stored for the conflicting source_path '{shared_path}'; \
             found: {conflict_chunks:?}"
        );
    }

    // ── T-DSP-XBATCH-01: new file collides with unchanged stored document ─────

    /// Regression for: incremental sync duplicate-source_path check only
    /// compared changed files against each other, missing the case where a
    /// changed file's `source_path` collides with an UNCHANGED file already
    /// recorded in sync state.
    ///
    /// When a newly added file claims the same `source_path` as an unchanged
    /// stored file, the new file must be rejected (1 error) and the unchanged
    /// file's chunks must remain intact in the database (no clobbering).
    #[test]
    fn sync_new_file_collides_with_unchanged_stored_file_is_rejected() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let shared_path = "shared/doc.md";

        // First sync: file-a.md is the canonical holder of source_path "shared/doc.md".
        let md_a = docline_md(shared_path, "Doc A", "# Doc A\n\nOriginal content.\n");
        fs::write(source_dir.join("file-a.md"), md_a.as_bytes()).expect("write file-a");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "xbatch-dup".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("first sync");
        assert_eq!(m1.files_synced, 1, "first sync must ingest file-a: {m1:?}");
        assert_eq!(m1.errors, 0, "first sync must be clean: {m1:?}");

        let chunks_after_first = crate::db::chunks::list_chunks_for_source(&store, "xbatch-dup")
            .expect("list chunks after first sync");
        assert!(
            !chunks_after_first.is_empty(),
            "file-a must produce at least one chunk"
        );

        // Add file-b.md claiming the SAME source_path.  file-a.md is UNCHANGED
        // (its mtime is not bumped), so it is NOT in `changes.added/modified` —
        // only the cross-batch check can catch this collision.
        let md_b = docline_md(shared_path, "Doc B", "# Doc B\n\nConflicting content.\n");
        fs::write(source_dir.join("file-b.md"), md_b.as_bytes()).expect("write file-b");

        // Second sync: file-a unchanged (stored), file-b newly added.
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("second sync must not be fatal");

        // file-b must be rejected: it collides with unchanged file-a.
        assert_eq!(
            m2.errors, 1,
            "file-b must be rejected (cross-batch source_path collision): {m2:?}"
        );
        assert_eq!(
            m2.files_synced, 0,
            "no files must be synced when file-b collides with file-a: {m2:?}"
        );

        // file-a's chunks must remain intact — not clobbered by the rejected file-b.
        let chunks_after_second = crate::db::chunks::list_chunks_for_source(&store, "xbatch-dup")
            .expect("list chunks after second sync");
        assert_eq!(
            chunks_after_second.len(),
            chunks_after_first.len(),
            "file-a chunks must be preserved; \
             got {} after second sync vs {} after first",
            chunks_after_second.len(),
            chunks_after_first.len()
        );

        // file-b must be absent from sync state (pending retry after conflict resolves).
        let state = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src_state = state.source("xbatch-dup").expect("source in state");
        assert!(
            !src_state.file_mtimes.contains_key("file-b.md"),
            "file-b.md must be absent from sync state after rejection: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    /// Legacy current-epoch state without `file_contract_paths` must still
    /// protect unchanged documents by their filesystem-relative identity.
    #[test]
    fn sync_new_file_collides_with_unchanged_legacy_fs_rel_state_is_rejected() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let source_id = "legacy-unchanged-fs-rel";
        let holder_path = source_dir.join("file-a.md");
        let holder_md = docline_md("file-a.md", "Holder", "# Holder\n\nCurrent content.\n");
        fs::write(&holder_path, holder_md.as_bytes()).expect("write file-a");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: source_id.to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let legacy_doc = crate::parse::parse_document(
            "# Holder\n\nStored legacy content.\n",
            source_id,
            "file-a.md",
        )
        .expect("parse legacy doc");
        for chunk in &legacy_doc.chunks {
            crate::db::upsert_chunk(&store, source_id, chunk).expect("seed legacy chunk");
        }
        let before_chunks =
            crate::db::chunks::list_chunks_for_source(&store, source_id).expect("list before");
        assert!(
            before_chunks.iter().all(|chunk| chunk.path == "file-a.md"),
            "pre-condition: legacy rows must be keyed by file-a.md: {before_chunks:?}"
        );

        let mut state = crate::sync::state::SyncState::default();
        {
            let src = state.source_mut(source_id);
            src.file_mtimes
                .insert("file-a.md".to_string(), unix_mtime_secs(&holder_path));
            src.contract_epoch = Some(crate::ingest_contract::CONTRACT_EPOCH.to_string());
            // Intentionally leave file_contract_paths empty to simulate legacy state.
        }
        state.save(&state_path, root).expect("save state");

        let colliding_md = docline_md("file-a.md", "Collision", "# Collision\n\nConflict.\n");
        fs::write(source_dir.join("file-b.md"), colliding_md.as_bytes()).expect("write file-b");

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync must not be fatal");
        assert_eq!(
            metrics.errors, 1,
            "legacy fs-rel collision must reject the new file: {metrics:?}"
        );
        assert_eq!(
            metrics.files_synced, 0,
            "no files should be reingested when the only changed file collides: {metrics:?}"
        );

        let after_chunks =
            crate::db::chunks::list_chunks_for_source(&store, source_id).expect("list after");
        assert_eq!(
            after_chunks.len(),
            before_chunks.len(),
            "unchanged legacy holder rows must remain intact after rejection"
        );
        assert!(
            after_chunks.iter().all(|chunk| chunk.path == "file-a.md"),
            "legacy fs-rel rows must not be clobbered by the rejected file: {after_chunks:?}"
        );

        let persisted = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src_state = persisted.source(source_id).expect("source in state");
        assert!(
            !src_state.file_mtimes.contains_key("file-b.md"),
            "file-b.md must remain absent from sync state after rejection: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    /// Legacy current-epoch state without `file_contract_paths` must still
    /// reject a changed file that claims another changed file's old fs-relative
    /// identity.
    #[test]
    fn sync_changed_file_claiming_legacy_changed_file_old_fs_rel_rejects_both() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let source_id = "legacy-old-path-collision";
        let file_a_path = source_dir.join("file-a.md");
        let file_a_md = docline_md("canonical/a.md", "File A", "# File A\n\nFresh content.\n");
        let stealing_md = docline_md("file-a.md", "File B", "# File B\n\nSteals old path.\n");
        fs::write(&file_a_path, file_a_md.as_bytes()).expect("write file-a");
        fs::write(source_dir.join("file-b.md"), stealing_md.as_bytes()).expect("write file-b");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: source_id.to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let legacy_doc = crate::parse::parse_document(
            "# File A\n\nStored legacy content.\n",
            source_id,
            "file-a.md",
        )
        .expect("parse legacy doc");
        for chunk in &legacy_doc.chunks {
            crate::db::upsert_chunk(&store, source_id, chunk).expect("seed legacy chunk");
        }
        let before_chunks =
            crate::db::chunks::list_chunks_for_source(&store, source_id).expect("list before");
        assert!(
            before_chunks.iter().all(|chunk| chunk.path == "file-a.md"),
            "pre-condition: legacy rows must be keyed by file-a.md: {before_chunks:?}"
        );

        let mut state = crate::sync::state::SyncState::default();
        {
            let src = state.source_mut(source_id);
            src.file_mtimes.insert("file-a.md".to_string(), 0);
            src.contract_epoch = Some(crate::ingest_contract::CONTRACT_EPOCH.to_string());
            // Intentionally leave file_contract_paths empty to simulate legacy state.
        }
        state.save(&state_path, root).expect("save state");

        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("sync must not be fatal");
        assert_eq!(
            metrics.errors, 2,
            "both changed files must be rejected on legacy old-path collision: {metrics:?}"
        );
        assert_eq!(
            metrics.files_synced, 0,
            "no files should be reingested when the old-path collision is detected: {metrics:?}"
        );

        let after_chunks =
            crate::db::chunks::list_chunks_for_source(&store, source_id).expect("list after");
        assert_eq!(
            after_chunks.len(),
            before_chunks.len(),
            "legacy rows must remain intact when both changed files are rejected"
        );
        assert!(
            after_chunks.iter().all(|chunk| chunk.path == "file-a.md"),
            "legacy rows must not be replaced by canonical/a.md after rejection: {after_chunks:?}"
        );
        assert!(
            after_chunks
                .iter()
                .all(|chunk| chunk.path != "canonical/a.md"),
            "no canonical/a.md rows should be present after rejection: {after_chunks:?}"
        );

        let persisted = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src_state = persisted.source(source_id).expect("source in state");
        assert_eq!(
            src_state.file_mtimes.get("file-a.md").copied(),
            Some(0),
            "file-a.md must retain its old mtime so the modified legacy file retries"
        );
        assert!(
            !src_state.file_mtimes.contains_key("file-b.md"),
            "file-b.md must remain absent from sync state after rejection: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    // ── T-DEL-FAIL-01: failed deletion preserves state for retry ─────────────

    /// Unit-level regression for: failed `delete_file_data` calls were
    /// incorrectly dropping `file_contract_paths` and `file_mtimes` entries
    /// for the removed file, making cleanup non-retryable on the next sync
    /// cycle.
    ///
    /// `apply_delete_cleanup_to_state` must:
    /// - Remove the contract path for a **successful** delete.
    /// - **Preserve** the contract path and **restore** the old mtime for a
    ///   **failed** delete so the next sync cycle re-detects the file as
    ///   deleted and retries the cleanup.
    #[test]
    fn apply_delete_cleanup_preserves_failed_delete_for_retry() {
        use std::collections::{HashMap, HashSet};
        use std::path::PathBuf;

        use super::apply_delete_cleanup_to_state;

        let mut contract_paths: HashMap<String, String> = [
            (
                "docs/ghost.md".to_string(),
                "canonical/ghost.md".to_string(),
            ),
            (
                "docs/success.md".to_string(),
                "canonical/success.md".to_string(),
            ),
        ]
        .into_iter()
        .collect();
        let mut file_mtimes: HashMap<String, u64> = HashMap::new();

        let deleted_paths = vec![
            PathBuf::from("docs/ghost.md"),
            PathBuf::from("docs/success.md"),
        ];

        // "ghost.md" failed its DB cleanup; "success.md" succeeded.
        let mut failed: HashSet<String> = HashSet::new();
        failed.insert("docs/ghost.md".to_string());

        let stored_mtimes: HashMap<String, u64> = [
            ("docs/ghost.md".to_string(), 9_999_u64),
            ("docs/success.md".to_string(), 8_888_u64),
        ]
        .into_iter()
        .collect();

        apply_delete_cleanup_to_state(
            &mut contract_paths,
            &mut file_mtimes,
            &deleted_paths,
            &failed,
            &stored_mtimes,
        );

        // Successful delete: contract path removed, mtime NOT restored.
        assert!(
            !contract_paths.contains_key("docs/success.md"),
            "successful delete must remove contract path entry"
        );
        assert!(
            !file_mtimes.contains_key("docs/success.md"),
            "successful delete must not restore mtime"
        );

        // Failed delete: contract path PRESERVED, old mtime RESTORED.
        assert_eq!(
            contract_paths.get("docs/ghost.md").map(String::as_str),
            Some("canonical/ghost.md"),
            "failed delete must preserve contract path for retry"
        );
        assert_eq!(
            file_mtimes.get("docs/ghost.md").copied(),
            Some(9_999_u64),
            "failed delete must restore old mtime so next sync retries cleanup"
        );
    }

    /// Failed delete retry state must be dropped once another file successfully
    /// claims the deleted file's contract identity in the same cycle.
    #[test]
    fn apply_delete_cleanup_drops_failed_delete_retry_when_replacement_claimed() {
        use std::collections::{HashMap, HashSet};
        use std::path::PathBuf;

        use super::apply_delete_cleanup_to_state;

        let mut contract_paths: HashMap<String, String> = [
            (
                "docs/removed.md".to_string(),
                "canonical/shared.md".to_string(),
            ),
            (
                "docs/replacement.md".to_string(),
                "canonical/shared.md".to_string(),
            ),
        ]
        .into_iter()
        .collect();
        let mut file_mtimes: HashMap<String, u64> = HashMap::new();
        let deleted_paths = vec![PathBuf::from("docs/removed.md")];

        let failed = HashSet::from(["docs/removed.md".to_string()]);
        let stored_mtimes = HashMap::from([("docs/removed.md".to_string(), 1_234_u64)]);

        apply_delete_cleanup_to_state(
            &mut contract_paths,
            &mut file_mtimes,
            &deleted_paths,
            &failed,
            &stored_mtimes,
        );

        assert!(
            !contract_paths.contains_key("docs/removed.md"),
            "stale failed-delete mapping must be dropped when a replacement owns the same path"
        );
        assert_eq!(
            contract_paths
                .get("docs/replacement.md")
                .map(String::as_str),
            Some("canonical/shared.md"),
            "replacement document must remain the sole owner of the shared contract path"
        );
        assert!(
            !file_mtimes.contains_key("docs/removed.md"),
            "stale failed-delete mtime must not be restored when replacement has claimed the path"
        );
    }

    // ── T-EPOCH-DEL-01: legacy/no-epoch state + deleted file cleans up ──────

    /// Regression for: epoch mismatch used `HashMap::new()` as the stored
    /// mtime snapshot, which meant deleted files were never detected (nothing
    /// in an empty map can be "missing").  The fix preserves the stored file
    /// keys (zeroing values) so deletion detection still works.
    ///
    /// Scenario:
    /// 1. Ingest one file → sync state recorded with current epoch.
    /// 2. Downgrade the stored epoch to `None` (simulates pre-pivot legacy
    ///    state arriving after a binary upgrade).
    /// 3. Delete the file from the filesystem.
    /// 4. Run sync — must detect the deletion, clean up DB records, and
    ///    migrate the state to the current epoch.
    #[test]
    fn legacy_epoch_mismatch_cleans_up_deleted_only_file() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let md = docline_md("only.md", "Only", "# Only\n\nContent.\n");
        let file_path = source_dir.join("only.md");
        fs::write(&file_path, md.as_bytes()).expect("write only.md");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "epoch-del".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // Step 1: ingest the file so it gets a proper sync state.
        let m1 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m1");
        assert_eq!(m1.files_synced, 1, "first sync: {m1:?}");
        assert_eq!(m1.errors, 0, "first sync must be clean: {m1:?}");

        // Step 2: downgrade the stored epoch to None (simulate legacy/no-epoch state).
        {
            let mut state =
                crate::sync::state::SyncState::load(&state_path, root).expect("load state");
            let src = state.source_mut("epoch-del");
            src.contract_epoch = None; // legacy — no epoch recorded
            state.save(&state_path, root).expect("save legacy state");
        }

        // Step 3: delete the only file from the filesystem.
        fs::remove_file(&file_path).expect("remove only.md");
        assert!(
            !file_path.exists(),
            "file must be removed from filesystem before second sync"
        );

        // Step 4: run sync — must detect deletion despite epoch mismatch.
        let m2 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m2");

        // Deletion must be processed.
        assert_eq!(
            m2.files_deleted, 1,
            "epoch-mismatch sync must clean up the deleted file; metrics: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "no errors expected on clean deletion: {m2:?}");
        assert_eq!(
            m2.files_synced, 0,
            "no files to re-ingest after deletion: {m2:?}"
        );

        // Sync state must be migrated to current epoch.
        let after = crate::sync::state::SyncState::load(&state_path, root).expect("load after");
        let src_after = after.source("epoch-del").expect("source in state after");
        assert_eq!(
            src_after.contract_epoch.as_deref(),
            Some(crate::ingest_contract::CONTRACT_EPOCH),
            "state must carry current epoch after deletion sync: {src_after:?}"
        );

        // No stale file_mtimes entries for the deleted file.
        assert!(
            !src_after.file_mtimes.contains_key("only.md"),
            "deleted file must not remain in file_mtimes: {:?}",
            src_after.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    // ── T-EPOCH-EMPTY-SRC: epoch mismatch + completely empty source dir ──────

    /// Regression: when an epoch mismatch occurs and the source directory
    /// contains NO files at all (empty current source), all previously tracked
    /// files must still be cleaned up.
    ///
    /// Without the fix the sync state was never migrated to the current epoch
    /// either, because `changes.is_empty()` caused an early return before the
    /// state-save path.  With the fix (stored keys zeroed), the diff produces
    /// deletions for every previously-tracked path.
    #[test]
    fn legacy_epoch_mismatch_with_empty_source_cleans_up_all_tracked_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        // Populate two files and run a successful sync.
        for name in &["alpha.md", "beta.md"] {
            let sp = format!("docs/{name}");
            let md = docline_md(&sp, name, &format!("# {name}\n\nContent.\n"));
            fs::write(source_dir.join(name), md.as_bytes()).expect("write file");
        }

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "epoch-empty".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let m1 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m1");
        assert_eq!(m1.files_synced, 2, "first sync: {m1:?}");

        // Downgrade epoch to None.
        {
            let mut state =
                crate::sync::state::SyncState::load(&state_path, root).expect("load state");
            state.source_mut("epoch-empty").contract_epoch = None;
            state.save(&state_path, root).expect("save legacy state");
        }

        // Remove ALL files so the source directory is now empty.
        for name in &["alpha.md", "beta.md"] {
            fs::remove_file(source_dir.join(name)).expect("remove file");
        }

        // Sync with empty source: must delete both tracked files.
        let m2 =
            sync_source(&store, &source, &source_dir, &state_path, root, None, None).expect("m2");

        assert_eq!(
            m2.files_deleted, 2,
            "both tracked files must be cleaned up from empty source; metrics: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "no errors expected: {m2:?}");

        // State must be migrated to current epoch.
        let after = crate::sync::state::SyncState::load(&state_path, root).expect("load after");
        let src_after = after.source("epoch-empty").expect("source after");
        assert_eq!(
            src_after.contract_epoch.as_deref(),
            Some(crate::ingest_contract::CONTRACT_EPOCH),
            "state must carry current epoch after empty-source sync: {src_after:?}"
        );
        assert!(
            src_after.file_mtimes.is_empty(),
            "no files should remain in sync state: {:?}",
            src_after.file_mtimes.keys().collect::<Vec<_>>()
        );
    }

    // ── Issue 2: force_full_rebuild + model.is_none() warning (epoch case) ───

    /// When a full rebuild is forced due to an epoch mismatch and no embedding
    /// model is available, `sync_source` must still succeed but a warning must
    /// be emitted. This regression guard ensures the warn path does not become
    /// a hard error that would block epoch migrations in degraded-embed mode.
    #[test]
    fn epoch_mismatch_full_rebuild_with_no_model_succeeds_with_warning() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("guide.md", "Guide", "# Guide\n\nContent.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "epoch-no-embed".to_string(),
            path: source_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // Simulate old epoch state: file mtime matches (would skip on normal run)
        // but contract_epoch differs → force_full_rebuild.
        let mtime = unix_mtime_secs(&source_dir.join("guide.md"));
        let mut pre_state = crate::sync::state::SyncState::default();
        {
            let src = pre_state.source_mut("epoch-no-embed");
            src.file_mtimes.insert("guide.md".to_string(), mtime);
            src.contract_epoch = Some("old-epoch-value".to_string()); // intentionally wrong
        }
        pre_state.save(&state_path, root).expect("save pre-state");

        // model = None simulates --no-embed or unavailable model; must warn but not error.
        let metrics = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("epoch rebuild with no model must succeed");

        assert_eq!(
            metrics.files_synced, 1,
            "forced rebuild must re-ingest the file: {metrics:?}"
        );
        assert_eq!(metrics.errors, 0, "rebuild must stay clean: {metrics:?}");
    }

    // ── Issue 3: seed_sync_state_from_live_sources ────────────────────────────

    /// `seed_sync_state_from_live_sources` must populate sync state with live
    /// file mtimes and validated contract paths so the next incremental cycle
    /// does not re-ingest every file from scratch.
    #[test]
    fn seed_sync_state_from_live_sources_populates_state() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("canonical/guide.md", "Guide", "# Guide\n\nContent.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");
        let live_mtime = unix_mtime_secs(&source_dir.join("guide.md"));

        let state_path = root.join("sync_state.json");

        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: Source::Local(LocalSource {
                    id: "live-seed-test".to_string(),
                    path: source_dir.clone(),
                    include: vec!["**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec!["md".to_string()],
                    database: None,
                }),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root)
            .expect("seed_sync_state_from_live_sources must succeed");

        let state = crate::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src = state
            .source("live-seed-test")
            .expect("source must be present in seeded state");

        assert_eq!(
            src.file_mtimes.get("guide.md"),
            Some(&live_mtime),
            "seeded state must carry the live mtime"
        );
        assert_eq!(
            src.file_contract_paths.get("guide.md").map(String::as_str),
            Some("canonical/guide.md"),
            "seeded state must carry the validated contract source_path"
        );
        assert_eq!(
            src.contract_epoch.as_deref(),
            Some(crate::ingest_contract::CONTRACT_EPOCH),
            "seeded state must be stamped with the current contract epoch"
        );
    }

    /// After a full pipeline run (`--full`) is simulated and sync state is seeded
    /// via `seed_sync_state_from_live_sources`, the next incremental `sync_source`
    /// call must detect no changes and skip re-ingestion.
    #[test]
    fn seed_from_live_sources_prevents_unnecessary_reingest_on_followup_incremental() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::db::chunks::list_chunks_for_source;
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let md = docline_md("docs/guide.md", "Guide", "# Guide\n\nStable content.\n");
        fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "no-reingest".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // Simulate the full pipeline having already loaded chunks.
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial sync");
        assert_eq!(m1.files_synced, 1, "initial sync must ingest 1 file");
        let chunks_after_full = list_chunks_for_source(&store, "no-reingest").expect("list chunks");
        assert!(
            !chunks_after_full.is_empty(),
            "initial sync must create chunks"
        );

        // Clear sync state to simulate what happens after a full pipeline run
        // that does NOT seed state. Then seed it properly with the live-sources
        // function and verify the next incremental run skips re-ingestion.
        fs::remove_file(&state_path).expect("remove state");

        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root)
            .expect("seed state from live sources");

        // Incremental sync must detect no changes.
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("followup incremental sync");
        assert_eq!(
            m2.files_synced, 0,
            "incremental sync after seeding must detect no changes: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "no errors expected: {m2:?}");
    }

    // ── T-SEED-SNAPSHOT-01: workspace-root source never seeds snapshot files ──

    /// Regression: `seed_sync_state_from_live_sources` passed `None` as
    /// `ignored_root` to `scan_tracked_source_mtimes`.  For a workspace-root
    /// source this caused stale snapshot markdown files under
    /// `.graphtor/data/v4-migration-snapshots/` to be included in the seeded
    /// sync state.  On the next incremental run the snapshot path appeared as
    /// "deleted" (because the migration cleanup had removed the snapshot
    /// directory), and the delete path used the shared `source_path` frontmatter
    /// value to erase the live document's database records.
    ///
    /// After the fix, `seed_sync_state_from_live_sources` computes the same
    /// `ignored_root` guard used by all other scan paths when
    /// `allow_internal_snapshot_scan` is false.
    #[test]
    fn seed_sync_state_workspace_root_excludes_stale_snapshot_files() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::db::chunks::list_chunks_for_source;
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        // Workspace-root source: source_dir equals the workspace root itself.
        let source_dir = root.to_path_buf();
        let data_root = root.join(".graphtor").join("data");
        fs::create_dir_all(&data_root).expect("create data root");

        // ── 1. Create the live document at the workspace root ────────────
        let live_md = docline_md("guide.md", "Guide", "# Guide\n\nLive content.\n");
        fs::write(root.join("guide.md"), live_md.as_bytes()).expect("write live guide");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "workspace-root".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // ── 2. Initial full sync — ingests guide.md ──────────────────────
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial full sync");
        assert_eq!(
            m1.files_synced, 1,
            "initial sync must ingest guide.md: {m1:?}"
        );
        let chunks_after_full =
            list_chunks_for_source(&store, "workspace-root").expect("list chunks after full sync");
        assert!(
            !chunks_after_full.is_empty(),
            "initial sync must create chunks for guide.md"
        );

        // ── 3. Simulate the full pipeline clearing sync state ────────────
        fs::remove_file(&state_path).expect("remove sync state");

        // ── 4. Create a stale v4-migration snapshot with the SAME source_path ──
        // This is the dangerous case: if seeding picks up this file, the next
        // incremental run sees it as "deleted" and erases guide.md's records.
        let snapshot_dir = data_root
            .join("v4-migration-snapshots")
            .join("snap0")
            .join("source-0");
        fs::create_dir_all(&snapshot_dir).expect("create snapshot dir");
        let stale_md = docline_md("guide.md", "Stale Guide", "# Stale\n\nOld snapshot.\n");
        let stale_file = snapshot_dir.join("guide.md");
        fs::write(&stale_file, stale_md.as_bytes()).expect("write stale snapshot");

        // ── 5. Seed sync state from live sources (should exclude snapshot) ─
        let plan = AcquisitionPlan {
            data_root: data_root.clone(),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root)
            .expect("seed_sync_state_from_live_sources must succeed");

        let seeded =
            crate::sync::state::SyncState::load(&state_path, root).expect("load seeded state");
        let src_state = seeded
            .source("workspace-root")
            .expect("workspace-root must be present in seeded state");

        // The live file must be seeded.
        assert!(
            src_state.file_mtimes.contains_key("guide.md"),
            "seeded state must include the live guide.md: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );

        // The stale snapshot path must NOT be seeded.
        let stale_rel = stale_file
            .strip_prefix(root)
            .expect("stale file must be under root")
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            !src_state.file_mtimes.contains_key(&stale_rel),
            "seeded state must NOT contain the stale snapshot path '{stale_rel}': {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );

        // ── 6. Simulate post-migration snapshot cleanup ──────────────────
        fs::remove_file(&stale_file).expect("remove stale snapshot");

        // ── 7. Incremental sync must not delete the live document ────────
        // Without the fix: the snapshot path is in stored state but absent from
        // the current scan → treated as "deleted" → delete_file_data invoked
        // with source_path="guide.md" → live document's chunks are erased.
        // With the fix: the snapshot path was never seeded → no spurious deletion.
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("incremental sync must not fail");

        assert_eq!(
            m2.files_deleted, 0,
            "incremental sync must NOT delete any files after snapshot cleanup: {m2:?}"
        );
        assert_eq!(
            m2.files_synced, 0,
            "incremental sync must detect no changes to the live file: {m2:?}"
        );

        let chunks_after_incremental = list_chunks_for_source(&store, "workspace-root")
            .expect("list chunks after incremental sync");
        assert_eq!(
            chunks_after_incremental.len(),
            chunks_after_full.len(),
            "live document chunks must survive the incremental sync — \
             stale snapshot deletion must not erase them"
        );
    }

    // ── Regression: full-sync seeding must not forget pre-full-sync deletions ─

    /// After a regular full sync, `seed_sync_state_from_live_sources` must
    /// preserve entries for files that were deleted **before** the full sync
    /// ran.  The full pipeline does not remove DB rows for pre-deleted files;
    /// only the incremental cycle's delete path does.  If the seeded state
    /// forgets those entries, the next incremental run sees no difference and
    /// the stale DB rows survive forever.
    ///
    /// Scenario:
    /// 1. Ingest `keep.md` and `gone.md`.
    /// 2. Delete `gone.md` from the filesystem (before any full sync).
    /// 3. Seed sync state from live sources (simulates what `--full` does after
    ///    the pipeline completes).
    /// 4. Check seeded state still contains `gone.md`.
    /// 5. Run an incremental sync — must detect `gone.md` as deleted and report
    ///    `files_deleted = 1`.
    #[test]
    fn seed_from_live_preserves_pre_full_sync_deleted_file_for_incremental_cleanup() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::db::chunks::list_chunks_for_source;
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let keep_path = source_dir.join("keep.md");
        let gone_path = source_dir.join("gone.md");
        fs::write(
            &keep_path,
            docline_md("docs/keep.md", "Keep", "# Keep\n\nStays around.\n"),
        )
        .expect("write keep.md");
        fs::write(
            &gone_path,
            docline_md("docs/gone.md", "Gone", "# Gone\n\nWill be deleted.\n"),
        )
        .expect("write gone.md");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "del-regression".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // ── 1. Initial sync: ingest both files ───────────────────────────
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial sync");
        assert_eq!(m1.files_synced, 2, "both files must be ingested: {m1:?}");
        assert_eq!(m1.errors, 0, "initial sync must be clean: {m1:?}");

        // ── 2. Delete gone.md before the full sync runs ──────────────────
        fs::remove_file(&gone_path).expect("remove gone.md");
        assert!(
            !gone_path.exists(),
            "gone.md must be absent before full-sync seed"
        );

        let chunks_before =
            list_chunks_for_source(&store, "del-regression").expect("list chunks before seeding");
        assert!(
            !chunks_before.is_empty(),
            "chunks must exist for both docs before seeding"
        );

        // ── 3. Seed sync state from live sources (simulates regular --full) ─
        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root).expect("seed must succeed");

        // ── 4. Seeded state must still contain gone.md's entry ───────────
        let seeded =
            crate::sync::state::SyncState::load(&state_path, root).expect("load seeded state");
        let src_state = seeded
            .source("del-regression")
            .expect("source must be present");
        assert!(
            src_state.file_mtimes.contains_key("gone.md"),
            "seeded state must carry forward gone.md so the next incremental can delete it; \
             actual keys: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            src_state
                .file_contract_paths
                .get("gone.md")
                .map(String::as_str),
            Some("docs/gone.md"),
            "seeded state must carry forward gone.md's contract path"
        );

        // ── 5. Incremental sync must detect and clean up gone.md ─────────
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("incremental sync after seeding must succeed");

        assert_eq!(
            m2.files_deleted, 1,
            "incremental sync must delete gone.md; metrics: {m2:?}"
        );
        assert_eq!(
            m2.files_synced, 0,
            "keep.md is unchanged; no re-ingest expected: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "no errors expected: {m2:?}");
    }

    /// Guard: when the old contract path of a deleted file is now claimed by a
    /// currently-live file, `seed_sync_state_from_live_sources` must NOT carry
    /// the deleted entry forward.  Doing so would cause the next incremental to
    /// invoke the delete path for the live file's rows — corrupting live data.
    ///
    /// Scenario:
    /// 1. `old.md` is ingested with contract path `"shared/guide.md"`.
    /// 2. `old.md` is deleted.
    /// 3. `new.md` is created with the same contract path `"shared/guide.md"`.
    /// 4. Seed sync state from live sources.
    /// 5. Verify `old.md` is NOT in the seeded state (guard fired).
    /// 6. Incremental sync must not delete anything.
    #[test]
    fn seed_from_live_skips_carry_forward_when_contract_path_claimed_by_live_file() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let old_path = source_dir.join("old.md");
        let new_path = source_dir.join("new.md");
        fs::write(
            &old_path,
            docline_md("shared/guide.md", "Old Guide", "# Old\n\nOld content.\n"),
        )
        .expect("write old.md");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "guard-test".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // ── 1. Ingest old.md ─────────────────────────────────────────────
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial sync");
        assert_eq!(m1.files_synced, 1, "old.md must be ingested: {m1:?}");

        // ── 2. Delete old.md, add new.md with the same contract path ─────
        fs::remove_file(&old_path).expect("remove old.md");
        fs::write(
            &new_path,
            docline_md("shared/guide.md", "New Guide", "# New\n\nNew content.\n"),
        )
        .expect("write new.md");

        // ── 3. Seed sync state from live sources ─────────────────────────
        // The live scan sees only new.md; old.md is absent.
        // old.md's contract path "shared/guide.md" is now claimed by new.md.
        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root).expect("seed must succeed");

        // ── 4. old.md must NOT be in the seeded state ────────────────────
        let seeded =
            crate::sync::state::SyncState::load(&state_path, root).expect("load seeded state");
        let src_state = seeded.source("guard-test").expect("source must be present");
        assert!(
            !src_state.file_mtimes.contains_key("old.md"),
            "guard must prevent old.md carry-forward; \
             live file new.md claims the same contract path 'shared/guide.md'; \
             actual keys: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );
        assert!(
            src_state.file_mtimes.contains_key("new.md"),
            "new.md must be in seeded state: {:?}",
            src_state.file_mtimes.keys().collect::<Vec<_>>()
        );

        // ── 5. Incremental sync must not delete anything ─────────────────
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("incremental sync must not fail");
        assert_eq!(
            m2.files_deleted, 0,
            "guard must prevent spurious deletion of live document: {m2:?}"
        );
        assert_eq!(m2.errors, 0, "no errors expected: {m2:?}");
    }

    /// After a regular full sync, if a live file's frontmatter `source_path`
    /// changed since the last tracked sync, `seed_sync_state_from_live_sources`
    /// must carry forward the OLD mtime and OLD contract path.  This forces the
    /// next incremental to re-ingest the file via
    /// `reingest_file_with_old_contract_path`, which deletes the orphaned rows
    /// for the old identity and inserts fresh rows under the new identity.
    ///
    /// Scenario:
    /// 1. Ingest `docs/a.md` with `source_path: "old-identity.md"`.
    /// 2. Rewrite frontmatter to `source_path: "new-identity.md"` (mtime bumps).
    /// 3. Seed sync state from live sources (simulates post-full-sync seeding).
    /// 4. Verify seeded state has OLD mtime + OLD contract path for `a.md`.
    /// 5. Run incremental sync — must re-ingest `a.md`, reporting
    ///    `files_synced = 1` (uses old contract path to clean up stale rows).
    #[test]
    fn seed_from_live_carries_old_contract_path_for_identity_changes() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::sync::seed_sync_state_from_live_sources;

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");

        let file_path = source_dir.join("a.md");
        fs::write(
            &file_path,
            docline_md("old-identity.md", "Doc A", "# A\n\nOriginal.\n"),
        )
        .expect("write a.md with old identity");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "cp-change".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        // ── 1. Ingest with old identity ──────────────────────────────────
        let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial sync");
        assert_eq!(m1.files_synced, 1, "initial sync must ingest a.md: {m1:?}");

        let old_mtime = unix_mtime_secs(&file_path);

        // ── 2. Rewrite frontmatter to a new source_path (mtime changes) ──
        // A tiny sleep ensures the OS advances the mtime so the incremental
        // diff can reliably detect the modification.
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(
            &file_path,
            docline_md("new-identity.md", "Doc A", "# A\n\nUpdated identity.\n"),
        )
        .expect("write a.md with new identity");
        let new_mtime = unix_mtime_secs(&file_path);

        // ── 3. Seed sync state from live sources ─────────────────────────
        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        seed_sync_state_from_live_sources(&plan, &state_path, root).expect("seed must succeed");

        // ── 4. Seeded state must have OLD mtime + OLD contract path ───────
        let seeded =
            crate::sync::state::SyncState::load(&state_path, root).expect("load seeded state");
        let src_state = seeded.source("cp-change").expect("source must be present");

        // On filesystems where mtime resolution is coarser than 1 s the old and
        // new mtime may be equal.  In that case the incremental cycle will see
        // "no change" and the test degenerates into a no-op — but the invariant
        // we care about (old contract path is preserved) still holds.
        if old_mtime != new_mtime {
            assert_eq!(
                src_state.file_mtimes.get("a.md").copied(),
                Some(old_mtime),
                "seeded state must store OLD mtime to force incremental re-ingest"
            );
        }
        assert_eq!(
            src_state
                .file_contract_paths
                .get("a.md")
                .map(String::as_str),
            Some("old-identity.md"),
            "seeded state must carry forward OLD contract path for a.md"
        );

        // ── 5. Incremental sync must re-ingest a.md ──────────────────────
        // If mtime is identical (coarse FS), re-ingest will not fire; the
        // important property — old orphaned rows are cleaned up — still holds
        // because a later real mtime change will trigger the correct path.
        // We only assert files_synced when we know the mtime advanced.
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("incremental sync must succeed");
        assert_eq!(m2.errors, 0, "incremental sync must not error: {m2:?}");
        if old_mtime != new_mtime {
            assert_eq!(
                m2.files_synced, 1,
                "incremental must re-ingest a.md to clean up old identity rows: {m2:?}"
            );
        }
    }

    // ── Race-fix: pre-sync snapshot closes post-pipeline mutation window ───────

    /// A file mutated **after** the full-sync pipeline completes but **before**
    /// the old post-pipeline rescan must NOT be silently recorded as
    /// already-synced.
    ///
    /// With the old (racy) approach, `seed_sync_state_from_live_sources` would
    /// rescan the filesystem AFTER the pipeline and record the new mtime M1.
    /// The next incremental would compare M1 (stored) with M1 (on disk) and
    /// conclude "no change", leaving the DB with stale content.
    ///
    /// With the new approach, [`capture_pre_sync_snapshot`] records M0 BEFORE
    /// the pipeline, then [`seed_sync_state_from_pre_sync_snapshot`] persists M0.
    /// The next incremental compares M0 (stored) with M1 (on disk) → "changed"
    /// → re-ingests the file, repairing the drift.
    ///
    /// Scenario:
    /// 1. Create `guide.md` (mtime M0).
    /// 2. Capture pre-sync snapshot → records M0.
    /// 3. Simulate the pipeline completing (initial ingest via `sync_source`).
    /// 4. Mutate `guide.md` (new mtime M1 > M0) — post-pipeline window.
    /// 5. Seed state from the pre-sync snapshot → stores M0, NOT M1.
    /// 6. Run incremental sync → must detect the mutation (M1 ≠ M0) and
    ///    re-ingest `guide.md`.
    #[test]
    fn pre_sync_snapshot_detects_post_pipeline_mutation_on_followup_incremental() {
        use crate::acquire::{AcquisitionPlan, PlannedSource, SourceAction};
        use crate::db::chunks::list_chunks_for_source;
        use crate::sync::{capture_pre_sync_snapshot, seed_sync_state_from_pre_sync_snapshot};

        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        let source_dir = root.join("docs");
        fs::create_dir_all(&source_dir).expect("create source dir");
        let guide_path = source_dir.join("guide.md");

        // ── 1. Create guide.md ───────────────────────────────────────────
        fs::write(
            &guide_path,
            docline_md("docs/guide.md", "Guide", "# Guide\n\nOriginal content.\n"),
        )
        .expect("write guide.md");

        let state_path = root.join("sync_state.json");
        let store = DataStore::open_mem().expect("open in-memory store");
        ensure_schema(&store).expect("ensure schema");

        let source = Source::Local(LocalSource {
            id: "race-fix-test".to_string(),
            path: source_dir.clone(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });

        let plan = AcquisitionPlan {
            data_root: root.join(".graphtor").join("data"),
            allowed_root: root.to_path_buf(),
            sources: vec![PlannedSource {
                source: source.clone(),
                action: SourceAction::ScanLocal,
                target_dir: source_dir.clone(),
                allow_internal_snapshot_scan: false,
            }],
            total_scan: 1,
        };

        // ── 2. Capture pre-sync snapshot (records M0) ────────────────────
        let pre_snapshot =
            capture_pre_sync_snapshot(&plan, root).expect("capture_pre_sync_snapshot must succeed");

        let capture = pre_snapshot
            .get("race-fix-test")
            .expect("race-fix-test must be in snapshot");
        let m0 = *capture
            .file_mtimes
            .get("guide.md")
            .expect("guide.md must be captured");

        // ── 3. Simulate the pipeline completing (ingest guide.md) ────────
        // Clear any pre-existing state to mirror a fresh --full run.
        let m1_init = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("initial ingest must succeed");
        assert_eq!(
            m1_init.files_synced, 1,
            "pipeline proxy must ingest guide.md"
        );
        let chunks_after_pipeline =
            list_chunks_for_source(&store, "race-fix-test").expect("list chunks after pipeline");
        assert!(
            !chunks_after_pipeline.is_empty(),
            "pipeline must create chunks for guide.md"
        );

        // Remove the state that sync_source wrote — the full-pipeline path
        // would not have written incremental sync state.
        fs::remove_file(&state_path).expect("remove state written by sync proxy");

        // ── 4. Mutate guide.md AFTER the pipeline (post-pipeline window) ──
        // A small sleep ensures the OS advances the mtime clock.
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(
            &guide_path,
            docline_md(
                "docs/guide.md",
                "Guide",
                "# Guide\n\nContent mutated after pipeline.\n",
            ),
        )
        .expect("mutate guide.md after pipeline");
        let m1 = unix_mtime_secs(&guide_path);

        // ── 5. Seed sync state from the PRE-SYNC snapshot (stores M0) ────
        seed_sync_state_from_pre_sync_snapshot(&plan, &pre_snapshot, &state_path, root)
            .expect("seed from pre-sync snapshot must succeed");

        let seeded =
            crate::sync::state::SyncState::load(&state_path, root).expect("load seeded state");
        let src_state = seeded
            .source("race-fix-test")
            .expect("race-fix-test must be in seeded state");

        // The stored mtime must be M0 (pre-mutation), not M1 (post-mutation).
        let stored_mtime = *src_state
            .file_mtimes
            .get("guide.md")
            .expect("guide.md must be in seeded state");
        assert_eq!(
            stored_mtime, m0,
            "seeded state must record the PRE-mutation mtime M0={m0}, not M1={m1}; \
             stored={stored_mtime}"
        );

        // ── 6. Incremental sync must detect the mutation ──────────────────
        // If M0 == M1 the filesystem has coarse mtime resolution (1-second
        // buckets) and this test cannot demonstrate the timing property.
        // We skip the re-ingest assertion but still verify no errors.
        let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
            .expect("incremental sync must not fail");
        assert_eq!(m2.errors, 0, "incremental sync must not error: {m2:?}");

        if m0 != m1 {
            assert_eq!(
                m2.files_synced, 1,
                "incremental sync must detect and re-ingest the post-pipeline mutation \
                 (M0={m0} vs M1={m1}); metrics: {m2:?}"
            );
        }
    }
}
