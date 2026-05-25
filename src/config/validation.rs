//! Configuration validation logic for `sources.yaml`.
//!
//! Validates semantic constraints that YAML deserialization alone cannot
//! enforce: duplicate source IDs, empty required fields, and glob pattern
//! syntax validity.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use walkdir::WalkDir;

use crate::config::source::{LocalSource, Source, SourceConfig};
use crate::error::GraphtorError;

/// Extension strings accepted by the ingestion pipeline.
///
/// `"markdown"` is included as an alias for `"md"` because the pipeline
/// canonicalises the `.markdown` file extension to `"md"` at runtime.
/// Validation accepts both spellings so that user config is consistent with
/// what the pipeline actually processes.
const VALID_FORMATS: &[&str] = &["md", "pdf", "docx", "markdown"];

/// Validate a parsed [`SourceConfig`] for semantic correctness.
///
/// Checks performed:
/// - No empty source IDs.
/// - No duplicate source IDs across all sources.
/// - All glob patterns in `include` and `exclude` fields compile successfully.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] describing the first validation failure.
pub fn validate(config: &SourceConfig) -> Result<(), GraphtorError> {
    let mut seen_ids: HashSet<&str> = HashSet::new();

    for source in &config.sources {
        let id = source.id();

        if id.is_empty() {
            return Err(GraphtorError::Config {
                message: "source id must not be empty".to_string(),
                field: Some("id".to_string()),
            });
        }

        // Reject IDs that contain path separators or `..` path traversal components.
        // Uses `Path::components()` to catch `..` as a discrete `ParentDir` component,
        // avoiding false positives on substrings like `"v1..v2"` (RI-007, CC1).
        let has_separator = id.contains('/') || id.contains('\\');
        let has_parent_dir = Path::new(id)
            .components()
            .any(|c| c == Component::ParentDir);
        if has_separator || has_parent_dir {
            return Err(GraphtorError::Config {
                message: format!(
                    "source id must not contain path separators or '..' components: '{id}'"
                ),
                field: Some("id".to_string()),
            });
        }

        if !seen_ids.insert(id) {
            return Err(GraphtorError::Config {
                message: format!("duplicate source id: '{id}'"),
                field: Some("id".to_string()),
            });
        }

        let (include, exclude) = match source {
            Source::Git(g) => (&g.include, &g.exclude),
            Source::Local(l) => (&l.include, &l.exclude),
            Source::Url(u) => {
                if !u.url.starts_with("https://") && !u.url.starts_with("http://") {
                    return Err(GraphtorError::Config {
                        message: format!(
                            "url source '{}' url must use https:// or http://: '{}'",
                            u.id, u.url
                        ),
                        field: Some("url".to_string()),
                    });
                }
                if u.max_pages == 0 {
                    return Err(GraphtorError::Config {
                        message: format!("url source '{}' max_pages must be greater than 0", u.id),
                        field: Some("max_pages".to_string()),
                    });
                }
                (&u.include, &u.exclude)
            }
        };

        if let Some(db_name) = source.database() {
            validate_database_name(db_name, id)?;
        }
        validate_globs(include, id)?;
        validate_globs(exclude, id)?;
        validate_formats(source.formats(), id)?;
    }

    Ok(())
}

/// Validate a database name value.
///
/// Rules:
/// - Must not be empty (empty string would be confusing and unparseable).
/// - Must not contain path separators (`/` or `\`).
/// - Must not contain `..` path-traversal components.
fn validate_database_name(name: &str, source_id: &str) -> Result<(), GraphtorError> {
    if name.is_empty() {
        return Err(GraphtorError::Config {
            message: format!(
                "source '{source_id}' database name must not be empty; \
                 omit the field entirely to use the default database"
            ),
            field: Some("database".to_string()),
        });
    }
    let has_separator = name.contains('/') || name.contains('\\');
    let has_parent_dir = Path::new(name)
        .components()
        .any(|c| c == Component::ParentDir);
    if has_separator || has_parent_dir {
        return Err(GraphtorError::Config {
            message: format!(
                "source '{source_id}' database name must not contain path separators \
                 or '..' components: '{name}'"
            ),
            field: Some("database".to_string()),
        });
    }
    Ok(())
}

