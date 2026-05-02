//! Source acquisition module — clone Git repositories, scan local directories, and crawl URLs.
//!
//! This module orchestrates the full source acquisition pipeline:
//! planning what to acquire, executing clones, scans, and URL crawls, applying glob filtering,
//! and collecting results into an [`AcquisitionResult`].
//!
//! # Pipeline stages
//!
//! ```text
//! SourceConfig → plan() → AcquisitionPlan → execute() → AcquisitionResult
//!                                                  ↳ clone_git_source()   (per Git source)
//!                                                  ↳ scan_local_source()  (per local source)
//!                                                  ↳ crawl_url_source()   (per URL source)
//!                                                  ↳ apply_source_filter() (per acquired source)
//! ```

use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::config::source::{LocalSource, Source, UrlSource};
use crate::error::GraphtorError;

pub mod filter;
pub mod git;
pub mod local;
pub mod plan;
pub mod result;
pub mod url;

pub use filter::filter_files;
pub use git::clone_git_source;
pub use local::scan_local_source;
pub use plan::{plan, validate_sources};
pub use result::{
    AcquiredSource, AcquisitionPlan, AcquisitionResult, FilteredFileSet, PlannedSource,
    SourceAction, SourceOutcome, SourceType, ValidationError, ValidationReport,
};
pub use url::crawl_url_source;

/// Apply include/exclude glob patterns to a file list and wrap the result in a [`FilteredFileSet`].
///
/// This is the pipeline integration point for US3: after acquisition (clone or scan),
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

/// Execute an acquisition plan: clone Git repos, scan local dirs, filter files.
///
/// Processes each source in the plan. Failures in one source do not stop
/// processing of others. Returns an aggregate result with per-source outcomes.
/// Emits a summary INFO log on completion (FR-016, FR-018).
///
/// When `dry_run` is `true`, no filesystem or network operations are performed.
/// All sources are reported as skipped with zero files (FR-019).
#[must_use]
pub fn execute(acq_plan: &AcquisitionPlan, dry_run: bool) -> AcquisitionResult {
    let mut outcomes: Vec<SourceOutcome> = Vec::new();
    let mut succeeded: usize = 0;
    let mut skipped: usize = 0;
    let mut failed: usize = 0;
    let mut total_files: usize = 0;

    for planned in &acq_plan.sources {
        let outcome = if dry_run {
            // FR-019: dry-run — report the plan without performing any I/O
            info!(source_id = %planned.source.id(), action = %planned.action, "dry-run: skipping");
            SourceOutcome::Skipped {
                source_id: planned.source.id().to_string(),
            }
        } else {
            dispatch_planned_source(planned, acq_plan)
        };

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
        SourceAction::CrawlUrl => execute_crawl_url(planned, acq_plan),
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

/// Crawl a URL source and apply glob filtering to the downloaded Markdown pages.
fn execute_crawl_url(planned: &PlannedSource, acq_plan: &AcquisitionPlan) -> SourceOutcome {
    let Source::Url(url_src) = &planned.source else {
        return SourceOutcome::Failed {
            source_id: planned.source.id().to_string(),
            error: "internal: CrawlUrl action on non-url source".to_string(),
        };
    };

    match do_crawl_url(url_src, planned, acq_plan) {
        Ok(ffs) => SourceOutcome::Success(ffs),
        Err(e) => SourceOutcome::Failed {
            source_id: url_src.id.clone(),
            error: e.to_string(),
        },
    }
}

/// Inner fallible body for [`execute_crawl_url`].
fn do_crawl_url(
    url_src: &UrlSource,
    planned: &PlannedSource,
    _acq_plan: &AcquisitionPlan,
) -> Result<FilteredFileSet, GraphtorError> {
    let crawled_files = crawl_url_source(url_src, &planned.target_dir)?;

    let original_count = crawled_files.len();

    // Re-use apply_source_filter: absolute paths from the crawler are matched against
    // include/exclude patterns the same way local-source files are.
    let ffs = apply_source_filter(
        &url_src.id,
        &crawled_files,
        &url_src.include,
        &url_src.exclude,
    )?;

    if ffs.filtered_count == 0 && original_count > 0 {
        warn!(
            source_id = %url_src.id,
            original_count,
            "url crawl filtering removed all files from source"
        );
    }

    Ok(ffs)
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
) -> Result<FilteredFileSet, GraphtorError> {
    let files = scan_local_source(source, &acq_plan.allowed_root)?;

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
