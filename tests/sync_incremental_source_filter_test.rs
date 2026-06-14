//! Regression tests for incremental sync source filtering — docline pivot edition.
//!
//! After the pivot, only standardised Markdown files are supported. These tests
//! verify that the include-pattern filter operates correctly for `.md`-only
//! sources and that non-Markdown files are silently ignored.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use graphtor_core::db::chunks::list_chunks_by_path;
use graphtor_core::DataStore;
use serde_json::Value;

fn graphtor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

/// A source filtered to `**/*.md` ingests Markdown files and produces stable
/// incremental sync semantics: second run reports zero work remaining.
#[test]
fn sync_incremental_md_only_filter_is_stable_across_two_runs() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs").join("reference");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");

    // Write a valid docline v1 Markdown fixture.
    let md_content = b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: reference/guide.md\n---\n# Guide\n\nHello world.\n";
    std::fs::write(docs_dir.join("guide.md"), md_content).expect("write guide");

    // Write a non-Markdown file that should be silently ignored.
    std::fs::write(docs_dir.join("ignore_me.txt"), "not a doc").expect("write txt");

    let config_path = workspace.path().join("md-only.sources.yaml");
    std::fs::write(
        &config_path,
        "sources:\n  - type: local\n    id: md-only\n    path: docs\n    include:\n      - \"**/*.md\"\n    formats:\n      - md\n",
    )
    .expect("write config");

    let db_path = workspace.path().join("graph.db");

    let first = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        first.status.success(),
        "first sync should succeed: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );

    let first_stdout = String::from_utf8(first.stdout).expect("first stdout utf-8");
    let first_metrics: Value = serde_json::from_str(&first_stdout).expect("first metrics json");
    // One .md file scanned; txt silently excluded.
    assert_eq!(first_metrics["files_total"], 1, "stdout: {first_stdout}");

    let store = DataStore::open_sqlite_readonly(&db_path, workspace.path()).expect("open db");
    let ignore_chunks =
        list_chunks_by_path(&store, "reference/ignore_me.txt").expect("query txt chunks");
    assert!(
        ignore_chunks.is_empty(),
        "txt file should not be ingested by md-only source"
    );

    // Second sync: file unchanged — should be a no-op.
    let second = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        second.status.success(),
        "second sync should be a no-op: stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );

    let second_stdout = String::from_utf8(second.stdout).expect("second stdout utf-8");
    let second_metrics: Value = serde_json::from_str(&second_stdout).expect("second metrics json");
    assert_eq!(
        second_metrics["files_total"], 0,
        "no new work: {second_stdout}"
    );
    assert_eq!(
        second_metrics["files_synced"], 0,
        "no new work: {second_stdout}"
    );
    assert_eq!(
        second_metrics["errors"], 0,
        "no errors on no-op: {second_stdout}"
    );
}

// ── T021.002: formats: ["markdown"] alias works in incremental sync ──────────

/// Regression: `formats: ["markdown"]` must track `.md` files in the incremental
/// sync state.  Before the fix, `is_tracked_source_path` compared the raw alias
/// `"markdown"` against the normalised extension `"md"` (always false), meaning
/// no `.md` files were tracked — every subsequent sync detected them as deleted
/// and purged the database.
#[test]
fn sync_incremental_markdown_alias_tracks_md_files() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");

    let md_content = b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\n\
        doc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello.\n";
    std::fs::write(docs_dir.join("guide.md"), md_content).expect("write guide.md");

    // Use the "markdown" alias (not "md") to reproduce the regression scenario.
    let config_path = workspace.path().join("test.sources.yaml");
    std::fs::write(
        &config_path,
        "sources:\n  - type: local\n    id: markdown-alias\n    path: docs\n    \
         formats:\n      - markdown\n",
    )
    .expect("write config");

    let db_path = workspace.path().join("graph.db");

    // First sync: file must be tracked (files_total = 1) and ingested.
    let first = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        first.status.success(),
        "first sync should succeed: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8(first.stdout).expect("first stdout utf-8");
    let first_metrics: serde_json::Value =
        serde_json::from_str(&first_stdout).expect("first metrics json");
    assert_eq!(
        first_metrics["files_total"], 1,
        "formats: [\"markdown\"] must track the .md file on the first sync; \
         got metrics: {first_stdout}"
    );
    assert_eq!(
        first_metrics["files_synced"], 1,
        "the .md file must be ingested on first sync: {first_stdout}"
    );

    // Second sync (no file changes): must be a no-op — not a purge.
    let second = run_sync(workspace.path(), &config_path, &db_path);
    assert!(
        second.status.success(),
        "second sync should succeed: stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8(second.stdout).expect("second stdout utf-8");
    let second_metrics: serde_json::Value =
        serde_json::from_str(&second_stdout).expect("second metrics json");
    assert_eq!(
        second_metrics["files_deleted"], 0,
        "formats: [\"markdown\"] must not purge tracked .md files on second sync \
         (regression: alias mismatch causes spurious deletes); \
         got metrics: {second_stdout}"
    );
    assert_eq!(
        second_metrics["files_synced"], 0,
        "second sync must be a no-op: {second_stdout}"
    );
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
