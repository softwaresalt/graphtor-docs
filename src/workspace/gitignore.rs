//! `.gitignore` management for the workspace installation.
//!
//! Appends `.graphtor/` to the project `.gitignore` during install and
//! removes the entry during uninstall. Both operations are idempotent.

use std::fs;
use std::path::Path;

use graphtor_core::path::is_reparse_point;
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

    // Workspace-containment guard (Constitution III/IV): the `read_to_string`
    // and `fs::write` below both FOLLOW a symlinked/junction `.gitignore`, so a
    // `.gitignore` planted as a link before install would redirect the
    // managed-block write into an external target OUTSIDE the project. The
    // uninstall/read paths (`has_managed_gitignore_block`,
    // `remove_gitignore_entry`) already refuse a linked `.gitignore`; the
    // install WRITE path must refuse it too. Fail closed rather than write
    // through the link.
    if is_reparse_point(&path) {
        return Err(GraphtorError::Config {
            message: ".gitignore is a symlink or junction; refusing to write the managed \
                      graphtor-docs entry through a linked file"
                .to_string(),
            field: None,
        });
    }

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

/// Returns `true` when `project_root/.gitignore` exists and contains the
/// graphtor-docs managed marker header — i.e. a managed block that
/// [`add_gitignore_entry`] wrote is actually present.
///
/// Uninstall planning uses this (rather than merely inferring from the
/// workspace footprint) so that a Full install created with `--no-gitignore`
/// (or one whose `.graphtor/` line is a user's own, unmarked entry) is never
/// scheduled for gitignore cleanup.
#[must_use]
pub fn has_managed_gitignore_block(project_root: &Path) -> bool {
    let path = project_root.join(".gitignore");
    // Workspace-containment guard (Constitution III/IV): `read_to_string`
    // FOLLOWS a symlinked/junction `.gitignore`, so reading a linked file would
    // report a managed block that lives OUTSIDE the project — and drive
    // uninstall into rewriting that external file. Treat a linked `.gitignore`
    // as having no managed block so cleanup is never planned for it.
    if is_reparse_point(&path) {
        return false;
    }
    let Ok(existing) = fs::read_to_string(&path) else {
        return false;
    };
    existing.lines().any(|l: &str| l.trim() == MARKER_HEADER)
}

