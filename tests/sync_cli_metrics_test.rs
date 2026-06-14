//! Integration test for `graphtor-docs sync --metrics`.

use std::process::Command;

use serde_json::Value;

#[test]
fn sync_metrics_flag_emits_raw_json_metrics() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");
    std::fs::write(docs_dir.join("guide.md"), b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello world.\n").expect("write guide");

    let config_dir = workspace.path().join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        "sources:\n  - type: local\n    id: guide\n    path: docs\n    include:\n      - \"**/*.md\"\n",
    )
    .expect("write sources.yaml");

    let db_path = workspace.path().join("graph.db");

    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));

    let output = Command::new(&exe)
        .current_dir(workspace.path())
        .arg("--db-path")
        .arg(&db_path)
        .arg("sync")
        .arg("--no-embed")
        .arg("--metrics")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        output.status.success(),
        "sync command failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(&stdout).expect("raw metrics json");

    assert_eq!(parsed["files_total"], 1, "stdout: {stdout}");
    assert_eq!(parsed["files_synced"], 1, "stdout: {stdout}");
    assert_eq!(parsed["files_deleted"], 0, "stdout: {stdout}");
    assert_eq!(parsed["chunks_deleted"], 0, "stdout: {stdout}");
    assert_eq!(parsed["errors"], 0, "stdout: {stdout}");
    assert!(
        parsed["chunks_created"].as_u64().is_some_and(|v| v > 0),
        "stdout: {stdout}"
    );
    assert!(
        parsed["duration_ms"].as_u64().is_some_and(|v| v > 0),
        "stdout: {stdout}"
    );
}
