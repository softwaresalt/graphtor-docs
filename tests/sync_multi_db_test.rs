//! Integration test: sync routes sources to separate database files.

use std::process::Command;

use graphtor_core::lock::DatabaseLock;

/// Two local sources with different `database` fields each produce a
/// separate `.db` file under `.graphtor/` after `sync --no-embed`.
#[test]
fn sync_routes_sources_to_separate_database_files() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let src_a = ws.join("docs_a");
    let src_b = ws.join("docs_b");
    std::fs::create_dir_all(&src_a).unwrap();
    std::fs::create_dir_all(&src_b).unwrap();
    std::fs::write(src_a.join("a.md"), "# A\n\nContent A.\n").unwrap();
    std::fs::write(src_b.join("b.md"), "# B\n\nContent B.\n").unwrap();

    let graphtor_dir = ws.join(".graphtor");
    let config_dir = graphtor_dir.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
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
    std::fs::write(config_dir.join("sources.yaml"), &sources_yaml).unwrap();

    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));

    let output = Command::new(&exe)
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        output.status.success(),
        "sync must succeed: status={:?}\nstderr={}\nstdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );

    assert!(
        graphtor_dir.join("primary.db").exists(),
        "primary.db must be created by sync"
    );
    assert!(
        graphtor_dir.join("secondary.db").exists(),
        "secondary.db must be created by sync"
    );
}

#[test]
fn sync_fails_gracefully_when_database_lock_is_held() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();

    let src = ws.join("docs");
    std::fs::create_dir_all(&src).expect("create docs");
    std::fs::write(src.join("a.md"), "# A\n\nContent A.\n").expect("write a");

    let graphtor_dir = ws.join(".graphtor");
    let config_dir = graphtor_dir.join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let sources_yaml = format!(
        r#"sources:
  - type: local
    id: docs-a
    path: {src}
    database: "primary.db"
"#,
        src = src.display(),
    );
    std::fs::write(config_dir.join("sources.yaml"), &sources_yaml).expect("write sources.yaml");

    let primary_db = graphtor_dir.join("primary.db");
    let _lock = DatabaseLock::acquire(&graphtor_dir, &primary_db, false)
        .expect("database lock should be acquired");

    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));
    let output = Command::new(&exe)
        .current_dir(ws)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        !output.status.success(),
        "sync should fail while the database lock is held: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("locked") || stderr.contains("primary.db"),
        "sync failure should mention the held database lock, got: {stderr}"
    );
}

#[test]
fn sync_creates_missing_parent_directories_for_custom_database_path() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();
    std::fs::write(ws.join("guide.md"), "# Guide\n\nHello world.\n").expect("write guide");

    let custom_db = std::path::PathBuf::from(".graphtor")
        .join("nested")
        .join("graph.db");
    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));
    let output = Command::new(&exe)
        .current_dir(ws)
        .arg("--db-path")
        .arg(&custom_db)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        output.status.success(),
        "sync should create missing parent directories for the custom database path: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        ws.join(&custom_db).exists(),
        "custom database path should exist after sync"
    );
}

#[test]
fn sync_rejects_custom_database_path_escaping_workspace_root() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let ws = workspace.path();
    std::fs::write(ws.join("guide.md"), "# Guide\n\nHello world.\n").expect("write guide");

    let escaped_db = std::path::PathBuf::from("..")
        .join("escaped")
        .join("graph.db");
    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"));
    let output = Command::new(&exe)
        .current_dir(ws)
        .arg("--db-path")
        .arg(&escaped_db)
        .arg("sync")
        .arg("--no-embed")
        .output()
        .expect("run graphtor-docs sync");

    assert!(
        !output.status.success(),
        "sync should reject a database path outside the workspace: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("path_violation") || stderr.contains("must be within"),
        "sync failure should report a path violation, got: {stderr}"
    );
}
