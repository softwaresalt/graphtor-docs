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

use crate::error::GraphtorError;

pub mod filter;
pub mod git;
pub mod local;
pub mod plan;
pub mod result;

pub use filter::filter_files;
pub use git::clone_git_source;
pub use local::scan_local_source;
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
