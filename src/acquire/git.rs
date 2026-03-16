//! Git repository acquisition — shallow clone via `git2`.
//!
//! Provides [`clone_git_source`] which clones a Git source using a shallow
//! fetch (depth=1) for remote URLs (FR-001, FR-004). Local `file://` URLs
//! are cloned without a depth limit because `file://` transport does not
//! support shallow fetches on all libgit2 versions and platforms.

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

    // `file://` transport does not support shallow fetches on all libgit2 versions
    // and platforms (error class and message vary by OS). For local URLs (used in
    // test environments only), skip depth entirely and do a full clone.
    // For remote URLs (http/https/ssh), use depth=1 to minimise disk usage (FR-001).
    let is_local_transport = source.url.starts_with("file://");

    let mut builder = git2::build::RepoBuilder::new();
    builder.branch(&source.branch); // FR-004: checkout only the specified branch
    if !is_local_transport {
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.depth(1); // FR-001: shallow clone minimises disk usage and clone time
        builder.fetch_options(fetch_opts);
    }

    let clone_result = builder.clone(&source.url, target_dir);

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
pub(crate) fn git_error_to_pipeline(e: &git2::Error, source_id: &str) -> GraphtorError {
    GraphtorError::Pipeline {
        message: format!("source '{}': {}", source_id, e.message()),
        stage: "acquire".to_string(),
    }
}
