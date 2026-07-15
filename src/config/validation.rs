//! Configuration validation logic for `sources.yaml`.
//!
//! Validates semantic constraints that YAML deserialization alone cannot
//! enforce: duplicate source IDs, empty required fields, and glob pattern
//! syntax validity. Only Markdown-format local sources are accepted.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use crate::config::source::{LocalSource, Source, SourceConfig};
use crate::error::GraphtorError;

/// Extension strings accepted by the ingestion pipeline.
///
/// Only Markdown variants are supported. `"markdown"` is an alias for `"md"`.
const VALID_FORMATS: &[&str] = &["md", "markdown"];

/// Validate a parsed [`SourceConfig`] for semantic correctness.
///
/// Checks performed:
/// - No empty source IDs.
/// - No duplicate source IDs.
/// - All glob patterns in `include` and `exclude` fields compile successfully.
/// - All `formats` values are recognized Markdown extensions.
/// - No `database` names with path separators or `..` components.
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

        if let Some(db_name) = source.database() {
            validate_database_name(db_name, id)?;
        }

        // Glob/format validation is local-ingestion-specific — a future
        // non-ingestible source variant (e.g. an explicit read-only db
        // entry) has no `include`/`exclude`/`formats` semantics to check.
        if let Some(l) = source.as_local() {
            validate_globs(&l.include, id)?;
            validate_globs(&l.exclude, id)?;
            validate_formats(source.formats(), id)?;
        }
    }

    Ok(())
}

/// Validate a database name value.
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

/// Validate that all strings in `formats` are recognized Markdown extensions.
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

// ── DuplicateIntakeReport ─────────────────────────────────────────────────────

/// A single cross-database duplicate intake conflict.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateEntry {
    /// The intake key (local path) shared by the conflicting sources.
    pub intake_key: String,
    /// Conflicting sources as `(source_id, database_name)` pairs.
    pub conflicts: Vec<(String, String)>,
}

/// Report of cross-database duplicate intake sources.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DuplicateIntakeReport {
    /// All cross-database duplicate entries detected.
    pub entries: Vec<DuplicateEntry>,
}

impl DuplicateIntakeReport {
    /// Detect cross-database duplicate intakes in `config`.
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
                let distinct_dbs: BTreeSet<&str> =
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

