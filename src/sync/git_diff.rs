//! Git commit-based file change detection.
//!
//! Uses the `git2` crate to compute a tree-to-tree diff between the last
//! processed commit and current `HEAD`, returning the set of added, modified,
//! and deleted Markdown files.

use std::path::{Path, PathBuf};

use git2::Repository;
use tracing::debug;

use crate::error::GraphtorError;

/// The set of Markdown files that changed between two git tree states.
#[derive(Debug, Clone, Default)]
pub struct ChangedFiles {
    /// Paths of Markdown files added since the last sync.
    pub added: Vec<PathBuf>,
    /// Paths of Markdown files modified since the last sync.
    pub modified: Vec<PathBuf>,
    /// Paths of Markdown files deleted since the last sync.
    pub deleted: Vec<PathBuf>,
}

impl ChangedFiles {
    /// Returns `true` if no files changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    /// Total count of changed files across all categories.
    #[must_use]
    pub fn total(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }
}

/// Compute which Markdown files changed between `old_commit_oid` and the
/// current `HEAD` in the repository at `repo_path`.
///
/// Only `.md` files (case-insensitive) are included in the result.
///
/// When `old_commit_oid` is `None` (first-time sync), all Markdown files
/// present in `HEAD` are returned as `added`.
///
/// # Errors
///
/// Returns [`GraphtorError::Pipeline`] if the repository cannot be opened,
/// `HEAD` cannot be resolved, the stored commit cannot be found, or the diff
/// cannot be computed.
pub fn compute_git_diff(
    repo_path: &Path,
    old_commit_oid: Option<&str>,
) -> Result<ChangedFiles, GraphtorError> {
    let repo = Repository::open(repo_path).map_err(|e| GraphtorError::Pipeline {
        message: format!(
            "failed to open git repository at {}: {e}",
            repo_path.display()
        ),
        stage: "git_diff".to_string(),
    })?;

    let head_commit = resolve_head(&repo)?;
    let head_tree = head_commit.tree().map_err(|e| GraphtorError::Pipeline {
        message: format!("failed to get HEAD tree: {e}"),
        stage: "git_diff".to_string(),
    })?;

    let Some(old_oid_str) = old_commit_oid else {
        // First sync: every .md file in HEAD is "added".
        return all_md_files_as_added(&repo, &head_tree);
    };

    let old_oid = git2::Oid::from_str(old_oid_str).map_err(|e| GraphtorError::Pipeline {
        message: format!("invalid commit OID '{old_oid_str}': {e}"),
        stage: "git_diff".to_string(),
    })?;

    let old_commit = repo
        .find_commit(old_oid)
        .map_err(|e| GraphtorError::Pipeline {
            message: format!("failed to find stored commit '{old_oid_str}': {e}"),
            stage: "git_diff".to_string(),
        })?;

    let old_tree = old_commit.tree().map_err(|e| GraphtorError::Pipeline {
        message: format!("failed to get tree for commit '{old_oid_str}': {e}"),
        stage: "git_diff".to_string(),
    })?;

    let diff = repo
        .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
        .map_err(|e| GraphtorError::Pipeline {
            message: format!("failed to compute tree-to-tree diff: {e}"),
            stage: "git_diff".to_string(),
        })?;

    let mut result = ChangedFiles::default();

    diff.foreach(
        &mut |delta, _| {
            let new_path = delta.new_file().path().map(PathBuf::from);
            let old_path = delta.old_file().path().map(PathBuf::from);

            match delta.status() {
                git2::Delta::Added => {
                    if let Some(p) = new_path.filter(|p| is_markdown(p)) {
                        result.added.push(p);
                    }
                }
                git2::Delta::Modified => {
                    if let Some(p) = new_path.filter(|p| is_markdown(p)) {
                        result.modified.push(p);
                    }
                }
                git2::Delta::Deleted => {
                    if let Some(p) = old_path.filter(|p| is_markdown(p)) {
                        result.deleted.push(p);
                    }
                }
                git2::Delta::Renamed => {
                    // Treat as delete-old + add-new.
                    if let Some(p) = old_path.filter(|p| is_markdown(p)) {
                        result.deleted.push(p);
                    }
                    if let Some(p) = new_path.filter(|p| is_markdown(p)) {
                        result.added.push(p);
                    }
                }
                _ => {}
            }
            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| GraphtorError::Pipeline {
        message: format!("diff iteration failed: {e}"),
        stage: "git_diff".to_string(),
    })?;

    debug!(
        added = result.added.len(),
        modified = result.modified.len(),
        deleted = result.deleted.len(),
        "git diff computed"
    );
    Ok(result)
}

/// Resolve the current `HEAD` commit.
fn resolve_head(repo: &Repository) -> Result<git2::Commit<'_>, GraphtorError> {
    let head = repo.head().map_err(|e| GraphtorError::Pipeline {
        message: format!("failed to resolve HEAD: {e}"),
        stage: "git_diff".to_string(),
    })?;
    let oid = head.target().ok_or_else(|| GraphtorError::Pipeline {
        message: "HEAD is not a direct reference (detached or symbolic)".to_string(),
        stage: "git_diff".to_string(),
    })?;
    repo.find_commit(oid).map_err(|e| GraphtorError::Pipeline {
        message: format!("failed to find HEAD commit object: {e}"),
        stage: "git_diff".to_string(),
    })
}

/// Walk `tree` and return all `.md` blobs as `added` (first-run path).
fn all_md_files_as_added(
    repo: &Repository,
    tree: &git2::Tree<'_>,
) -> Result<ChangedFiles, GraphtorError> {
    let mut added = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let name = entry.name().unwrap_or("");
            let path = PathBuf::from(format!("{root}{name}"));
            if is_markdown(&path) {
                added.push(path);
            }
        }
        git2::TreeWalkResult::Ok
    })
    .map_err(|e| GraphtorError::Pipeline {
        message: format!("failed to walk git tree: {e}"),
        stage: "git_diff".to_string(),
    })?;
    let _ = repo; // satisfies borrow checker
    Ok(ChangedFiles {
        added,
        modified: Vec::new(),
        deleted: Vec::new(),
    })
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}
