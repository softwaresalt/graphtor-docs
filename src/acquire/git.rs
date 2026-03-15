//! Git repository acquisition — shallow clone via `git2`.
//!
//! Provides [`clone_git_source`] which clones a Git source using a shallow
//! fetch (depth=1) and single-branch fetch strategy (FR-001, FR-004).