/// Remove ONLY the graphtor-docs managed block (the [`MARKER_HEADER`] comment
/// and the `.graphtor/` entry line it introduces) from `.gitignore`.
///
/// The managed entry is identified by its adjacency to the marker header:
/// [`add_gitignore_entry`] always writes the entry on the line immediately
/// after the marker, so removal strips the marker line and a single
/// immediately-following `.graphtor/` entry line. A user's own, unmarked
/// `.graphtor/` line elsewhere in the file is PRESERVED — uninstall must never
/// delete a gitignore line graphtor-docs did not author.
///
/// If `.gitignore` does not exist or contains no managed marker, returns
/// without modification (idempotent).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn remove_gitignore_entry(project_root: &Path) -> Result<(), GraphtorError> {
    let path = project_root.join(".gitignore");
    if !path.exists() {
        return Ok(());
    }
    // Workspace-containment guard (Constitution III/IV): `read_to_string` and
    // `fs::write` both FOLLOW a symlinked/junction `.gitignore`, so touching a
    // linked file would rewrite a file OUTSIDE the project. Refuse to modify a
    // `.gitignore` that is itself a link — skip cleanup rather than reach out of
    // the workspace. Planning already excludes this case (see
    // `has_managed_gitignore_block`); this is defence-in-depth against a plan
    // that went stale after the file became a link.
    if is_reparse_point(&path) {
        return Ok(());
    }

    let existing = fs::read_to_string(&path).map_err(|e| GraphtorError::Config {
        message: format!("failed to read .gitignore: {e}"),
        field: None,
    })?;

    let lines: Vec<&str> = existing.lines().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        if lines[idx].trim() == MARKER_HEADER {
            // Skip the marker line, plus a single immediately-following
            // managed `.graphtor/` entry line (our block writes the entry
            // directly after the marker). Any unmarked user entry elsewhere is
            // untouched.
            idx += 1;
            if idx < lines.len() {
                let next = lines[idx].trim();
                if next == GITIGNORE_ENTRY || next == GITIGNORE_ENTRY.trim_end_matches('/') {
                    idx += 1;
                }
            }
            continue;
        }
        kept.push(lines[idx]);
        idx += 1;
    }

    fs::write(&path, kept.join("\n") + "\n").map_err(|e| GraphtorError::Config {
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

    #[test]
    fn remove_preserves_user_authored_unmarked_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".gitignore");
        // A user's own, unmarked `.graphtor/` line plus unrelated entries.
        fs::write(&path, "target/\n.graphtor/\n.env\n").expect("seed");
        remove_gitignore_entry(tmp.path()).expect("remove");
        let content = fs::read_to_string(&path).expect("read");
        assert!(
            content.contains(GITIGNORE_ENTRY),
            "user-authored unmarked .graphtor/ line must be preserved"
        );
        assert!(content.contains("target/"));
        assert!(content.contains(".env"));
    }

    #[test]
    fn remove_strips_managed_block_but_keeps_user_lines() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".gitignore");
        fs::write(&path, "target/\n.env\n").expect("seed");
        add_gitignore_entry(tmp.path()).expect("add");
        remove_gitignore_entry(tmp.path()).expect("remove");
        let content = fs::read_to_string(&path).expect("read");
        assert!(!content.contains(MARKER_HEADER), "marker removed");
        assert!(!content.contains(GITIGNORE_ENTRY), "managed entry removed");
        assert!(content.contains("target/"), "user line preserved");
        assert!(content.contains(".env"), "user line preserved");
    }

    #[test]
    fn has_managed_block_reflects_marker_presence() {
        // No .gitignore -> no managed block.
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(
            !has_managed_gitignore_block(tmp.path()),
            "no .gitignore -> no managed block"
        );

        // An unmarked user `.graphtor/` line is not a managed block.
        let tmp2 = tempfile::tempdir().expect("tempdir");
        fs::write(tmp2.path().join(".gitignore"), ".graphtor/\n").expect("seed");
        assert!(
            !has_managed_gitignore_block(tmp2.path()),
            "unmarked .graphtor/ line is not a managed block"
        );

        // add writes the marker -> managed block present.
        let tmp3 = tempfile::tempdir().expect("tempdir");
        add_gitignore_entry(tmp3.path()).expect("add");
        assert!(
            has_managed_gitignore_block(tmp3.path()),
            "marker written by add -> managed block present"
        );
    }

    // ── workspace containment: symlinked `.gitignore` (X4) ──────────────────

    /// Create a file symlink cross-platform, returning `Err` when the platform
    /// refuses (e.g. Windows without the symlink privilege) so the caller can
    /// self-skip rather than fail.
    fn try_symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }

    #[test]
    fn symlinked_gitignore_is_never_treated_as_managed_or_rewritten() {
        // A `.gitignore` that is itself a symlink/junction points at a file
        // OUTSIDE the project. `has_managed_gitignore_block` must not read it as
        // managed, and `remove_gitignore_entry` must not rewrite the external
        // target (Constitution III/IV).
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let external_file = external.path().join("real-gitignore");
        let original = format!("{MARKER_HEADER}\n{GITIGNORE_ENTRY}\nnode_modules/\n");
        fs::write(&external_file, &original).expect("write external gitignore");

        let link = project.path().join(".gitignore");
        if try_symlink_file(&external_file, &link).is_err() {
            return; // platform refused symlink creation — skip
        }

        assert!(
            !has_managed_gitignore_block(project.path()),
            "a symlinked .gitignore must not be reported as holding a managed block"
        );
        remove_gitignore_entry(project.path()).expect("remove must skip a symlinked .gitignore");
        assert_eq!(
            fs::read_to_string(&external_file).expect("read external"),
            original,
            "the external file behind a symlinked .gitignore must be left unchanged"
        );
    }

    #[test]
    fn add_refuses_to_write_through_a_symlinked_gitignore() {
        // A6C7EDB3: a `.gitignore` planted as a symlink before install points at
        // a file OUTSIDE the project. `add_gitignore_entry`'s read + write would
        // FOLLOW the link and rewrite the external target (Constitution III/IV).
        // The install WRITE path must refuse a linked `.gitignore` and fail
        // closed, matching the uninstall/read guards.
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let external_file = external.path().join("real-gitignore");
        let original = "node_modules/\n".to_string();
        fs::write(&external_file, &original).expect("write external gitignore");

        let link = project.path().join(".gitignore");
        if try_symlink_file(&external_file, &link).is_err() {
            return; // platform refused symlink creation — skip
        }

        let result = add_gitignore_entry(project.path());
        assert!(
            result.is_err(),
            "add_gitignore_entry through a symlinked .gitignore must fail closed"
        );
        assert_eq!(
            fs::read_to_string(&external_file).expect("read external"),
            original,
            "the external file behind a symlinked .gitignore must be left unchanged"
        );
    }
}
