//! `GraphtorError` — categorized error type hierarchy.
//!
//! All errors produced by this library are variants of [`GraphtorError`].
//! Each variant encodes the error category, a human-readable message, and
//! enough context for the caller to diagnose the failure.

use std::path::PathBuf;

/// Top-level error enum for all `graphtor-core` failures.
///
/// Every public function in this library returns `Result<_, GraphtorError>`.
/// Variants correspond to distinct failure categories so callers can match
/// on the specific kind of failure.
///
/// # Display format
///
/// Each variant produces a human-readable message in the form
/// `[{category}] {message}: {context}`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphtorError {
    /// Configuration parsing or validation error.
    #[error("[config] {message}{}", .field.as_deref().map(|f| format!(": field '{f}'")).unwrap_or_default())]
    Config {
        /// Human-readable description of the problem.
        message: String,
        /// The configuration field that caused the error, if applicable.
        field: Option<String>,
    },

    /// Database operation failure.
    #[error("[database] {message}: {operation}")]
    Database {
        /// Human-readable description of the failure.
        message: String,
        /// The database operation that failed (e.g., `"insert"`, `"query"`).
        operation: String,
    },

    /// Database access blocked by an active advisory lock.
    #[error(
        "[database_locked] database '{db_name}' is locked{}",
        .holder_pid.map_or_else(String::new, |pid| format!(" by process {pid}"))
    )]
    DatabaseLocked {
        /// Database file name associated with the lock.
        db_name: String,
        /// Process identifier recorded in the lock file, when available.
        holder_pid: Option<u32>,
    },

    /// Pipeline stage execution failure.
    #[error("[pipeline] {message}: {stage}")]
    Pipeline {
        /// Human-readable description of the failure.
        message: String,
        /// The pipeline stage that failed (e.g., `"normalize"`, `"chunk"`).
        stage: String,
    },

    /// Markdown parsing error.
    #[error("[parse] {message}{}", .path.as_ref().map(|p| format!(": {}", p.display())).unwrap_or_default())]
    Parse {
        /// Human-readable description of the parse failure.
        message: String,
        /// Path to the file that failed to parse, if applicable.
        path: Option<PathBuf>,
    },

    /// Embedding generation failure.
    #[error("[embed] {message}{}", .chunk_id.as_deref().map(|id| format!(": chunk {id}")).unwrap_or_default())]
    Embed {
        /// Human-readable description of the embedding failure.
        message: String,
        /// The chunk identifier that failed embedding, if applicable.
        chunk_id: Option<String>,
    },

    /// Path escapes the allowed workspace root.
    #[error(
        "[path_violation] attempted '{}': must be within '{}'",
        .attempted.display(),
        .allowed_root.display()
    )]
    PathViolation {
        /// The path that was attempted.
        attempted: PathBuf,
        /// The allowed root directory that the path violated.
        allowed_root: PathBuf,
    },

    /// Sync state or diff detection failure.
    #[error("[sync] {message}: {source_id}")]
    Sync {
        /// Human-readable description of the sync failure.
        message: String,
        /// The source identifier that experienced the sync failure.
        source_id: String,
    },

    /// Filesystem I/O error.
    #[error("[io] {0}")]
    Io(#[from] std::io::Error),
}

impl GraphtorError {
    /// Returns `true` when this error represents invalid client input.
    ///
    /// Client errors map to `invalid_params` in MCP error responses.
    /// All other variants map to `internal_error`.
    ///
    /// Currently [`GraphtorError::PathViolation`] is the only client error —
    /// it indicates the caller supplied a path that escapes the allowed root.
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        matches!(self, Self::PathViolation { .. })
    }
}

impl From<serde_yaml::Error> for GraphtorError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Config {
            message: e.to_string(),
            field: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── T006: variant construction and Display output ─────────────────────

    #[test]
    fn config_with_field_includes_category_message_and_field() {
        let e = GraphtorError::Config {
            message: "missing required value".to_string(),
            field: Some("id".to_string()),
        };
        let s = e.to_string();
        assert!(
            s.starts_with("[config]"),
            "expected '[config]' prefix, got: {s}"
        );
        assert!(s.contains("missing required value"), "missing message: {s}");
        assert!(s.contains("id"), "missing field name: {s}");
    }

