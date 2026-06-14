//! Regression tests for explicit database targets without a source registry.

use std::path::{Path, PathBuf};
use std::process::Command;

use graphtor_core::db::{upsert_source, DataStore, SourceRecord};
use serde_json::Value;

fn graphtor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

fn seed_status_database(db_path: &Path, cwd: &Path) {
    let store = DataStore::open_sqlite(db_path, cwd).expect("open sqlite database");
    store.ensure_schema().expect("ensure schema");
    upsert_source(
        &store,
        &SourceRecord {
            source_id: String::from("explicit-source"),
            url: String::from("file:///docs"),
            kind: String::from("local"),
            name: String::from("Explicit Source"),
            synced_at: None,
        },
    )
    .expect("seed source");
}

fn seed_pre_v4_database(db_path: &Path, cwd: &Path) {
    let store = DataStore::open_sqlite(db_path, cwd).expect("open sqlite database");
    store.ensure_schema().expect("ensure schema");
    store
        .set_schema_version_for_test(3)
        .expect("downgrade schema version");
}

#[test]
fn status_json_inspects_explicit_db_path_without_registry() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let db_path = workspace.path().join("explicit.db");
    seed_status_database(&db_path, workspace.path());

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args([
            "--db-path",
            db_path.to_str().expect("utf-8 db path"),
            "status",
            "--json",
        ])
        .output()
        .expect("run graphtor-docs status --json");

    assert!(
        output.status.success(),
        "status failed: status={:?}\nstderr={}\nstdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );

    let stdout = String::from_utf8(output.stdout).expect("status stdout utf-8");
    let parsed: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    let databases = parsed["result"]["databases"]
        .as_array()
        .expect("result.databases should be an array");

    assert_eq!(
        databases.len(),
        1,
        "explicit --db-path should inspect the target database even without a registry: {stdout}"
    );
    assert_eq!(
        databases[0]["database"].as_str(),
        Some(db_path.display().to_string().as_str()),
        "status should report the explicit database path: {stdout}"
    );
    assert_eq!(
        databases[0]["sources"].as_array().map(std::vec::Vec::len),
        Some(1),
        "status should report sources from the explicit database: {stdout}"
    );
    assert_eq!(
        databases[0]["sources"][0]["id"].as_str(),
        Some("explicit-source"),
        "status should inspect the explicit database instead of returning an empty list: {stdout}"
    );
}

#[test]
fn serve_explicit_db_path_without_registry_reaches_v4_gate() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let db_path = workspace.path().join("explicit-pre-v4.db");
    seed_pre_v4_database(&db_path, workspace.path());

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .args([
            "--db-path",
            db_path.to_str().expect("utf-8 db path"),
            "serve",
        ])
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(
        output.status.code(),
        Some(2),
        "serve should fail at the database migration gate; stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pre-v4 schema"),
        "serve should reach the migration gate for an explicit db target without a registry: {stderr}"
    );
    assert!(
        stderr.contains("run `graphtor-docs sync` to rebuild the index before starting serve"),
        "serve should explain how to clear the migration gate: {stderr}"
    );
    assert!(
        !stderr.contains("config file"),
        "serve should not treat a missing auto-discovered registry as a missing explicit config when --db-path is provided: {stderr}"
    );
}
