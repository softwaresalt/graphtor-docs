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

use crate::workspace::gitignore::{has_managed_gitignore_block, remove_gitignore_entry};
use crate::workspace::mcp_config::{
    file_has_managed_entry, managed_config_candidates, remove_mcp_config_from, McpConfigAction,
};
use crate::workspace::paths::{GRAPHTOR_DIR, GRAPHTOR_SUBDIRS};
use graphtor_core::path::is_reparse_point;
use graphtor_core::GraphtorError;

/// Filename of the workspace advisory lock created inside `.graphtor/`.
/// Mirrors `graphtor_core::lock`'s private `WORKSPACE_LOCK_FILE` — the root
/// cleanup planner must ignore it because the caller releases the lock (which
/// deletes this file) before attempting to remove the now-empty root.
const WORKSPACE_LOCK_FILE: &str = "graphtor.lock";

/// Result of an uninstall operation.
#[derive(Debug)]
pub struct UninstallResult {
    /// Files and directories deleted.
    pub removed: Vec<String>,
    /// Shared MCP configs updated in place (graphtor-docs entry pruned, file kept).
    pub updated: Vec<String>,
}

/// The complete set of destructive mutations an uninstall will perform,
/// computed WITHOUT changing anything so it can be shown to the operator and
/// serialized before execution (see [`plan_uninstall_full`]).
///
/// This is the richer, whole-operation counterpart to [`plan_uninstall`]
/// (which enumerates only the managed subdirectories). It exists so every
/// destructive effect — not just directory removals — is enumerated up front:
/// the `.gitignore` cleanup, the MCP client config files that may be pruned or
/// deleted, and whether the `.graphtor/` root itself is expected to be
/// removed.
#[derive(Debug)]
pub struct UninstallPlan {
    /// Managed subdirectories under `.graphtor/` that will be removed (the
    /// exact PA-3 deletion set — see [`plan_uninstall`]).
    pub managed_dirs: Vec<PathBuf>,
    /// Whether the managed `.graphtor/` block will be removed from
    /// `.gitignore` (only a full-footprint install ever wrote it).
    pub gitignore_cleanup: bool,
    /// MCP client config files (relative to the project root) that currently
    /// exist and may have their graphtor-docs entry pruned — or be deleted
    /// entirely if that entry was the file's sole server.
    pub mcp_config_files: Vec<String>,
    /// Best-effort prediction of whether the `.graphtor/` root itself will be
    /// removed: `true` only when nothing non-managed (a dropped `.db`, a
    /// preserved `config/`, a skipped symlink) would remain after the managed
    /// removals and lock release. A late concurrent write simply leaves the
    /// root in place, which is safe.
    pub root_removal: bool,
}

/// Returns `true` when `path` is itself a symlink (Unix) or a
/// junction/reparse point (Windows). Thin alias over
/// [`graphtor_core::path::is_reparse_point`] so this module's deletion guards
/// read locally; the check uses `symlink_metadata` and never follows the link.
fn is_symlink(path: &Path) -> bool {
    is_reparse_point(path)
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
    // Workspace-containment guard (Constitution III/IV): if the `.graphtor`
    // root itself is a symlink/junction, every `workspace_dir.join(sub)`
    // resolves THROUGH the link, so a planned `remove_dir_all` would delete a
    // directory OUTSIDE the project. A linked root has no in-project managed
    // subdirectories to remove — plan nothing.
    if is_symlink(workspace_dir) || !workspace_dir.is_dir() {
        return Vec::new();
    }
    GRAPHTOR_SUBDIRS
        .iter()
        .filter(|sub| !(keep_config && **sub == "config"))
        .map(|sub| workspace_dir.join(sub))
        .filter(|dir| dir.is_dir() && !is_symlink(dir))
        .collect()
}