    #[test]
    fn config_without_field_omits_field_context() {
        let e = GraphtorError::Config {
            message: "parse failed".to_string(),
            field: None,
        };
        let s = e.to_string();
        assert!(s.starts_with("[config]"), "expected '[config]' prefix: {s}");
        assert!(s.contains("parse failed"), "missing message: {s}");
    }

    #[test]
    fn database_error_includes_category_message_and_operation() {
        let e = GraphtorError::Database {
            message: "connection refused".to_string(),
            operation: "insert".to_string(),
        };
        let s = e.to_string();
        assert!(
            s.starts_with("[database]"),
            "expected '[database]' prefix: {s}"
        );
        assert!(s.contains("connection refused"), "missing message: {s}");
        assert!(s.contains("insert"), "missing operation: {s}");
    }

    #[test]
    fn database_locked_error_includes_database_name_and_holder_pid() {
        let e = GraphtorError::DatabaseLocked {
            db_name: "primary.db".to_string(),
            holder_pid: Some(42),
        };
        let s = e.to_string();
        assert!(
            s.starts_with("[database_locked]"),
            "expected '[database_locked]' prefix: {s}"
        );
        assert!(s.contains("primary.db"), "missing database name: {s}");
        assert!(s.contains("42"), "missing holder pid: {s}");
    }

    #[test]
    fn pipeline_error_includes_category_message_and_stage() {
        let e = GraphtorError::Pipeline {
            message: "processing failed".to_string(),
            stage: "normalize".to_string(),
        };
        let s = e.to_string();
        assert!(
            s.starts_with("[pipeline]"),
            "expected '[pipeline]' prefix: {s}"
        );
        assert!(s.contains("processing failed"), "missing message: {s}");
        assert!(s.contains("normalize"), "missing stage: {s}");
    }

    #[test]
    fn parse_error_with_path_includes_path() {
        let e = GraphtorError::Parse {
            message: "unexpected token".to_string(),
            path: Some(PathBuf::from("/docs/auth.md")),
        };
        let s = e.to_string();
        assert!(s.starts_with("[parse]"), "expected '[parse]' prefix: {s}");
        assert!(s.contains("unexpected token"), "missing message: {s}");
        assert!(s.contains("auth.md"), "missing file path: {s}");
    }

    #[test]
    fn parse_error_without_path_omits_path_context() {
        let e = GraphtorError::Parse {
            message: "invalid frontmatter".to_string(),
            path: None,
        };
        let s = e.to_string();
        assert!(s.starts_with("[parse]"), "expected '[parse]' prefix: {s}");
        assert!(s.contains("invalid frontmatter"), "missing message: {s}");
    }

    #[test]
    fn embed_error_with_chunk_id_includes_id() {
        let e = GraphtorError::Embed {
            message: "model timeout".to_string(),
            chunk_id: Some("abc123".to_string()),
        };
        let s = e.to_string();
        assert!(s.starts_with("[embed]"), "expected '[embed]' prefix: {s}");
        assert!(s.contains("model timeout"), "missing message: {s}");
        assert!(s.contains("abc123"), "missing chunk_id: {s}");
    }

    #[test]
    fn embed_error_without_chunk_id_omits_id() {
        let e = GraphtorError::Embed {
            message: "embedding failed".to_string(),
            chunk_id: None,
        };
        let s = e.to_string();
        assert!(s.starts_with("[embed]"), "expected '[embed]' prefix: {s}");
        assert!(s.contains("embedding failed"), "missing message: {s}");
    }

    #[test]
    fn path_violation_includes_both_paths() {
        let e = GraphtorError::PathViolation {
            attempted: PathBuf::from("/tmp/secret"),
            allowed_root: PathBuf::from("/workspace"),
        };
        let s = e.to_string();
        assert!(
            s.starts_with("[path_violation]"),
            "expected '[path_violation]' prefix: {s}"
        );
        assert!(s.contains("secret"), "missing attempted path: {s}");
        assert!(s.contains("workspace"), "missing allowed_root: {s}");
    }

