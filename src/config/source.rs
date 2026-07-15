//! Configuration structs for parsing `sources.yaml`.
//!
//! Defines [`SourceConfig`] and [`LocalSource`] for the standardized-markdown
//! local-source ingestion model. Only local filesystem sources targeting
//! docline-emitted Markdown files are supported.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::GraphtorError;

/// Top-level configuration parsed from `sources.yaml`.
///
/// Contains an ordered list of local documentation sources that the pipeline
/// will scan, validate, chunk, embed, and load into the graph. Each source
/// must reference a directory of docline-emitted standardized Markdown files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Ordered list of local standardized-markdown documentation sources.
    pub sources: Vec<Source>,
}

impl SourceConfig {
    /// Parse a `sources.yaml` file from disk and validate it.
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
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] if any validation rule fails.
    pub fn validate(&self) -> Result<(), GraphtorError> {
        crate::config::validation::validate(self)
    }
}

/// A documentation source — a local directory of docline-emitted Markdown files.
///
/// Only the `local` type is supported. Discriminated in YAML by `type: local`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// A local filesystem directory of standardized Markdown files to index.
    Local(LocalSource),
}

impl Source {
    /// Returns the unique identifier for this source.
    ///
    /// `pub` (widened from `pub(crate)`) so the bin crate (`src/main.rs`)
    /// and external integration tests — separate crates from this
    /// library — can call it across the crate boundary once every
    /// `Source::Local` consumer is routed through variant-safe accessors
    /// (P1-RF1..P1-RF5).
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Local(l) => &l.id,
        }
    }

    /// Returns the allowed file format extensions for this source.
    pub(crate) fn formats(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.formats,
        }
    }

    /// Returns the source-relative include glob patterns for this source.
    pub(crate) fn include(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.include,
        }
    }

    /// Returns the source-relative exclude glob patterns for this source.
    pub(crate) fn exclude(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.exclude,
        }
    }

    /// Returns the target database file name for this source, if set.
    #[must_use]
    pub fn database(&self) -> Option<&str> {
        match self {
            Self::Local(l) => l.database.as_deref(),
        }
    }

    /// Returns this source as a [`LocalSource`] reference, or `None` when it
    /// is not a local ingestion source.
    ///
    /// This is the variant-safe replacement for irrefutably destructuring
    /// `Source::Local(..)`: every consumer that only knows how to handle
    /// local ingestion sources MUST route through this accessor (or
    /// [`Source::is_ingestible`]) instead, so a future additive,
    /// non-ingestible `Source` variant compiles without breaking any
    /// existing call site. Always `Some` while `Local` is the only variant.
    #[must_use]
    pub fn as_local(&self) -> Option<&LocalSource> {
        match self {
            Self::Local(l) => Some(l),
        }
    }

    /// Returns `true` when this source is eligible for ingestion (scanning,
    /// acquisition planning, and sync).
    ///
    /// `pub(crate)` — only called from within this library (the acquisition
    /// plan loop and sync path in P1-RF2/P1-RF3). Always `true` while
    /// `Local` is the only variant.
    #[allow(dead_code)] // consumed by P1-RF3 (050.012-T src/sync/mod.rs)
    pub(crate) fn is_ingestible(&self) -> bool {
        self.as_local().is_some()
    }
}

/// A local filesystem directory of docline-emitted standardized Markdown files.
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
    /// File extension allow-list for this source.
    ///
    /// Only Markdown extensions are accepted (`md` and `markdown`).
    /// Defaults to `["md"]` when the field is absent from YAML.
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
    /// Optional target database file name (e.g. `"rust-docs.db"`).
    #[serde(default)]
    pub database: Option<String>,
}

/// Default file formats processed when `formats` is absent from YAML.
fn default_formats() -> Vec<String> {
    vec!["md".to_string()]
}

