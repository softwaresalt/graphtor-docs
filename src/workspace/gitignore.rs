//! `.gitignore` management for the workspace installation.
//!
//! Appends `.graphtor/` to the project `.gitignore` during install and
//! removes the entry during uninstall. Both operations are idempotent.

use std::fs;
use std::path::Path;

use graphtor_core::GraphtorError;

/// The gitignore entry managed by graphtor-docs.
const GITIGNORE_ENTRY: &str = ".graphtor/";

/// Marker comment written before the managed entry.
const MARKER_HEADER: &str = "# graphtor-docs — managed by graphtor-docs install";

/// Append `.graphtor/` to `project_root/.gitignore`.
///
/// - If `.gitignore` does not exist it is created.
/// - If the entry already exists the function returns without modification
///   (idempotent).
///
/// Called by `cmd_install`'s `--with-ingestion` full-scaffold path
/// (P2-T2b); the consumption-first minimal default has no managed
/// `.gitignore` side effect.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn add_gitignore_entry(project_root: &Path) -> Result<(), GraphtorError> {
    let path = project_root.join(".gitignore");

    // Read existing content; create if missing.
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| GraphtorError::Config {
            message: format!("failed to read .gitignore: {e}"),
            field: None,
        })?
    } else {
        String::new()
    };

    // Check whether the entry is already present.
    if existing.lines().any(|l: &str| {
        l.trim() == GITIGNORE_ENTRY.trim_end_matches('/') || l.trim() == GITIGNORE_ENTRY
    }) {
        return Ok(());
    }

    let mut updated = existing.clone();
    if !updated.ends_with('\n') && !updated.is_empty() {
        updated.push('\n');
    }
    updated.push('\n');
    updated.push_str(MARKER_HEADER);
    updated.push('\n');
    updated.push_str(GITIGNORE_ENTRY);
    updated.push('\n');

    fs::write(&path, updated).map_err(|e| GraphtorError::Config {
        message: format!("failed to write .gitignore: {e}"),
        field: None,
    })?;

    Ok(())
}

/// Remove the `.graphtor/` entry (and its marker comment) from `.gitignore`.
///
/// If `.gitignore` does not exist or the entry is absent, returns without
/// modification (idempotent).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn remove_gitignore_entry(project_root: &Path) -> Result<(), GraphtorError> {
    let path = project_root.join(".gitignore");
    if !path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(&path).map_err(|e| GraphtorError::Config {
        message: format!("failed to read .gitignore: {e}"),
        field: None,
    })?;

    let cleaned: Vec<&str> = existing
        .lines()
        .filter(|l: &&str| {
            let t = l.trim();
            t != MARKER_HEADER && t != GITIGNORE_ENTRY && t != GITIGNORE_ENTRY.trim_end_matches('/')
        })
        .collect();

    fs::write(&path, cleaned.join("\n") + "\n").map_err(|e| GraphtorError::Config {
        message: format!("failed to write .gitignore: {e}"),
        field: None,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_creates_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        add_gitignore_entry(tmp.path()).expect("add");
        let content = fs::read_to_string(tmp.path().join(".gitignore")).expect("read");
        assert!(content.contains(GITIGNORE_ENTRY));
    }

    #[test]
    fn add_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        add_gitignore_entry(tmp.path()).expect("first");
        add_gitignore_entry(tmp.path()).expect("second");
        let content = fs::read_to_string(tmp.path().join(".gitignore")).expect("read");
        let count = content.matches(GITIGNORE_ENTRY).count();
        assert_eq!(count, 1, "entry should appear exactly once");
    }

    #[test]
    fn remove_cleans_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        add_gitignore_entry(tmp.path()).expect("add");
        remove_gitignore_entry(tmp.path()).expect("remove");
        let content = fs::read_to_string(tmp.path().join(".gitignore")).expect("read");
        assert!(!content.contains(GITIGNORE_ENTRY));
    }
}
