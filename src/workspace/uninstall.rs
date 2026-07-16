//! Workspace uninstall workflow.
//!
//! Removes ONLY the graphtor-created filesystem artifacts (the known
//! ingestion-capable subdirectories of a full install) and cleans up MCP
//! client config files and a managed `.gitignore` entry. Requires explicit
//! `--confirm` to prevent accidental data loss.
//!
//! Footprint-safe (P2-T5a): a user-dropped `.db` file living directly in
//! `.graphtor/` — the read-only serve auto-discovery drop location (P1-T1)
//! — is NEVER a deletion candidate, regardless of footprint or
//! `keep_config`. A subdirectory that is itself a symlink is never followed
//! or removed, so an operator's own reparse-point trick can never cause
//! uninstall to reach outside `.graphtor/`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::workspace::doctor::{detect_footprint, WorkspaceFootprint};
use crate::workspace::gitignore::remove_gitignore_entry;
use crate::workspace::mcp_config::{remove_mcp_config, McpConfigAction};
use crate::workspace::paths::{GRAPHTOR_DIR, GRAPHTOR_SUBDIRS};
use graphtor_core::GraphtorError;

/// Result of an uninstall operation.
#[derive(Debug)]
pub struct UninstallResult {
    /// Files and directories deleted.
    pub removed: Vec<String>,
    /// Shared MCP configs updated in place (graphtor-docs entry pruned, file kept).
    pub updated: Vec<String>,
}

/// Returns `true` when `path` is itself a symlink (Unix) or a
/// junction/reparse point (Windows) — checked via `symlink_metadata` so the
/// check itself never follows the link.
fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}

/// Compute the graphtor-created filesystem artifacts that [`uninstall`]
/// would remove, WITHOUT removing anything.
///
/// Used to enumerate the exact deletion set for operator approval (PA-3)
/// before [`uninstall`] runs, and as the single source of truth for the
/// deletion allowlist [`uninstall`] itself applies.
///
/// Returns the subset of the known ingestion-capable subdirectories
/// (`bin/`, `data/`, `cache/`, `config/`, `logs/`) that currently exist as
/// real directories — skipping `config/` when `keep_config` is `true`, and
/// skipping any subdirectory that is itself a symlink (those are never
/// touched). A user-dropped file directly in `.graphtor/` (for example, an
/// auto-discovered `.db` file) is NEVER included — only these specific,
/// graphtor-created subdirectory names are ever deletion candidates.
#[must_use]
pub fn plan_uninstall(workspace_dir: &Path, keep_config: bool) -> Vec<PathBuf> {
    if !workspace_dir.is_dir() {
        return Vec::new();
    }
    GRAPHTOR_SUBDIRS
        .iter()
        .filter(|sub| !(keep_config && **sub == "config"))
        .map(|sub| workspace_dir.join(sub))
        .filter(|dir| dir.is_dir() && !is_symlink(dir))
        .collect()
}

