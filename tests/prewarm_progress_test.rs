//! Integration tests for `graphtor-docs prewarm`.

use std::process::Command;

use serde_json::Value;

/// Helper: path to the compiled binary.
fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn prewarm_emits_stderr_progress_and_stdout_jsonl() {
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
        .arg("prewarm")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs prewarm");

    assert!(
        output.status.success(),
        "prewarm failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert_eq!(
        parsed["event_type"], "prewarm.complete",
        "event_type mismatch; stdout: {stdout}"
    );
    assert!(
        parsed["timestamp"].is_string(),
        "timestamp field missing or not a string; stdout: {stdout}"
    );
    assert!(
        parsed["payload"]["files_total"].is_number(),
        "payload.files_total missing; stdout: {stdout}"
    );
    assert!(
        parsed["payload"]["files_synced"].is_number(),
        "payload.files_synced missing; stdout: {stdout}"
    );
    assert!(
        parsed["payload"]["chunks_created"].is_number(),
        "payload.chunks_created missing; stdout: {stdout}"
    );
    assert!(
        parsed["payload"]["duration_ms"].is_number(),
        "payload.duration_ms missing; stdout: {stdout}"
    );
    assert!(
        parsed["payload"]["sources_count"].is_number(),
        "payload.sources_count missing; stdout: {stdout}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
    assert!(
        stderr.contains("[syncing]"),
        "stderr should contain [syncing] progress lines; got: {stderr}"
    );
}

#[test]
fn prewarm_quiet_suppresses_stderr_progress() {
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
        .arg("prewarm")
        .arg("--no-embed")
        .arg("--quiet")
        .output()
        .expect("run graphtor-docs prewarm --quiet");

    assert!(
        output.status.success(),
        "prewarm --quiet failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON even with --quiet");

    assert_eq!(
        parsed["event_type"], "prewarm.complete",
        "event_type mismatch with --quiet; stdout: {stdout}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr utf-8");
    assert!(
        !stderr.contains("[syncing]"),
        "stderr should not contain [syncing] progress lines with --quiet; got: {stderr}"
    );
}
