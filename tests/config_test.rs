//! Integration tests for `graphtor_core::config` module.
//!
//! Tests end-to-end configuration parsing from YAML files on disk,
//! verifying both successful parsing and error path handling.

use graphtor_core::config::{Source, SourceConfig};
use graphtor_core::GraphtorError;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_yaml(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("failed to write yaml");
    file
}

/// Valid sources.yaml with one Git and one local source parses cleanly.
#[test]
fn parse_valid_mixed_sources_yaml() {
    let yaml = r#"
sources:
  - type: git
    id: ms-azure-docs
    url: https://github.com/MicrosoftDocs/azure-docs.git
    branch: main
    include:
      - "**/*.md"
    exclude:
      - "**/drafts/**"
  - type: local
    id: internal-wiki
    path: /workspace/wiki
    include:
      - "**/*.md"
"#;
    let file = write_yaml(yaml);
    let config = SourceConfig::parse(file.path()).expect("should parse valid yaml");

    assert_eq!(config.sources.len(), 2);

    let Source::Git(git) = &config.sources[0] else {
        panic!("first source should be Git");
    };
    assert_eq!(git.id, "ms-azure-docs");
    assert_eq!(git.branch, "main");

    let Source::Local(local) = &config.sources[1] else {
        panic!("second source should be Local");
    };
    assert_eq!(local.id, "internal-wiki");
}

/// Malformed YAML produces a `GraphtorError::Config` with a `[config]` prefix.
#[test]
fn parse_malformed_yaml_returns_config_error() {
    let bad_yaml = "sources:\n  - type: git\n    id: [unclosed\n";
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
  - type: git
    id: duplicate-id
    url: https://github.com/example/repo.git
    include: ["**/*.md"]
  - type: local
    id: duplicate-id
    path: /docs
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
