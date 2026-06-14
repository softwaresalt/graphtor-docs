//! Integration tests for fail-closed registry behavior (042.016-T).
//!
//! Verifies that `discover_source_files` returns an empty vec when no registry
//! exists, and that `load_multi_file_config` succeeds for valid registries.

use graphtor_core::config::{discover_source_files, load_multi_file_config};

/// When no config directory exists, `discover_source_files` returns an empty vec.
///
/// The caller (main.rs `load_source_config`) should respond by failing closed
/// with an actionable error — not by auto-discovering sources.
#[test]
fn no_registry_returns_empty_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    // config_dir does not exist
    let files = discover_source_files(&config_dir).expect("discover_source_files");
    assert!(
        files.is_empty(),
        "must return empty list when no registry config exists"
    );
}

/// When a valid sources.yaml exists, `load_multi_file_config` loads it correctly.
#[test]
fn valid_registry_loads_correctly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_dir = dir.path().join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let sources_yaml = config_dir.join("sources.yaml");
    std::fs::write(
        &sources_yaml,
        "sources:\n  - type: local\n    id: docs\n    path: /docs\n",
    )
    .expect("write sources.yaml");

    let files = discover_source_files(&config_dir).expect("discover_source_files");
    assert_eq!(files.len(), 1);

    let cfg = load_multi_file_config(&files).expect("load_multi_file_config");
    assert_eq!(cfg.sources.len(), 1);
}