/// Validate that all strings in `formats` are recognized pipeline extensions.
///
/// Comparison is case-insensitive: `"MD"`, `"Pdf"`, and `"DOCX"` are all
/// accepted.  This matches the pipeline's runtime behaviour, which lower-cases
/// file extensions before applying the allow-list.
fn validate_formats(formats: &[String], source_id: &str) -> Result<(), GraphtorError> {
    for fmt in formats {
        let normalised = fmt.to_ascii_lowercase();
        if !VALID_FORMATS.contains(&normalised.as_str()) {
            return Err(GraphtorError::Config {
                message: format!(
                    "source '{source_id}' has invalid format '{fmt}'; \
                     valid formats are: {}",
                    VALID_FORMATS.join(", ")
                ),
                field: Some("formats".to_string()),
            });
        }
    }
    Ok(())
}

/// Validate that all patterns compile as `globset` globs.
fn validate_globs(patterns: &[String], source_id: &str) -> Result<(), GraphtorError> {
    for pattern in patterns {
        Glob::new(pattern).map_err(|e| GraphtorError::Config {
            message: format!("invalid glob pattern '{pattern}' in source '{source_id}': {e}"),
            field: Some("include/exclude".to_string()),
        })?;
    }
    Ok(())
}

// ── T040.003–T040.004: DuplicateIntakeReport ─────────────────────────────────

/// A single cross-database duplicate intake conflict.
///
/// Records the shared intake key (URL for Git and URL sources, canonical path
/// for local sources) and the conflicting `(source_id, database)` pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateEntry {
    /// The intake key shared by the conflicting sources (URL or path string).
    pub intake_key: String,
    /// Conflicting sources as `(source_id, database_name)` pairs.
    ///
    /// `database_name` is `""` when the source omits the `database` field
    /// and routes to the default database.
    pub conflicts: Vec<(String, String)>,
}

/// Report of cross-database duplicate intake sources.
///
/// A "duplicate intake" occurs when two sources with different `database`
/// values share the same acquisition target (git URL, local path, or crawl
/// URL). Sources that share an intake target _within the same database_ are
/// not flagged — they are redundant but not ambiguous.
///
/// Use [`DuplicateIntakeReport::detect`] to build a report from a
/// [`SourceConfig`], then inspect [`is_empty`](Self::is_empty) to decide
/// whether the conflict should block or warn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DuplicateIntakeReport {
    /// All cross-database duplicate entries detected.
    pub entries: Vec<DuplicateEntry>,
}

impl DuplicateIntakeReport {
    /// Detect cross-database duplicate intakes in `config`.
    ///
    /// Groups sources by their intake key.  Within each group, checks whether
    /// more than one distinct `database` value is present.  If so, that group
    /// is added to the report.
    ///
    /// Same-database duplicates are not flagged.
    #[must_use]
    pub fn detect(config: &SourceConfig) -> Self {
        let mut by_key: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

        for source in &config.sources {
            let key = intake_key(source);
            let db = source.database().unwrap_or("").to_string();
            by_key
                .entry(key)
                .or_default()
                .push((source.id().to_string(), db));
        }

        let entries = by_key
            .into_iter()
            .filter_map(|(key, conflicts)| {
                let distinct_dbs: std::collections::BTreeSet<&str> =
                    conflicts.iter().map(|(_, db)| db.as_str()).collect();
                if distinct_dbs.len() > 1 {
                    Some(DuplicateEntry {
                        intake_key: key,
                        conflicts,
                    })
                } else {
                    None
                }
            })
            .collect();

        Self { entries }
    }

