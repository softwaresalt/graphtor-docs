//! Integration tests for local directory scanning (US2).
//!
//! Tests cover: happy-path discovery, deterministic ordering,
//! non-existent directory error, and path security violation.

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use graphtor_core::acquire::scan_local_source;
use graphtor_core::config::source::LocalSource;
use graphtor_core::error::GraphtorError;

fn make_local_source(id: &str, path: impl Into<PathBuf>) -> LocalSource {
    LocalSource {
        id: id.to_string(),
        path: path.into(),
        include: vec![],
        exclude: vec![],
        formats: vec![],
    }
}

/// Create a small nested directory structure with 3 files under `root`.
fn create_nested_files(root: &std::path::Path) {
    fs::create_dir_all(root.join("a/b")).expect("create dirs");
    fs::write(root.join("README.md"), "# readme").expect("write README.md");
    fs::write(root.join("a/guide.md"), "# guide").expect("write guide.md");
    fs::write(root.join("a/b/nested.rs"), "fn main() {}").expect("write nested.rs");
}

// ── T019: S017 — Happy-path local scan ─────────────────────────────────────────

#[test]
fn s017_scan_discovers_all_files_recursively() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_nested_files(root);

    let source = make_local_source("local-docs", root);
    let result = scan_local_source(&source, root).expect("scan should succeed");

    assert_eq!(result.len(), 3, "should discover exactly 3 files");
    let names: Vec<&OsStr> = result
        .iter()
        .map(|p: &PathBuf| p.file_name().unwrap())
        .collect();
    assert!(
        names.contains(&OsStr::new("README.md")),
        "README.md must be discovered"
    );
    assert!(
        names.contains(&OsStr::new("guide.md")),
        "guide.md must be discovered"
    );
    assert!(
        names.contains(&OsStr::new("nested.rs")),
        "nested.rs must be discovered"
    );
}

// ── T020: S019 — Deterministic sort order ──────────────────────────────────────

#[test]
fn s019_scan_results_are_sorted_deterministically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_nested_files(root);

    let source = make_local_source("local-docs", root);
    let first = scan_local_source(&source, root).expect("first scan");
    let second = scan_local_source(&source, root).expect("second scan");

    assert_eq!(
        first, second,
        "two scans of the same directory must return identical ordering"
    );

    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(
        first, sorted,
        "result must be in sorted (lexicographic) order"
    );
}

// ── T021: S020 — Non-existent directory returns Pipeline error ──────────────────

#[test]
fn s020_nonexistent_dir_returns_pipeline_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let missing = root.join("does-not-exist");

    let source = make_local_source("missing-docs", &missing);
    let result = scan_local_source(&source, root);

    assert!(result.is_err(), "scan of non-existent dir must fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("[pipeline]"),
        "error must be a Pipeline error, got: {msg}"
    );
}

// ── T022: S021 — Path security violation returns PathViolation error ────────────

#[test]
fn s021_path_outside_allowed_root_returns_path_violation() {
    let allowed_dir = tempfile::tempdir().expect("allowed tempdir");
    let outside_dir = tempfile::tempdir().expect("outside tempdir");

    // Create a file so the dir is non-empty and exists
    fs::write(outside_dir.path().join("file.md"), "content").expect("write file");

    let source = make_local_source("outside-docs", outside_dir.path());
    let result = scan_local_source(&source, allowed_dir.path());

    assert!(result.is_err(), "scan outside allowed root must fail");
    assert!(
        matches!(result.unwrap_err(), GraphtorError::PathViolation { .. }),
        "error must be PathViolation"
    );
}
