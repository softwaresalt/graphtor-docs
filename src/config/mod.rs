//! Configuration parsing and validation for `sources.yaml`.
//!
//! This module provides [`SourceConfig`], [`Source`], [`GitSource`], and
//! [`LocalSource`] types for reading and validating the documentation source
//! registry. Configuration is parsed from a YAML file and validated before
//! any pipeline stage begins.
//!
//! # Multi-file discovery
//!
//! When multiple `*.sources.yaml` files are present under `.graphtor/config/`,
//! [`discover_source_files`] returns them in alphabetical order and
//! [`load_multi_file_config`] merges them into a single [`SourceConfig`].
//! Each source in a multi-file config must declare an explicit `database`
//! field so routing is unambiguous.

pub mod source;
pub(crate) mod validation;

pub use source::{GitSource, LocalSource, Source, SourceConfig};
pub use validation::{resolve_source_db_path, DuplicateIntakeReport};

use std::path::{Path, PathBuf};

use tracing::info;

use crate::error::GraphtorError;

/// Auto-generate a minimal `sources.yaml` stub when an existing database file
/// is found but no source configuration exists in `config_dir`.
///
/// The stub contains `sources: []\n`, which declares an empty source registry.
/// This prevents workspace auto-discovery from triggering background sync when
/// loading configuration for a database that was imported without a source
/// configuration. The function is called from `load_source_config`, so it runs
/// on every command that loads configuration, not only `serve`.
///
/// # Behaviour
///
/// * When `db_path` exists AND [`discover_source_files`] finds no config files
///   in `config_dir` (neither `sources.yaml` nor any `*.sources.yaml` pattern
///   file) → creates the parent directory if needed, writes
///   `sources: []\n` to `config_dir/sources.yaml`, logs an `info!` message,
///   and returns `Ok(Some(path_to_stub))`.
/// * When `db_path` does not exist → no-op (returns `Ok(None)`).
/// * When any source config already exists in `config_dir` → no-op (returns
///   `Ok(None)`; never overwrites existing configuration).
///
/// # Errors
///
/// Returns [`GraphtorError::Io`] when directory enumeration, directory
/// creation, or file write fails.
pub fn ensure_sources_stub(
    config_dir: &Path,
    db_path: &Path,
) -> Result<Option<PathBuf>, GraphtorError> {
    if !db_path.exists() {
        return Ok(None);
    }
    if !discover_source_files(config_dir)?.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(config_dir)?;
    let stub_path = config_dir.join("sources.yaml");
    std::fs::write(&stub_path, "sources: []\n")?;
    info!(
        path = %stub_path.display(),
        "generated empty sources stub (no source config found for existing database)"
    );
    Ok(Some(stub_path))
}

fn collect_pattern_source_files<I>(entries: I) -> Result<Vec<PathBuf>, GraphtorError>
where
    I: IntoIterator<Item = Result<PathBuf, std::io::Error>>,
{
    let mut matches = Vec::new();
    for entry in entries {
        match entry {
            Ok(path) => {
                let is_pattern_source = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".sources.yaml"));
                if is_pattern_source {
                    matches.push(path);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(GraphtorError::Io(error)),
        }
    }
    Ok(matches)
}

/// Discover source configuration files within `config_dir`.
///
/// Searches `config_dir` for files matching the `*.sources.yaml` pattern
/// and returns them sorted alphabetically by file name.  This enables the
/// multi-file registry layout, where each team or product maintains its own
/// file (for example `graph.sources.yaml` or `powerbi.sources.yaml`).
///
/// If no `*.sources.yaml` files are found, the function falls back to
/// `sources.yaml` in the same directory for backward compatibility.
///
/// Returns `Ok(Vec::new())` when `config_dir` does not exist or contains
/// neither pattern files nor the fallback.  Returns `Err` when `config_dir`
/// exists but cannot be read (for example, due to a permission error).
///
/// # Errors
///
/// Returns [`GraphtorError::Io`] when `config_dir` exists but directory
/// enumeration fails for a reason other than the directory not being found.
pub fn discover_source_files(config_dir: &Path) -> Result<Vec<PathBuf>, GraphtorError> {
    let mut matches = match std::fs::read_dir(config_dir) {
        Ok(entries) => collect_pattern_source_files(entries.map(|entry| entry.map(|e| e.path())))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(GraphtorError::Io(e)),
    };

    if !matches.is_empty() {
        matches.sort();
        return Ok(matches);
    }

    let fallback = config_dir.join("sources.yaml");
    if fallback.exists() {
        Ok(vec![fallback])
    } else {
        Ok(Vec::new())
    }
}

/// Returns `true` when `path` is a multi-file pattern file (`*.sources.yaml`)
/// as opposed to the legacy `sources.yaml` fallback.
fn is_pattern_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".sources.yaml") && n != "sources.yaml")
}

