//! Integration tests for end-to-end filter pipeline (US3).
//!
//! Tests combine `scan_local_source()` + `apply_source_filter()` to verify
//! the full acquire → scan → filter flow for both include-only, exclude-only,
//! and combined pattern scenarios.

use std::fs;
use std::path::PathBuf;

use graphtor_core::acquire::{apply_source_filter, scan_local_source};
use graphtor_core::config::source::LocalSource;

fn make_local_source(id: &str, path: impl Into<PathBuf>) -> LocalSource {
    LocalSource {
        id: id.to_string(),
        path: path.into(),
        include: vec![],
        exclude: vec![],
        formats: vec![],
        database: None,
    }
}

/// Create a mixed directory with 4 files: 2 markdown, 1 text, 1 shell script.
fn create_mixed_files(root: &std::path::Path) {
    fs::create_dir_all(root.join("internal")).expect("create dirs");
    fs::write(root.join("README.md"), "# readme").expect("write README.md");
    fs::write(root.join("guide.md"), "# guide").expect("write guide.md");
    fs::write(root.join("internal/notes.txt"), "private notes").expect("write notes.txt");
    fs::write(root.join("build.sh"), "#!/bin/bash").expect("write build.sh");
}

// ── S026: include only .md files ──────────────────────────────────────────────

#[test]
fn e2e_scan_then_include_md_only_returns_two_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_mixed_files(root);

    let source = make_local_source("test-docs", root);
    let all_files = scan_local_source(&source, root).expect("scan");
    assert_eq!(all_files.len(), 4, "should have 4 files before filtering");

    let result = apply_source_filter("test-docs", &all_files, &["**/*.md".to_string()], &[])
        .expect("filter");

    assert_eq!(
        result.original_count, 4,
        "original_count must reflect pre-filter total"
    );
    assert_eq!(result.filtered_count, 2, "only 2 .md files should pass");
    assert_eq!(result.files.len(), 2);
    for f in &result.files {
        assert!(
            f.extension().is_some_and(|e: &std::ffi::OsStr| e == "md"),
            "expected .md extension, got: {f:?}"
        );
    }
}

// ── S029: exclude wins when both include and exclude match ────────────────────

#[test]
fn e2e_scan_then_exclude_internal_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_mixed_files(root);

    let source = make_local_source("test-docs", root);
    let all_files = scan_local_source(&source, root).expect("scan");

    let result = apply_source_filter(
        "test-docs",
        &all_files,
        &[],
        &["**/internal/**".to_string()],
    )
    .expect("filter");

    assert_eq!(result.original_count, 4);
    assert_eq!(
        result.filtered_count, 3,
        "notes.txt under internal/ must be excluded"
    );
    for f in &result.files {
        assert!(
            !f.to_string_lossy().contains("internal"),
            "file under internal/ must be excluded: {f:?}"
        );
    }
}

// ── S032: all files excluded → empty FilteredFileSet (WARN logged) ───────────

#[test]
fn e2e_all_excluded_returns_empty_file_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_mixed_files(root);

    let source = make_local_source("test-docs", root);
    let all_files = scan_local_source(&source, root).expect("scan");

    let result = apply_source_filter(
        "test-docs",
        &all_files,
        &["**/*.nonexistent".to_string()],
        &[],
    )
    .expect("filter");

    assert_eq!(result.original_count, 4);
    assert_eq!(result.filtered_count, 0);
    assert!(result.files.is_empty(), "all files should be excluded");
}

// ── FilteredFileSet metadata ──────────────────────────────────────────────────

#[test]
fn apply_source_filter_sets_source_id_and_counts_correctly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    create_mixed_files(root);

    let source = make_local_source("my-source", root);
    let all_files = scan_local_source(&source, root).expect("scan");

    let result = apply_source_filter("my-source", &all_files, &["**/*.md".to_string()], &[])
        .expect("filter");

    assert_eq!(result.source_id, "my-source");
    assert_eq!(result.original_count, 4);
    assert_eq!(result.filtered_count, result.files.len());
}
