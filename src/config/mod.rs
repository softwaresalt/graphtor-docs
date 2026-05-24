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
pub use validation::DuplicateIntakeReport;

use std::path::{Path, PathBuf};

use crate::error::GraphtorError;

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
/// Returns an empty `Vec` when neither pattern files nor the fallback exist.
#[must_use]
pub fn discover_source_files(config_dir: &Path) -> Vec<PathBuf> {
    let mut matches: Vec<PathBuf> = std::fs::read_dir(config_dir)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".sources.yaml"))
                })
                .collect()
        })
        .unwrap_or_default();

    if !matches.is_empty() {
        matches.sort();
        return matches;
    }

    let fallback = config_dir.join("sources.yaml");
    if fallback.exists() {
        vec![fallback]
    } else {
        Vec::new()
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
        let content = std::fs::read_to_string(path)?;
        let config: SourceConfig = serde_yaml::from_str(&content)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

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

        let files = discover_source_files(d);
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
    fn discover_falls_back_to_sources_yaml_when_no_pattern_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();

        write_yaml(d, "sources.yaml", "sources: []\n");

        let files = discover_source_files(d);
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_str().unwrap(),
            "sources.yaml"
        );
    }

    #[test]
    fn discover_returns_empty_when_nothing_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let files = discover_source_files(dir.path());
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

        let files = discover_source_files(d);
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

        let files = discover_source_files(d);
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

        let files = discover_source_files(d);
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

        let files = discover_source_files(d);
        let config = load_multi_file_config(&files).expect("should merge cleanly");
        assert_eq!(config.sources.len(), 2, "both sources should be merged");
    }
}