/// Load and merge source configuration from one or more YAML files.
///
/// When any of the provided files is a `*.sources.yaml` pattern file (as
/// opposed to the legacy `sources.yaml` fallback), the function operates in
/// **multi-file mode** and enforces an additional constraint: every source
/// must declare an explicit `database` field.  This prevents ambiguous
/// routing when multiple files contribute to the same merged config.
///
/// In single-file mode (the legacy `sources.yaml`), the `database` field
/// remains optional for backward compatibility.
///
/// # Errors
///
/// Returns [`GraphtorError::Io`] if a file cannot be read.
/// Returns [`GraphtorError::Config`] if a file contains invalid YAML, fails
/// semantic validation, or (in multi-file mode) omits a required `database`
/// field on any source.
pub fn load_multi_file_config(files: &[PathBuf]) -> Result<SourceConfig, GraphtorError> {
    let multi_file_mode = files.iter().any(|f| is_pattern_file(f));
    let mut all_sources = Vec::new();

    for path in files {
        let config = load_source_config_file(path)?;

        if multi_file_mode {
            for source in &config.sources {
                if source.database().is_none() {
                    return Err(GraphtorError::Config {
                        message: format!(
                            "source '{}' in multi-file config '{}' must declare a 'database' field",
                            source.id(),
                            path.display()
                        ),
                        field: Some("database".to_string()),
                    });
                }
            }
        }

        all_sources.extend(config.sources);
    }

    let merged = SourceConfig {
        sources: all_sources,
    };
    validation::validate(&merged)?;
    Ok(merged)
}

