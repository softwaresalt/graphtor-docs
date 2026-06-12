//! Configuration structs for parsing `sources.yaml`.
//!
//! Defines [`SourceConfig`], [`Source`], [`GitSource`], and [`LocalSource`]
//! with `serde` derives for YAML deserialization.

use std::path::{Path, PathBuf};

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
        let config: Self = serde_yaml::from_str(&content)?;
        crate::config::validation::validate(&config)?;
        Ok(config)
    }

    /// Validate this configuration against all semantic rules.
    ///
    /// Checks for duplicate source IDs, empty required fields, and valid glob
    /// patterns. Validation is also run automatically by [`parse`](Self::parse).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] if any validation rule fails.
    pub fn validate(&self) -> Result<(), GraphtorError> {
        crate::config::validation::validate(self)
    }
}

/// A documentation source — a remote Git repository, local directory, or web URL.
///
/// Discriminated in YAML by the `type` field (`git`, `local`, or `url`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// A Git repository to shallow-clone and index.
    Git(GitSource),
    /// A local filesystem directory to index.
    Local(LocalSource),
    /// A web URL to crawl and index.
    Url(UrlSource),
}

impl Source {
    /// Returns the unique identifier for this source.
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Git(g) => &g.id,
            Self::Local(l) => &l.id,
            Self::Url(u) => &u.id,
        }
    }

    /// Returns the allowed file format extensions for this source.
    ///
    /// An empty slice means no restriction — all pipeline-supported extensions
    /// are processed.  A non-empty slice acts as an allow-list.
    pub(crate) fn formats(&self) -> &[String] {
        match self {
            Self::Git(g) => &g.formats,
            Self::Local(l) => &l.formats,
            Self::Url(u) => &u.formats,
        }
    }

    /// Returns the source-relative include glob patterns for this source.
    pub(crate) fn include(&self) -> &[String] {
        match self {
            Self::Git(g) => &g.include,
            Self::Local(l) => &l.include,
            Self::Url(u) => &u.include,
        }
    }

    /// Returns the source-relative exclude glob patterns for this source.
    pub(crate) fn exclude(&self) -> &[String] {
        match self {
            Self::Git(g) => &g.exclude,
            Self::Local(l) => &l.exclude,
            Self::Url(u) => &u.exclude,
        }
    }

    /// Returns the target database file name for this source, if set.
    ///
    /// When `Some`, the source's content is routed to the named database
    /// file relative to `.graphtor/`. When `None`, the default database
    /// is used.
    #[must_use]
    pub fn database(&self) -> Option<&str> {
        match self {
            Self::Git(g) => g.database.as_deref(),
            Self::Local(l) => l.database.as_deref(),
            Self::Url(u) => u.database.as_deref(),
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
    /// File extension allow-list for this source (e.g., `["md", "pdf"]`).
    ///
    /// Only files whose extension matches one of the listed strings (case-insensitive)
    /// are passed to the parse stage.  An empty list means no restriction — all
    /// extensions supported by the pipeline are processed.
    ///
    /// Defaults to `["md", "pdf", "docx"]` when the field is absent from YAML.
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    /// Optional target database file name (e.g. `"rust-docs.db"`).
    ///
    /// When set, content from this source is routed to the named database
    /// file relative to `.graphtor/`. When absent, content goes to the
    /// default database. The value must not be empty, must not contain
    /// path separators, and must not contain `..` components.
    #[serde(default)]
    pub database: Option<String>,
}

/// A local filesystem directory documentation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSource {
    /// Unique identifier for this source (e.g., `"internal-api-docs"`).
    pub id: String,
    /// Filesystem path to the local documentation directory.
    pub path: PathBuf,
    /// Glob patterns selecting files to include (e.g., `["**/*.md"]`).
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns selecting files to exclude. Defaults to empty.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// File extension allow-list for this source (e.g., `["md", "pdf"]`).
    ///
    /// Only files whose extension matches one of the listed strings (case-insensitive)
    /// are passed to the parse stage.  An empty list means no restriction — all
    /// extensions supported by the pipeline are processed.
    ///
    /// Defaults to `["md", "pdf", "docx"]` when the field is absent from YAML.
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    /// Optional target database file name (e.g. `"rust-docs.db"`).
    ///
    /// When set, content from this source is routed to the named database
    /// file relative to `.graphtor/`. When absent, content goes to the
    /// default database. The value must not be empty, must not contain
    /// path separators, and must not contain `..` components.
    #[serde(default)]
    pub database: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

/// Default file formats processed when `formats` is absent from YAML.
fn default_formats() -> Vec<String> {
    vec!["md".to_string(), "pdf".to_string(), "docx".to_string()]
}

/// Maximum BFS crawl depth relative to the start URL.
fn default_max_depth() -> u32 {
    3
}

/// Maximum number of pages to crawl.
fn default_max_pages() -> usize {
    100
}

/// Whether to restrict the crawl to the start URL's registered domain.
fn default_domain_lock() -> bool {
    true
}

/// Minimum milliseconds to wait between consecutive HTTP requests.
fn default_rate_limit_ms() -> u64 {
    500
}

/// A web URL documentation source crawled via HTTP.
///
/// The crawler performs a BFS traversal starting from `url`, converts each
/// HTML page to Markdown via `htmd`, and writes the results to a local cache
/// directory for subsequent pipeline stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlSource {
    /// Unique identifier for this source (e.g., `"ms-learn-dotnet"`).
    pub id: String,
    /// Start URL for the crawl (must use `https://` or `http://`).
    pub url: String,
    /// Maximum BFS depth relative to `url`. Defaults to `3`.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Maximum number of pages to crawl. Defaults to `100`.
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    /// When `true`, the crawler stays within `url`'s domain. Defaults to `true`.
    #[serde(default = "default_domain_lock")]
    pub domain_lock: bool,
    /// Minimum wait between HTTP requests in milliseconds. Defaults to `500`.
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,
    /// Glob patterns selecting crawled-page file paths to include.
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns selecting crawled-page file paths to exclude.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// File extension allow-list for this source (e.g., `["md", "pdf"]`).
    ///
    /// Only files whose extension matches one of the listed strings (case-insensitive)
    /// are passed to the parse stage.  An empty list means no restriction — all
    /// extensions supported by the pipeline are processed.
    ///
    /// Defaults to `["md", "pdf", "docx"]` when the field is absent from YAML.
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    /// Optional target database file name (e.g. `"rust-docs.db"`).
    ///
    /// When set, content from this source is routed to the named database
    /// file relative to `.graphtor/`. When absent, content goes to the
    /// default database. The value must not be empty, must not contain
    /// path separators, and must not contain `..` components.
    #[serde(default)]
    pub database: Option<String>,
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
        assert_eq!(local.path, PathBuf::from("/workspace/docs"));
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
            s.starts_with("[io]"),
            "missing file must produce an Io error, got: {s}"
        );
    }

    // ── T021.002: formats field ───────────────────────────────────────────

    #[test]
    fn formats_defaults_to_all_three_when_absent_from_yaml() {
        const NO_FORMATS: &str = r#"
sources:
  - type: git
    id: test-repo
    url: https://github.com/example/repo.git
    include: ["**/*.md"]
"#;
        let config: SourceConfig = serde_yaml::from_str(NO_FORMATS).unwrap();
        let Source::Git(git) = &config.sources[0] else {
            panic!("expected GitSource");
        };
        assert_eq!(
            git.formats,
            vec!["md", "pdf", "docx"],
            "formats must default to all three when absent"
        );
    }

    #[test]
    fn formats_parsed_from_yaml() {
        const WITH_FORMATS: &str = r"
sources:
  - type: local
    id: test-local
    path: /docs
    formats:
      - md
      - pdf
";
        let config: SourceConfig = serde_yaml::from_str(WITH_FORMATS).unwrap();
        let Source::Local(local) = &config.sources[0] else {
            panic!("expected LocalSource");
        };
        assert_eq!(local.formats, vec!["md", "pdf"]);
    }

    #[test]
    fn source_formats_accessor_returns_inner_slice() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string(), "pdf".to_string()],
            database: None,
        });
        assert_eq!(src.formats(), &["md", "pdf"]);
    }

    #[test]
    fn git_formats_accessor_returns_inner_slice() {
        let src = Source::Git(GitSource {
            id: "t".to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec![],
            exclude: vec![],
            formats: vec!["docx".to_string()],
            database: None,
        });
        assert_eq!(src.formats(), &["docx"]);
    }

    #[test]
    fn empty_formats_list_parsed_correctly() {
        const EMPTY_FORMATS: &str = r"
sources:
  - type: local
    id: empty-fmt
    path: /docs
    formats: []
";
        let config: SourceConfig = serde_yaml::from_str(EMPTY_FORMATS).unwrap();
        let Source::Local(local) = &config.sources[0] else {
            panic!("expected LocalSource");
        };
        assert!(
            local.formats.is_empty(),
            "empty formats list must parse as empty"
        );
    }

    // ── T038.001: database field ──────────────────────────────────────────

    #[test]
    fn database_field_defaults_to_none_when_absent_git() {
        const YAML: &str = r#"
sources:
  - type: git
    id: test-repo
    url: https://github.com/example/repo.git
    include: ["**/*.md"]
"#;
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let Source::Git(git) = &config.sources[0] else {
            panic!("expected GitSource");
        };
        assert!(git.database.is_none(), "database must default to None");
    }

    #[test]
    fn database_field_defaults_to_none_when_absent_local() {
        const YAML: &str = r"
sources:
  - type: local
    id: local-docs
    path: /docs
";
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let Source::Local(local) = &config.sources[0] else {
            panic!("expected LocalSource");
        };
        assert!(local.database.is_none(), "database must default to None");
    }

    #[test]
    fn database_field_parsed_from_yaml_git() {
        const YAML: &str = r#"
sources:
  - type: git
    id: rust-docs
    url: https://github.com/example/repo.git
    include: ["**/*.md"]
    database: "rust-docs.db"
"#;
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let Source::Git(git) = &config.sources[0] else {
            panic!("expected GitSource");
        };
        assert_eq!(git.database.as_deref(), Some("rust-docs.db"));
    }

    #[test]
    fn database_field_parsed_from_yaml_local() {
        const YAML: &str = r#"
sources:
  - type: local
    id: local-docs
    path: /docs
    database: "local.db"
"#;
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let Source::Local(local) = &config.sources[0] else {
            panic!("expected LocalSource");
        };
        assert_eq!(local.database.as_deref(), Some("local.db"));
    }

    #[test]
    fn source_database_accessor_returns_some_when_set() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some("target.db".to_string()),
        });
        assert_eq!(src.database(), Some("target.db"));
    }

    #[test]
    fn source_database_accessor_returns_none_when_absent() {
        let src = Source::Git(GitSource {
            id: "t".to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        assert!(src.database().is_none());
    }
}
