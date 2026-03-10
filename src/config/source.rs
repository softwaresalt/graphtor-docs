//! Configuration structs for parsing `sources.yaml`.
//!
//! Defines [`SourceConfig`], [`Source`], [`GitSource`], and [`LocalSource`]
//! with `serde` derives for YAML deserialization.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::GraphtorError;

/// Top-level configuration parsed from `sources.yaml`.
///
/// Contains an ordered list of documentation sources that the pipeline
/// will acquire, normalize, chunk, embed, and load into the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Ordered list of documentation sources.
    pub sources: Vec<Source>,
}

impl SourceConfig {
    /// Parse a `sources.yaml` file from disk and validate it.
    ///
    /// Reads the file at `path`, deserializes the YAML into a [`SourceConfig`],
    /// then runs semantic validation (duplicate IDs, empty fields, glob syntax).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Io`] if the file cannot be read.
    /// Returns [`GraphtorError::Config`] if the YAML is malformed or fails validation.
    pub fn parse(path: &Path) -> Result<Self, GraphtorError> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content).map_err(|e| GraphtorError::Config {
            message: e.to_string(),
            field: None,
        })?;
        crate::config::validation::validate(&config)?;
        Ok(config)
    }
}

/// A documentation source — either a remote Git repository or a local directory.
///
/// Discriminated in YAML by the `type` field (`git` or `local`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// A Git repository to shallow-clone and index.
    Git(GitSource),
    /// A local filesystem directory to index.
    Local(LocalSource),
}

impl Source {
    /// Returns the unique identifier for this source.
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Git(g) => &g.id,
            Self::Local(l) => &l.id,
        }
    }
}

/// A remote Git repository documentation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitSource {
    /// Unique identifier for this source (e.g., `"ms-azure-core"`).
    pub id: String,
    /// Git clone URL (HTTPS or SSH).
    pub url: String,
    /// Branch to clone. Defaults to `"main"`.
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Glob patterns selecting files to include (e.g., `["**/*.md"]`).
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns selecting files to exclude (e.g., `["**/drafts/**"]`).
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// A local filesystem directory documentation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSource {
    /// Unique identifier for this source (e.g., `"internal-api-docs"`).
    pub id: String,
    /// Filesystem path to the local documentation directory.
    pub path: String,
    /// Glob patterns selecting files to include (e.g., `["**/*.md"]`).
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns selecting files to exclude. Defaults to empty.
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T013: valid YAML deserialization ─────────────────────────────────

    const VALID_GIT_YAML: &str = r#"
sources:
  - type: git
    id: ms-azure-core
    url: https://github.com/MicrosoftDocs/azure-docs.git
    branch: main
    include:
      - "**/*.md"
    exclude:
      - "**/drafts/**"
"#;

    const VALID_LOCAL_YAML: &str = r#"
sources:
  - type: local
    id: internal-docs
    path: /workspace/docs
    include:
      - "**/*.md"
"#;

    const VALID_MIXED_YAML: &str = r#"
sources:
  - type: git
    id: ms-azure
    url: https://github.com/MicrosoftDocs/azure-docs.git
    include:
      - "**/*.md"
  - type: local
    id: local-guide
    path: /docs
    include:
      - "**/*.md"
    exclude:
      - "**/private/**"
"#;

    #[test]
    fn git_source_deserializes_all_fields() {
        let config: SourceConfig = serde_yaml::from_str(VALID_GIT_YAML).unwrap();
        assert_eq!(config.sources.len(), 1);
        let Source::Git(git) = &config.sources[0] else {
            panic!("expected GitSource");
        };
        assert_eq!(git.id, "ms-azure-core");
        assert_eq!(git.url, "https://github.com/MicrosoftDocs/azure-docs.git");
        assert_eq!(git.branch, "main");
        assert_eq!(git.include, vec!["**/*.md"]);
        assert_eq!(git.exclude, vec!["**/drafts/**"]);
    }

    #[test]
    fn git_source_branch_defaults_to_main() {
        const NO_BRANCH: &str = r#"
sources:
  - type: git
    id: test-repo
    url: https://github.com/example/repo.git
    include: ["**/*.md"]
"#;
        let config: SourceConfig = serde_yaml::from_str(NO_BRANCH).unwrap();
        let Source::Git(git) = &config.sources[0] else {
            panic!("expected GitSource");
        };
        assert_eq!(git.branch, "main", "branch should default to 'main'");
    }

    #[test]
    fn local_source_deserializes_all_fields() {
        let config: SourceConfig = serde_yaml::from_str(VALID_LOCAL_YAML).unwrap();
        assert_eq!(config.sources.len(), 1);
        let Source::Local(local) = &config.sources[0] else {
            panic!("expected LocalSource");
        };
        assert_eq!(local.id, "internal-docs");
        assert_eq!(local.path, "/workspace/docs");
        assert_eq!(local.include, vec!["**/*.md"]);
        assert!(local.exclude.is_empty(), "exclude should default to empty");
    }

    #[test]
    fn mixed_sources_deserialize_correctly() {
        let config: SourceConfig = serde_yaml::from_str(VALID_MIXED_YAML).unwrap();
        assert_eq!(config.sources.len(), 2);
        assert!(matches!(config.sources[0], Source::Git(_)));
        assert!(matches!(config.sources[1], Source::Local(_)));
    }

    // ── T015: edge cases ──────────────────────────────────────────────────

    #[test]
    fn empty_sources_list_parses_without_error() {
        let yaml = "sources: []\n";
        let config: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.sources.is_empty());
    }

    #[test]
    fn wrong_yaml_structure_returns_error() {
        let yaml = "this_is_not_a_sources_config: true\n";
        let result: Result<SourceConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "should fail on wrong top-level key");
    }

    #[test]
    fn parse_missing_file_returns_io_error() {
        let result = SourceConfig::parse(Path::new("/nonexistent/path/sources.yaml"));
        assert!(result.is_err());
        let e = result.unwrap_err();
        let s = e.to_string();
        assert!(
            s.starts_with("[io]") || s.starts_with("[config]"),
            "unexpected error type: {s}"
        );
    }
}
