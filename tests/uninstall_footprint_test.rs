//! Integration tests for footprint-safe uninstall + PA-3 approval-set
//! enumeration (P2-T5a): the CLI enumerates the exact deletion set before
//! any deletion, and a user-dropped `.db` file always survives.

use std::process::Command;

use serde_json::Value;

fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn uninstall_enumerates_planned_deletion_before_confirming_completion() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let install_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--with-ingestion", "--no-gitignore"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(install_output.status.success());

    let uninstall_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["uninstall", "--confirm"])
        .output()
        .expect("run graphtor-docs uninstall --confirm");
    assert!(
        uninstall_output.status.success(),
        "uninstall failed: stderr={}",
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    let stdout = String::from_utf8(uninstall_output.stdout).expect("stdout utf-8");
    let plan_pos = stdout
        .find("the following graphtor-managed directories will be removed:")
        .unwrap_or_else(|| panic!("expected PA-3 enumeration header in stdout: {stdout}"));
    let complete_pos = stdout
        .find("uninstall complete")
        .unwrap_or_else(|| panic!("expected completion line in stdout: {stdout}"));
    assert!(
        plan_pos < complete_pos,
        "the deletion plan must be enumerated BEFORE the completion line: {stdout}"
    );
    assert!(stdout.contains("bin"), "plan should list bin/: {stdout}");
}

#[test]
fn uninstall_preserves_a_dropped_db_end_to_end() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let install_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--with-ingestion", "--no-gitignore"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(install_output.status.success());

    let dropped_db = ws.join(".graphtor").join("dropped.db");
    std::fs::write(&dropped_db, b"marker").expect("write dropped db");

    let uninstall_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["uninstall", "--confirm", "--json"])
        .output()
        .expect("run graphtor-docs uninstall --confirm --json");
    assert!(uninstall_output.status.success());

    assert!(
        dropped_db.exists(),
        "a user-dropped .db file must survive uninstall end-to-end"
    );

    let stdout = String::from_utf8(uninstall_output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let planned = parsed["result"]["planned_removal"]
        .as_array()
        .expect("planned_removal array");
    assert!(
        planned
            .iter()
            .all(|p| !p.as_str().unwrap_or_default().contains("dropped.db")),
        "the dropped db must never appear in the planned removal set: {stdout}"
    );
}

#[test]
fn uninstall_removes_the_emptied_workspace_root_end_to_end() {
    // F4 regression: the workspace lock file (`.graphtor/graphtor.lock`) lives
    // inside `.graphtor/` while the lock is held, so an in-lock root removal
    // could never see the directory as empty. After the managed removals the
    // CLI must release the lock and then reclaim the now-empty root, so
    // `.graphtor/` must NOT survive an otherwise-empty uninstall.
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let install_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--with-ingestion", "--no-gitignore"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(
        install_output.status.success(),
        "install failed: stderr={}",
        String::from_utf8_lossy(&install_output.stderr)
    );

    let uninstall_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["uninstall", "--confirm"])
        .output()
        .expect("run graphtor-docs uninstall --confirm");
    assert!(
        uninstall_output.status.success(),
        "uninstall failed: stderr={}",
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    assert!(
        !ws.join(".graphtor").exists(),
        "the emptied .graphtor/ workspace root must be removed once the lock is released: \
         stdout={}",
        String::from_utf8_lossy(&uninstall_output.stdout)
    );
}

#[test]
fn uninstall_json_enumerates_all_destructive_mutations() {
    // F5 regression: the plan/result JSON must enumerate EVERY destructive
    // mutation before execution — not just managed subdirectories, but also
    // the `.gitignore` cleanup, the MCP config changes, and the `.graphtor/`
    // root removal.
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    // A full install WITH gitignore management writes both the managed
    // `.gitignore` block and the workspace-root `.mcp.json`.
    let install_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--with-ingestion"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(
        install_output.status.success(),
        "install failed: stderr={}",
        String::from_utf8_lossy(&install_output.stderr)
    );

    let uninstall_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["uninstall", "--confirm", "--json"])
        .output()
        .expect("run graphtor-docs uninstall --confirm --json");
    assert!(
        uninstall_output.status.success(),
        "uninstall failed: stderr={}",
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    let stdout = String::from_utf8(uninstall_output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let result = &parsed["result"];

    assert!(
        result["planned_removal"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "planned_removal must enumerate the managed subdirectories: {stdout}"
    );
    assert_eq!(
        result["planned_gitignore_cleanup"], true,
        "the plan must record the managed .gitignore cleanup: {stdout}"
    );
    assert!(
        result["planned_mcp_config_changes"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p == ".mcp.json")),
        "the plan must record the MCP config change to .mcp.json: {stdout}"
    );
    assert_eq!(
        result["planned_root_removal"], true,
        "the plan must record that the emptied .graphtor/ root will be removed: {stdout}"
    );
    assert!(
        result["removed"].as_array().is_some_and(|a| a
            .iter()
            .any(|p| p.as_str().unwrap_or_default().ends_with(".graphtor"))),
        "the executed result must report the removed .graphtor/ root: {stdout}"
    );
}