fn load_source_config_file(path: &Path) -> Result<SourceConfig, GraphtorError> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        GraphtorError::Io(std::io::Error::new(
            error.kind(),
            format!("failed to read source config '{}': {error}", path.display()),
        ))
    })?;

    serde_yaml::from_str(&content).map_err(|error| GraphtorError::Config {
        message: format!(
            "failed to parse source config '{}': {error}",
            path.display()
        ),
        field: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // ── T041.001: ensure_sources_stub ─────────────────────────────────────

    #[test]
    fn ensure_sources_stub_creates_file_when_db_exists_and_yaml_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("config");
        let db_path = dir.path().join("graph.db");

        std::fs::write(&db_path, b"").expect("create db file");

        let result = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
        assert!(
            result.is_some(),
            "should return Some(path) when stub is written"
        );

        let stub_path = config_dir.join("sources.yaml");
        assert!(stub_path.exists(), "stub file should have been created");
        let content = std::fs::read_to_string(&stub_path).expect("read stub");
        assert_eq!(content, "sources: []\n");
    }

    #[test]
    fn ensure_sources_stub_does_not_overwrite_existing_yaml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let db_path = dir.path().join("graph.db");

        std::fs::write(&db_path, b"").expect("create db file");

        let stub_path = config_dir.join("sources.yaml");
        let existing = "sources:\n  - type: local\n    id: docs\n    path: /docs\n";
        std::fs::write(&stub_path, existing).expect("write existing yaml");

        let result = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
        assert!(
            result.is_none(),
            "should return None when sources.yaml already exists"
        );

        let content = std::fs::read_to_string(&stub_path).expect("read stub");
        assert_eq!(
            content, existing,
            "existing sources.yaml should not be overwritten"
        );
    }

    #[test]
    fn ensure_sources_stub_is_noop_when_pattern_file_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let db_path = dir.path().join("graph.db");

        std::fs::write(&db_path, b"").expect("create db file");

        // A *.sources.yaml pattern file is already present — no stub should be written.
        let pattern_file = config_dir.join("graph.sources.yaml");
        let existing =
            "sources:\n  - type: local\n    id: docs\n    path: /docs\n    database: graph.db\n";
        std::fs::write(&pattern_file, existing).expect("write pattern file");

        let result = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
        assert!(
            result.is_none(),
            "should return None when a *.sources.yaml pattern file exists"
        );

        // The pattern file must be unchanged.
        let content = std::fs::read_to_string(&pattern_file).expect("read pattern file");
        assert_eq!(
            content, existing,
            "existing *.sources.yaml should not be overwritten"
        );
        // No stub sources.yaml should have been created.
        assert!(
            !config_dir.join("sources.yaml").exists(),
            "stub sources.yaml must not be created when a pattern file exists"
        );
    }

    #[test]
    fn ensure_sources_stub_is_noop_when_db_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("config");
        let db_path = dir.path().join("graph.db"); // does not exist

        let result = ensure_sources_stub(&config_dir, &db_path).expect("ensure_sources_stub");
        assert!(result.is_none(), "should return None when db is absent");

        assert!(
            !config_dir.exists(),
            "config dir should not be created when db is absent"
        );
    }

    fn write_yaml(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).expect("create yaml file");
        f.write_all(content.as_bytes()).expect("write yaml");
        path
    }

    // ── T040.001: discover_source_files ───────────────────────────────────

    #[test]
    fn discover_returns_pattern_files_alphabetically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        write_yaml(
            d,
            "powerbi.sources.yaml",
            "sources:\n  - type: local\n    id: pb\n    path: /pb\n    database: pb.db\n",
        );
        write_yaml(
            d,
            "graph.sources.yaml",
            "sources:\n  - type: local\n    id: gr\n    path: /gr\n    database: gr.db\n",
        );

        let files = discover_source_files(d).expect("discover_source_files");
        assert_eq!(files.len(), 2, "should find both *.sources.yaml files");

        let names: Vec<&str> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["graph.sources.yaml", "powerbi.sources.yaml"],
            "files should be in alphabetical order"
        );
    }

    #[test]
    fn collect_pattern_source_files_propagates_entry_errors() {
        let result = collect_pattern_source_files(vec![Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))]);

        let err = result.expect_err("entry error should propagate");
        assert!(
            matches!(err, GraphtorError::Io(_)),
            "expected Io error: {err:?}"
        );
    }

    #[test]
    fn collect_pattern_source_files_skips_not_found_entries() {
        let result = collect_pattern_source_files(vec![
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "vanished entry",
            )),
            Ok(PathBuf::from("graph.sources.yaml")),
        ])
        .expect("not-found entry should be ignored");

        assert_eq!(result, vec![PathBuf::from("graph.sources.yaml")]);
    }

    #[test]
    fn discover_falls_back_to_sources_yaml_when_no_pattern_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        write_yaml(d, "sources.yaml", "sources: []\n");

        let files = discover_source_files(d).expect("discover_source_files");
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_str().unwrap(),
            "sources.yaml"
        );
    }

    #[test]
    fn discover_returns_empty_when_nothing_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let files = discover_source_files(dir.path()).expect("discover_source_files");
        assert!(
            files.is_empty(),
            "should return empty vec when no files exist"
        );
    }

    #[test]
    fn discover_pattern_files_take_priority_over_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        // Both a pattern file and sources.yaml exist.
        write_yaml(
            d,
            "graph.sources.yaml",
            "sources:\n  - type: local\n    id: gr\n    path: /gr\n    database: gr.db\n",
        );
        write_yaml(d, "sources.yaml", "sources: []\n");

        let files = discover_source_files(d).expect("discover_source_files");
        assert_eq!(files.len(), 1, "pattern file takes priority over fallback");
        assert_eq!(
            files[0].file_name().unwrap().to_str().unwrap(),
            "graph.sources.yaml"
        );
    }

    // ── T040.002: multi-file mode database field requirement ──────────────

    #[test]
    fn multi_file_mode_rejects_source_without_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        // *.sources.yaml file with a source that has no database field.
        write_yaml(
            d,
            "graph.sources.yaml",
            "sources:\n  - type: local\n    id: no-db\n    path: /docs\n",
        );

        let files = discover_source_files(d).expect("discover_source_files");
        let result = load_multi_file_config(&files);
        assert!(
            result.is_err(),
            "multi-file mode should reject missing database"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[config]"),
            "should produce Config error: {msg}"
        );
        assert!(
            msg.contains("database"),
            "error should mention the database field: {msg}"
        );
    }

    #[test]
    fn single_file_mode_allows_missing_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        // Legacy sources.yaml without database field.
        write_yaml(
            d,
            "sources.yaml",
            "sources:\n  - type: local\n    id: no-db\n    path: /docs\n",
        );

        let files = discover_source_files(d).expect("discover_source_files");
        let result = load_multi_file_config(&files);
        assert!(
            result.is_ok(),
            "single-file mode must allow missing database: {:?}",
            result.err()
        );
    }

    #[test]
    fn multi_file_mode_merges_all_sources() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        write_yaml(
            d,
            "alpha.sources.yaml",
            "sources:\n  - type: local\n    id: src-a\n    path: /a\n    database: a.db\n",
        );
        write_yaml(
            d,
            "beta.sources.yaml",
            "sources:\n  - type: local\n    id: src-b\n    path: /b\n    database: b.db\n",
        );

        let files = discover_source_files(d).expect("discover_source_files");
        let config = load_multi_file_config(&files).expect("should merge cleanly");
        assert_eq!(config.sources.len(), 2, "both sources should be merged");
    }

    #[test]
    fn load_multi_file_config_read_error_includes_file_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.sources.yaml");

        let err = load_multi_file_config(std::slice::from_ref(&missing))
            .expect_err("missing config file should fail");

        let message = err.to_string();
        assert!(message.starts_with("[io]"), "expected Io error: {message}");
        assert!(
            message.contains("missing.sources.yaml"),
            "error should include the missing file path: {message}"
        );
    }

    #[test]
    fn load_multi_file_config_yaml_error_includes_file_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let invalid = write_yaml(
            dir.path(),
            "invalid.sources.yaml",
            "sources:\n  - type: local\n    id: broken\n    path: [unterminated\n",
        );

        let err = load_multi_file_config(std::slice::from_ref(&invalid))
            .expect_err("invalid yaml should fail");

        let message = err.to_string();
        assert!(
            message.starts_with("[config]"),
            "expected Config error: {message}"
        );
        assert!(
            message.contains("invalid.sources.yaml"),
            "error should include the invalid file path: {message}"
        );
    }

    #[test]
    fn discover_returns_ok_empty_for_missing_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nonexistent");
        let files = discover_source_files(&missing).expect("should return Ok for non-existent dir");
        assert!(
            files.is_empty(),
            "should return empty vec when config dir does not exist"
        );
    }
}
