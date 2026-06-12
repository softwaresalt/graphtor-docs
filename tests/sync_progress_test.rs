//! Integration tests for `graphtor-docs sync` progress reporting (041.005-T, 041.007-T).
//!
//! These tests verify the contract introduced by shipment 032-S:
//!
//! * Incremental sync emits `[sync]` progress lines on **stderr**.
//! * Full sync emits `[sync-full] stage-start` / `stage-complete` lines on
//!   **stderr** for the acquire/parse/embed/load stages.
//! * `--metrics` output on **stdout** remains parseable JSON regardless of
//!   the stderr progress chatter (041.005-T).

use std::process::Command;

use serde_json::Value;

fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

/// Incremental sync emits start and completion progress lines on stderr.
#[test]
fn sync_incremental_emits_stderr_progress() {
    let workspace = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        workspace.path().join("guide.md"),
        "# Guide\n\nHello world.\n",
    )
    .expect("write guide");
    let db_path = workspace.path().join("graph.db");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("--db-path")
        .arg(&db_path)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        output.status.success(),
        "sync failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
    assert!(
        stderr.contains("[sync]"),
        "stderr should contain [sync] progress lines; got: {stderr}"
    );
    assert!(
        stderr.contains("starting incremental sync"),
        "stderr should announce sync start; got: {stderr}"
    );
    assert!(
        stderr.contains("processing guide.md (1/1) [100%]"),
        "stderr should announce file processing start; got: {stderr}"
    );
    assert!(
        stderr.contains("completed guide.md (1/1) [100%]"),
        "stderr should announce file processing completion; got: {stderr}"
    );
}

/// Incremental sync with `--metrics` keeps stdout as parseable JSON while
/// emitting human-readable progress on stderr.
#[test]
fn sync_incremental_metrics_preserved_alongside_stderr_progress() {
    let workspace = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        workspace.path().join("guide.md"),
        "# Guide\n\nHello world.\n",
    )
    .expect("write guide");
    let db_path = workspace.path().join("graph.db");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("--db-path")
        .arg(&db_path)
        .arg("sync")
        .arg("--no-embed")
        .arg("--metrics")
        .output()
        .expect("run graphtor-docs sync --metrics");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value =
        serde_json::from_str(&stdout).expect("stdout should still be parseable JSON");
    assert_eq!(parsed["files_synced"], 1, "stdout: {stdout}");

    let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
    assert!(
        stderr.contains("[sync]"),
        "progress should still appear on stderr; got: {stderr}"
    );
}

/// Full sync emits stage-start / stage-complete announcements on stderr
/// (041.007-T contract).
#[test]
fn sync_full_emits_stage_announcements_on_stderr() {
    let workspace = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        workspace.path().join("guide.md"),
        "# Guide\n\nHello world.\n",
    )
    .expect("write guide");
    let db_path = workspace.path().join("graph.db");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("--db-path")
        .arg(&db_path)
        .arg("sync")
        .arg("--full")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync --full");

    assert!(
        output.status.success(),
        "full sync failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
    for stage in ["acquire", "parse", "embed", "load"] {
        assert!(
            stderr.contains(&format!("stage-start: {stage}")),
            "stderr missing stage-start for {stage}; got: {stderr}"
        );
        assert!(
            stderr.contains(&format!("stage-complete: {stage}")),
            "stderr missing stage-complete for {stage}; got: {stderr}"
        );
    }
}
