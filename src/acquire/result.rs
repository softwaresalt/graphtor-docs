//! Result types for the source acquisition pipeline.
//!
//! All data structures produced and consumed by the acquisition stages
//! are defined here to avoid circular module dependencies.

use std::path::PathBuf;

use crate::config::Source;

/// Resolved action for a single source in the acquisition plan.
///
/// Determines what the executor must do with each source entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAction {
    /// Local source needs to be recursively scanned.
    ScanLocal,
}

impl std::fmt::Display for SourceAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScanLocal => f.write_str("ScanLocal"),
        }
    }
}

/// Distinguishes the kind of documentation source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    /// Scanned from a local filesystem directory.
    Local,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => f.write_str("Local"),
        }
    }
}

/// A single source with its resolved action, ready for execution.
///
/// Produced by [`crate::acquire::plan::plan`] from a [`SourceConfig`].
///
/// [`SourceConfig`]: crate::config::SourceConfig
#[derive(Debug, Clone)]
pub struct PlannedSource {
    /// The original source definition from `sources.yaml`.
    pub source: Source,
    /// What the executor must do with this source.
    pub action: SourceAction,
    /// Resolved local filesystem path for this source.
    pub target_dir: PathBuf,
    /// Whether this planned source may intentionally scan the internal
    /// `data_root/v4-migration-snapshots` subtree.
    ///
    /// Normal configured sources must leave this disabled. Only the staged v4
    /// migration rebuild plan enables it for its frozen snapshot inputs.
    pub allow_internal_snapshot_scan: bool,
}

/// The full plan of actions across all configured sources.
///
/// Produced by [`crate::acquire::plan::plan`] before any I/O is performed.
#[derive(Debug, Clone)]
pub struct AcquisitionPlan {
    /// Resolved data root directory (auto-created if missing).
    pub data_root: PathBuf,
    /// Boundary root used for path security checks during execution.
    pub allowed_root: PathBuf,
    /// Ordered list of sources with their resolved actions.
    pub sources: Vec<PlannedSource>,
    /// Number of local sources that will be scanned.
    pub total_scan: usize,
}

/// A single source after successful acquisition.
///
/// Contains all files discovered before glob filtering is applied.
#[derive(Debug, Clone)]
pub struct AcquiredSource {
    /// Source identifier from the configuration.
    pub source_id: String,
    /// Source kind — always `Local` after the docline pivot.
    pub source_type: SourceType,
    /// Local directory containing the acquired files.
    pub local_dir: PathBuf,
    /// All files discovered, before any include/exclude filtering.
    pub discovered_files: Vec<PathBuf>,
}

/// Files from a source after include/exclude glob filtering.
///
/// Produced by [`crate::acquire::filter::filter_files`].
#[derive(Debug, Clone)]
pub struct FilteredFileSet {
    /// Source identifier.
    pub source_id: String,
    /// Number of files before filtering.
    pub original_count: usize,
    /// Number of files after filtering.
    pub filtered_count: usize,
    /// Selected file paths as absolute canonical paths.
    ///
    /// Paths are in the same absolute form as [`crate::acquire::scan_local_source`]
    /// produces and are suitable for direct use with `std::fs::read` and downstream
    /// pipeline stages. The path-relativization applied during glob matching is an
    /// internal implementation detail and does not affect the paths stored here.
    pub files: Vec<PathBuf>,
}

/// Per-source outcome after the acquisition attempt.
#[derive(Debug, Clone)]
pub enum SourceOutcome {
    /// Source was acquired and filtered successfully.
    Success(FilteredFileSet),
    /// Acquisition failed — other sources continued.
    Failed {
        /// The source identifier that failed.
        source_id: String,
        /// Human-readable error description.
        error: String,
    },
}

impl SourceOutcome {
    /// Return the source identifier regardless of outcome variant.
    #[must_use]
    pub fn source_id(&self) -> &str {
        match self {
            Self::Success(f) => &f.source_id,
            Self::Failed { source_id, .. } => source_id,
        }
    }
}

/// Aggregate result of the full acquisition process across all sources.
///
/// Returned by [`crate::acquire::execute`] after processing all planned sources.
#[derive(Debug, Clone)]
pub struct AcquisitionResult {
    /// Per-source results in the order sources were processed.
    pub outcomes: Vec<SourceOutcome>,
    /// Total number of sources attempted.
    pub total_sources: usize,
    /// Sources successfully acquired (scanned).
    pub succeeded: usize,
    /// Sources that failed with an error.
    pub failed: usize,
    /// Total files available after filtering across all successful sources.
    pub total_files: usize,
}

