//! Binary integration tests for source registry normalization (040-F).
//!
//! Verifies that `graphtor-docs sync`, `prewarm`, and `serve` all block when
//! cross-database duplicate intakes are detected (exit code 2), and that
//! `sync --force` proceeds past duplicates while emitting a warning.

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

/// Create a shared source directory with one valid docline markdown file and
/// write two `*.sources.yaml` registry files pointing to the same path but
/// different databases.  Returns the absolute path of the shared source dir
/// (for use in `path:` fields).
fn setup_cross_db_duplicate_workspace(workspace: &Path) -> std::path::PathBuf {
    setup_workspace(workspace);

    let shared = workspace.join("shared");
    fs::create_dir_all(&shared).expect("create shared dir");
    fs::write(
        shared.join("readme.md"),
        b"---\ntitle: Shared\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\n\
          doc_type: markdown\nsource_path: readme.md\n---\n# Shared\n",
    )
    .expect("write readme");

    let shared_str = shared.to_string_lossy().replace('\\', "/");
    let config_dir = workspace.join(".graphtor/config");

    write_yaml(
        &config_dir,
        "alpha.sources.yaml",
        &format!(
            "sources:\n  - type: local\n    id: shared-a\n    path: {shared_str}\n    database: alpha.db\n"
        ),
    );
    write_yaml(
        &config_dir,
        "beta.sources.yaml",
        &format!(
            "sources:\n  - type: local\n    id: shared-b\n    path: {shared_str}\n    database: beta.db\n"
        ),
    );

    shared
}

// ── T040.005: sync preflight blocks on cross-db duplicates ───────────────────

#[test]
fn sync_preflight_blocks_on_cross_db_duplicates_without_force() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();
    setup_cross_db_duplicate_workspace(workspace);

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
    setup_cross_db_duplicate_workspace(workspace);

    let output = Command::new(graphtor_bin())
        .arg("sync")
        .arg("--no-embed")
        .arg("--force")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs");

    // With --force the exit code must not be 2 (the preflight must not block).
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

// ── T040.007: prewarm preflight blocks on cross-db duplicates ─────────────────

/// `prewarm` is a write path that mutates databases; it must be protected by
/// the same fail-closed duplicate-intake preflight as `sync`.
#[test]
fn prewarm_preflight_blocks_on_cross_db_duplicates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();
    setup_cross_db_duplicate_workspace(workspace);

    let output = Command::new(graphtor_bin())
        .arg("prewarm")
        .arg("--no-embed")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs prewarm");

    assert_eq!(
        output.status.code(),
        Some(2),
        "prewarm should exit 2 on duplicate detection (fail-closed)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cross-database"),
        "prewarm stderr should describe the cross-database conflict: {stderr}"
    );
    // prewarm has no --force flag; the message should NOT suggest it as an escape
    // hatch in a confusing way — but checking the error prefix is sufficient.
    assert!(
        stderr.contains("error:"),
        "prewarm should emit 'error:' prefix on duplicate detection: {stderr}"
    );
}

// ── T040.008: serve preflight blocks on cross-db duplicates ──────────────────

/// `serve` spawns a background write path (incremental sync) on startup; it
/// must be protected by the same fail-closed duplicate-intake preflight.
///
/// When duplicates are detected the binary must exit with code 2 **before**
/// binding the MCP STDIO server (i.e. before blocking indefinitely).
/// If the preflight is absent or broken this test will hang until the test
/// harness times it out.
#[test]
fn serve_preflight_blocks_on_cross_db_duplicates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();
    setup_cross_db_duplicate_workspace(workspace);

    let output = Command::new(graphtor_bin())
        .arg("serve")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(
        output.status.code(),
        Some(2),
        "serve should exit 2 on duplicate detection before starting the MCP server"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cross-database"),
        "serve stderr should describe the cross-database conflict: {stderr}"
    );
    assert!(
        stderr.contains("error:"),
        "serve should emit 'error:' prefix on duplicate detection: {stderr}"
    );
}

// ── T040.009: serve fails closed on malformed/invalid registry ───────────────

/// `serve` must exit with a non-zero code (matching `sync`/`status` behaviour)
/// when the source registry exists but is malformed.  Before the fix,
/// `cmd_serve` swallowed the error, logged a warning, and started the MCP
/// server against the primary database only — hiding the misconfiguration from
/// the operator.
#[test]
fn serve_exits_nonzero_on_malformed_registry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workspace = tmp.path();

    // Create .graphtor/config/ with a syntactically invalid sources.yaml.
    let config_dir = workspace.join(".graphtor/config");
    fs::create_dir_all(&config_dir).expect("create config dir");
    write_yaml(
        &config_dir,
        "sources.yaml",
        // Unterminated YAML sequence — guaranteed parse failure.
        "sources:\n  - type: local\n    id: broken\n    path: [unterminated\n",
    );

    let output = Command::new(graphtor_bin())
        .arg("serve")
        .current_dir(workspace)
        .output()
        .expect("run graphtor-docs serve");

    // Must exit non-zero (the process should not hang and must not return 0).
    assert_ne!(
        output.status.code(),
        Some(0),
        "serve must exit non-zero on malformed registry"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "serve must write an error message for malformed registry; got empty stderr"
    );
    // The error message should indicate it's a registry/config problem.
    assert!(
        stderr.contains("error:"),
        "serve should emit 'error:' prefix for malformed registry: {stderr}"
    );
}