/// Canonicalize a configured format alias to its canonical file extension.
///
/// Maps the `"markdown"` long-form alias to `"md"` (case-insensitive).
/// All other values are returned as-is so unknown extensions pass through
/// unchanged.
///
/// Callers should compare the result against the value produced by
/// [`crate::parse::normalized_document_extension`], which applies the same
/// `.markdown` → `md` normalisation on the file side.  Using this function
/// on the _configured_ format ensures both sides of the comparison are in
/// canonical form.
#[must_use]
pub(crate) fn canonicalize_format_ext(fmt: &str) -> &str {
    if fmt.eq_ignore_ascii_case("markdown") {
        "md"
    } else {
        fmt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_LOCAL_YAML: &str = r#"
sources:
  - type: local
    id: internal-docs
    path: /workspace/docs
    include:
      - "**/*.md"
"#;

    #[test]
    fn local_source_deserializes_all_fields() {
        let config: SourceConfig = serde_yaml::from_str(VALID_LOCAL_YAML).unwrap();
        assert_eq!(config.sources.len(), 1);
        let local = config.sources[0].as_local().expect("local source");
        assert_eq!(local.id, "internal-docs");
        assert_eq!(local.path, PathBuf::from("/workspace/docs"));
        assert_eq!(local.include, vec!["**/*.md"]);
        assert!(local.exclude.is_empty());
    }

    #[test]
    fn empty_sources_list_parses_without_error() {
        let yaml = "sources: []\n";
        let config: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.sources.is_empty());
    }

    #[test]
    fn parse_missing_file_returns_io_error() {
        let result = SourceConfig::parse(Path::new("/nonexistent/path/sources.yaml"));
        assert!(result.is_err());
        let s = result.unwrap_err().to_string();
        assert!(s.starts_with("[io]"), "got: {s}");
    }

    #[test]
    fn formats_defaults_to_md_when_absent_from_yaml() {
        const YAML: &str = "sources:\n  - type: local\n    id: t\n    path: /docs\n";
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let local = config.sources[0].as_local().expect("local source");
        assert_eq!(local.formats, vec!["md"]);
    }

    #[test]
    fn formats_parsed_from_yaml() {
        const YAML: &str =
            "sources:\n  - type: local\n    id: t\n    path: /docs\n    formats:\n      - md\n      - markdown\n";
        let config: SourceConfig = serde_yaml::from_str(YAML).unwrap();
        let local = config.sources[0].as_local().expect("local source");
        assert_eq!(local.formats, vec!["md", "markdown"]);
    }

    #[test]
    fn source_database_accessor_returns_none_when_absent() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        assert!(src.database().is_none());
    }

    // ── P1-RF1: variant-safe accessors ────────────────────────────────────

    #[test]
    fn as_local_returns_some_for_a_local_source() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        assert!(src.as_local().is_some());
        assert_eq!(src.as_local().unwrap().id, "t");
    }

    #[test]
    fn is_ingestible_returns_true_for_a_local_source() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        assert!(src.is_ingestible());
    }

    #[test]
    fn id_accessor_is_reachable_as_a_public_item() {
        // Compiles only if `id()` is `pub` (widened from `pub(crate)`) —
        // this test module is technically still inside the library crate,
        // so the real cross-crate check is the bin crate/external test
        // build succeeding; this test pins the intended visibility.
        let src = Source::Local(LocalSource {
            id: "cross-crate-id".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        let id: &str = Source::id(&src);
        assert_eq!(id, "cross-crate-id");
    }

    #[test]
    fn git_source_type_is_rejected_by_deserialization() {
        const GIT_YAML: &str = "sources:\n  - type: git\n    id: repo\n    url: https://github.com/example/repo.git\n    branch: main\n";
        let result: Result<SourceConfig, _> = serde_yaml::from_str(GIT_YAML);
        assert!(result.is_err(), "git source type must be rejected");
    }

    #[test]
    fn url_source_type_is_rejected_by_deserialization() {
        const URL_YAML: &str =
            "sources:\n  - type: url\n    id: web\n    url: https://example.com/\n";
        let result: Result<SourceConfig, _> = serde_yaml::from_str(URL_YAML);
        assert!(result.is_err(), "url source type must be rejected");
    }

    #[test]
    fn pdf_format_is_rejected_by_validation() {
        let config: SourceConfig = serde_yaml::from_str(
            "sources:\n  - type: local\n    id: t\n    path: /docs\n    formats:\n      - pdf\n",
        )
        .unwrap();
        let err = config.validate().expect_err("pdf must be rejected");
        assert!(err.to_string().contains("[config]"), "{err}");
    }

    #[test]
    fn docx_format_is_rejected_by_validation() {
        let config: SourceConfig = serde_yaml::from_str(
            "sources:\n  - type: local\n    id: t\n    path: /docs\n    formats:\n      - docx\n",
        )
        .unwrap();
        let err = config.validate().expect_err("docx must be rejected");
        assert!(err.to_string().contains("[config]"), "{err}");
    }

    #[test]
    fn html_format_is_rejected_by_validation() {
        let config: SourceConfig = serde_yaml::from_str(
            "sources:\n  - type: local\n    id: t\n    path: /docs\n    formats:\n      - html\n",
        )
        .unwrap();
        let err = config.validate().expect_err("html must be rejected");
        assert!(err.to_string().contains("[config]"), "{err}");
    }
}
