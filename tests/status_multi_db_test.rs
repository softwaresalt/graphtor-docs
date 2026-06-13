//! Integration tests for multi-database `graphtor-docs status`.

use std::process::Command;

use graphtor_core::lock::DatabaseLock;
use serde_json::Value;

/// Helper: path to the compiled binary.
fn graphtor_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn status_lists_sources_from_multiple_databases() {
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

    let sync_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");
    assert!(
        sync_output.status.success(),
        "sync failed: status={:?}\nstderr={}\nstdout={}",
        sync_output.status.code(),
        String::from_utf8_lossy(&sync_output.stderr),
        String::from_utf8_lossy(&sync_output.stdout),
    );

    let status_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("status")
        .output()
        .expect("run graphtor-docs status");
    assert!(
        status_output.status.success(),
        "status failed: status={:?}\nstderr={}\nstdout={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stderr),
        String::from_utf8_lossy(&status_output.stdout),
    );

    let stdout = String::from_utf8(status_output.stdout).expect("status stdout utf-8");
    assert!(stdout.contains("primary.db"), "stdout: {stdout}");
    assert!(stdout.contains("secondary.db"), "stdout: {stdout}");
    assert!(stdout.contains("docs-a"), "stdout: {stdout}");
    assert!(stdout.contains("docs-b"), "stdout: {stdout}");
}

#[test]
fn status_succeeds_while_database_lock_is_held() {
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

    let sync_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");
    assert!(
        sync_output.status.success(),
        "sync failed: status={:?}\nstderr={}\nstdout={}",
        sync_output.status.code(),
        String::from_utf8_lossy(&sync_output.stderr),
        String::from_utf8_lossy(&sync_output.stdout),
    );

    let primary_db = graphtor_dir.join("primary.db");
    let _lock = DatabaseLock::acquire(&graphtor_dir, &primary_db, false)
        .expect("primary database lock should be acquired");

    let status_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("status")
        .output()
        .expect("run graphtor-docs status");
    assert!(
        status_output.status.success(),
        "status failed while lock held: status={:?}\nstderr={}\nstdout={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stderr),
        String::from_utf8_lossy(&status_output.stdout),
    );

    let stdout = String::from_utf8(status_output.stdout).expect("status stdout utf-8");
    assert!(stdout.contains("primary.db"), "stdout: {stdout}");
    assert!(stdout.contains("secondary.db"), "stdout: {stdout}");
    assert!(stdout.contains("docs-a"), "stdout: {stdout}");
    assert!(stdout.contains("docs-b"), "stdout: {stdout}");
}

#[test]
fn status_json_single_database_always_emits_databases_array() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();
    let docs_dir = ws.join("docs");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");
    std::fs::write(docs_dir.join("guide.md"), b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello world.\n").expect("write guide");

    let config_dir = ws.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        "sources:\n  - type: local\n    id: guide\n    path: docs\n    include:\n      - \"**/*.md\"\n",
    )
    .expect("write sources.yaml");

    let sync_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");
    assert!(
        sync_output.status.success(),
        "sync failed: status={:?}\nstderr={}\nstdout={}",
        sync_output.status.code(),
        String::from_utf8_lossy(&sync_output.stderr),
        String::from_utf8_lossy(&sync_output.stdout),
    );

    let status_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("status")
        .arg("--json")
        .output()
        .expect("run graphtor-docs status --json");
    assert!(
        status_output.status.success(),
        "status failed: status={:?}\nstderr={}\nstdout={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stderr),
        String::from_utf8_lossy(&status_output.stdout),
    );

    let stdout = String::from_utf8(status_output.stdout).expect("status stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert!(
        parsed["result"]["databases"].is_array(),
        "expected result.databases array, got: {stdout}"
    );
    assert_eq!(
        parsed["result"]["databases"].as_array().map(Vec::len),
        Some(1),
        "expected exactly one database entry, got: {stdout}"
    );
    assert!(
        parsed["result"].get("database").is_none(),
        "legacy single-database shape should not be emitted: {stdout}"
    );
}

#[test]
fn status_json_missing_single_database_always_emits_databases_array() {
    // With no registry at all, status must return an empty databases array —
    // not a phantom primary.db entry.
    let workspace = tempfile::tempdir().expect("tempdir");

    let status_output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("status")
        .arg("--json")
        .output()
        .expect("run graphtor-docs status --json");
    assert!(
        status_output.status.success(),
        "status failed: status={:?}\nstderr={}\nstdout={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stderr),
        String::from_utf8_lossy(&status_output.stdout),
    );

    let stdout = String::from_utf8(status_output.stdout).expect("status stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    assert!(
        parsed["result"]["databases"].is_array(),
        "expected result.databases array, got: {stdout}"
    );
    // No registry → empty databases list (deterministic, not phantom primary.db).
    assert_eq!(
        parsed["result"]["databases"].as_array().map(Vec::len),
        Some(0),
        "no registry should yield an empty databases array, got: {stdout}"
    );
    assert!(
        parsed["result"].get("database").is_none(),
        "legacy single-database shape should not be emitted: {stdout}"
    );
}

#[test]
fn status_fails_closed_when_explicit_config_is_missing() {
    // When --config points to a file that does not exist, `status` must exit
    // non-zero.  Silently returning an empty result would hide a typo or stale
    // path from the operator and violates the fail-closed contract.
    let workspace = tempfile::tempdir().expect("tempdir");
    let missing = workspace.path().join("nonexistent_sources.yaml");

    let status_output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args(["--config", missing.to_str().expect("utf-8 path"), "status"])
        .output()
        .expect("run graphtor-docs status");

    assert!(
        !status_output.status.success(),
        "status with missing --config must fail; got: status={:?}\nstdout={}\nstderr={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stdout),
        String::from_utf8_lossy(&status_output.stderr),
    );
}

#[test]
fn status_json_fails_closed_when_explicit_config_is_missing() {
    // Same fail-closed guarantee applies to the JSON output path (--json).
    let workspace = tempfile::tempdir().expect("tempdir");
    let missing = workspace.path().join("nonexistent_sources.yaml");

    let status_output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args([
            "--config",
            missing.to_str().expect("utf-8 path"),
            "status",
            "--json",
        ])
        .output()
        .expect("run graphtor-docs status --json");

    assert!(
        !status_output.status.success(),
        "status --json with missing --config must fail; got: status={:?}\nstdout={}\nstderr={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stdout),
        String::from_utf8_lossy(&status_output.stderr),
    );
}

#[test]
fn status_fails_closed_on_malformed_registry() {
    // A malformed sources.yaml (parse error) must cause status to exit non-zero.
    // This is fail-closed: a broken config should not silently fall back to
    // phantom primary-database output.
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();
    let config_dir = ws.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("sources.yaml"),
        b"sources: [invalid yaml: {\n",
    )
    .expect("write bad sources.yaml");

    let status_output = Command::new(graphtor_bin())
        .current_dir(ws)
        .arg("status")
        .output()
        .expect("run graphtor-docs status");

    assert!(
        !status_output.status.success(),
        "status with malformed registry must fail; got: status={:?}\nstdout={}\nstderr={}",
        status_output.status.code(),
        String::from_utf8_lossy(&status_output.stdout),
        String::from_utf8_lossy(&status_output.stderr),
    );
}
