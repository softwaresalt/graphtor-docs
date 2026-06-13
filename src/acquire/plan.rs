//! Acquisition planning and source validation.
//!
//! Provides:
//! - [`plan`]: resolve a [`SourceConfig`] into an [`AcquisitionPlan`] with per-source actions.
//! - [`validate_sources`]: check all sources for configuration errors in a single pass.
//!
//! [`SourceConfig`]: crate::config::SourceConfig
//! [`AcquisitionPlan`]: crate::acquire::result::AcquisitionPlan

use std::path::{Path, PathBuf};

use tracing::info;

use crate::acquire::result::{AcquisitionPlan, PlannedSource, SourceAction, ValidationReport};
use crate::config::SourceConfig;
use crate::error::GraphtorError;

/// Resolve an acquisition plan from a parsed source configuration.
///
/// Examines each source in the config and produces a plan with per-source actions.
/// Auto-creates `data_root` if it does not yet exist (FR-021).
///
/// # Errors
///
/// Returns [`GraphtorError::Io`] if `data_root` cannot be created.
/// Returns [`GraphtorError::PathViolation`] if any source path escapes `allowed_root`.
pub fn plan(
    config: &SourceConfig,
    data_root: &Path,
    allowed_root: &Path,
) -> Result<AcquisitionPlan, GraphtorError> {
    // Validate data_root against allowed_root BEFORE any I/O. validate_path() handles
    // non-existent paths by resolving to the deepest existing ancestor, so the security
    // check runs without needing the directory to exist first.
    let pre_validated = crate::path::validate_path(data_root, allowed_root)?;

    // FR-021: create data root now that the path is confirmed within allowed_root.
    std::fs::create_dir_all(&pre_validated).map_err(GraphtorError::Io)?;

    // Re-canonicalize after creation to expand Windows 8.3 short path names.
    let canonical_data_root =
        crate::path::canonicalize_clean(&pre_validated).map_err(GraphtorError::Io)?;

    let mut sources: Vec<PlannedSource> = Vec::new();
    let mut total_scan: usize = 0;

    for source in &config.sources {
        let target_dir = resolve_source_dir(source, allowed_root)?;
        total_scan += 1;
        sources.push(PlannedSource {
            source: source.clone(),
            action: SourceAction::ScanLocal,
            target_dir,
            allow_internal_snapshot_scan: false,
        });
    }

    info!(
        total_scan,
        data_root = %canonical_data_root.display(),
        "acquisition plan ready"
    );

    // Canonicalize allowed_root for consistent storage alongside the canonical data_root (RI-005).
    let canonical_allowed_root =
        crate::path::canonicalize_clean(allowed_root).map_err(GraphtorError::Io)?;

    Ok(AcquisitionPlan {
        data_root: canonical_data_root,
        allowed_root: canonical_allowed_root,
        sources,
        total_scan,
    })
}

/// Validate all source definitions without performing acquisition.
///
/// Checks path existence and glob syntax. Collects ALL errors across all sources
/// in a single pass.
///
/// This function is intentionally infallible — it returns a [`ValidationReport`]
/// that may contain zero or more errors. Call [`ValidationReport::is_valid`] to
/// check the overall outcome.
#[must_use]
pub fn validate_sources(config: &SourceConfig, allowed_root: &Path) -> ValidationReport {
    let mut errors: Vec<crate::acquire::result::ValidationError> = Vec::new();
    let total_count = config.sources.len();

    for source in &config.sources {
        let crate::config::Source::Local(local) = source;

        if local.path.exists() {
            // FR-017: path security — must be within allowed_root
            if let Err(e) = crate::path::validate_path(&local.path, allowed_root) {
                errors.push(crate::acquire::result::ValidationError {
                    source_id: local.id.clone(),
                    field: "path".to_string(),
                    message: e.to_string(),
                });
            }
        } else {
            // FR-013: path must exist on disk
            errors.push(crate::acquire::result::ValidationError {
                source_id: local.id.clone(),
                field: "path".to_string(),
                message: format!("path does not exist: '{}'", local.path.display()),
            });
        }
        // FR-014: glob patterns
        validate_globs(&local.id, "include", &local.include, &mut errors);
        validate_globs(&local.id, "exclude", &local.exclude, &mut errors);
        // FR-021.002: format allow-list (md/markdown only after docline pivot)
        validate_format_list(&local.id, &local.formats, &mut errors);
    }

    // A source is "valid" if it produced no errors
    let errored_ids: std::collections::HashSet<&str> =
        errors.iter().map(|e| e.source_id.as_str()).collect();
    let valid_count = total_count.saturating_sub(errored_ids.len());

    ValidationReport {
        errors,
        valid_count,
        total_count,
    }
}

// ── Private helpers ────────────────────────────────────────────────────────────

/// Determine the resolved target directory for a local source.
fn resolve_source_dir(
    source: &crate::config::Source,
    allowed_root: &Path,
) -> Result<PathBuf, GraphtorError> {
    let crate::config::Source::Local(local) = source;
    crate::path::validate_path(&local.path, allowed_root)
}

/// Collect [`ValidationError`]s for any invalid glob patterns in `patterns`.
fn validate_globs(
    source_id: &str,
    field: &str,
    patterns: &[String],
    errors: &mut Vec<crate::acquire::result::ValidationError>,
) {
    for pattern in patterns {
        if globset::Glob::new(pattern).is_err() {
            errors.push(crate::acquire::result::ValidationError {
                source_id: source_id.to_string(),
                field: field.to_string(),
                message: format!("invalid glob pattern: '{pattern}'"),
            });
        }
    }
}

/// Collect [`ValidationError`]s for any format strings not in the supported Markdown list.
///
/// Only `"md"` and `"markdown"` are valid after the docline pivot.
fn validate_format_list(
    source_id: &str,
    formats: &[String],
    errors: &mut Vec<crate::acquire::result::ValidationError>,
) {
    const VALID: &[&str] = &["md", "markdown"];
    for fmt in formats {
        let normalised = fmt.to_ascii_lowercase();
        if !VALID.contains(&normalised.as_str()) {
            errors.push(crate::acquire::result::ValidationError {
                source_id: source_id.to_string(),
                field: "formats".to_string(),
                message: format!(
                    "invalid format '{fmt}'; valid formats are: {}",
                    VALID.join(", ")
                ),
            });
        }
    }
}