/// A single validation error for one source field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Source identifier that has the error.
    pub source_id: String,
    /// Field name that failed validation (e.g., `"path"`, `"include"`).
    pub field: String,
    /// Human-readable description of the validation failure.
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] field '{}': {}",
            self.source_id, self.field, self.message
        )
    }
}

/// Aggregate validation results for all sources.
///
/// Produced by [`crate::acquire::plan::validate_sources`] in a single pass
/// so all errors are reported together rather than stopping at the first failure.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// All validation errors found across all sources.
    pub errors: Vec<ValidationError>,
    /// Number of sources that passed validation.
    pub valid_count: usize,
    /// Total number of sources checked.
    pub total_count: usize,
}

impl ValidationReport {
    /// Return `true` if no validation errors were found.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SourceAction display and equality ────────────────────────────────

    #[test]
    fn source_action_scan_local_display() {
        assert_eq!(SourceAction::ScanLocal.to_string(), "ScanLocal");
    }

    #[test]
    fn source_action_equality() {
        assert_eq!(SourceAction::ScanLocal, SourceAction::ScanLocal);
    }

    // ── SourceType display and equality ──────────────────────────────────

    #[test]
    fn source_type_local_display() {
        assert_eq!(SourceType::Local.to_string(), "Local");
    }

    #[test]
    fn source_type_equality() {
        assert_eq!(SourceType::Local, SourceType::Local);
    }

    // ── SourceOutcome::source_id ─────────────────────────────────────────

    #[test]
    fn source_outcome_success_returns_source_id() {
        let ffs = FilteredFileSet {
            source_id: "docs-azure".to_string(),
            original_count: 10,
            filtered_count: 5,
            files: vec![],
        };
        let outcome = SourceOutcome::Success(ffs);
        assert_eq!(outcome.source_id(), "docs-azure");
    }

    #[test]
    fn source_outcome_failed_returns_source_id() {
        let outcome = SourceOutcome::Failed {
            source_id: "docs-azure".to_string(),
            error: "network error".to_string(),
        };
        assert_eq!(outcome.source_id(), "docs-azure");
    }

    // ── ValidationError display ──────────────────────────────────────────

    #[test]
    fn validation_error_display_includes_all_fields() {
        let e = ValidationError {
            source_id: "my-source".to_string(),
            field: "path".to_string(),
            message: "path not found".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("my-source"), "missing source_id: {s}");
        assert!(s.contains("path"), "missing field: {s}");
        assert!(s.contains("path not found"), "missing message: {s}");
    }

    #[test]
    fn validation_error_equality() {
        let a = ValidationError {
            source_id: "s".to_string(),
            field: "f".to_string(),
            message: "m".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // ── ValidationReport::is_valid ───────────────────────────────────────

    #[test]
    fn validation_report_is_valid_when_no_errors() {
        let report = ValidationReport {
            errors: vec![],
            valid_count: 3,
            total_count: 3,
        };
        assert!(report.is_valid());
    }

    #[test]
    fn validation_report_is_not_valid_when_errors_present() {
        let report = ValidationReport {
            errors: vec![ValidationError {
                source_id: "s".to_string(),
                field: "path".to_string(),
                message: "bad path".to_string(),
            }],
            valid_count: 2,
            total_count: 3,
        };
        assert!(!report.is_valid());
    }

    // ── FilteredFileSet construction ─────────────────────────────────────

    #[test]
    fn filtered_file_set_fields_accessible() {
        let ffs = FilteredFileSet {
            source_id: "s".to_string(),
            original_count: 20,
            filtered_count: 10,
            files: vec![PathBuf::from("a.md"), PathBuf::from("b.md")],
        };
        assert_eq!(ffs.source_id, "s");
        assert_eq!(ffs.original_count, 20);
        assert_eq!(ffs.filtered_count, 10);
        assert_eq!(ffs.files.len(), 2);
    }

    // ── AcquisitionResult construction ───────────────────────────────────

    #[test]
    fn acquisition_result_fields_accessible() {
        let result = AcquisitionResult {
            outcomes: vec![],
            total_sources: 5,
            succeeded: 4,
            failed: 1,
            total_files: 150,
        };
        assert_eq!(result.total_sources, 5);
        assert_eq!(result.succeeded + result.failed, 5);
    }
}
