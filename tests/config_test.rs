//! Integration tests for `graphtor_core::config` module — local-only after docline pivot.

use graphtor_core::config::SourceConfig;
use graphtor_core::GraphtorError;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_yaml(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("failed to write yaml");
    file
}

/// Valid sources.yaml with one local source parses cleanly.
#[test]
fn parse_valid_local_source_yaml() {
    let yaml = r#"
sources:
  - type: local
    id: internal-wiki
    path: /workspace/wiki
    include:
      - "**/*.md"
"#;
    let file = write_yaml(yaml);
    let config = SourceConfig::parse(file.path()).expect("should parse valid yaml");

    assert_eq!(config.sources.len(), 1);

    let local = config.sources[0].as_local().expect("local source");
    assert_eq!(local.id, "internal-wiki");
}

/// Git-type sources are rejected by the YAML deserializer.
#[test]
fn parse_git_source_type_is_rejected() {
    let yaml = r"
sources:
  - type: git
    id: ms-azure-docs
    url: https://github.com/MicrosoftDocs/azure-docs.git
    branch: main
";
    let file = write_yaml(yaml);
    let result = SourceConfig::parse(file.path());
    assert!(result.is_err(), "git source type must be rejected");
}

/// Malformed YAML produces a `GraphtorError::Config` with a `[config]` prefix.
#[test]
fn parse_malformed_yaml_returns_config_error() {
    let bad_yaml = "sources:\n  - type: local\n    id: [unclosed\n";
    let file = write_yaml(bad_yaml);
    let result = SourceConfig::parse(file.path());
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.starts_with("[config]"),
        "expected Config error, got: {msg}"
    );
}

/// Duplicate source IDs produce a validation error.
#[test]
fn parse_duplicate_ids_returns_config_error() {
    let yaml = r#"
sources:
  - type: local
    id: duplicate-id
    path: /docs-a
    include: ["**/*.md"]
  - type: local
    id: duplicate-id
    path: /docs-b
    include: ["**/*.md"]
"#;
    let file = write_yaml(yaml);
    let result = SourceConfig::parse(file.path());
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("duplicate-id"),
        "error should name the duplicate id: {msg}"
    );
}

/// A non-existent file path produces an `[io]` error.
#[test]
fn parse_nonexistent_file_returns_io_error() {
    let result = SourceConfig::parse(std::path::Path::new("/does/not/exist/sources.yaml"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.starts_with("[io]"),
        "expected Io error for missing file, got: {msg}"
    );
}

/// Empty sources list is valid and returns zero sources.
#[test]
fn parse_empty_sources_list_is_valid() {
    let yaml = "sources: []\n";
    let file = write_yaml(yaml);
    let config = SourceConfig::parse(file.path()).expect("empty sources should parse");
    assert!(config.sources.is_empty(), "sources list should be empty");
}

/// `From<serde_yaml::Error>` conversion is exercised through the public `parse()` path.
#[test]
fn serde_yaml_parse_error_converts_to_graphtor_config_error() {
    let bad_yaml = "not_sources_key: 42\n";
    let file = write_yaml(bad_yaml);
    let result = SourceConfig::parse(file.path());
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(
        matches!(e, GraphtorError::Config { .. }),
        "expected GraphtorError::Config, got: {e:?}"
    );
}