/// Enumerate EVERY destructive mutation an uninstall of `project_root` will
/// perform, WITHOUT changing anything.
///
/// Expands [`plan_uninstall`] (managed subdirectories only) into the full
/// operation footprint so an operator can be shown — and JSON can serialize —
/// the complete blast radius before any deletion: the managed subdirectories,
/// whether the `.gitignore` managed block will be cleaned, which MCP client
/// config files may be pruned or deleted, and whether the `.graphtor/` root
/// itself is expected to be removed once emptied.
///
/// The root-removal field is best-effort: it predicts emptiness from the
/// current directory contents (ignoring the workspace lock file, which the
/// caller releases before removing the root). A late concurrent write simply
/// leaves the root in place, which is safe.
#[must_use]
pub fn plan_uninstall_full(project_root: &Path, keep_config: bool) -> UninstallPlan {
    let workspace_dir = project_root.join(GRAPHTOR_DIR);
    let managed_dirs = plan_uninstall(&workspace_dir, keep_config);
    // Clean the managed `.gitignore` block only when it is actually present —
    // identified by the graphtor-docs marker header, NOT inferred from the
    // workspace footprint. A Full install created with `--no-gitignore` (or one
    // whose `.graphtor/` line is a user's own, unmarked entry) has no managed
    // block, so uninstall must not touch `.gitignore` and risk deleting a line
    // graphtor-docs never authored.
    let gitignore_cleanup = has_managed_gitignore_block(project_root);
    // MCP client config files that currently hold a managed graphtor-docs entry
    // and will therefore have it pruned (or the file deleted if it was the sole
    // server). The candidate set comes from `mcp_config::managed_config_candidates`
    // (its single source of truth) and is filtered through the SAME managed-entry
    // predicate execution uses (`file_has_managed_entry`), so the operator-approval
    // preview lists only files that will actually be modified — a candidate that
    // exists but holds no managed entry (a user's own `.mcp.json`) is left off the
    // plan and untouched by execution.
    let mcp_config_files: Vec<String> = managed_config_candidates()
        .into_iter()
        .filter(|rel| file_has_managed_entry(project_root, rel))
        .collect();
    let root_removal = predict_root_removal(&workspace_dir, &managed_dirs);
    UninstallPlan {
        managed_dirs,
        gitignore_cleanup,
        mcp_config_files,
        root_removal,
    }
}

/// Predict whether the `.graphtor/` root will be empty (and thus removed)
/// after the managed removals and lock release.
///
/// The root is expected to be removed only when every current entry is either
/// a managed directory slated for removal or the workspace lock file (which
/// the caller releases before root removal). Any other entry — a user-dropped
/// `.db`, a preserved `config/` under `--keep-config`, or a skipped symlinked
/// subdirectory — keeps the root alive.
fn predict_root_removal(workspace_dir: &Path, managed_dirs: &[PathBuf]) -> bool {
    // Containment (Constitution III/IV): a symlinked/junction `.graphtor` root
    // is never removed by `remove_empty_workspace_root` (it fails safe), so the
    // plan must not predict its removal either — following the link to report an
    // empty external target as a removable root would misstate the blast radius.
    if is_symlink(workspace_dir) {
        return false;
    }
    if !workspace_dir.is_dir() {
        return false;
    }
    let Ok(entries) = fs::read_dir(workspace_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // The workspace lock file is deleted when the caller releases the lock
        // before root removal, so it does not block emptiness.
        if path.file_name() == Some(std::ffi::OsStr::new(WORKSPACE_LOCK_FILE)) {
            continue;
        }
        // A managed directory that will actually be removed does not block
        // removal either.
        if managed_dirs.contains(&path) {
            continue;
        }
        // Anything else keeps the root intact.
        return false;
    }
    true
}

