//! Path security validation — boundary enforcement via canonicalization.
//!
//! Provides [`validate_path`] to resolve and check that a file path stays
//! within an allowed root directory, preventing directory traversal attacks.
//! All pipeline stages that accept user-supplied or manifest-derived paths
//! MUST validate them through this function before any file I/O.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::error::GraphtorError;

/// Resolve `path` to an absolute, `..`-free form without touching the filesystem.
///
/// If `path` is relative it is first joined onto `current_dir()`. `..` and `.`
/// components are then resolved in order. The result is syntactically normalized
/// but may still contain short path names (8.3 format) on Windows.
fn normalize_absolute(path: &Path) -> std::io::Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for component in abs.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop(); // no-op at root — safe
            }
            _ => result.push(component),
        }
    }
    Ok(result)
}

/// Canonicalize `path` and strip the Windows verbatim `\\?\` prefix.
///
/// On Windows, [`std::fs::canonicalize`] returns paths with a `\\?\` prefix
/// (verbatim long-path form). Stripping it here allows [`Path::starts_with`]
/// comparisons to work correctly against non-verbatim paths.
pub(crate) fn canonicalize_clean(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    let s = canonical.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        Ok(PathBuf::from(stripped))
    } else {
        Ok(canonical)
    }
}

/// Resolve `path` to a canonical absolute form, handling non-existent leafs.
///
/// For existing paths: calls `canonicalize_clean` which follows symlinks and
/// expands Windows short path names (8.3 format).
///
/// For non-existent paths: normalises `..` syntactically, then walks up the
/// path to find the deepest existing ancestor, canonicalises that, and
/// reconstructs the remaining components. This ensures short path expansion
/// even when the final component does not yet exist.
fn resolve_path(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return canonicalize_clean(path);
    }

    // Normalize `..`/`.` without hitting the filesystem
    let normalized = normalize_absolute(path)?;

    let mut tail: Vec<OsString> = Vec::new();
    let mut current = normalized.clone();

    loop {
        if current.exists() {
            // Canonicalize the existing ancestor (expands short paths)
            let mut canonical = canonicalize_clean(&current)?;
            for component in tail.into_iter().rev() {
                canonical.push(component);
            }
            return Ok(canonical);
        }
        match (
            current.file_name().map(OsString::from),
            current.parent().map(PathBuf::from),
        ) {
            (Some(name), Some(parent)) if parent != current => {
                tail.push(name);
                current = parent;
            }
            _ => {
                // At the filesystem root with nothing found; return syntactically normalized form
                return Ok(normalized);
            }
        }
    }
}

