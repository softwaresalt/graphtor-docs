//! Source acquisition module — scan local directories and apply glob filtering.
//!
//! This module orchestrates the full source acquisition pipeline:
//! planning what to acquire, executing scans, applying glob filtering,
//! and collecting results into an [`AcquisitionResult`].
//!
//! # Pipeline stages
//!
//! ```text
//! SourceConfig → plan() → AcquisitionPlan → execute() → AcquisitionResult
//!                                                  ↳ scan_local_source()  (per local source)
//!                                                  ↳ apply_source_filter() (per acquired source)
//! ```

use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::config::source::LocalSource;
use crate::error::GraphtorError;

pub mod filter;
pub mod local;
pub mod plan;
pub mod result;

pub use filter::{filter_files, FileFilter};
pub use local::{scan_local_source, scan_local_source_with_ignored_root};
pub use plan::{plan, validate_sources};
pub use result::{
    AcquiredSource, AcquisitionPlan, AcquisitionResult, FilteredFileSet, PlannedSource,
    SourceAction, SourceOutcome, SourceType, ValidationError, ValidationReport,
};

/// Apply include/exclude glob patterns to a file list and wrap the result in a [`FilteredFileSet`].
///
/// This is the pipeline integration point for US3: after acquisition (scan),
/// call this function to produce the `FilteredFileSet` used in [`SourceOutcome::Success`].
///
/// `original_count` is set to `files.len()` before filtering; `filtered_count` reflects
/// the number of files that survived the include/exclude pass.
///
/// # Path contract
///
/// `files` is matched against the glob patterns as-is. For patterns without a leading `**`
/// (e.g. `docs/**/*.md`), `files` must contain **source-root-relative** paths, not absolute
/// ones. When `files` comes from [`scan_local_source`] (which returns absolute canonical
/// paths), use [`execute`] instead — the internal [`scan_and_filter`] pipeline strips the
/// source root automatically before calling [`filter_files`].
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if any glob pattern is invalid.
pub fn apply_source_filter(
    source_id: &str,
    files: &[PathBuf],
    include: &[String],
    exclude: &[String],
) -> Result<FilteredFileSet, GraphtorError> {
    let original_count = files.len();
    let filtered = filter_files(files, include, exclude)?;
    let filtered_count = filtered.len();
    Ok(FilteredFileSet {
        source_id: source_id.to_string(),
        original_count,
        filtered_count,
        files: filtered,
    })
}

/// Execute an acquisition plan: scan local dirs and filter files.
///
/// Processes each source in the plan. Failures in one source do not stop
/// processing of others. Returns an aggregate result with per-source outcomes.
/// Emits a summary INFO log on completion (FR-016, FR-018).
///
/// When `dry_run` is `true`, no filesystem operations are performed.
/// All sources are reported with zero files (FR-019).
#[must_use]
pub fn execute(acq_plan: &AcquisitionPlan, dry_run: bool) -> AcquisitionResult {
    let mut outcomes: Vec<SourceOutcome> = Vec::new();
    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    let mut total_files: usize = 0;

    for planned in &acq_plan.sources {
        let outcome = if dry_run {
            // FR-019: dry-run — report the plan without performing any I/O
            info!(source_id = %planned.source.id(), action = %planned.action, "dry-run: skipping");
            SourceOutcome::Failed {
                source_id: planned.source.id().to_string(),
                error: "dry-run".to_string(),
            }
        } else {
            dispatch_planned_source(planned, acq_plan)
        };

        match &outcome {
            SourceOutcome::Success(ffs) => {
                succeeded += 1;
                total_files += ffs.filtered_count;
            }
            SourceOutcome::Failed {
                source_id,
                error: err,
            } => {
                if err == "dry-run" {
                    // dry-run: don't log as error
                } else {
                    failed += 1;
                    error!(source_id, err, "source acquisition failed");
                }
            }
        }

        outcomes.push(outcome);
    }

    let total_sources = outcomes.len();

    // FR-016, FR-018: summary logging
    info!(
        total_sources,
        succeeded, failed, total_files, "acquisition complete"
    );

    AcquisitionResult {
        outcomes,
        total_sources,
        succeeded,
        failed,
        total_files,
    }
}

/// Dispatch a single planned source to the correct acquisition handler.
fn dispatch_planned_source(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    execute_scan_local(planned, acq_plan)
}

