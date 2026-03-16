//! Acquisition planning and source validation.
//!
//! Provides:
//! - [`plan`]: resolve a [`SourceConfig`] into an [`AcquisitionPlan`] with per-source actions.
//! - [`validate_sources`]: check all sources for configuration errors in a single pass (FR-011–FR-014).
//!
//! [`SourceConfig`]: crate::config::SourceConfig
//! [`AcquisitionPlan`]: crate::acquire::result::AcquisitionPlan

use std::path::{Path, PathBuf};

use tracing::info;

use crate::acquire::result::{AcquisitionPlan, PlannedSource, SourceAction, ValidationReport};
use crate::config::source::Source;
use crate::config::SourceConfig;
use crate::error::GraphtorError;

/// Resolve an acquisition plan from a parsed source configuration.
///
/// Examines each source in the config, checks whether a local directory
/// already exists for Git sources, and produces a plan with per-source actions.
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
    let mut total_clone: usize = 0;
    let mut total_skip: usize = 0;
    let mut total_scan: usize = 0;

    for source in &config.sources {
        let (action, target_dir) =
            resolve_source_action(source, &canonical_data_root, allowed_root)?;
        match action {
            SourceAction::CloneGit => total_clone += 1,
            SourceAction::SkipGit => total_skip += 1,
            SourceAction::ScanLocal => total_scan += 1,
        }
        sources.push(PlannedSource {
            source: source.clone(),
            action,
            target_dir,
        });
    }

    info!(
        total_clone,
        total_skip,
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
        total_clone,
        total_skip,
        total_scan,
    })
}

/// Validate all source definitions without performing acquisition.
///
/// Checks URL format (Git), path existence (local), and glob syntax.
/// Collects ALL errors across all sources in a single pass.
///
/// This function is intentionally infallible — it returns a [`ValidationReport`]
/// that may contain zero or more errors. Call [`ValidationReport::is_valid`] to
/// check the overall outcome.
#[must_use]
pub fn validate_sources(config: &SourceConfig, allowed_root: &Path) -> ValidationReport {
    let mut errors: Vec<crate::acquire::result::ValidationError> = Vec::new();
    let total_count = config.sources.len();

    for source in &config.sources {
        match source {
            Source::Git(git) => {
                // FR-011, FR-012: URL format
                if !is_valid_git_url(&git.url) {
                    errors.push(crate::acquire::result::ValidationError {
                        source_id: git.id.clone(),
                        field: "url".to_string(),
                        message: format!("invalid URL format: '{}'", git.url),
                    });
                }
                // FR-014: glob patterns
                validate_globs(&git.id, "include", &git.include, &mut errors);
                validate_globs(&git.id, "exclude", &git.exclude, &mut errors);
            }
            Source::Local(local) => {
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
            }
        }
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

/// Determine the [`SourceAction`] and resolved target directory for one source.
fn resolve_source_action(
    source: &Source,
    canonical_data_root: &Path,
    allowed_root: &Path,
) -> Result<(SourceAction, PathBuf), GraphtorError> {
    match source {
        Source::Git(git) => {
            // Security: validate and use the canonical form so IDs containing `..` or
            // path separators resolve to a deterministic, auditable location (RI-002).
            let target_dir =
                crate::path::validate_path(&canonical_data_root.join(&git.id), allowed_root)?;
            let action = if target_dir.join(".git").exists() {
                SourceAction::SkipGit
            } else {
                SourceAction::CloneGit
            };
            Ok((action, target_dir))
        }
        Source::Local(local) => {
            let canonical_local = crate::path::validate_path(&local.path, allowed_root)?;
            Ok((SourceAction::ScanLocal, canonical_local))
        }
    }
}

/// Return `true` if `url` is a valid Git remote URL (HTTPS or SSH format).
///
/// Accepts:
/// - `https://...` — HTTPS URLs with a scheme and host (FR-011)
/// - `git@host:path` — SSH URLs (FR-012)
fn is_valid_git_url(url: &str) -> bool {
    is_valid_https_url(url) || is_valid_ssh_url(url)
}

/// Return `true` if `url` is a valid HTTPS URL with host present (FR-011).
fn is_valid_https_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        !rest.is_empty() && !rest.starts_with('/')
    } else {
        false
    }
}

/// Return `true` if `url` matches the `git@host:path` SSH format (FR-012).
fn is_valid_ssh_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some(colon_pos) = rest.find(':') {
            let host = &rest[..colon_pos];
            let path = &rest[colon_pos + 1..];
            !host.is_empty() && !path.is_empty()
        } else {
            false
        }
    } else {
        false
    }
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
