//! Local directory scanning — recursive file discovery via `walkdir`.
//!
//! Provides [`scan_local_source`] which recursively walks a local directory,
//! collecting all regular files in deterministic sort order (FR-005).

use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::config::source::LocalSource;
use crate::error::GraphtorError;

/// Recursively scan a local directory and return all regular file paths.
///
/// Symlinks are not followed. Results are sorted for deterministic ordering.
/// The source path must be within `allowed_root`; otherwise a
/// [`GraphtorError::PathViolation`] is returned (FR-017).
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if `source.path` escapes `allowed_root`.
/// Returns [`GraphtorError::Pipeline`] if the path is not a directory or any
/// directory entry cannot be read.
pub fn scan_local_source(
    source: &LocalSource,
    allowed_root: &Path,
) -> Result<Vec<PathBuf>, GraphtorError> {
    let canonical_path = crate::path::validate_path(&source.path, allowed_root)?;

    if !canonical_path.is_dir() {
        return Err(GraphtorError::Pipeline {
            stage: "acquire".to_string(),
            message: format!(
                "source '{}': path is not a directory: {}",
                source.id,
                canonical_path.display()
            ),
        });
    }

    info!(
        source_id = %source.id,
        path = %canonical_path.display(),
        "scanning local source"
    );

    let mut files: Vec<PathBuf> = Vec::new();

    for entry in walkdir::WalkDir::new(&canonical_path).follow_links(false) {
        match entry {
            Ok(e) => {
                if e.file_type().is_file() {
                    let path = e.into_path();
                    debug!(source_id = %source.id, file = %path.display(), "discovered file");
                    files.push(path);
                }
            }
            Err(e) => {
                return Err(GraphtorError::Pipeline {
                    stage: "acquire".to_string(),
                    message: format!("source '{}': {e}", source.id),
                });
            }
        }
    }

    files.sort();

    info!(
        source_id = %source.id,
        count = files.len(),
        "local scan complete"
    );

    Ok(files)
}
