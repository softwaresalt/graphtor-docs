//! Binary integration tests for source registry normalization (040-F).
//!
//! Verifies that `graphtor-docs sync` blocks when cross-database duplicate
//! intakes are detected (exit code 2) and proceeds when `--force` is passed.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;

fn write_yaml(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    let mut f = fs::File::create(&path).expect("create yaml file");
    f.write_all(content.as_bytes()).expect("write yaml");
}

fn graphtor_bin() -> String {
    env!("CARGO_BIN_EXE_graphtor-docs").to_string()
}

/// Set up a temporary workspace with a `.graphtor/config` directory.
fn setup_workspace(dir: &Path) {
    let config_dir = dir.join(".graphtor/config");
    fs::create_dir_all(&config_dir).expect("create config dir");
}

// ── T040.005: sync preflight blocks on cross-db duplicates ───────────────────

#[test]
fn sync_preflight_blocks_on_cross_db_duplicates_without_force() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();
    setup_workspace(workspace);

    let config_dir = workspace.join(".graphtor/config");

    // Two pattern files pointing the same URL to different databases.
    write_yaml(
        &config_dir,
        "alpha.sources.yaml",
        "sources:\n  - type: git\n    id: shared-a\n    url: https://github.com/example/shared.git\n    branch: main\n    database: alpha.db\n",
    );
    write_yaml(
        &config_dir,
        "beta.sources.yaml",
        "sources:\n  - type: git\n    id: shared-b\n    url: https://github.com/example/shared.git\n    branch: main\n    database: beta.db\n",
    );

    let output = Command::new(graphtor_bin())
        .arg("sync")
        .arg("--no-embed")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs");

    assert_eq!(
        output.status.code(),
        Some(2),
        "should exit 2 on duplicate detection without --force"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cross-database"),
        "stderr should describe the cross-database conflict: {stderr}"
    );
    assert!(
        stderr.contains("--force"),
        "stderr should hint at --force: {stderr}"
    );
}

// ── T040.006: sync --force proceeds past duplicates ──────────────────────────

#[test]
fn sync_force_flag_proceeds_past_cross_db_duplicates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();
    setup_workspace(workspace);

    let config_dir = workspace.join(".graphtor/config");

    write_yaml(
        &config_dir,
        "alpha.sources.yaml",
        "sources:\n  - type: git\n    id: shared-a\n    url: https://github.com/example/shared.git\n    branch: main\n    database: alpha.db\n",
    );
    write_yaml(
        &config_dir,
        "beta.sources.yaml",
        "sources:\n  - type: git\n    id: shared-b\n    url: https://github.com/example/shared.git\n    branch: main\n    database: beta.db\n",
    );

    let output = Command::new(graphtor_bin())
        .arg("sync")
        .arg("--no-embed")
        .arg("--force")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs");

    // With --force the exit code must not be 2 (even if acquisition fails
    // for network reasons, the preflight must not be the blocker).
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        output.status.code(),
        Some(2),
        "should not exit 2 on duplicate detection with --force; stderr: {stderr}"
    );
    assert!(
        stderr.contains("warning"),
        "--force should emit a warning, not an error: {stderr}"
    );
    assert!(
        stderr.contains("cross-database"),
        "warning should describe the cross-database conflict: {stderr}"
    );
}
