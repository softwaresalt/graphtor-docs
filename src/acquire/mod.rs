//! Source acquisition module — clone Git repositories and scan local directories.
//!
//! This module orchestrates the full source acquisition pipeline:
//! planning what to acquire, executing clones and scans, applying glob filtering,
//! and collecting results into an [`AcquisitionResult`].
//!
//! # Pipeline stages
//!
//! ```text
//! SourceConfig → plan() → AcquisitionPlan → execute() → AcquisitionResult
//!                                                  ↳ clone_git_source()  (per Git source)
//!                                                  ↳ scan_local_source() (per local source)
//!                                                  ↳ apply_source_filter() (per acquired source)
//! ```

use std::path::PathBuf;

use tracing::{error, info, warn};

use crate::config::source::{LocalSource, Source};
use crate::error::GraphtorError;

pub mod filter;
pub mod git;
pub mod local;
pub mod plan;
pub mod result;

pub use filter::filter_files;
pub use git::clone_git_source;
pub use local::scan_local_source;
pub use plan::{plan, validate_sources};
pub use result::{
    AcquiredSource, AcquisitionPlan, AcquisitionResult, FilteredFileSet, PlannedSource,
    SourceAction, SourceOutcome, SourceType, ValidationError, ValidationReport,
};

/// Apply include/exclude glob patterns to a file list and wrap the result in a [`FilteredFileSet`].
///
/// This is the pipeline integration point for US3: after acquisition (clone or scan),
/// call this function to produce the `FilteredFileSet` used in [`SourceOutcome::Success`].
///
/// `original_count` is set to `files.len()` before filtering; `filtered_count` reflects
/// the number of files that survived the include/exclude pass.
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

/// Execute an acquisition plan: clone Git repos, scan local dirs, filter files.
///
/// Processes each source in the plan. Failures in one source do not stop
/// processing of others. Returns an aggregate result with per-source outcomes.
/// Emits a summary INFO log on completion (FR-016, FR-018).
#[must_use]
pub fn execute(acq_plan: &AcquisitionPlan) -> AcquisitionResult {
    let mut outcomes: Vec<SourceOutcome> = Vec::new();
    let mut succeeded: usize = 0;
    let mut skipped: usize = 0;
    let mut failed: usize = 0;
    let mut total_files: usize = 0;

    for planned in &acq_plan.sources {
        let outcome = dispatch_planned_source(planned, acq_plan);

        match &outcome {
            SourceOutcome::Success(ffs) => {
                succeeded += 1;
                total_files += ffs.filtered_count;
            }
            SourceOutcome::Skipped { .. } => skipped += 1,
            SourceOutcome::Failed {
                source_id,
                error: err,
            } => {
                failed += 1;
                error!(source_id, err, "source acquisition failed");
            }
        }

        outcomes.push(outcome);
    }

    let total_sources = outcomes.len();

    // FR-016, FR-018: summary logging
    info!(
        total_sources,
        succeeded, skipped, failed, total_files, "acquisition complete"
    );

    AcquisitionResult {
        outcomes,
        total_sources,
        succeeded,
        skipped,
        failed,
        total_files,
    }
}

/// Dispatch a single planned source to the correct acquisition handler.
fn dispatch_planned_source(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    match &planned.action {
        SourceAction::SkipGit => {
            info!(source_id = %planned.source.id(), "skipping already-cloned git source");
            SourceOutcome::Skipped {
                source_id: planned.source.id().to_string(),
            }
        }
        SourceAction::CloneGit => execute_clone_git(planned, acq_plan),
        SourceAction::ScanLocal => execute_scan_local(planned, acq_plan),
    }
}

/// Clone a Git source and apply glob filtering to the cloned content.
fn execute_clone_git(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    let Source::Git(git) = &planned.source else {
        return SourceOutcome::Failed {
            source_id: planned.source.id().to_string(),
            error: "internal: CloneGit action on non-git source".to_string(),
        };
    };

    if let Err(e) = clone_git_source(git, &planned.target_dir) {
        return SourceOutcome::Failed {
            source_id: git.id.clone(),
            error: e.to_string(),
        };
    }

    // After clone: scan the cloned directory
    let scan_source = LocalSource {
        id: git.id.clone(),
        path: planned.target_dir.clone(),
        include: git.include.clone(),
        exclude: git.exclude.clone(),
    };

    match scan_and_filter(&scan_source, acq_plan) {
        Ok(ffs) => SourceOutcome::Success(ffs),
        Err(e) => SourceOutcome::Failed {
            source_id: git.id.clone(),
            error: e.to_string(),
        },
    }
}

/// Scan a local source directory and apply glob filtering.
fn execute_scan_local(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    let Source::Local(local) = &planned.source else {
        return SourceOutcome::Failed {
            source_id: planned.source.id().to_string(),
            error: "internal: ScanLocal action on non-local source".to_string(),
        };
    };

    let scan_source = LocalSource {
        id: local.id.clone(),
        path: planned.target_dir.clone(),
        include: local.include.clone(),
        exclude: local.exclude.clone(),
    };

    match scan_and_filter(&scan_source, acq_plan) {
        Ok(ffs) => SourceOutcome::Success(ffs),
        Err(e) => SourceOutcome::Failed {
            source_id: local.id.clone(),
            error: e.to_string(),
        },
    }
}

/// Scan a local directory then apply include/exclude filtering to produce a [`FilteredFileSet`].
fn scan_and_filter(
    source: &LocalSource,
    acq_plan: &AcquisitionPlan,
) -> Result<FilteredFileSet, GraphtorError> {
    let files = scan_local_source(source, &acq_plan.allowed_root)?;
    let ffs = apply_source_filter(&source.id, &files, &source.include, &source.exclude)?;
    if ffs.filtered_count == 0 && ffs.original_count > 0 {
        warn!(
            source_id = %source.id,
            original_count = ffs.original_count,
            "filtering removed all files from source"
        );
    }
    Ok(ffs)
}
