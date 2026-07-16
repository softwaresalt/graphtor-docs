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

/// A documentation source — either a local ingestion directory or an
/// explicit, pre-built, workspace-contained read-only database entry.
///
/// Discriminated in YAML by `type: local` or `type: database`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// A local filesystem directory of standardized Markdown files to index.
    Local(LocalSource),
    /// An explicit, pre-built database file to serve read-only.
    ///
    /// Workspace-contained: `path` MUST resolve within the same authorized
    /// root as auto-discovery (external/out-of-root paths are rejected, not
    /// served — external-path support is explicitly out of Phase-1 scope).
    /// This variant is inherently read-only; there is no `read_only` field.
    Database(DatabaseSource),
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
            Self::Database(d) => &d.id,
        }
    }

    /// Returns the allowed file format extensions for this source.
    ///
    /// A [`Source::Database`] entry has no ingestion formats: empty.
    pub(crate) fn formats(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.formats,
            Self::Database(_) => &[],
        }
    }

    /// Returns the source-relative include glob patterns for this source.
    ///
    /// A [`Source::Database`] entry has no ingestion globs: empty.
    pub(crate) fn include(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.include,
            Self::Database(_) => &[],
        }
    }

    /// Returns the source-relative exclude glob patterns for this source.
    ///
    /// A [`Source::Database`] entry has no ingestion globs: empty.
    pub(crate) fn exclude(&self) -> &[String] {
        match self {
            Self::Local(l) => &l.exclude,
            Self::Database(_) => &[],
        }
    }

    /// Returns the target database file name for this source, if set.
    ///
    /// This is local-target-only: it feeds the ingestion/generation WRITE
    /// path (`resolve_source_db_path`, `discover_db_files`). A
    /// [`Source::Database`] entry's pre-built served path is NEVER returned
    /// here — use [`Source::served_db_path`] instead — otherwise a served
    /// read-only db could be routed into the write/sync path.
    #[must_use]
    pub fn database(&self) -> Option<&str> {
        match self {
            Self::Local(l) => l.database.as_deref(),
            Self::Database(_) => None,
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
    /// existing call site.
    #[must_use]
    pub fn as_local(&self) -> Option<&LocalSource> {
        match self {
            Self::Local(l) => Some(l),
            Self::Database(_) => None,
        }
    }

    /// Returns `true` when this source is eligible for ingestion (scanning,
    /// acquisition planning, and sync).
    ///
    /// `pub(crate)` — only called from within this library (the acquisition
    /// plan loop and sync path in P1-RF2/P1-RF3). `false` for
    /// [`Source::Database`] — it is never ingested.
    pub(crate) fn is_ingestible(&self) -> bool {
        self.as_local().is_some()
    }

    /// Returns the pre-built, workspace-contained path to serve read-only
    /// for a [`Source::Database`] entry, or `None` for any other variant.
    ///
    /// Distinct from [`Source::database`] (a generation TARGET filename fed
    /// to the ingestion/write path): this accessor exposes the READ-ONLY
    /// served path and is consumed only by `serve_discovery`'s merge step.
    /// Containment (workspace-root validation) is the caller's
    /// responsibility, matching every other served-path consumer.
    #[must_use]
    pub fn served_db_path(&self) -> Option<&Path> {
        match self {
            Self::Local(_) => None,
            Self::Database(d) => Some(&d.path),
        }
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

/// An explicit, pre-built database file to serve read-only.
///
/// LOCKED serialized contract (Phase 1): exactly two REQUIRED fields, `id`
/// and `path`, and no others — this variant is inherently read-only, so
/// there is deliberately no `read_only` field. `#[serde(deny_unknown_fields)]`
/// enforces the closed shape: any extra key (e.g. a `read_only` typo) is
/// rejected at parse time rather than silently ignored and still served
/// read-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseSource {
    /// Unique alias/name for this served database (e.g. `"legacy-docs"`).
    pub id: String,
    /// Workspace-contained filesystem path to the pre-built database file.
    ///
    /// Validated (by `serve_discovery`) to resolve within the same
    /// authorized root as auto-discovery; an out-of-root path (`..`,
    /// symlink, or Windows junction/reparse escape) is rejected, not
    /// served.
    pub path: PathBuf,
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

    // ── P1-T6: Source::Database variant ────────────────────────────────────

    #[test]
    fn database_source_round_trips_through_yaml() {
        const YAML: &str =
            "sources:\n  - type: database\n    id: legacy-docs\n    path: legacy.db\n";
        let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse type: database");
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].id(), "legacy-docs");
        assert_eq!(
            config.sources[0].served_db_path(),
            Some(Path::new("legacy.db"))
        );

        let reserialized = serde_yaml::to_string(&config).expect("reserialize");
        let round_tripped: SourceConfig =
            serde_yaml::from_str(&reserialized).expect("parse reserialized yaml");
        assert_eq!(config, round_tripped);
    }

    #[test]
    fn database_source_has_exactly_id_and_path_fields() {
        // LOCKED contract: no `read_only` field and no other fields. A valid
        // `{id, path}` entry parses into exactly those two fields; the
        // unknown-field rejection is covered by
        // `database_source_rejects_an_unknown_field`.
        const YAML: &str =
            "sources:\n  - type: database\n    id: legacy-docs\n    path: legacy.db\n";
        let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse");
        let Source::Database(d) = &config.sources[0] else {
            panic!("expected a Database source");
        };
        assert_eq!(d.id, "legacy-docs");
        assert_eq!(d.path, PathBuf::from("legacy.db"));
    }

    #[test]
    fn database_source_rejects_an_unknown_field() {
        // LOCKED contract (Copilot source.rs:201): the variant carries exactly
        // `id` and `path`. An extra field such as `read_only: false` is a typo
        // (or an attempt to override the inherent read-only posture) and MUST
        // be rejected at parse time, not silently ignored and still served
        // read-only. `#[serde(deny_unknown_fields)]` enforces this.
        const YAML: &str = "sources:\n  - type: database\n    id: legacy-docs\n    \
             path: legacy.db\n    read_only: false\n";
        let result: Result<SourceConfig, _> = serde_yaml::from_str(YAML);
        assert!(
            result.is_err(),
            "a `type: database` entry carrying an unknown field must be rejected"
        );
    }

    #[test]
    fn database_source_valid_entry_still_parses_with_deny_unknown_fields() {
        // Prove `deny_unknown_fields` on the variant struct does NOT reject the
        // internally-tagged enum's own `type` discriminator: serde strips the
        // tag before handing the remaining content to `DatabaseSource`, so a
        // valid `{type, id, path}` entry round-trips unchanged.
        const YAML: &str =
            "sources:\n  - type: database\n    id: legacy-docs\n    path: legacy.db\n";
        let config: SourceConfig =
            serde_yaml::from_str(YAML).expect("a valid type: database entry must still parse");
        let Source::Database(d) = &config.sources[0] else {
            panic!("expected a Database source");
        };
        assert_eq!(d.id, "legacy-docs");
        assert_eq!(d.path, PathBuf::from("legacy.db"));

        let reserialized = serde_yaml::to_string(&config).expect("reserialize");
        let round_tripped: SourceConfig =
            serde_yaml::from_str(&reserialized).expect("parse reserialized yaml");
        assert_eq!(config, round_tripped);
    }

    #[test]
    fn database_source_accessors_are_variant_safe() {
        let src = Source::Database(DatabaseSource {
            id: "legacy-docs".to_string(),
            path: PathBuf::from("legacy.db"),
        });
        assert_eq!(src.id(), "legacy-docs");
        assert!(src.formats().is_empty());
        assert!(src.include().is_empty());
        assert!(src.exclude().is_empty());
        assert!(
            src.database().is_none(),
            "database() is local-target-only and must never return the served path"
        );
        assert!(src.as_local().is_none());
        assert!(!src.is_ingestible());
        assert_eq!(src.served_db_path(), Some(Path::new("legacy.db")));
    }

    #[test]
    fn local_source_served_db_path_is_none() {
        let src = Source::Local(LocalSource {
            id: "t".to_string(),
            path: PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        assert!(src.served_db_path().is_none());
    }

    #[test]
    fn existing_local_only_sources_yaml_parses_unchanged_after_database_variant_added() {
        // Backward compatibility: adding the additive `Database` variant
        // must not change how a pre-existing `type: local` entry parses.
        let config: SourceConfig = serde_yaml::from_str(VALID_LOCAL_YAML).unwrap();
        assert_eq!(config.sources.len(), 1);
        let local = config.sources[0].as_local().expect("still a local source");
        assert_eq!(local.id, "internal-docs");
    }

    #[test]
    fn mixed_local_and_database_config_parses_both_variants() {
        const YAML: &str = "sources:\n  - type: local\n    id: docs\n    path: docs\n  - type: database\n    id: legacy\n    path: legacy.db\n";
        let config: SourceConfig = serde_yaml::from_str(YAML).expect("parse mixed config");
        assert_eq!(config.sources.len(), 2);
        assert!(config.sources[0].as_local().is_some());
        assert!(config.sources[1].served_db_path().is_some());
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
