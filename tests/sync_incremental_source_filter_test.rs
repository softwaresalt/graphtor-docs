//! Regression tests for incremental sync source filtering.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use graphtor_core::db::chunks::list_chunks_by_path;
use graphtor_core::DataStore;
use serde_json::Value;

fn graphtor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

#[test]
fn sync_incremental_honors_pdf_only_filters_and_persists_filtered_state() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs").join("reference");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");
    std::fs::write(docs_dir.join("guide.md"), "# Guide\n\nHello world.\n").expect("write guide");
    std::fs::write(docs_dir.join("bad.pdf"), [0xFF, 0x00, 0xFE, 0x7F]).expect("write bad pdf");

    let config_path = workspace.path().join("pdf-only.sources.yaml");
    std::fs::write(
        &config_path,
        "sources:\n  - type: local\n    id: pdf-only\n    path: docs\n    include:\n      - \"**/*.pdf\"\n    formats:\n      - pdf\n",
    )
    .expect("write config");

    let db_path = workspace.path().join("graph.db");

    let first = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        !first.status.success(),
        "first sync should surface the bad PDF parse failure: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );

    let first_stdout = String::from_utf8(first.stdout).expect("first stdout utf-8");
    let first_metrics: Value = serde_json::from_str(&first_stdout).expect("first metrics json");
    assert_eq!(first_metrics["files_total"], 1, "stdout: {first_stdout}");
    assert_eq!(first_metrics["files_synced"], 0, "stdout: {first_stdout}");
    assert_eq!(first_metrics["errors"], 1, "stdout: {first_stdout}");

    let store = DataStore::open_sqlite_readonly(&db_path, workspace.path()).expect("open db");
    let markdown_chunks =
        list_chunks_by_path(&store, "reference/guide.md").expect("query markdown chunks");
    assert!(
        markdown_chunks.is_empty(),
        "markdown file should not be ingested by a pdf-only source"
    );

    let second = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        second.status.success(),
        "second sync should be a no-op after persisting filtered state: stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );

    let second_stdout = String::from_utf8(second.stdout).expect("second stdout utf-8");
    let second_metrics: Value = serde_json::from_str(&second_stdout).expect("second metrics json");
    assert_eq!(second_metrics["files_total"], 0, "stdout: {second_stdout}");
    assert_eq!(second_metrics["files_synced"], 0, "stdout: {second_stdout}");
    assert_eq!(second_metrics["errors"], 0, "stdout: {second_stdout}");
}

fn run_sync(workspace: &Path, config_path: &Path, db_path: &Path) -> Output {
    Command::new(graphtor_bin())
        .current_dir(workspace)
        .arg("--config")
        .arg(config_path)
        .arg("--db-path")
        .arg(db_path)
        .arg("sync")
        .arg("--no-embed")
        .arg("--metrics")
        .output()
        .expect("run graphtor-docs sync")
}