    /// Returns `true` when no duplicate intakes were detected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Detect cross-database duplicate intakes using resolved DB paths and,
    /// for local sources, actual file-level overlap checking.
    ///
    /// Unlike [`detect`], this method:
    ///
    /// 1. Resolves each source's effective database path relative to
    ///    `base_db_path`, eliminating false positives from `database: null`
    ///    vs `database: "graph.db"` (the default filename).
    ///
    /// 2. For local sources that share the same root path, compares actual
    ///    filtered file intakes when `workspace_root` is `Some`. Sources are
    ///    only flagged when their filtered file sets **overlap**. When
    ///    `workspace_root` is `None`, local sources with distinct resolved DBs
    ///    are flagged conservatively (same as [`detect`]).
    ///
    /// Git and URL sources cannot be enumerated at preflight time; any
    /// same-URL sources with distinct resolved DBs are always flagged.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] if any include or exclude glob
    /// pattern cannot be compiled.
    pub fn detect_with_context(
        config: &SourceConfig,
        base_db_path: &Path,
        workspace_root: Option<&Path>,
    ) -> Result<Self, GraphtorError> {
        let mut by_key: BTreeMap<String, Vec<(usize, String, PathBuf)>> = BTreeMap::new();
        for (idx, source) in config.sources.iter().enumerate() {
            let key = intake_key(source);
            let db = resolve_source_db_path(base_db_path, source);
            by_key
                .entry(key)
                .or_default()
                .push((idx, source.id().to_string(), db));
        }

        let mut entries = Vec::new();
        for (key, group) in by_key {
            // Clone DB paths to avoid borrow-lifetime conflicts later.
            let distinct_dbs: BTreeSet<PathBuf> =
                group.iter().map(|(_, _, db)| db.clone()).collect();
            if distinct_dbs.len() <= 1 {
                continue;
            }

            let all_local = group
                .iter()
                .all(|(idx, _, _)| matches!(&config.sources[*idx], Source::Local(_)));

            let is_conflict = if all_local && workspace_root.is_some() {
                // If any local root is missing or unreadable, WalkDir would
                // silently yield zero entries, producing an empty file set and
                // a false negative.  Fall back conservatively and flag the pair
                // as a conflict whenever enumeration cannot be trusted.
                let any_unreadable = group.iter().any(|(idx, _, _)| {
                    matches!(&config.sources[*idx], Source::Local(l) if !l.path.exists())
                });
                if any_unreadable {
                    true
                } else {
                    let mut db_to_files: BTreeMap<PathBuf, HashSet<PathBuf>> = BTreeMap::new();
                    for (idx, _, db) in &group {
                        if let Source::Local(local_src) = &config.sources[*idx] {
                            let files = enumerate_and_filter_local(local_src)?;
                            db_to_files.entry(db.clone()).or_default().extend(files);
                        }
                    }
                    let sets: Vec<&HashSet<PathBuf>> = db_to_files.values().collect();
                    sets.iter().enumerate().any(|(i, set_i)| {
                        sets.iter()
                            .skip(i + 1)
                            .any(|set_j| set_i.iter().any(|f| set_j.contains(f)))
                    })
                }
            } else {
                true
            };

            if is_conflict {
                let conflicts: Vec<(String, String)> = group
                    .into_iter()
                    .map(|(_, id, db)| {
                        let db_name = db
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        (id, db_name)
                    })
                    .collect();
                entries.push(DuplicateEntry {
                    intake_key: key,
                    conflicts,
                });
            }
        }

        Ok(Self { entries })
    }
}

impl fmt::Display for DuplicateIntakeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} cross-database duplicate intake(s) detected:",
            self.entries.len()
        )?;
        for entry in &self.entries {
            writeln!(f, "  intake: {}", entry.intake_key)?;
            for (id, db) in &entry.conflicts {
                let db_display = if db.is_empty() {
                    "<default>"
                } else {
                    db.as_str()
                };
                writeln!(f, "    - source '{id}' -> database '{db_display}'")?;
            }
        }
        Ok(())
    }
}

/// Compute the intake key for a source — the acquisition target that
/// identifies what content will be indexed.
///
/// For local sources, the path is normalised lexically so that semantically
/// identical paths written in different forms (e.g. `./docs` vs `docs`, or
/// `/abs/path/../docs` vs `/abs/docs`) produce the same key.
fn intake_key(source: &Source) -> String {
    match source {
        Source::Git(g) => g.url.clone(),
        Source::Local(l) => normalize_path_key(&l.path),
        Source::Url(u) => u.url.clone(),
    }
}

/// Normalise a local filesystem path into a canonical string key.
///
/// Resolves `.` (current-directory) and `..` (parent-directory) components
/// **lexically** — without touching the filesystem — so that
/// `./docs`, `docs`, and `some/../docs` all produce the same key.
///
/// This is intentionally a lexical operation: the path need not exist on disk
/// at validation time, and callers should not expect symlinks or mount-points
/// to be resolved.
fn normalize_path_key(path: &Path) -> String {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // "." — drop silently
            Component::ParentDir => {
                // ".." — pop the last normal segment if present; otherwise
                // retain the ".." so that `../../foo` stays meaningful.
                if matches!(parts.last(), Some(Component::Normal(_))) {
                    parts.pop();
                } else {
                    parts.push(component);
                }
            }
            other => parts.push(other),
        }
    }
    let normalised: PathBuf = parts.into_iter().collect();
    normalised.display().to_string()
}

