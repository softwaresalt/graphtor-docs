//! Configuration validation logic for `sources.yaml`.
//!
//! Validates semantic constraints that YAML deserialization alone cannot
//! enforce: duplicate source IDs, empty required fields, and glob pattern
//! syntax validity.

use std::collections::HashSet;
use std::path::{Component, Path};

use globset::Glob;

use crate::config::source::{Source, SourceConfig};
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

        validate_globs(include, id)?;
        validate_globs(exclude, id)?;
        validate_formats(source.formats(), id)?;
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
        })
    }

    fn local(id: &str) -> Source {
        Source::Local(LocalSource {
            id: id.to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
            formats: vec![],
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
}
