//! Integration tests for `ensure_sources_stub` — the safety net that prevents
//! workspace auto-discovery when serving an imported database that has no
//! source configuration.

use graphtor_core::config::{discover_source_files, ensure_sources_stub, load_multi_file_config};

/// When a DB file exists and no `sources.yaml` is present, `ensure_sources_stub`
/// must create `.graphtor/config/sources.yaml` containing exactly `sources: []\n`.
#[test]
fn stub_created_for_imported_db_without_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    let db_path = dir.path().join(".graphtor").join("graph.db");

    std::fs::create_dir_all(db_path.parent().expect("parent")).expect("create .graphtor dir");
    std::fs::write(&db_path, b"").expect("create db file");

    let stub = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
    let stub_path = stub.expect("should return Some(path) when stub is created");

    assert!(stub_path.exists(), "stub sources.yaml should be created");

    let content = std::fs::read_to_string(&stub_path).expect("read stub");
    assert_eq!(
        content, "sources: []\n",
        "stub must declare an empty sources list"
    );
}

/// After stub creation, `discover_source_files` must find exactly one file
/// and `load_multi_file_config` must return an empty sources list.
/// This is the behaviour that prevents background sync from triggering.
#[test]
fn stub_produces_empty_source_config_preventing_auto_discovery() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    let db_path = dir.path().join(".graphtor").join("graph.db");

    std::fs::create_dir_all(db_path.parent().expect("parent")).expect("create .graphtor dir");
    std::fs::write(&db_path, b"").expect("create db file");

    let stub = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
    assert!(
        stub.is_some(),
        "should return Some(path) when stub is created"
    );

    let files = discover_source_files(&config_dir).expect("discover_source_files");
    assert_eq!(files.len(), 1, "should find the generated stub");

    let cfg = load_multi_file_config(&files).expect("load_multi_file_config");
    assert!(
        cfg.sources.is_empty(),
        "stub config must have zero sources so background sync is skipped"
    );
}

/// When a DB file exists and only a `*.sources.yaml` pattern file is present
/// (no `sources.yaml`), `ensure_sources_stub` must be a no-op: the pattern
/// file must remain intact and no stub `sources.yaml` must be created.
#[test]
fn stub_not_created_when_pattern_file_exists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    let db_path = dir.path().join(".graphtor").join("graph.db");

    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::create_dir_all(db_path.parent().expect("parent")).expect("create .graphtor dir");
    std::fs::write(&db_path, b"").expect("create db file");

    let pattern_file = config_dir.join("graph.sources.yaml");
    let existing =
        "sources:\n  - type: local\n    id: docs\n    path: /docs\n    database: graph.db\n";
    std::fs::write(&pattern_file, existing).expect("write pattern file");

    let stub = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
    assert!(
        stub.is_none(),
        "should return None when a *.sources.yaml pattern file exists"
    );

    // Pattern file must be unchanged.
    let content = std::fs::read_to_string(&pattern_file).expect("read pattern file");
    assert_eq!(
        content, existing,
        "*.sources.yaml must not be overwritten by ensure_sources_stub"
    );
    // No stub sources.yaml must have been created.
    assert!(
        !config_dir.join("sources.yaml").exists(),
        "stub sources.yaml must not appear when a *.sources.yaml pattern file already exists"
    );
}

/// When no DB file exists, `ensure_sources_stub` is a no-op and
/// `discover_source_files` returns an empty list (triggering auto-discovery).
#[test]
fn no_stub_created_when_db_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    let db_path = dir.path().join(".graphtor").join("graph.db"); // does not exist

    let stub = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
    assert!(stub.is_none(), "should return None when db is absent");

    let files = discover_source_files(&config_dir).expect("discover_source_files");
    assert!(
        files.is_empty(),
        "no files should be discovered when db is absent and no config exists"
    );
}