/// Resolve the effective database path for a source given the base DB path.
///
/// When `database` is `None`, the source uses `base_db_path` as-is.  When
/// `database` is set, the source uses `base_db_path.parent() / database_name`,
/// mirroring the `source_db_path` logic in the `graphtor-docs` binary.
#[must_use]
pub fn resolve_source_db_path(base_db_path: &Path, source: &Source) -> PathBuf {
    source.database().map_or_else(
        || base_db_path.to_path_buf(),
        |database| {
            base_db_path
                .parent()
                .map_or_else(|| PathBuf::from(database), |parent| parent.join(database))
        },
    )
}

/// Build an optional [`GlobSet`] from a slice of pattern strings.
///
/// Returns `None` when `patterns` is empty (interpreted as "match all" for
/// include, or "match none" for exclude, by the caller).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] for any syntactically invalid pattern.
fn build_preflight_glob_set(
    patterns: &[String],
    source_id: &str,
) -> Result<Option<GlobSet>, GraphtorError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| GraphtorError::Config {
            message: format!("invalid glob pattern '{pattern}' in source '{source_id}': {e}"),
            field: Some("include/exclude".to_string()),
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|e| GraphtorError::Config {
        message: format!("failed to compile glob set for source '{source_id}': {e}"),
        field: Some("include/exclude".to_string()),
    })?;
    Ok(Some(set))
}