    /// Detect cross-database duplicate intakes with file-level overlap checking.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] if any glob pattern cannot be compiled.
    pub fn detect_with_context(
        config: &SourceConfig,
        base_db_path: &Path,
        workspace_root: Option<&Path>,
    ) -> Result<Self, GraphtorError> {
        let mut entries = Vec::new();
        let mut local_sources: Vec<(&LocalSource, String, PathBuf)> = Vec::new();

        for source in &config.sources {
            let db = resolve_source_db_path(base_db_path, source);
            // A non-local (e.g. served, read-only) source is not an
            // ingestion source and never participates in duplicate-intake
            // detection.
            let Some(local_src) = source.as_local() else {
                continue;
            };
            local_sources.push((local_src, source.id().to_string(), db));
        }

        let prepared_locals = if let Some(root) = workspace_root {
            local_sources
                .iter()
                .map(|(local_src, id, db)| {
                    prepare_local_source(local_src, id.clone(), db.clone(), root)
                })
                .collect::<Result<Vec<_>, GraphtorError>>()?
        } else {
            local_sources
                .iter()
                .map(|(local_src, id, db)| PreparedLocalSource {
                    id: id.clone(),
                    db_path: db.clone(),
                    root_path: lexically_normalize_path(&local_src.path),
                    root_key: normalize_path_key(&local_src.path),
                    root_exists: false,
                })
                .collect()
        };
        let mut local_file_states: Vec<LocalFileState> =
            std::iter::repeat_with(LocalFileState::default)
                .take(prepared_locals.len())
                .collect();

        for (idx, left) in prepared_locals.iter().enumerate() {
            for (right_idx, right) in prepared_locals.iter().enumerate().skip(idx + 1) {
                if left.db_path == right.db_path
                    || !local_roots_may_overlap(&left.root_path, &right.root_path)
                {
                    continue;
                }

                let is_conflict = if workspace_root.is_some() {
                    ensure_local_files(idx, left, &local_sources, &mut local_file_states)?;
                    ensure_local_files(right_idx, right, &local_sources, &mut local_file_states)?;

                    match (
                        &local_file_states[idx].files,
                        &local_file_states[right_idx].files,
                    ) {
                        (Some(left_files), Some(right_files)) => {
                            left_files.iter().any(|file| right_files.contains(file))
                        }
                        _ => true,
                    }
                } else {
                    true
                };

                if is_conflict {
                    entries.push(DuplicateEntry {
                        intake_key: local_overlap_key(left, right),
                        conflicts: format_conflicts(vec![
                            (left.id.clone(), left.db_path.clone()),
                            (right.id.clone(), right.db_path.clone()),
                        ]),
                    });
                }
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

fn intake_key(source: &Source) -> String {
    // A non-local source has no ingestion path to key on. Fall back to a
    // per-source key namespaced with a NUL byte (never valid in a real
    // filesystem path) so distinct non-local sources can never collide
    // with each other or with a real local path's key.
    source.as_local().map_or_else(
        || format!("\0non-local:{}", source.id()),
        |l| normalize_path_key(&l.path),
    )
}

fn normalize_path_key(path: &Path) -> String {
    lexically_normalize_path(path).display().to_string()
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    let absolute_path = path.is_absolute();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                Some(Component::RootDir) => {}
                Some(Component::Prefix(_)) if absolute_path => {}
                _ => parts.push(component),
            },
            other => parts.push(other),
        }
    }
    parts.into_iter().collect()
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    normalize_path_components(path)
}

/// Resolve the effective database path for a source given the base DB path.
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

#[derive(Debug)]
struct PreparedLocalSource {
    id: String,
    db_path: PathBuf,
    root_path: PathBuf,
    root_key: String,
    root_exists: bool,
}

#[derive(Debug, Default)]
struct LocalFileState {
    evaluated: bool,
    files: Option<HashSet<PathBuf>>,
}

#[derive(Debug)]
struct EnumeratedLocalFiles {
    files: HashSet<PathBuf>,
    complete: bool,
}

fn resolve_local_source_root(local_path: &Path, workspace_root: &Path) -> PathBuf {
    if local_path.is_absolute() {
        local_path.to_path_buf()
    } else {
        workspace_root.join(local_path)
    }
}

fn prepare_local_source(
    local_src: &LocalSource,
    id: String,
    db_path: PathBuf,
    workspace_root: &Path,
) -> Result<PreparedLocalSource, GraphtorError> {
    let resolved_root = resolve_local_source_root(&local_src.path, workspace_root);
    let normalized_root = lexically_normalize_path(&resolved_root);
    let normalized_workspace_root = lexically_normalize_path(workspace_root);
    if !normalized_root.starts_with(&normalized_workspace_root) {
        return Err(GraphtorError::PathViolation {
            attempted: resolved_root,
            allowed_root: workspace_root.to_path_buf(),
        });
    }

    Ok(PreparedLocalSource {
        id,
        db_path,
        root_path: normalized_root.clone(),
        root_key: normalize_path_key(&normalized_root),
        root_exists: normalized_root.exists(),
    })
}

fn local_roots_may_overlap(left: &Path, right: &Path) -> bool {
    let left = lexically_normalize_path(left);
    let right = lexically_normalize_path(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn local_overlap_key(left: &PreparedLocalSource, right: &PreparedLocalSource) -> String {
    if left.root_path == right.root_path {
        left.root_key.clone()
    } else {
        format!(
            "overlapping local files between '{}' and '{}'",
            left.root_key, right.root_key
        )
    }
}

fn format_conflicts(conflicts: Vec<(String, PathBuf)>) -> Vec<(String, String)> {
    conflicts
        .into_iter()
        .map(|(id, db)| {
            let db_name = db
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            (id, db_name)
        })
        .collect()
}

fn ensure_local_files(
    index: usize,
    prepared: &PreparedLocalSource,
    local_sources: &[(&LocalSource, String, PathBuf)],
    local_file_states: &mut [LocalFileState],
) -> Result<(), GraphtorError> {
    let state = &mut local_file_states[index];
    if state.evaluated {
        return Ok(());
    }

    state.files = if prepared.root_exists {
        let (local_src, _, _) = &local_sources[index];
        let enumerated = enumerate_and_filter_local(&prepared.root_path, local_src)?;
        if enumerated.complete {
            Some(enumerated.files)
        } else {
            None
        }
    } else {
        None
    };
    state.evaluated = true;
    Ok(())
}

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

fn path_to_preflight_fwd_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn enumerate_and_filter_local(
    root: &Path,
    source: &LocalSource,
) -> Result<EnumeratedLocalFiles, GraphtorError> {
    let include_set = build_preflight_glob_set(&source.include, &source.id)?;
    let exclude_set = build_preflight_glob_set(&source.exclude, &source.id)?;
    let mut complete = true;
    let mut files = HashSet::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let Ok(rel) = entry.path().strip_prefix(root) else {
            complete = false;
            continue;
        };

        let rel_str = path_to_preflight_fwd_slash(rel);
        let included = match &include_set {
            None => true,
            Some(set) => set.is_match(&rel_str),
        };
        if !included {
            continue;
        }
        if matches!(&exclude_set, Some(set) if set.is_match(&rel_str)) {
            continue;
        }

        files.insert(entry.path().to_path_buf());
    }

    Ok(EnumeratedLocalFiles { files, complete })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::source::{LocalSource, Source, SourceConfig};

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

    // ── Validation rules ──────────────────────────────────────────────────

    #[test]
    fn valid_config_passes_validation() {
        let config = SourceConfig {
            sources: vec![local("source-a"), local("source-b")],
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn duplicate_ids_fail_validation() {
        let config = SourceConfig {
            sources: vec![local("same-id"), local("same-id")],
        };
        let result = validate(&config);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("same-id"),
            "error should mention the id: {msg}"
        );
    }

    #[test]
    fn invalid_glob_pattern_fails_validation() {
        let bad_glob = Source::Local(LocalSource {
            id: "bad-glob".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec!["[invalid-glob".to_string()],
            exclude: vec![],
            formats: vec![],
            database: None,
        });
        let config = SourceConfig {
            sources: vec![bad_glob],
        };
        let result = validate(&config);
        assert!(result.is_err(), "invalid glob should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("[config]"),
            "should produce Config error: {msg}"
        );
    }

    #[test]
    fn empty_id_fails_validation() {
        let config = SourceConfig {
            sources: vec![local("")],
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
            sources: vec![local("nested/id")],
        };
        let result = validate(&config);
        assert!(result.is_err(), "id with path separator should fail");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("path separators"), "{msg}");
    }

    #[test]
    fn id_with_dotdot_fails_validation() {
        let config = SourceConfig {
            sources: vec![local("../escape")],
        };
        let result = validate(&config);
        assert!(result.is_err(), "id with '..' should fail");
    }

    // ── Format validation ─────────────────────────────────────────────────

    #[test]
    fn md_and_markdown_formats_pass_validation() {
        let src = Source::Local(LocalSource {
            id: "valid-fmt".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string(), "markdown".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        assert!(validate(&config).is_ok(), "md and markdown must pass");
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
        assert!(validate(&config).is_ok(), "empty formats list must pass");
    }

    #[test]
    fn pdf_format_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "pdf-src".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["pdf".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "pdf format must fail");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("[config]"), "{msg}");
        assert!(msg.contains("pdf"), "{msg}");
    }

    #[test]
    fn docx_format_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "docx-src".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec!["docx".to_string()],
            database: None,
        });
        let config = SourceConfig { sources: vec![src] };
        assert!(validate(&config).is_err(), "docx format must fail");
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
        assert!(result.is_err(), "unknown format must fail");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("[config]"), "{msg}");
        assert!(msg.contains("txt"), "{msg}");
        assert!(msg.contains("formats"), "{msg}");
    }

    // ── Database name validation ───────────────────────────────────────────

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
        assert!(msg.contains("[config]"), "{msg}");
        assert!(msg.contains("database"), "{msg}");
    }

    #[test]
    fn database_path_traversal_fails_validation() {
        let src = Source::Local(LocalSource {
            id: "p".to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: Some("../escape.db".to_string()),
        });
        let config = SourceConfig { sources: vec![src] };
        let result = validate(&config);
        assert!(result.is_err(), "path traversal in database name must fail");
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
        assert!(msg.contains("path separators"), "{msg}");
    }

    // ── DuplicateIntakeReport ─────────────────────────────────────────────

    #[test]
    fn duplicate_report_empty_when_no_conflicts() {
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "/docs-alpha", "alpha.db"),
                local_with_db("b", "/docs-beta", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(report.is_empty(), "no duplicates expected");
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
                local_with_db("a", "/docs", "shared.db"),
                local_with_db("b", "/docs", "shared.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(report.is_empty(), "same-db duplicates must not be flagged");
    }

    #[test]
    fn duplicate_report_normalizes_dotslash_local_paths() {
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "./docs", "alpha.db"),
                local_with_db("b", "docs", "beta.db"),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        assert!(
            !report.is_empty(),
            "'./docs' and 'docs' must be detected as same"
        );
    }

    #[test]
    fn duplicate_report_display_shows_default_for_missing_database() {
        let config = SourceConfig {
            sources: vec![
                local_with_db("a", "/docs", "other.db"),
                Source::Local(LocalSource {
                    id: "b".to_string(),
                    path: std::path::PathBuf::from("/docs"),
                    include: vec![],
                    exclude: vec![],
                    formats: vec![],
                    database: None,
                }),
            ],
        };
        let report = DuplicateIntakeReport::detect(&config);
        let display = report.to_string();
        assert!(
            display.contains("<default>"),
            "must use '<default>': {display}"
        );
    }
}
