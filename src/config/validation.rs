//! Configuration validation logic for `sources.yaml`.
//!
//! Validates semantic constraints that YAML deserialization alone cannot
//! enforce: duplicate source IDs, empty required fields, and glob pattern
//! syntax validity.

use std::collections::HashSet;

use globset::Glob;

use crate::config::source::{Source, SourceConfig};
use crate::error::GraphtorError;

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

        if !seen_ids.insert(id) {
            return Err(GraphtorError::Config {
                message: format!("duplicate source id: '{id}'"),
                field: Some("id".to_string()),
            });
        }

        let (include, exclude) = match source {
            Source::Git(g) => (&g.include, &g.exclude),
            Source::Local(l) => (&l.include, &l.exclude),
        };

        validate_globs(include, id)?;
        validate_globs(exclude, id)?;
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
        })
    }

    fn local(id: &str) -> Source {
        Source::Local(LocalSource {
            id: id.to_string(),
            path: std::path::PathBuf::from("/docs"),
            include: vec!["**/*.md".to_string()],
            exclude: vec![],
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
}