/// Validate that `path` is within `allowed_root` after canonicalization.
///
/// Resolves both paths to their canonical absolute form, then checks that
/// `path` is a descendant of `allowed_root`. Handles `..` traversal,
/// symlinks (for existing paths), Windows short path expansion, redundant
/// separators, and relative paths.
///
/// If `path` does not yet exist on the filesystem, the deepest existing
/// ancestor is canonicalised and the remaining components are appended.
///
/// # Returns
///
/// The canonical absolute path on success.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if the resolved path escapes
/// `allowed_root`, or [`GraphtorError::Io`] if `allowed_root` cannot be
/// resolved.
///
/// # Limitations
///
/// **TOCTOU:** There is a brief window between the `path.exists()` check and
/// the subsequent `canonicalize` call during which the filesystem may change
/// (file created, deleted, or replaced with a symlink). This is an inherent
/// limitation of filesystem-based path checks. For batch ingestion pipelines
/// this risk is acceptable; high-security server contexts should use
/// `openat`/`O_PATH`-based approaches instead.
///
/// **Symlinks for non-existent paths:** If an *intermediate* component in a
/// non-existent path is a symlink pointing outside `allowed_root`, the
/// walk-up algorithm will canonicalize at that ancestor, which expands the
/// symlink — the violation will still be detected. However, if the symlink
/// itself doesn't exist yet, it cannot be detected at validation time.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use graphtor_core::path::validate_path;
///
/// let root = Path::new("/data/workspace");
/// let safe = Path::new("/data/workspace/docs/guide.md");
/// let result = validate_path(safe, root);
/// assert!(result.is_ok());
/// ```
pub fn validate_path(path: &Path, allowed_root: &Path) -> Result<PathBuf, GraphtorError> {
    let canonical_root = canonicalize_clean(allowed_root).map_err(GraphtorError::Io)?;

    let resolved = resolve_path(path).map_err(GraphtorError::Io)?;

    if !resolved.starts_with(&canonical_root) {
        return Err(GraphtorError::PathViolation {
            attempted: resolved,
            allowed_root: canonical_root,
        });
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn setup_root() -> TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    // ── T036: Valid paths ─────────────────────────────────────────────────

    #[test]
    fn valid_path_within_root_returns_ok() {
        let root = setup_root();
        let canonical_root = canonicalize_clean(root.path()).unwrap();
        let file = root.path().join("docs").join("guide.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"content").unwrap();

        let result = validate_path(&file, root.path());
        assert!(
            result.is_ok(),
            "path within root must be accepted: {result:?}"
        );
        assert!(result.unwrap().starts_with(&canonical_root));
    }

    #[test]
    fn valid_absolute_path_within_root_returns_canonical_path() {
        let root = setup_root();
        let canonical_root = canonicalize_clean(root.path()).unwrap();
        let file = root.path().join("file.txt");
        fs::write(&file, b"data").unwrap();

        let resolved = validate_path(&file, root.path()).unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.starts_with(&canonical_root));
    }

    #[test]
    fn path_equal_to_root_returns_ok() {
        let root = setup_root();
        let result = validate_path(root.path(), root.path());
        assert!(result.is_ok(), "root itself should be accepted: {result:?}");
    }

    // ── T037: Rejected paths ──────────────────────────────────────────────

    #[test]
    fn dotdot_traversal_escaping_root_is_rejected() {
        let root = setup_root();
        let subdir = root.path().join("sub");
        fs::create_dir_all(&subdir).unwrap();
        // `sub/../../secret` escapes the temp root
        let traversal = subdir.join("..").join("..").join("secret");

        let result = validate_path(&traversal, root.path());
        assert!(
            matches!(result, Err(GraphtorError::PathViolation { .. })),
            "dotdot traversal must be rejected: {result:?}"
        );
    }

    #[test]
    fn absolute_path_outside_root_is_rejected() {
        let root = setup_root();
        // Use a second real temp dir that is guaranteed to exist but is NOT
        // a descendant of root — canonicalize succeeds on both, so the
        // violation is always PathViolation, never Io.
        let other = tempfile::tempdir().expect("failed to create outside temp dir");

        let result = validate_path(other.path(), root.path());
        assert!(
            matches!(result, Err(GraphtorError::PathViolation { .. })),
            "path outside root must return PathViolation: {result:?}"
        );
    }

    // ── T038: Edge cases ──────────────────────────────────────────────────

    #[test]
    fn non_existent_path_within_root_is_accepted() {
        let root = setup_root();
        let canonical_root = canonicalize_clean(root.path()).unwrap();
        let new_file = root.path().join("new_document.md");

        let result = validate_path(&new_file, root.path());
        assert!(
            result.is_ok(),
            "non-existent file inside root must be accepted: {result:?}"
        );
        let resolved = result.unwrap();
        assert!(resolved.starts_with(&canonical_root));
        assert_eq!(resolved.file_name().unwrap(), "new_document.md");
    }

    #[test]
    fn path_with_redundant_separators_is_normalized() {
        let root = setup_root();
        let canonical_root = canonicalize_clean(root.path()).unwrap();
        let file = root.path().join("sub").join("file.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"x").unwrap();

        // Construct a path with double separators
        let raw = format!("{}//sub//file.txt", root.path().to_string_lossy());
        let messy_path = PathBuf::from(&raw);

        let result = validate_path(&messy_path, root.path());
        match result {
            Ok(p) => assert!(
                p.starts_with(&canonical_root),
                "normalized path must still be within root"
            ),
            Err(GraphtorError::Io(_)) => {
                // Acceptable: OS rejected the malformed path literal
            }
            Err(other) => panic!("unexpected error for redundant separators: {other:?}"),
        }
    }

    #[test]
    fn nested_subdirectory_within_root_is_accepted() {
        let root = setup_root();
        let deep = root.path().join("a").join("b").join("c").join("d.txt");
        fs::create_dir_all(deep.parent().unwrap()).unwrap();
        fs::write(&deep, b"deep").unwrap();

        let result = validate_path(&deep, root.path());
        assert!(
            result.is_ok(),
            "deeply nested path within root must be accepted: {result:?}"
        );
    }
}
