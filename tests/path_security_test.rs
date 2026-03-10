//! Integration tests for `graphtor_core::path::validate_path`.
//!
//! Verifies path security enforcement using real filesystem operations
//! via temporary directories. Tests boundary enforcement, traversal
//! rejection, and non-existent path handling through the public API.

use std::fs;

use graphtor_core::path::validate_path;
use graphtor_core::GraphtorError;

/// Strip the Windows verbatim `\\?\` prefix that `canonicalize` adds on Windows.
///
/// On other platforms this is a no-op. Used in assertions to ensure
/// `starts_with` comparisons work against non-verbatim paths.
fn strip_verbatim(path: &std::path::Path) -> std::path::PathBuf {
    let c = std::fs::canonicalize(path).expect("canonicalize failed in test helper");
    let s = c.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(stripped)
    } else {
        c
    }
}

// ── T039: Integration tests with real filesystem ──────────────────────────

/// A file inside the temp root is accepted and the resolved path is returned.
#[test]
fn validate_path_accepts_file_inside_root() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    let canonical_root = strip_verbatim(root.path());
    let file = root.path().join("docs").join("guide.md");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, b"# guide").unwrap();

    let result = validate_path(&file, root.path());
    assert!(
        result.is_ok(),
        "valid path inside root must be accepted: {result:?}"
    );
    let resolved = result.unwrap();
    assert!(
        resolved.starts_with(&canonical_root),
        "resolved path must be under root"
    );
    assert!(resolved.is_absolute());
}

/// A `..` traversal that escapes the root is rejected with `PathViolation`.
#[test]
fn validate_path_rejects_dotdot_traversal() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    let subdir = root.path().join("sub");
    fs::create_dir_all(&subdir).unwrap();

    // `sub/../../..` escapes the temp root entirely
    let escape = subdir.join("..").join("..").join("secret");
    let result = validate_path(&escape, root.path());
    assert!(
        matches!(result, Err(GraphtorError::PathViolation { .. })),
        "dotdot traversal must be rejected with PathViolation: {result:?}"
    );
}

/// An absolute path that exists outside the root is rejected with `PathViolation`.
#[test]
fn validate_path_rejects_absolute_path_outside_root() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    // A second real temp dir: guaranteed to exist, guaranteed outside root.
    // Both paths canonicalize successfully, so the result is always PathViolation.
    let other = tempfile::tempdir().expect("failed to create outside temp dir");

    let result = validate_path(other.path(), root.path());
    assert!(
        matches!(result, Err(GraphtorError::PathViolation { .. })),
        "path outside allowed root must return PathViolation: {result:?}"
    );
}

/// A non-existent file inside the root is accepted; resolved path is returned.
#[test]
fn validate_path_accepts_non_existent_leaf_inside_root() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    let canonical_root = strip_verbatim(root.path());
    let new_file = root.path().join("new_output.json");

    // The file doesn't exist yet — but its parent (root) does
    let result = validate_path(&new_file, root.path());
    assert!(
        result.is_ok(),
        "non-existent path whose parent is inside root must be accepted: {result:?}"
    );
    let resolved = result.unwrap();
    assert!(resolved.starts_with(&canonical_root));
    assert_eq!(resolved.file_name().unwrap(), "new_output.json");
}

/// Deeply nested valid path is accepted.
#[test]
fn validate_path_accepts_deeply_nested_existing_path() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    let deep = root
        .path()
        .join("a")
        .join("b")
        .join("c")
        .join("d")
        .join("e.txt");
    fs::create_dir_all(deep.parent().unwrap()).unwrap();
    fs::write(&deep, b"deep").unwrap();

    let result = validate_path(&deep, root.path());
    assert!(
        result.is_ok(),
        "deeply nested path inside root must be accepted: {result:?}"
    );
}
