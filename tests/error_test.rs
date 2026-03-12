//! Integration tests for `graphtor_core::error` module.
//!
//! Verifies that all 8 `GraphtorError` variants produce distinct,
//! human-readable messages with appropriate context fields, and that
//! `From` conversions for external error types work correctly end-to-end.

use graphtor_core::error::GraphtorError;
use std::path::PathBuf;

/// All 8 variants must produce distinct, non-empty error messages.
#[test]
fn all_variants_produce_non_empty_distinct_messages() {
    let variants: Vec<(&str, GraphtorError)> = vec![
        (
            "Config",
            GraphtorError::Config {
                message: "missing id".to_string(),
                field: Some("id".to_string()),
            },
        ),
        (
            "Database",
            GraphtorError::Database {
                message: "write failed".to_string(),
                operation: "upsert".to_string(),
            },
        ),
        (
            "Pipeline",
            GraphtorError::Pipeline {
                message: "chunk failed".to_string(),
                stage: "chunk".to_string(),
            },
        ),
        (
            "Parse",
            GraphtorError::Parse {
                message: "bad frontmatter".to_string(),
                path: Some(PathBuf::from("docs/guide.md")),
            },
        ),
        (
            "Embed",
            GraphtorError::Embed {
                message: "embedding timeout".to_string(),
                chunk_id: Some("abc".to_string()),
            },
        ),
        (
            "PathViolation",
            GraphtorError::PathViolation {
                attempted: PathBuf::from("/etc/passwd"),
                allowed_root: PathBuf::from("/workspace"),
            },
        ),
        (
            "Sync",
            GraphtorError::Sync {
                message: "stale state".to_string(),
                source_id: "ms-core".to_string(),
            },
        ),
        (
            "Io",
            GraphtorError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing file",
            )),
        ),
    ];

    let messages: Vec<String> = variants
        .iter()
        .map(|(name, e)| {
            let s = e.to_string();
            assert!(!s.is_empty(), "{name} variant produced empty message");
            s
        })
        .collect();

    // All messages must start with their respective category prefix
    let prefixes = [
        "[config]",
        "[database]",
        "[pipeline]",
        "[parse]",
        "[embed]",
        "[path_violation]",
        "[sync]",
        "[io]",
    ];
    for (i, (msg, prefix)) in messages.iter().zip(prefixes.iter()).enumerate() {
        assert!(
            msg.starts_with(prefix),
            "variant {} expected prefix '{}', got: {msg}",
            variants[i].0,
            prefix
        );
    }

    // All 8 messages must be distinct
    let unique: std::collections::HashSet<&str> = messages.iter().map(String::as_str).collect();
    assert_eq!(
        unique.len(),
        8,
        "expected 8 distinct error messages, some may be identical: {messages:?}"
    );
}

/// `From<serde_yaml::Error>` produces a Config variant with the YAML error text.
#[test]
fn serde_yaml_error_converts_to_config_with_message() {
    let bad_yaml = "sources:\n  - id: [unclosed";
    let yaml_err = serde_yaml::from_str::<serde_yaml::Value>(bad_yaml).unwrap_err();
    let e: GraphtorError = yaml_err.into();

    assert!(
        matches!(e, GraphtorError::Config { .. }),
        "expected Config variant, got: {e:?}"
    );
    let msg = e.to_string();
    assert!(
        msg.starts_with("[config]"),
        "expected '[config]' prefix: {msg}"
    );
    assert!(!msg.is_empty(), "Config message must not be empty");
}

/// `From<std::io::Error>` produces an Io variant.
#[test]
fn io_error_converts_to_io_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let e: GraphtorError = io_err.into();
    assert!(
        matches!(e, GraphtorError::Io(_)),
        "expected Io variant, got: {e:?}"
    );
    let msg = e.to_string();
    assert!(msg.starts_with("[io]"), "expected '[io]' prefix: {msg}");
    assert!(
        msg.contains("permission denied"),
        "message should contain io error description: {msg}"
    );
}

/// The re-exported `GraphtorError` from crate root is the same type.
#[test]
fn crate_root_reexport_is_same_type() {
    let e: graphtor_core::GraphtorError = GraphtorError::Config {
        message: "test".to_string(),
        field: None,
    };
    assert!(e.to_string().starts_with("[config]"));
}