/// Convert a path to a forward-slash string for cross-platform glob matching.
fn path_to_preflight_fwd_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Enumerate files in a local source directory, strip the source root prefix,
/// and apply include/exclude filters.
///
/// Returns relative [`PathBuf`]s (relative to `source.path`) for files that
/// pass the filters.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if any glob pattern cannot be compiled.
fn enumerate_and_filter_local(source: &LocalSource) -> Result<HashSet<PathBuf>, GraphtorError> {
    let root = &source.path;
    let include_set = build_preflight_glob_set(&source.include, &source.id)?;
    let exclude_set = build_preflight_glob_set(&source.exclude, &source.id)?;

    let files: HashSet<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.path().strip_prefix(root).ok().map(Path::to_path_buf))
        .filter(|rel| {
            let s = path_to_preflight_fwd_slash(rel);
            let included = match &include_set {
                None => true,
                Some(set) => set.is_match(&s),
            };
            if !included {
                return false;
            }
            match &exclude_set {
                None => true,
                Some(set) => !set.is_match(&s),
            }
        })
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::source::{GitSource, LocalSource, Source, SourceConfig};

    fn git(id: &str) -> Source {
        Source::Git(GitSource {
            id: id.to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec![],
            database: None,
        })
    }

    fn local(id: &str) -> Source {
        Source::Local(LocalSource {
            id: id.to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec![],
            database: None,
        })
    }

    // ── T014: validation rules ────────────────────────────────────────────

    #[test]
    fn valid_config_passes_validation() {
        let config = SourceConfig {
            sources: vec![git("source-a"), local("source-b")],
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn duplicate_ids_fail_validation() {
        let config = SourceConfig {
            sources: vec![git("same-id"), local("same-id")],
        };
        let result = validate(&config);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("same-id"),
            "error should mention the duplicate id: {msg}"
        );
    }

    #[test]
    fn invalid_glob_pattern_fails_validation() {
        let bad_glob = Source::Git(GitSource {
            id: "bad-glob".to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec!["[invalid-glob".to_string()],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        let config = SourceConfig {
            sources: vec![bad_glob],
        };
        let result = validate(&config);
        assert!(result.is_err(), "invalid glob should fail validation");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[config]"),
            "should produce a Config error: {msg}"
        );
    }

    #[test]
    fn empty_id_fails_validation() {
        let config = SourceConfig {
            sources: vec![git("")],
        };
        let result = validate(&config);
        assert!(result.is_err(), "empty id should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[config]"),
            "should produce Config error: {msg}"
        );
    }

    #[test]
    fn id_with_path_separator_fails_validation() {
        let config = SourceConfig {
            sources: vec![git("nested/id")],
        };
        let result = validate(&config);
        assert!(result.is_err(), "id with path separator should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("path separators"),
            "error should mention path separators: {msg}"
        );
    }

    #[test]
    fn id_with_dotdot_fails_validation() {
        let config = SourceConfig {
            sources: vec![git("../escape")],
        };
        let result = validate(&config);
        assert!(result.is_err(), "id with '..' should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("path separators") || msg.contains(".."),
            "error should mention the issue: {msg}"
        );
    }

    // ── T021.003: format validation ───────────────────────────────────────

    #[test]
    fn valid_formats_pass_validation() {
        let src = Source::Local(LocalSource {
            id: "valid-fmt".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string(), "pdf".to_string(), "docx".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        assert!(
            validate(&config).is_ok(),
            "valid formats must pass validation"
        );
    }

    #[test]
    fn empty_formats_list_passes_validation() {
        let src = Source::Local(LocalSource {
            id: "empty-fmt".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        assert!(
            validate(&config).is_ok(),
            "empty formats list must pass validation"
        );
    }

    #[test]
    fn unknown_format_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "bad-fmt-source".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["txt".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "unknown format must fail validation");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[config]"),
            "must produce a Config error: {msg}"
        );
        assert!(
            msg.contains("txt"),
            "error must mention the invalid format: {msg}"
        );
        assert!(
            msg.contains("formats"),
            "error must reference the formats field: {msg}"
        );
    }

    #[test]
    fn mixed_valid_invalid_formats_fails_on_first_invalid() {
        let src = Source::Git(GitSource {
            id: "mixed-fmt".to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string(), "zip".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "invalid format in list must fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("zip"),
            "error must identify the bad format: {msg}"
        );
    }

    // ── T038.001 database validation ─────────────────────────────────────

    #[test]
    fn database_valid_name_passes_validation() {
        let src = Source::Local(LocalSource {
            id: "v".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some("rust-docs.db".to_string()),
        });
        let config = SourceConfig { sources: vec![src] };
        assert!(validate(&config).is_ok(), "valid database name must pass");
    }

    #[test]
    fn database_empty_string_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "e".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some(String::new()),
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "empty database name must fail");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("[config]"), "must produce Config error: {msg}");
        assert!(
            msg.contains("database"),
            "must mention database field: {msg}"
        );
    }

    #[test]
    fn database_path_traversal_fails_validation() {
        let src = Source::Git(GitSource {
            id: "p".to_string(),
            url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some("../escape.db".to_string()),
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "path traversal in database name must fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("path separators") || msg.contains(".."),
            "must describe the error: {msg}"
        );
    }

    #[test]
    fn database_path_separator_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "ps".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some("subdir/evil.db".to_string()),
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "path separator in database name must fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("path separators"),
            "must mention separators: {msg}"
        );
    }

    // ── T040.003: DuplicateIntakeReport ──────────────────────────────────────

    fn git_with_db(id: &str, url: &str, db: &str) -> Source {
        Source::Git(GitSource {
            id: id.to_string(),
            url: url.to_string(),
            branch: "main".to_string(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some(db.to_string()),
        })
    }

    fn local_with_db(id: &str, path: &str, db: &str) -> Source {
        Source::Local(LocalSource {
            id: id.to_string(),
            path: std::path::PathBuf::from(path),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some(db.to_string()),
        })
    }

    #[test]
    fn duplicate_report_empty_when_no_conflicts() {
        let config = SourceConfig {
            sources: vec![
                git_with_db("a", "https://github.com/example/repo-a.git", "alpha.db"),
                git_with_db("b", "https://github.com/example/repo-b.git", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(report.is_empty(), "no duplicates expected");
    }

    #[test]
    fn duplicate_report_detects_cross_db_git_url_conflict() {
        let config = SourceConfig {
            sources: vec![
                git_with_db("a", "https://github.com/example/repo.git", "alpha.db"),
                git_with_db("b", "https://github.com/example/repo.git", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(!report.is_empty(), "cross-db git url should be reported");
        assert_eq!(report.entries.len(), 1);
        assert_eq!(
            report.entries[0].intake_key,
            "https://github.com/example/repo.git"
        );
        assert_eq!(report.entries[0].conflicts.len(), 2);
    }

    #[test]
    fn duplicate_report_detects_cross_db_local_path_conflict() {
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "/shared/docs", "alpha.db"),
                local_with_db("b", "/shared/docs", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(!report.is_empty(), "cross-db local path should be reported");
        assert_eq!(report.entries.len(), 1);
    }

    #[test]
    fn duplicate_report_allows_same_db_duplicates() {
        let config = SourceConfig {
            sources: vec![
                git_with_db("a", "https://github.com/example/repo.git", "shared.db"),
                git_with_db("b", "https://github.com/example/repo.git", "shared.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(
            report.is_empty(),
            "same-db duplicates must not be flagged: {report}"
        );
    }

    // ── T040.004: DuplicateIntakeReport Display ───────────────────────────────

    #[test]
    fn duplicate_report_display_is_human_readable() {
        let config = SourceConfig {
            sources: vec![
                git_with_db("a", "https://github.com/example/repo.git", "alpha.db"),
                git_with_db("b", "https://github.com/example/repo.git", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        let display = report.to_string();
        assert!(
            display.contains("cross-database"),
            "display must describe the conflict type: {display}"
        );
        assert!(
            display.contains("https://github.com/example/repo.git"),
            "display must include the intake key: {display}"
        );
        assert!(
            display.contains("alpha.db") && display.contains("beta.db"),
            "display must include database names: {display}"
        );
    }

    #[test]
    fn duplicate_report_display_shows_default_for_missing_database() {
        let config = SourceConfig {
            sources: vec![
                git("url-src-a"),
                Source::Git(GitSource {
                    id: "url-src-b".to_string(),
                    url: "https://github.com/example/repo.git".to_string(),
                    branch: "main".to_string(),
                    include: vec![],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("other.db".to_string()),
                }),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        let display = report.to_string();
        assert!(
            display.contains("<default>"),
            "display must use '<default>' for missing database field: {display}"
        );
    }

    // ── T040.001: local-path normalization in duplicate detection ────────────

    #[test]
    fn duplicate_report_normalizes_dotslash_local_paths() {
        // "./docs" and "docs" are the same directory — must be detected as a
        // cross-database duplicate.
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "./docs", "alpha.db"),
                local_with_db("b", "docs", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(
            !report.is_empty(),
            "'./docs' and 'docs' must be detected as the same intake key: {report}"
        );
    }

    #[test]
    fn duplicate_report_normalizes_parent_dir_in_absolute_paths() {
        // "/abs/path/../docs" and "/abs/docs" resolve to the same directory.
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "/abs/path/../docs", "alpha.db"),
                local_with_db("b", "/abs/docs", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(
            !report.is_empty(),
            "lexically equivalent absolute paths must be detected as the same intake key: {report}"
        );
    }

    #[test]
    fn duplicate_report_same_path_different_writing_same_db_is_not_flagged() {
        // Same logical path, same database — redundant but not a cross-db conflict.
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "./docs", "shared.db"),
                local_with_db("b", "docs", "shared.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(
            report.is_empty(),
            "same-db duplicates (even via different path forms) must not be flagged: {report}"
        );
    }

    #[test]
    fn normalize_path_key_strips_leading_dot_slash() {
        assert_eq!(
            normalize_path_key(Path::new("./docs")),
            normalize_path_key(Path::new("docs"))
        );
    }

    #[test]
    fn normalize_path_key_resolves_parent_dir_component() {
        assert_eq!(
            normalize_path_key(Path::new("/abs/path/../docs")),
            normalize_path_key(Path::new("/abs/docs"))
        );
    }

    #[test]
    fn normalize_path_key_preserves_unresolvable_parent_dirs() {
        // Leading ".." cannot be resolved without cwd — distinct paths must
        // remain distinct so that "../../foo" and "../../bar" don't collide.
        let a = normalize_path_key(Path::new("../../foo"));
        let b = normalize_path_key(Path::new("../../bar"));
        assert_ne!(a, b, "distinct unresolvable paths must stay distinct");
    }

    // ── T040.007: detect_with_context ─────────────────────────────────────

    /// Same local root, different DBs, DISJOINT include globs — must NOT flag.
    ///
    /// Source A covers `docs/**/*.md`, source B covers `api/**/*.md`. The
    /// files they index do not overlap, so no conflict should be reported.
    #[test]
    fn detect_with_context_disjoint_local_globs_different_dbs_not_conflict() {
        use tempfile::TempDir;
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("api")).unwrap();
        std::fs::write(root.join("docs/README.md"), b"# docs").unwrap();
        std::fs::write(root.join("api/reference.md"), b"# api").unwrap();

        let config = SourceConfig {
            sources: vec![
                Source::Local(LocalSource {
                    id: "docs-src".to_string(),
                    path: root.to_path_buf(),
                    include: vec!["docs/**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("alpha.db".to_string()),
                }),
                Source::Local(LocalSource {
                    id: "api-src".to_string(),
                    path: root.to_path_buf(),
                    include: vec!["api/**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("beta.db".to_string()),
                }),
            ],
        };

        let base_db = PathBuf::from("/workspace/.graphtor/graph.db");
        let report = DuplicateIntakeReport::detect_with_context(&config, &base_db, Some(root))
            .expect("detect_with_context should not error");

        assert!(
            report.is_empty(),
            "disjoint local include globs must not be flagged as a conflict: {report}"
        );
    }

    /// Same local root, different DBs, OVERLAPPING include globs — must flag.
    ///
    /// Source A includes `**/*.md` (matches everything), source B includes
    /// `api/**/*.md`. Both match `api/reference.md`, so a conflict must be
    /// reported.
    #[test]
    fn detect_with_context_overlapping_local_globs_different_dbs_is_conflict() {
        use tempfile::TempDir;
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("api")).unwrap();
        std::fs::write(root.join("api/reference.md"), b"# api ref").unwrap();

        let config = SourceConfig {
            sources: vec![
                Source::Local(LocalSource {
                    id: "all-src".to_string(),
                    path: root.to_path_buf(),
                    include: vec!["**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("alpha.db".to_string()),
                }),
                Source::Local(LocalSource {
                    id: "api-src".to_string(),
                    path: root.to_path_buf(),
                    include: vec!["api/**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("beta.db".to_string()),
                }),
            ],
        };

        let base_db = PathBuf::from("/workspace/.graphtor/graph.db");
        let report = DuplicateIntakeReport::detect_with_context(&config, &base_db, Some(root))
            .expect("detect_with_context should not error");

        assert!(
            !report.is_empty(),
            "overlapping local include globs across different DBs must be flagged: {report}"
        );
    }

    // ── T040.008: conservative fallback for non-existent local roots ─────────

    /// When any local source root is non-existent or unreadable, `detect_with_context`
    /// must fall back conservatively and flag the pair as a conflict.
    ///
    /// `WalkDir` silently yields zero entries for a non-existent directory, which
    /// would produce an empty file set and a false negative (no overlap detected).
    /// The conservative path ensures correctness when the intake cannot be enumerated.
    #[test]
    fn detect_with_context_nonexistent_local_root_flags_conservatively() {
        let nonexistent = PathBuf::from("/this/path/does/not/exist/graphtor_test_d2e9f1a3");
        let config = SourceConfig {
            sources: vec![
                Source::Local(LocalSource {
                    id: "src-a".to_string(),
                    path: nonexistent.clone(),
                    include: vec!["**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("alpha.db".to_string()),
                }),
                Source::Local(LocalSource {
                    id: "src-b".to_string(),
                    path: nonexistent.clone(),
                    include: vec!["**/*.md".to_string()],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("beta.db".to_string()),
                }),
            ],
        };

        let base_db = PathBuf::from("/workspace/.graphtor/graph.db");
        let workspace_root = PathBuf::from("/workspace");
        let report =
            DuplicateIntakeReport::detect_with_context(&config, &base_db, Some(&workspace_root))
                .expect("detect_with_context must not error on non-existent roots");

        assert!(
            !report.is_empty(),
            "non-existent local root must be flagged conservatively: {report}"
        );
    }

    /// `database: null` and `database: "graph.db"` (explicit default basename)
    /// both resolve to the same physical path — must NOT be flagged.
    #[test]
    fn detect_with_context_null_db_and_explicit_default_basename_are_same_db() {
        let config = SourceConfig {
            sources: vec![
                Source::Git(GitSource {
                    id: "src-null-db".to_string(),
                    url: "https://github.com/example/repo.git".to_string(),
                    branch: "main".to_string(),
                    include: vec![],
                    exclude: vec![],
                    formats: vec![],
                    database: None,
                }),
                Source::Git(GitSource {
                    id: "src-explicit-default".to_string(),
                    url: "https://github.com/example/repo.git".to_string(),
                    branch: "main".to_string(),
                    include: vec![],
                    exclude: vec![],
                    formats: vec![],
                    database: Some("graph.db".to_string()),
                }),
            ],
        };

        let base_db = PathBuf::from("/workspace/.graphtor/graph.db");
        let report = DuplicateIntakeReport::detect_with_context(&config, &base_db, None)
            .expect("detect_with_context should not error");

        assert!(
            report.is_empty(),
            "`database: null` and `database: \"graph.db\"` must not be flagged when they resolve to the same path: {report}"
        );
    }
}