/// Scan a local source directory and apply glob filtering.
fn execute_scan_local(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    // Defence-in-depth: the plan loop (`acquire::plan::plan`) only ever
    // constructs a `PlannedSource` for a local, ingestible source, so this
    // is structurally unreachable for a non-local source today — but a
    // variant-safe fail-closed outcome here means a future change to the
    // plan loop cannot silently panic instead of reporting a failure.
    let Some(local) = planned.source.as_local() else {
        return SourceOutcome::Failed {
            source_id: planned.source.id().to_string(),
            error: "source is not a local ingestion source".to_string(),
        };
    };

    let scan_source = LocalSource {
        id: local.id.clone(),
        path: planned.target_dir.clone(),
        include: local.include.clone(),
        exclude: local.exclude.clone(),
        formats: local.formats.clone(),
        database: local.database.clone(),
    };
    let ignored_snapshot_root = (!planned.allow_internal_snapshot_scan)
        .then(|| crate::path::v4_migration_snapshot_dir(&acq_plan.data_root));

    match scan_and_filter(&scan_source, acq_plan, ignored_snapshot_root.as_deref()) {
        Ok(ffs) => SourceOutcome::Success(ffs),
        Err(e) => SourceOutcome::Failed {
            source_id: local.id.clone(),
            error: e.to_string(),
        },
    }
}

/// Scan a local directory then apply include/exclude filtering to produce a [`FilteredFileSet`].
///
/// # RI-001: Path relativization
///
/// [`scan_local_source`] returns **absolute** canonical paths. User-supplied patterns
/// like `docs/**/*.md` must be matched against **root-relative** paths, not absolute
/// ones — otherwise the pattern prefix never aligns with the absolute path components.
///
/// This function strips the canonical source root from every discovered path before
/// passing them to [`filter_files`], then re-maps the filtered relative paths back to
/// their original absolute forms for the returned [`FilteredFileSet`].
fn scan_and_filter(
    source: &LocalSource,
    acq_plan: &AcquisitionPlan,
    ignored_root: Option<&Path>,
) -> Result<FilteredFileSet, GraphtorError> {
    let files =
        local::scan_local_source_with_ignored_root(source, &acq_plan.allowed_root, ignored_root)?;

    // Obtain the canonical source root so strip_prefix is reliable on all platforms.
    let canonical_root = crate::path::validate_path(&source.path, &acq_plan.allowed_root)?;

    // Build (relative, absolute) pairs — relative form is used for glob matching.
    let pairs: Vec<(PathBuf, PathBuf)> = files
        .into_iter()
        .map(|abs| {
            let rel = abs
                .strip_prefix(&canonical_root)
                .map_or_else(|_| abs.clone(), Path::to_path_buf);
            (rel, abs)
        })
        .collect();

    let original_count = pairs.len();
    let rel_only: Vec<PathBuf> = pairs.iter().map(|(r, _)| r.clone()).collect();
    let filtered_rel = filter_files(&rel_only, &source.include, &source.exclude)?;
    let filtered_count = filtered_rel.len();

    // Re-map filtered relative paths back to their original absolute forms.
    let kept: std::collections::HashSet<&PathBuf> = filtered_rel.iter().collect();
    let filtered_abs: Vec<PathBuf> = pairs
        .into_iter()
        .filter(|(rel, _)| kept.contains(rel))
        .map(|(_, abs)| abs)
        .collect();

    if filtered_count == 0 && original_count > 0 {
        warn!(
            source_id = %source.id,
            original_count,
            "filtering removed all files from source"
        );
    }

    Ok(FilteredFileSet {
        source_id: source.id.clone(),
        original_count,
        filtered_count,
        files: filtered_abs,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::config::{LocalSource, Source, SourceConfig};

    #[test]
    fn execute_ignores_internal_v4_migration_snapshot_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let data_root = root.join(".graphtor").join("data");
        fs::create_dir_all(&data_root).expect("create data root");

        let live_file = root.join("guide.md");
        fs::write(&live_file, "# Guide\n").expect("write live markdown");

        let stale_snapshot_file = data_root
            .join("v4-migration-snapshots")
            .join("stale")
            .join("source-0")
            .join("guide.md");
        fs::create_dir_all(
            stale_snapshot_file
                .parent()
                .expect("stale snapshot file should have a parent"),
        )
        .expect("create snapshot dir");
        fs::write(&stale_snapshot_file, "# Stale snapshot\n").expect("write stale snapshot");

        let config = SourceConfig {
            sources: vec![Source::Local(LocalSource {
                id: "workspace-root".to_string(),
                path: root.to_path_buf(),
                include: vec!["**/*.md".to_string()],
                exclude: vec![],
                formats: vec!["md".to_string()],
                database: None,
            })],
        };
        let plan = plan(&config, &data_root, root).expect("build acquisition plan");

        let result = execute(&plan, false);
        assert_eq!(result.failed, 0, "acquisition should succeed: {result:?}");

        let Some(SourceOutcome::Success(filtered)) = result.outcomes.first() else {
            panic!(
                "expected one successful outcome, got: {:?}",
                result.outcomes
            );
        };

        assert_eq!(
            filtered.filtered_count, 1,
            "internal migration snapshots must not be exposed as source content: {filtered:?}"
        );
        assert_eq!(
            filtered.files,
            vec![crate::path::validate_path(&live_file, root).expect("canonical live file")],
            "only the live workspace markdown file should remain after acquisition filtering"
        );
    }
}
