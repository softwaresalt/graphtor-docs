//! Integration tests for Git source acquisition (Phase 3 — User Story 1).
//!
//! All tests use local bare repositories created with `git2` — no network access required.

use std::path::PathBuf;

use graphtor_core::acquire::git::clone_git_source;
use graphtor_core::config::GitSource;
use graphtor_core::error::GraphtorError;

/// Construct a minimal [`GitSource`] for testing.
fn make_git_source(id: &str, url: &str, branch: &str) -> GitSource {
    GitSource {
        id: id.to_owned(),
        url: url.to_owned(),
        branch: branch.to_owned(),
        include: vec![],
        exclude: vec![],
    }
}

/// Create a local bare git repository with a single commit on the "main" branch.
///
/// Returns `(TempDir, path)` — the caller must keep `TempDir` alive for the
/// duration of the test to prevent premature deletion.
fn make_bare_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir for bare repo");
    let bare_path = dir.path().join("repo.git");

    let repo = git2::Repository::init_bare(&bare_path).expect("init bare repo");

    let blob = repo
        .blob(b"# Test Repository\n")
        .expect("create README blob");
    let mut tb = repo.treebuilder(None).expect("treebuilder");
    tb.insert("README.md", blob, 0o100_644)
        .expect("insert README.md");
    let tree_oid = tb.write().expect("write tree");
    let tree = repo.find_tree(tree_oid).expect("find tree");

    let sig =
        git2::Signature::now("Test Author", "test@example.com").expect("create git signature");
    repo.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "Initial commit",
        &tree,
        &[],
    )
    .expect("create initial commit");

    // Advertise "main" as the remote's HEAD branch.
    repo.set_head("refs/heads/main").expect("set HEAD to main");

    (dir, bare_path)
}

/// Convert a local filesystem path to a `file://` URL for cross-platform git cloning.
fn path_to_url(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        // Windows absolute path: C:/... → file:///C:/...
        format!("file:///{s}")
    }
}

// ── T011: S008 — Clone happy path ────────────────────────────────────────────

#[test]
fn s008_clone_git_source_creates_target_with_git_dir() {
    let (bare_dir, bare_path) = make_bare_repo();
    let target_dir = tempfile::tempdir().expect("target tempdir");
    let clone_to = target_dir.path().join("cloned-repo");

    let url = path_to_url(&bare_path);
    let source = make_git_source("test-source", &url, "main");

    let result = clone_git_source(&source, &clone_to);

    drop(bare_dir); // keep alive until after clone completes

    assert!(result.is_ok(), "clone should succeed: {result:?}");
    assert!(
        clone_to.join(".git").exists(),
        ".git directory must exist after a successful clone"
    );
    assert_eq!(
        result.unwrap(),
        clone_to,
        "returned path must equal target_dir"
    );
}

// ── T012: S010 — Skip already-cloned repository ──────────────────────────────

#[test]
fn s010_clone_skipped_when_dot_git_already_exists() {
    let target_dir = tempfile::tempdir().expect("target tempdir");
    let clone_to = target_dir.path().join("existing-repo");

    // Pre-create the target directory with a `.git` subdirectory to simulate
    // a previously completed clone.
    std::fs::create_dir_all(&clone_to).expect("create clone_to dir");
    std::fs::create_dir(clone_to.join(".git")).expect("create .git dir");

    // URL will never be contacted — skip must trigger before any network call.
    let source = make_git_source(
        "existing-source",
        "https://should-not-be-reached.invalid/repo.git",
        "main",
    );

    let result = clone_git_source(&source, &clone_to);

    assert!(result.is_ok(), "skip must return Ok: {result:?}");
    assert_eq!(
        result.unwrap(),
        clone_to,
        "returned path must equal target_dir on skip"
    );
    // The original .git dir must still be present (not overwritten).
    assert!(
        clone_to.join(".git").exists(),
        ".git directory must survive the skip"
    );
}

// ── T013: S012 — Non-existent branch returns Pipeline error ──────────────────

#[test]
fn s012_nonexistent_branch_returns_pipeline_error() {
    let (bare_dir, bare_path) = make_bare_repo();
    let target_dir = tempfile::tempdir().expect("target tempdir");
    let clone_to = target_dir.path().join("branch-test");

    let url = path_to_url(&bare_path);
    // The bare repo has "main" but not "does-not-exist".
    let source = make_git_source("branch-test-source", &url, "does-not-exist");

    let result = clone_git_source(&source, &clone_to);

    drop(bare_dir);

    assert!(
        matches!(result, Err(GraphtorError::Pipeline { .. })),
        "non-existent branch must return Pipeline error: {result:?}"
    );
    // Cleanup code must have removed any partial clone directory.
    assert!(
        !clone_to.exists(),
        "partial clone directory must be removed after failure"
    );
}

// ── T014: S011 — Unreachable URL returns Pipeline error ──────────────────────

#[test]
fn s011_unreachable_url_returns_pipeline_error() {
    let target_dir = tempfile::tempdir().expect("target tempdir");
    let clone_to = target_dir.path().join("unreachable-test");

    let source = make_git_source(
        "unreachable-source",
        "file:///nonexistent/path/to/repository_xyz_12345.git",
        "main",
    );

    let result = clone_git_source(&source, &clone_to);

    assert!(
        matches!(result, Err(GraphtorError::Pipeline { .. })),
        "unreachable URL must return Pipeline error: {result:?}"
    );
}
