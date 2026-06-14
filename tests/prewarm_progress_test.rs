//! Integration tests for `graphtor-docs prewarm`.

use std::process::Command;

use graphtor_core::lock::DatabaseLock;
use serde_json::Value;

/// Helper: path to the compiled binary.
fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn prewarm_emits_stderr_progress_and_stdout_jsonl() {
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

#[test]
fn prewarm_routes_sources_to_multiple_databases() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let src_a = ws.join("docs_a");
    let src_b = ws.join("docs_b");
    std::fs::create_dir_all(&src_a).expect("create docs_a");
    std::fs::create_dir_all(&src_b).expect("create docs_b");
    std::fs::write(src_a.join("a.md"), b"---\ntitle: A\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: a.md\n---\n# A\n\nContent A.\n").expect("write a");
    std::fs::write(src_b.join("b.md"), b"---\ntitle: B\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: b.md\n---\n# B\n\nContent B.\n").expect("write b");

    let graphtor_dir = ws.join(".graphtor");
    let config_dir = graphtor_dir.join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let sources_yaml = format!(
        r#"sources:
  - type: local
    id: docs-a
    path: {src_a}
    database: "primary.db"
  - type: local
    id: docs-b
    path: {src_b}
    database: "secondary.db"
"#,
        src_a = src_a.display(),
        src_b = src_b.display(),
    );
    std::fs::write(config_dir.join("sources.yaml"), sources_yaml).expect("write sources.yaml");

    let output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("prewarm")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs prewarm");

    assert!(
        output.status.success(),
        "prewarm failed: status={:?}, stderr={}, stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    assert_eq!(
        parsed["payload"]["sources_count"], 2,
        "sources_count mismatch; stdout: {stdout}"
    );

    assert!(
        graphtor_dir.join("primary.db").exists(),
        "primary.db must be created by prewarm"
    );
    assert!(
        graphtor_dir.join("secondary.db").exists(),
        "secondary.db must be created by prewarm"
    );
}

#[test]
fn prewarm_fails_gracefully_when_database_lock_is_held() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let docs_dir = ws.join("docs");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");
    std::fs::write(
        docs_dir.join("guide.md"),
        b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello world.\n",
    )
    .expect("write guide");

    let graphtor_dir = ws.join(".graphtor");
    let config_dir = graphtor_dir.join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        "sources:\n  - type: local\n    id: guide\n    path: docs\n    database: primary.db\n",
    )
    .expect("write sources.yaml");

    let primary_db = graphtor_dir.join("primary.db");
    let _lock = DatabaseLock::acquire(&graphtor_dir, &primary_db, false)
        .expect("database lock should be acquired");

    let output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("prewarm")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs prewarm");

    assert!(
        !output.status.success(),
        "prewarm should fail while the database lock is held: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("locked") || stderr.contains("primary.db"),
        "prewarm failure should mention the held database lock, got: {stderr}"
    );
}
