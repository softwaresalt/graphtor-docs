//! Git repository acquisition — shallow clone via `git2`.
//!
//! Provides [`clone_git_source`] which clones a Git source using a shallow
//! fetch (depth=1) and single-branch checkout strategy (FR-001, FR-004).

use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::{config::GitSource, error::GraphtorError};

/// Clone a Git source repository with a shallow fetch (depth=1).
///
/// Skips the clone and returns `Ok(target_dir)` if `target_dir/.git` already
/// exists, making this operation idempotent (FR-003).
///
/// On failure, any partial clone directory is removed so that a subsequent
/// retry starts cleanly (FR-001).
///
/// # Errors
///
/// Returns [`GraphtorError::Pipeline`] when the clone fails due to an
/// unreachable URL, authentication failure, or non-existent branch.
pub fn clone_git_source(source: &GitSource, target_dir: &Path) -> Result<PathBuf, GraphtorError> {
    // FR-003: skip if the repository has already been cloned here.
    if target_dir.join(".git").exists() {
        warn!(
            source_id = %source.id,
            target = %target_dir.display(),
            "git clone skipped — target already exists"
        );
        return Ok(target_dir.to_path_buf());
    }

    info!(
        source_id = %source.id,
        url = %source.url,
        branch = %source.branch,
        "cloning git source"
    );

    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.depth(1); // FR-001: shallow clone minimises disk usage and clone time

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);
    builder.branch(&source.branch); // FR-004: checkout only the specified branch

    // Attempt a shallow clone first. Fall back to a full clone when the transport
    // does not support depth (e.g., local file:// URLs in test environments).
    let clone_result = builder.clone(&source.url, target_dir).or_else(|e| {
        if e.class() == git2::ErrorClass::Net && e.message().contains("shallow") {
            warn!(
                source_id = %source.id,
                "shallow clone not supported by transport; falling back to full clone"
            );
            // Clean up the partial directory from the first attempt before retrying.
            if target_dir.exists() {
                let _ = std::fs::remove_dir_all(target_dir);
            }
            let mut builder2 = git2::build::RepoBuilder::new();
            builder2.branch(&source.branch);
            builder2.clone(&source.url, target_dir)
        } else {
            Err(e)
        }
    });

    match clone_result {
        Ok(_repo) => {
            info!(
                source_id = %source.id,
                path = %target_dir.display(),
                "cloned git source successfully"
            );
            Ok(target_dir.to_path_buf())
        }
        Err(e) => {
            // FR-001: remove any partial clone directory so the next retry
            // does not mistake it for a completed clone.
            if target_dir.exists() {
                if let Err(rm_err) = std::fs::remove_dir_all(target_dir) {
                    warn!(
                        source_id = %source.id,
                        remove_error = %rm_err,
                        "could not remove partial clone directory"
                    );
                }
            }
            error!(
                source_id = %source.id,
                url = %source.url,
                git_error = %e.message(),
                "git clone failed"
            );
            Err(git_error_to_pipeline(&e, &source.id))
        }
    }
}

/// Map a [`git2::Error`] to a [`GraphtorError::Pipeline`] for the acquire stage.
///
/// Embeds the source ID in the error message so callers can identify which
/// source caused the failure.
#[must_use]
pub fn git_error_to_pipeline(e: &git2::Error, source_id: &str) -> GraphtorError {
    GraphtorError::Pipeline {
        message: format!("source '{}': {}", source_id, e.message()),
        stage: "acquire".to_string(),
    }
}
