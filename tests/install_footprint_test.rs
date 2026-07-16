//! Integration tests for backward-compat detection + idempotency (P2-T6):
//! `install`'s reported `footprint` reflects ACTUAL on-disk state, and
//! repeated installs (default or `--with-ingestion`) are stable/idempotent.

use std::process::Command;

use serde_json::Value;

fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn default_install_over_existing_full_layout_reports_full_footprint_in_json() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    // First, a full (--with-ingestion) install.
    let full_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--with-ingestion", "--no-gitignore"])
        .output()
        .expect("run graphtor-docs install --with-ingestion");
    assert!(
        full_output.status.success(),
        "full install failed: stderr={}",
        String::from_utf8_lossy(&full_output.stderr)
    );

    // Now the new consumption-first DEFAULT, against the existing full layout.
    let default_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .args(["install", "--json"])
        .output()
        .expect("run graphtor-docs install --json");
    assert!(
        default_output.status.success(),
        "default install over an existing full layout failed: stderr={}",
        String::from_utf8_lossy(&default_output.stderr)
    );

    let stdout = String::from_utf8(default_output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(
        parsed["result"]["footprint"], "full",
        "footprint must reflect ACTUAL on-disk state (full), not just \"which install path \
         ran\": {stdout}"
    );
}

#[test]
fn default_install_json_footprint_is_minimal_on_a_fresh_workspace() {
    let workspace = tempfile::tempdir().expect("tempdir");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args(["install", "--json"])
        .output()
        .expect("run graphtor-docs install --json");
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert_eq!(parsed["result"]["footprint"], "minimal");
}
