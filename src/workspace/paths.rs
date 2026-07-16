//! Cross-platform workspace path resolution.
//!
//! Resolves the `.graphtor/` workspace root by walking up the directory tree
//! from the current working directory. Handles Windows UNC paths, macOS
//! case-insensitive filesystems, and Linux symlinks. Paths are stored with
//! forward-slash separators for portability but accessed via native
//! `std::path::PathBuf` on the host platform.

use std::path::{Component, Path, PathBuf};

use graphtor_core::GraphtorError;

/// Name of the workspace directory created inside a project root.
pub const GRAPHTOR_DIR: &str = ".graphtor";

/// Sub-directories created under `.graphtor/`.
pub const GRAPHTOR_SUBDIRS: &[&str] = &["bin", "data", "cache", "config", "logs"];

/// The ingestion-scaffold sub-directories: the subset of [`GRAPHTOR_SUBDIRS`]
/// created only by `install --with-ingestion`. `config/` is deliberately
/// excluded — it holds `sources.yaml`, which a consumption-only workspace
/// legitimately uses for explicit `type: database` entries WITHOUT any
/// ingestion scaffold — so its presence alone is not a signal of a full
/// ingestion footprint. Used by footprint detection to avoid misclassifying a
/// valid consumption-only workspace as a (broken) full install.
pub const GRAPHTOR_INGESTION_SUBDIRS: &[&str] = &["bin", "data", "cache", "logs"];

/// Locate the `.graphtor/` workspace directory.
///
/// Searches upward from `start_dir` until a `.graphtor/` directory is found
/// or the filesystem root is reached. Returns the path to the `.graphtor/`
/// directory (not the project root).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when no `.graphtor/` directory is found.
pub fn find_workspace_dir(start_dir: &Path) -> Result<PathBuf, GraphtorError> {
    let canonical = start_dir
        .canonicalize()
        .map_err(|e| GraphtorError::Config {
            message: format!("cannot resolve start directory: {e}"),
            field: None,
        })?;

    let mut current = canonical.as_path();
    loop {
        let candidate = current.join(GRAPHTOR_DIR);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(GraphtorError::Config {
                    message: format!(
                        "no {GRAPHTOR_DIR} directory found; run `graphtor-docs install` first"
                    ),
                    field: None,
                })
            }
        }
    }
}

/// Resolve the project root (parent of `.graphtor/`) from the current
/// working directory.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when resolution fails.
#[allow(dead_code)]
pub fn project_root(cwd: &Path) -> Result<PathBuf, GraphtorError> {
    let ws = find_workspace_dir(cwd)?;
    ws.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| GraphtorError::Config {
            message: "workspace dir has no parent".to_string(),
            field: None,
        })
}

/// Convert an absolute path to a portable forward-slash relative path
/// from `base`.
///
/// Used when storing paths in config files that may be committed to Git
/// and read on different platforms.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when `path` is not under `base`.
#[allow(dead_code)]
pub fn to_portable_relative(base: &Path, path: &Path) -> Result<String, GraphtorError> {
    path.strip_prefix(base)
        .map_err(|_| GraphtorError::Config {
            message: format!(
                "path {} is not under base {}",
                path.display(),
                base.display()
            ),
            field: None,
        })
        .map(portable_path_string)
}

/// Convert a [`Path`] to a forward-slash string, normalising any Windows
/// backslash separators and stripping UNC prefix verbatim components.
#[allow(dead_code)]
pub fn portable_path_string(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::CurDir => Some(".".to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_path_strips_separators() {
        let p = Path::new("a").join("b").join("c");
        assert_eq!(portable_path_string(&p), "a/b/c");
    }

    #[test]
    fn find_workspace_dir_returns_error_when_absent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = find_workspace_dir(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn find_workspace_dir_finds_parent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(GRAPHTOR_DIR);
        std::fs::create_dir_all(&ws).expect("create .graphtor");
        // Start from a nested subdir.
        let sub = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&sub).expect("create subdir");
        let found = find_workspace_dir(&sub).expect("found");
        // Canonicalize both sides: Windows may produce \\?\ prefix or 8.3 short names.
        let found_canon = found.canonicalize().unwrap_or(found);
        let ws_canon = ws.canonicalize().unwrap_or(ws);
        assert_eq!(found_canon, ws_canon);
    }
}