    #[test]
    fn sync_error_includes_source_id() {
        let e = GraphtorError::Sync {
            message: "hash mismatch".to_string(),
            source_id: "ms-azure-core".to_string(),
        };
        let s = e.to_string();
        assert!(s.starts_with("[sync]"), "expected '[sync]' prefix: {s}");
        assert!(s.contains("hash mismatch"), "missing message: {s}");
        assert!(s.contains("ms-azure-core"), "missing source_id: {s}");
    }

    #[test]
    fn io_error_includes_io_category() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let e = GraphtorError::Io(io_err);
        let s = e.to_string();
        assert!(s.starts_with("[io]"), "expected '[io]' prefix: {s}");
        assert!(s.contains("file not found"), "missing io message: {s}");
    }

    // ── T007: From conversions ─────────────────────────────────────────────

    #[test]
    fn from_io_error_produces_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let e: GraphtorError = io_err.into();
        assert!(
            matches!(e, GraphtorError::Io(_)),
            "expected Io variant, got: {e:?}"
        );
    }

    #[test]
    fn from_serde_yaml_error_produces_config_variant() {
        let yaml_src = "key: [unclosed";
        let yaml_err = serde_yaml::from_str::<serde_yaml::Value>(yaml_src).unwrap_err();
        let e: GraphtorError = yaml_err.into();
        assert!(
            matches!(e, GraphtorError::Config { .. }),
            "expected Config variant from serde_yaml::Error, got: {e:?}"
        );
        let s = e.to_string();
        assert!(s.starts_with("[config]"), "expected '[config]' prefix: {s}");
    }

    // ── T013.001: is_client_error() classification ────────────────────────

    #[test]
    fn path_violation_is_client_error() {
        let e = GraphtorError::PathViolation {
            attempted: PathBuf::from("/tmp/evil"),
            allowed_root: PathBuf::from("/workspace"),
        };
        assert!(
            e.is_client_error(),
            "PathViolation should be a client error"
        );
    }

    #[test]
    fn non_client_errors_return_false() {
        let errors: Vec<GraphtorError> = vec![
            GraphtorError::Config {
                message: "m".to_string(),
                field: None,
            },
            GraphtorError::Database {
                message: "m".to_string(),
                operation: "op".to_string(),
            },
            GraphtorError::DatabaseLocked {
                db_name: "graph.db".to_string(),
                holder_pid: Some(7),
            },
            GraphtorError::Pipeline {
                message: "m".to_string(),
                stage: "s".to_string(),
            },
            GraphtorError::Parse {
                message: "m".to_string(),
                path: None,
            },
            GraphtorError::Embed {
                message: "m".to_string(),
                chunk_id: None,
            },
            GraphtorError::Sync {
                message: "m".to_string(),
                source_id: "s".to_string(),
            },
            GraphtorError::Io(std::io::Error::other("e")),
        ];
        for e in &errors {
            assert!(!e.is_client_error(), "expected non-client error for: {e:?}");
        }
    }

    #[test]
    fn all_nine_variants_produce_distinct_categories() {
        let errors: Vec<String> = vec![
            GraphtorError::Config {
                message: "m".to_string(),
                field: None,
            }
            .to_string(),
            GraphtorError::Database {
                message: "m".to_string(),
                operation: "op".to_string(),
            }
            .to_string(),
            GraphtorError::DatabaseLocked {
                db_name: "graph.db".to_string(),
                holder_pid: Some(7),
            }
            .to_string(),
            GraphtorError::Pipeline {
                message: "m".to_string(),
                stage: "s".to_string(),
            }
            .to_string(),
            GraphtorError::Parse {
                message: "m".to_string(),
                path: None,
            }
            .to_string(),
            GraphtorError::Embed {
                message: "m".to_string(),
                chunk_id: None,
            }
            .to_string(),
            GraphtorError::PathViolation {
                attempted: PathBuf::from("/a"),
                allowed_root: PathBuf::from("/b"),
            }
            .to_string(),
            GraphtorError::Sync {
                message: "m".to_string(),
                source_id: "s".to_string(),
            }
            .to_string(),
            GraphtorError::Io(std::io::Error::other("e")).to_string(),
        ];

        let categories: std::collections::HashSet<&str> = errors
            .iter()
            .map(|s| s.split(']').next().unwrap_or(""))
            .collect();
        assert_eq!(
            categories.len(),
            9,
            "expected 9 distinct error categories, got {}: {categories:?}",
            categories.len()
        );
    }
}