/// Remove the `.graphtor/` workspace root, but ONLY when it is genuinely
/// empty right now.
///
/// Emptiness is re-checked immediately before removal to keep the window
/// between the check and the unlink as small as possible, and removal uses
/// [`fs::remove_dir`] — never `remove_dir_all` — so a concurrent re-install
/// that repopulated the directory in that window causes a harmless failure
/// rather than destroying freshly-written data.
///
/// Returns `Ok(Some(path))` when the root was removed and `Ok(None)` when it
/// was left intact (missing or non-empty).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if the (verified-empty) directory cannot
/// be removed.
pub fn remove_empty_workspace_root(workspace_dir: &Path) -> Result<Option<String>, GraphtorError> {
    // Workspace-containment guard (Constitution III/IV): never operate on a
    // symlinked/junction `.graphtor` root. `is_dir()`/`read_dir()` below follow
    // the link, so the emptiness check would pass for a link to an empty
    // external directory and `fs::remove_dir` would then unlink the operator's
    // reparse point (Windows) or hard-error with ENOTDIR (Unix) instead of
    // failing safe. A linked root is never a graphtor-created empty root — skip.
    if is_symlink(workspace_dir) {
        return Ok(None);
    }
    if !workspace_dir.is_dir() {
        return Ok(None);
    }
    let is_empty = fs::read_dir(workspace_dir).is_ok_and(|mut entries| entries.next().is_none());
    if !is_empty {
        return Ok(None);
    }
    if let Err(e) = fs::remove_dir(workspace_dir) {
        // Race tolerance: a concurrent re-install can repopulate the
        // just-verified-empty root between the emptiness check above and this
        // unlink, making `remove_dir` fail with "directory not empty". That is
        // a benign no-op (the freshly-written data must survive and the other
        // approved removals already succeeded), so surfacing it as an uninstall
        // failure would be misleading. Distinguish that race from a genuine
        // removal error WITHOUT the MSRV-1.75-unstable `ErrorKind::DirectoryNotEmpty`
        // (stabilized in 1.83): re-inspect the root. If it vanished (another
        // process removed it) or is now non-empty, treat it as `Ok(None)`; if it
        // is still an empty directory, the removal genuinely failed (permissions,
        // a lock) — surface that error.
        if !workspace_dir.exists() {
            return Ok(None);
        }
        let now_nonempty =
            fs::read_dir(workspace_dir).is_ok_and(|mut entries| entries.next().is_some());
        if now_nonempty {
            return Ok(None);
        }
        return Err(GraphtorError::Config {
            message: format!("failed to remove {}: {e}", workspace_dir.display()),
            field: None,
        });
    }
    Ok(Some(workspace_dir.display().to_string()))
}

