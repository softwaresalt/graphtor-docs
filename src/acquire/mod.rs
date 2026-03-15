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
//!                                                  ↳ filter_files()      (per acquired source)
//! ```

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