/// Execute a previously-computed uninstall plan EXACTLY (PA-3).
///
/// NEVER recomputes [`plan_uninstall`] internally — it operates only on the
/// `planned` entries the caller passes in. This closes the TOCTOU window
/// that would otherwise exist between "compute and display the
/// approval-set plan" and "compute and execute the deletion plan" as two
/// separate [`plan_uninstall`] calls, which could observe different
/// filesystem states (for example, a directory created between the two
/// calls that was never shown to the operator for approval).
///
/// Each entry in `planned` is re-validated immediately before removal —
/// it must still resolve to exactly one of the known graphtor-managed
/// subdirectory names directly under `.graphtor/`, still be a real
/// directory, and still not be a symlink — so a plan that has gone stale
/// since it was computed (an entry deleted, replaced, or turned into a
/// symlink in the interim) fails safe by skipping that entry rather than
/// deleting it unconditionally or using it to justify deleting something
/// else. This never EXPANDS the approved set: only entries present in
/// `planned` are ever considered. As a second, independent layer of
/// defence, `config/` is never deleted when `keep_config` is `true`
/// regardless of whether it appears in `planned` — a caller does not need
/// to trust that the plan it is replaying was itself built with the same
/// `keep_config` value.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when `project_root` cannot be
/// resolved or on I/O failure.
pub fn uninstall_planned(
    project_root: &Path,
    keep_config: bool,
    planned: &[PathBuf],
) -> Result<UninstallResult, GraphtorError> {
    let workspace_dir = project_root.join(GRAPHTOR_DIR);
    // Footprint MUST be captured before any deletion — afterward every
    // subdirectory may be gone, which would misreport a full install as
    // minimal and skip its .gitignore cleanup.
    let footprint = detect_footprint(&workspace_dir);
    let mut removed: Vec<String> = Vec::new();

    for dir in planned {
        let is_known_managed_subdir = dir.parent() == Some(workspace_dir.as_path())
            && dir.file_name().is_some_and(|name| {
                GRAPHTOR_SUBDIRS
                    .iter()
                    .any(|sub| name == std::ffi::OsStr::new(*sub))
            });
        let is_protected_config =
            keep_config && dir.file_name() == Some(std::ffi::OsStr::new("config"));
        // Re-validate immediately before deletion instead of trusting the
        // plan blindly (defence-in-depth against a plan that has gone
        // stale since it was computed): must still be exactly one of the
        // known graphtor-managed subdirectory names directly under
        // `workspace_dir`, still a real directory, still not a symlink,
        // and never `config/` when `keep_config` is `true` — independent
        // of whether the passed-in `planned` slice already honoured that.
        if !is_known_managed_subdir || is_protected_config || !dir.is_dir() || is_symlink(dir) {
            continue;
        }
        fs::remove_dir_all(dir).map_err(|e| GraphtorError::Config {
            message: format!("failed to remove {}: {e}", dir.display()),
            field: None,
        })?;
        removed.push(dir.display().to_string());
    }

    // Clean up the now-possibly-empty workspace root itself, but ONLY when
    // it is genuinely empty afterward. Anything left — a user-dropped
    // `.db` file, a preserved `config/`, or a symlinked subdirectory we
    // deliberately skipped — means the root must survive.
    if workspace_dir.is_dir() {
        let is_empty =
            fs::read_dir(&workspace_dir).is_ok_and(|mut entries| entries.next().is_none());
        if is_empty {
            fs::remove_dir(&workspace_dir).map_err(|e| GraphtorError::Config {
                message: format!("failed to remove {}: {e}", workspace_dir.display()),
                field: None,
            })?;
            removed.push(workspace_dir.display().to_string());
        }
    }

    // .gitignore parity (P2-T5a): only a full-footprint install ever wrote
    // the managed `.gitignore` block (P2-T2b); a minimal install never
    // touched `.gitignore` and uninstall must not either.
    if footprint == WorkspaceFootprint::Full {
        remove_gitignore_entry(project_root)?;
    }

    // Prune the graphtor-docs entry from MCP client configs.
    let mut updated: Vec<String> = Vec::new();
    for outcome in remove_mcp_config(project_root)? {
        match outcome.action {
            McpConfigAction::Removed => removed.push(outcome.path),
            McpConfigAction::Updated => updated.push(outcome.path),
            McpConfigAction::Created => {}
        }
    }

    Ok(UninstallResult { removed, updated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::gitignore::add_gitignore_entry;
    use crate::workspace::install::{install, install_minimal};

    /// Test-only convenience wrapper: computes a fresh [`plan_uninstall`]
    /// result and immediately executes it via [`uninstall_planned`]. Real
    /// callers (`cmd_uninstall` in `main.rs`) MUST reuse a single
    /// already-computed plan across "display" and "execute" — see
    /// [`uninstall_planned`]'s doc comment — so this two-calls-in-one
    /// shorthand exists only for tests that do not care about that
    /// PA-3 exact-plan guarantee.
    fn uninstall(project_root: &Path, keep_config: bool) -> Result<UninstallResult, GraphtorError> {
        let workspace_dir = project_root.join(GRAPHTOR_DIR);
        let planned = plan_uninstall(&workspace_dir, keep_config);
        uninstall_planned(project_root, keep_config, &planned)
    }

    #[test]
    fn uninstall_removes_workspace_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");
        uninstall(tmp.path(), false).expect("uninstall");
        assert!(!tmp.path().join(GRAPHTOR_DIR).exists());
    }

    #[test]
    fn uninstall_planned_never_deletes_a_directory_created_after_the_plan_was_computed() {
        // PA-3: the executed deletion set must be provably identical to
        // the plan an operator was shown for approval. Simulate the exact
        // TOCTOU window: `logs/` is absent when `planned` is computed (as
        // `cmd_uninstall` does, to print the approval set) — for example
        // because keep_config/a partial layout left it missing — THEN the
        // filesystem changes (the directory is created), THEN execute —
        // the late-appearing directory must survive because it was never
        // part of the approved plan, even though it now exists on disk
        // and structurally matches a known managed subdirectory name.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(GRAPHTOR_DIR);
        let late_dir = ws.join("logs");
        fs::remove_dir_all(&late_dir).expect("remove logs/ so it is absent at planning time");

        let planned = plan_uninstall(&ws, false);
        assert!(
            !planned.is_empty(),
            "precondition: a full install still has a non-empty plan without logs/"
        );
        assert!(
            !planned.contains(&late_dir),
            "precondition: logs/ must be absent from the plan computed while it did not exist"
        );

        // Filesystem changes AFTER the plan was computed and shown, but
        // BEFORE execution.
        fs::create_dir_all(&late_dir).expect("simulate late-created subdir");

        uninstall_planned(tmp.path(), false, &planned).expect("uninstall_planned");

        assert!(
            late_dir.exists(),
            "a directory that appeared after the plan was computed must survive — it was never \
             part of the approved deletion set"
        );
    }

    #[test]
    fn uninstall_planned_skips_a_planned_entry_that_became_a_symlink_before_execution() {
        // Defence-in-depth: if a planned entry's nature changed between
        // planning and execution (here, replaced with a symlink), it must
        // be re-validated and skipped rather than deleted unconditionally.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(GRAPHTOR_DIR);
        let planned = plan_uninstall(&ws, false);

        let logs_dir = ws.join("logs");
        assert!(
            planned.contains(&logs_dir),
            "precondition: logs/ is planned"
        );
        fs::remove_dir_all(&logs_dir).expect("remove real logs dir to replace with a symlink");
        let real_target = tmp.path().join("outside-logs-target");
        fs::create_dir_all(&real_target).expect("create real external target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_target, &logs_dir).expect("create symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real_target, &logs_dir).expect("create symlink");

        uninstall_planned(tmp.path(), false, &planned).expect("uninstall_planned");

        assert!(
            real_target.exists(),
            "a planned entry that became a symlink before execution must be skipped, never \
             followed and deleted"
        );
    }

    #[test]
    fn uninstall_keep_config_preserves_config_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        // Create a config file.
        let config_dir = tmp.path().join(GRAPHTOR_DIR).join("config");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(config_dir.join("sources.yaml"), "sources: []").expect("write");
        uninstall(tmp.path(), true).expect("uninstall keep-config");
        assert!(
            config_dir.join("sources.yaml").exists(),
            "sources.yaml should be preserved"
        );
    }

    #[test]
    fn uninstall_keep_config_false_still_retains_user_dropped_db() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let dropped_db = tmp.path().join(GRAPHTOR_DIR).join("dropped.db");
        fs::write(&dropped_db, b"not a real sqlite file, just a marker").expect("write db");

        uninstall(tmp.path(), false).expect("uninstall");

        assert!(
            dropped_db.exists(),
            "a user-dropped .db file directly in .graphtor/ must survive uninstall"
        );
        for sub in GRAPHTOR_SUBDIRS {
            assert!(
                !tmp.path().join(GRAPHTOR_DIR).join(sub).exists(),
                "the managed {sub} subdirectory must still be removed"
            );
        }
    }

    #[test]
    fn uninstall_keep_config_true_still_retains_user_dropped_db() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let dropped_db = tmp.path().join(GRAPHTOR_DIR).join("dropped.db");
        fs::write(&dropped_db, b"marker").expect("write db");

        uninstall(tmp.path(), true).expect("uninstall keep-config");

        assert!(
            dropped_db.exists(),
            "a user-dropped .db file must survive uninstall --keep-config too"
        );
        assert!(
            tmp.path().join(GRAPHTOR_DIR).join("config").is_dir(),
            "config/ must still be preserved"
        );
    }

    #[test]
    fn uninstall_minimal_workspace_preserves_dropped_db_and_leaves_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let dropped_db = tmp.path().join(GRAPHTOR_DIR).join("dropped.db");
        fs::write(&dropped_db, b"marker").expect("write db");

        uninstall(tmp.path(), false).expect("uninstall minimal");

        assert!(
            dropped_db.exists(),
            "a minimal-footprint uninstall must never delete a dropped .db"
        );
        assert!(
            tmp.path().join(GRAPHTOR_DIR).is_dir(),
            ".graphtor/ itself must survive since it still holds a dropped db"
        );
    }

    #[test]
    fn uninstall_minimal_workspace_never_touches_a_gitignore_it_never_created() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let gitignore_path = tmp.path().join(".gitignore");
        let unrelated_content = "node_modules/\ntarget/\n";
        fs::write(&gitignore_path, unrelated_content).expect("write gitignore");

        uninstall(tmp.path(), false).expect("uninstall minimal");

        let after = fs::read_to_string(&gitignore_path).expect("read gitignore");
        assert_eq!(
            after, unrelated_content,
            "a minimal-footprint uninstall must never modify a .gitignore it never created"
        );
    }

    #[test]
    fn uninstall_full_workspace_removes_its_own_managed_gitignore_block() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");
        let gitignore_path = tmp.path().join(".gitignore");
        assert!(
            fs::read_to_string(&gitignore_path)
                .expect("read")
                .contains(".graphtor/"),
            "precondition: managed entry present"
        );

        uninstall(tmp.path(), false).expect("uninstall");

        let after = fs::read_to_string(&gitignore_path).expect("read after uninstall");
        assert!(
            !after.contains(".graphtor/"),
            "a full-footprint uninstall must remove its own managed .gitignore block: {after}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn uninstall_does_not_follow_a_symlinked_subdir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");

        // Replace .graphtor/bin with a junction pointing at an EXTERNAL
        // directory containing a canary file, simulating an operator's own
        // reparse-point trick.
        let bin_dir = tmp.path().join(GRAPHTOR_DIR).join("bin");
        fs::remove_dir_all(&bin_dir).expect("remove real bin dir");
        let external = tmp.path().join("external-target");
        fs::create_dir_all(&external).expect("create external dir");
        let canary = external.join("canary.txt");
        fs::write(&canary, b"must survive").expect("write canary");

        let junction_result = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                bin_dir.to_str().expect("utf-8 path"),
                external.to_str().expect("utf-8 path"),
            ])
            .output();
        let Ok(output) = junction_result else {
            eprintln!("skipping junction test: unable to invoke mklink in this environment");
            return;
        };
        if !output.status.success() {
            eprintln!("skipping junction test: unable to create a junction in this environment");
            return;
        }

        uninstall(tmp.path(), false).expect("uninstall");

        assert!(
            canary.exists(),
            "uninstall must never follow a symlinked/junctioned subdirectory out of .graphtor/"
        );
    }

    #[test]
    fn plan_uninstall_enumerates_full_layout_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(GRAPHTOR_DIR);

        let planned = plan_uninstall(&ws, false);

        assert_eq!(planned.len(), GRAPHTOR_SUBDIRS.len());
        for sub in GRAPHTOR_SUBDIRS {
            assert!(planned.contains(&ws.join(sub)), "missing {sub} in plan");
        }
    }

    #[test]
    fn plan_uninstall_skips_config_when_keep_config_true() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(GRAPHTOR_DIR);

        let planned = plan_uninstall(&ws, true);

        assert!(!planned.contains(&ws.join("config")));
        assert_eq!(planned.len(), GRAPHTOR_SUBDIRS.len() - 1);
    }

    #[test]
    fn plan_uninstall_returns_empty_for_minimal_layout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(GRAPHTOR_DIR);

        assert!(plan_uninstall(&ws, false).is_empty());
    }
}