/// Execute a previously-computed uninstall plan EXACTLY (PA-3 / F5).
///
/// NEVER recomputes any part of the plan internally — it operates only on the
/// approved [`UninstallPlan`] the caller passes in. This closes the TOCTOU
/// window that would otherwise exist between "compute and display the
/// approval-set plan" and "compute and execute the mutations": every
/// destructive effect (managed subdirectory removals, the `.gitignore`
/// cleanup, and the MCP config pruning) is replayed from the approved plan, so
/// a directory, `.gitignore` block, or managed MCP config file that appeared
/// AFTER the plan was shown is never mutated without operator approval.
///
/// Re-validation only ever SHRINKS the approved set (skip), never EXPANDS it:
///
/// * Each managed directory in `plan.managed_dirs` is re-validated immediately
///   before removal — it must still resolve to exactly one of the known
///   graphtor-managed subdirectory names directly under `.graphtor/`, still be
///   a real directory, and still not be a symlink — so a stale plan (an entry
///   deleted, replaced, or turned into a symlink in the interim) fails safe by
///   skipping that entry. As a second, independent layer of defence, `config/`
///   is never deleted when `keep_config` is `true` regardless of the plan.
/// * `.gitignore` is cleaned ONLY when `plan.gitignore_cleanup` is `true`
///   (never when the plan said `false`).
/// * MCP configs are pruned ONLY for the files in `plan.mcp_config_files`; a
///   listed file that no longer holds a managed entry is skipped inside
///   [`remove_mcp_config_from`].
///
/// Root removal is deliberately NOT performed here — it is solely the caller's
/// responsibility, gated on `plan.root_removal` and performed AFTER releasing
/// the workspace lock (see `cmd_uninstall`), so the lock file never blocks an
/// otherwise-empty root and an unapproved root is never removed.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when `project_root` cannot be
/// resolved or on I/O failure.
pub fn uninstall_planned(
    project_root: &Path,
    keep_config: bool,
    plan: &UninstallPlan,
) -> Result<UninstallResult, GraphtorError> {
    let workspace_dir = project_root.join(GRAPHTOR_DIR);
    let mut removed: Vec<String> = Vec::new();

    // Workspace-containment guard (Constitution III/IV): never delete THROUGH a
    // symlinked/junction `.graphtor` root. `plan_uninstall` already returns an
    // empty managed set for a linked root, but re-validate here so a STALE plan
    // (root turned into a link after planning) can never drive `remove_dir_all`
    // into an external directory. The per-subdir `is_symlink` check below only
    // catches a linked SUBDIR, not a linked ROOT whose real child dirs report
    // as non-links. The `.gitignore`/MCP cleanup further down operates on
    // separately-guarded project files (not on `.graphtor`) and is unaffected.
    let root_is_link = is_symlink(&workspace_dir);

    for dir in &plan.managed_dirs {
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
        // stale since it was computed): the root must not be a link, and the
        // entry must still be exactly one of the known graphtor-managed
        // subdirectory names directly under `workspace_dir`, still a real
        // directory, still not a symlink, and never `config/` when
        // `keep_config` is `true` — independent of whether the passed-in plan
        // already honoured that.
        if root_is_link
            || !is_known_managed_subdir
            || is_protected_config
            || !dir.is_dir()
            || is_symlink(dir)
        {
            continue;
        }
        fs::remove_dir_all(dir).map_err(|e| GraphtorError::Config {
            message: format!("failed to remove {}: {e}", dir.display()),
            field: None,
        })?;
        removed.push(dir.display().to_string());
    }

    // .gitignore parity (P2-T5a) / F5: clean ONLY when the approved plan said
    // so. `plan.gitignore_cleanup` was set from the presence of the managed
    // marker block at plan time (not merely the footprint), and
    // `remove_gitignore_entry` is itself marker-scoped, so this never deletes a
    // user's own unmarked `.graphtor/` line and never cleans a block that
    // appeared after the plan was shown.
    if plan.gitignore_cleanup {
        remove_gitignore_entry(project_root)?;
    }

    // Prune the graphtor-docs entry from ONLY the MCP client configs the
    // approved plan enumerated (F5). A managed config file created after the
    // plan was shown is never in this list and is therefore never mutated; a
    // listed file that no longer holds a managed entry is a harmless skip.
    let mut updated: Vec<String> = Vec::new();
    for outcome in remove_mcp_config_from(project_root, &plan.mcp_config_files)? {
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

    /// Test-only convenience wrapper mirroring `cmd_uninstall`'s flow: compute
    /// the full plan once, execute it via [`uninstall_planned`], then perform
    /// the gated root removal (only when `plan.root_removal` was approved).
    /// Real callers (`cmd_uninstall` in `main.rs`) additionally hold and
    /// release the workspace lock around this sequence.
    fn uninstall(project_root: &Path, keep_config: bool) -> Result<UninstallResult, GraphtorError> {
        let plan = plan_uninstall_full(project_root, keep_config);
        let mut result = uninstall_planned(project_root, keep_config, &plan)?;
        if plan.root_removal {
            if let Some(removed_root) =
                remove_empty_workspace_root(&project_root.join(GRAPHTOR_DIR))?
            {
                result.removed.push(removed_root);
            }
        }
        Ok(result)
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

        // Replay the approved plan exactly (the manipulated managed_dirs set).
        let plan = UninstallPlan {
            managed_dirs: planned,
            ..plan_uninstall_full(tmp.path(), false)
        };
        uninstall_planned(tmp.path(), false, &plan).expect("uninstall_planned");

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

        let plan = UninstallPlan {
            managed_dirs: planned,
            ..plan_uninstall_full(tmp.path(), false)
        };
        uninstall_planned(tmp.path(), false, &plan).expect("uninstall_planned");

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

    #[test]
    fn plan_uninstall_full_enumerates_every_destructive_mutation() {
        // F5: the plan must enumerate not just managed subdirectories but also
        // the `.gitignore` cleanup, the MCP config files that may change, and
        // the `.graphtor/` root removal — all before any deletion.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");
        fs::write(
            tmp.path().join(".mcp.json"),
            "{\"mcpServers\":{\"graphtor-docs\":{\"command\":\"graphtor-docs\",\
             \"x-graphtor-managed\":true}}}",
        )
        .expect("write mcp config");

        let plan = plan_uninstall_full(tmp.path(), false);

        assert!(
            !plan.managed_dirs.is_empty(),
            "managed subdirectories must be enumerated"
        );
        assert!(
            plan.gitignore_cleanup,
            "a full install must plan to clean its managed .gitignore block"
        );
        assert!(
            plan.mcp_config_files.iter().any(|p| p == ".mcp.json"),
            "an existing .mcp.json must be listed as a planned MCP change: {:?}",
            plan.mcp_config_files
        );
        assert!(
            plan.root_removal,
            "an otherwise-empty full install must plan to remove the .graphtor/ root"
        );
    }

    #[test]
    fn plan_no_gitignore_cleanup_when_marker_absent_despite_full_footprint() {
        // W1: cleanup must be gated on the managed marker, NOT the footprint. A
        // Full install whose `.gitignore` holds only a user's own unmarked
        // `.graphtor/` line must NOT be scheduled for cleanup — doing so would
        // delete a line graphtor-docs never authored.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install"); // Full footprint
        fs::write(tmp.path().join(".gitignore"), ".graphtor/\n").expect("seed unmarked");
        let plan = plan_uninstall_full(tmp.path(), false);
        assert!(
            !plan.gitignore_cleanup,
            "an unmarked user .graphtor/ line must not schedule gitignore cleanup"
        );
    }

    #[test]
    fn plan_excludes_mcp_config_without_managed_entry() {
        // W2: a candidate config that exists but holds no managed graphtor-docs
        // entry must be left off the plan so the approval preview matches what
        // execution actually prunes.
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(
            tmp.path().join(".mcp.json"),
            "{\"mcpServers\":{\"other-tool\":{\"command\":\"foo\",\"args\":[\"bar\"]}}}",
        )
        .expect("write user-only mcp config");
        let plan = plan_uninstall_full(tmp.path(), false);
        assert!(
            !plan.mcp_config_files.iter().any(|p| p == ".mcp.json"),
            "a user-only .mcp.json (no managed entry) must not be in the plan: {:?}",
            plan.mcp_config_files
        );
    }

    #[test]
    fn plan_uninstall_full_keeps_root_when_a_dropped_db_remains() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        fs::write(tmp.path().join(GRAPHTOR_DIR).join("dropped.db"), b"marker")
            .expect("write dropped db");

        let plan = plan_uninstall_full(tmp.path(), false);

        assert!(
            !plan.root_removal,
            "a user-dropped .db in .graphtor/ must keep the root (root_removal=false)"
        );
    }

    #[test]
    fn plan_uninstall_full_keeps_root_when_keep_config_preserves_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");

        let plan = plan_uninstall_full(tmp.path(), true);

        assert!(
            !plan.root_removal,
            "a preserved config/ under --keep-config must keep the root"
        );
    }

    #[test]
    fn remove_empty_workspace_root_removes_empty_and_keeps_non_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let empty = tmp.path().join("empty-ws");
        fs::create_dir_all(&empty).expect("mkdir empty");
        let removed = remove_empty_workspace_root(&empty).expect("remove empty root");
        assert_eq!(removed, Some(empty.display().to_string()));
        assert!(!empty.exists(), "an empty root must be removed");

        let non_empty = tmp.path().join("non-empty-ws");
        fs::create_dir_all(&non_empty).expect("mkdir non-empty");
        fs::write(non_empty.join("keep.db"), b"x").expect("write marker");
        let kept = remove_empty_workspace_root(&non_empty).expect("no-op on non-empty root");
        assert_eq!(kept, None);
        assert!(
            non_empty.exists(),
            "a non-empty root must never be removed (remove_dir, not remove_dir_all)"
        );
    }

    #[test]
    #[cfg(unix)]
    fn remove_empty_workspace_root_propagates_a_genuine_removal_error() {
        // W6-3 makes `remove_dir` failures race-tolerant: a benign
        // concurrent-repopulation race is swallowed as `Ok(None)`. This must NOT
        // over-swallow a GENUINE removal failure. Deny write on the parent so
        // `remove_dir` on the still-empty child fails with EACCES while the child
        // remains an empty, existing directory — that must surface as an error,
        // not be misclassified as a race.
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let parent = tmp.path().join("locked-parent");
        let empty = parent.join("empty-ws");
        fs::create_dir_all(&empty).expect("mkdir empty child");

        let original = fs::metadata(&parent).expect("parent meta").permissions();
        // r-x: entries are listable (emptiness check passes) but not removable.
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).expect("lock parent");

        // Some CI environments run as root, which bypasses DAC permission bits;
        // there the removal would succeed and the error path cannot be exercised.
        // Probe by attempting a write into the now-read-only parent: if it
        // succeeds we are privileged — restore and self-skip.
        let probe = parent.join(".probe");
        if fs::write(&probe, b"x").is_ok() {
            let _ = fs::remove_file(&probe);
            fs::set_permissions(&parent, original).expect("restore parent perms");
            return;
        }

        let result = remove_empty_workspace_root(&empty);

        // Restore permissions BEFORE asserting so the tempdir always cleans up.
        fs::set_permissions(&parent, original).expect("restore parent perms");

        assert!(
            result.is_err(),
            "a genuine removal failure on a still-empty root must propagate, \
             not be swallowed as a race: {result:?}"
        );
        assert!(
            empty.exists(),
            "the child directory must still exist after a failed removal"
        );
    }

    #[test]
    fn uninstall_planned_does_not_mutate_config_created_after_the_plan() {
        // F5: execution replays the APPROVED plan, never a fresh rescan. A
        // managed `.mcp.json` created AFTER the plan was computed is not in the
        // approved list and must NOT be pruned; the approved `.gitignore`
        // cleanup still runs.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");

        let plan = plan_uninstall_full(tmp.path(), false);
        assert!(
            plan.mcp_config_files.is_empty(),
            "precondition: no MCP config existed at plan time"
        );
        assert!(
            plan.gitignore_cleanup,
            "precondition: a full install plans to clean its gitignore block"
        );

        // A managed MCP config appears AFTER the plan was shown for approval.
        let mcp_path = tmp.path().join(".mcp.json");
        fs::write(
            &mcp_path,
            "{\"mcpServers\":{\"graphtor-docs\":{\"command\":\"graphtor-docs\",\
             \"x-graphtor-managed\":true}}}",
        )
        .expect("write late mcp config");

        uninstall_planned(tmp.path(), false, &plan).expect("uninstall_planned");

        assert!(
            mcp_path.exists(),
            "an MCP config created after the plan must not be pruned by execution"
        );
        let mcp_after = fs::read_to_string(&mcp_path).expect("read mcp");
        assert!(
            mcp_after.contains("graphtor-docs"),
            "the unapproved managed entry must be left intact: {mcp_after}"
        );
        let gitignore_after =
            fs::read_to_string(tmp.path().join(".gitignore")).expect("read gitignore");
        assert!(
            !gitignore_after.contains(".graphtor/"),
            "the approved gitignore cleanup must still run: {gitignore_after}"
        );
    }

    #[test]
    fn uninstall_cleans_the_approved_gitignore_and_mcp_entries() {
        // F5 positive path: entries present at plan time ARE cleaned/pruned.
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");
        let mcp_path = tmp.path().join(".mcp.json");
        fs::write(
            &mcp_path,
            "{\"mcpServers\":{\"graphtor-docs\":{\"command\":\"graphtor-docs\",\
             \"x-graphtor-managed\":true}}}",
        )
        .expect("write mcp config");

        uninstall(tmp.path(), false).expect("uninstall");

        assert!(
            !mcp_path.exists(),
            "an approved .mcp.json whose sole server was graphtor-docs must be removed"
        );
        let gitignore_after =
            fs::read_to_string(tmp.path().join(".gitignore")).expect("read gitignore");
        assert!(
            !gitignore_after.contains(".graphtor/"),
            "the approved managed gitignore block must be cleaned: {gitignore_after}"
        );
    }

    #[test]
    fn uninstall_root_removal_is_gated_on_plan_approval() {
        // F6: root removal must be gated on the APPROVED plan, not on emptiness
        // alone. A dropped `.db` makes the plan predict the root stays
        // (`root_removal == false`); if that entry vanishes concurrently after
        // the managed removals, the now-empty root must STILL NOT be removed —
        // it was never in the approved plan (PA-3).
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let dropped_db = tmp.path().join(GRAPHTOR_DIR).join("dropped.db");
        fs::write(&dropped_db, b"marker").expect("write dropped db");

        let plan = plan_uninstall_full(tmp.path(), false);
        assert!(
            !plan.root_removal,
            "precondition: a dropped .db keeps the root in the approved plan"
        );

        uninstall_planned(tmp.path(), false, &plan).expect("uninstall_planned");
        // The dropped db vanishes concurrently, leaving the root otherwise empty.
        fs::remove_file(&dropped_db).expect("remove dropped db");

        // Gated root removal (as `cmd_uninstall` does): because the plan did
        // NOT approve it, the caller must not remove the root even though it is
        // now empty.
        let ws = tmp.path().join(GRAPHTOR_DIR);
        if plan.root_removal {
            remove_empty_workspace_root(&ws).expect("remove");
        }
        assert!(
            ws.exists(),
            "an unapproved root must never be removed even after it becomes empty"
        );
    }

    // ── workspace containment: symlinked `.graphtor` root (X2) ──────────────

    /// Create a directory symlink cross-platform, returning `Err` when the
    /// platform refuses (e.g. Windows without the symlink privilege) so the
    /// caller can self-skip rather than fail.
    fn try_symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link)
        }
    }

    #[test]
    fn plan_uninstall_plans_nothing_for_a_symlinked_workspace_root() {
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        // The link target contains a real, managed-looking `bin/` subdir.
        fs::create_dir_all(external.path().join("bin")).expect("external/bin");

        let workspace_dir = project.path().join(GRAPHTOR_DIR);
        if try_symlink_dir(external.path(), &workspace_dir).is_err() {
            return; // platform refused symlink creation — skip
        }

        // A linked root resolves its `bin/` through the link, so nothing under
        // it is an in-project managed directory: plan must be empty.
        assert!(
            plan_uninstall(&workspace_dir, false).is_empty(),
            "a symlinked .graphtor root must plan no deletions"
        );
    }

    #[test]
    fn uninstall_planned_never_deletes_through_a_symlinked_root_with_a_stale_plan() {
        // A STALE plan lists `.graphtor/bin` (as if the root were real when the
        // plan was computed), then the root becomes a symlink to an external
        // directory before execution. `remove_dir_all` must NOT delete the
        // external `bin/`; the root-link guard blocks the whole loop.
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let external_bin = external.path().join("bin");
        fs::create_dir_all(&external_bin).expect("external/bin");
        fs::write(external_bin.join("keep.txt"), b"external data").expect("write external file");

        let workspace_dir = project.path().join(GRAPHTOR_DIR);
        if try_symlink_dir(external.path(), &workspace_dir).is_err() {
            return; // platform refused symlink creation — skip
        }

        let stale_plan = UninstallPlan {
            managed_dirs: vec![workspace_dir.join("bin")],
            gitignore_cleanup: false,
            mcp_config_files: Vec::new(),
            root_removal: false,
        };
        let result =
            uninstall_planned(project.path(), false, &stale_plan).expect("uninstall_planned");

        assert!(
            result.removed.is_empty(),
            "no deletion must be recorded when the root is a symlink"
        );
        assert!(
            external_bin.join("keep.txt").exists(),
            "the external directory behind a symlinked .graphtor root must never be deleted"
        );
    }

    #[test]
    fn root_removal_is_skipped_for_a_symlinked_empty_workspace_root() {
        // A `.graphtor` root that is a symlink/junction to an EMPTY external
        // directory must not be treated as a removable empty root: predicting
        // removal would follow the link, and `fs::remove_dir` would unlink the
        // operator's reparse point (Windows) or hard-error with ENOTDIR (Unix)
        // instead of failing safe. Both the prediction and the mutation must
        // skip it, and the external target must survive.
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir"); // empty

        let workspace_dir = project.path().join(GRAPHTOR_DIR);
        if try_symlink_dir(external.path(), &workspace_dir).is_err() {
            return; // platform refused symlink creation — skip
        }

        assert!(
            !predict_root_removal(&workspace_dir, &[]),
            "a symlinked .graphtor root must never be predicted for removal"
        );
        assert!(
            remove_empty_workspace_root(&workspace_dir)
                .expect("guarded removal must fail safe, not error")
                .is_none(),
            "a symlinked .graphtor root must never be removed"
        );
        assert!(
            external.path().exists(),
            "the external target behind a symlinked .graphtor root must survive"
        );
        assert!(
            is_symlink(&workspace_dir),
            "the symlinked root itself must be left intact"
        );
    }
}
