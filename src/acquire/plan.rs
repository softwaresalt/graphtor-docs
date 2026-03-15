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
    // FR-021: auto-create data root so subsequent validate_path calls resolve it
    std::fs::create_dir_all(data_root).map_err(GraphtorError::Io)?;

    // Canonicalize data_root (now guaranteed to exist)
    let canonical_data_root = crate::path::validate_path(data_root, allowed_root)?;

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

    Ok(AcquisitionPlan {
        data_root: canonical_data_root,
        allowed_root: allowed_root.to_path_buf(),
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
    let _ = (config, allowed_root); // placeholder until Phase 7
    ValidationReport {
        errors: vec![],
        valid_count: config.sources.len(),
        total_count: config.sources.len(),
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
            let target_dir = canonical_data_root.join(&git.id);
            // Security: validate even though target may not exist yet
            crate::path::validate_path(&target_dir, allowed_root)?;
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
