//! Docline v1 frontmatter contract validator.
//!
//! This module validates documents against the stable v1 ingestion contract
//! that docline emits and graphtor-docs ingests. Validation is strict and
//! fail-closed: malformed YAML, missing required fields, unsupported major
//! schema versions, and `content_sha256` mismatches all return errors rather
//! than silently defaulting.
//!
//! # Contract surface
//!
//! The authoritative contract is defined in:
//! - `docs/design-docs/graphtor-docs-ingestion-contract.md`
//! - `schemas/docline/base-frontmatter-v1.schema.json`
//!
//! The schema JSON is embedded at compile time so installed binaries behave
//! identically to development builds without runtime file lookups.
//!
//! # Required fields (graphtor-docs policy)
//!
//! | Field | Requirement |
//! |---|---|
//! | `title` | Non-empty string |
//! | `source` | Non-empty string |
//! | `ingested_at` | Non-empty ISO-8601 timestamp string |
//! | `doc_type` | Non-empty string |
//! | `source_path` | Non-empty, workspace-relative, forward-slash normalized |
//!
//! # Version policy
//!
//! Only major version `1` is supported. A document with `schema_version: "2.0"`
//! will be rejected. Minor/patch increments within the 1.x line are accepted.

use sha2::{Digest, Sha256};

use crate::error::GraphtorError;

/// The compile-time-embedded JSON Schema for the v1 frontmatter contract.
///
/// This constant exists to prove at compile time that the schema file is
/// present and readable. Runtime code uses the Rust struct validation path
/// rather than a JSON-Schema validator, keeping binary size small.
pub const SCHEMA_V1_JSON: &str =
    include_str!("../../schemas/docline/base-frontmatter-v1.schema.json");

/// The only supported major schema version accepted by this validator.
pub const SUPPORTED_MAJOR_VERSION: u32 = 1;

/// Identifies this ingestion contract epoch in sync state.
///
/// When the sync state records a different epoch the source is treated as
/// needing a full reprocess, preventing stale pre-pivot incremental state
/// from suppressing reprocessing.
pub const CONTRACT_EPOCH: &str = "docline-v1";

/// Raw deserialization target for docline frontmatter YAML.
///
/// All fields are optional at the deserialization layer; required-field
/// enforcement happens in [`validate`] so error messages are precise.
#[derive(Debug, serde::Deserialize)]
struct FrontmatterRaw {
    title: Option<String>,
    source: Option<String>,
    ingested_at: Option<String>,
    doc_type: Option<String>,
    description: Option<String>,
    content_sha256: Option<String>,
    source_path: Option<String>,
    chunk_strategy: Option<String>,
    schema_version: Option<String>,
    canonical_url: Option<String>,
}

/// A fully validated docline v1 frontmatter record.
///
/// All fields have been checked against the contract surface. The values
/// stored here are safe to use directly in the ingestion pipeline.
#[derive(Debug, Clone)]
pub struct ValidatedFrontmatter {
    /// Human-readable document title (required, non-empty).
    pub title: String,
    /// Origin URI or path of the source document (required, non-empty).
    pub source: String,
    /// Timestamp when docline ingested the source (required, non-empty).
    pub ingested_at: String,
    /// Document-type identifier (required, non-empty).
    pub doc_type: String,
    /// Short human-readable description (defaults to `""`).
    pub description: String,
    /// SHA-256 hex digest of the markdown body (defaults to `""`).
    pub content_sha256: String,
    /// Project-relative POSIX path of the source artifact.
    ///
    /// graphtor-docs requires this field to be non-empty. It is normalized
    /// to forward slashes and must be relative (no leading `/`).
    pub source_path: String,
    /// Chunk-boundary strategy identifier (defaults to `"h1-h2-h3"`).
    pub chunk_strategy: String,
    /// `SemVer` contract version string (defaults to `"1.0"`).
    pub schema_version: String,
    /// Globally-unique published URL the document is served under, if docline
    /// emitted one (e.g. `/fabric/admin/foo`). Optional — used as the
    /// cross-source key for graph traversal. graphtor-docs does not derive this
    /// value; it reads whatever docline provides.
    pub canonical_url: Option<String>,
}

/// Validate the YAML frontmatter of a docline-emitted Markdown document.
///
/// `raw_yaml` is the YAML text between the `---` delimiters (without the
/// delimiters themselves). `body` is the markdown body after the closing
/// delimiter — used only when `content_sha256` is non-empty.
///
/// The `content_sha256` field is validated against the **LF-normalised** body.
/// Working-tree CRLF line endings (common on Windows) are stripped before
/// hashing so that the digest matches what docline emitted on any platform.
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] when:
/// - `raw_yaml` cannot be parsed as YAML
/// - A required field is missing or empty
/// - The `schema_version` major component is not `1`
/// - `source_path` is empty, absolute, drive-prefixed, or contains `.`/`..`
///   components
/// - `content_sha256` is non-empty and does not match the SHA-256 of the
///   LF-normalised `body`
pub fn validate(raw_yaml: &str, body: &str) -> Result<ValidatedFrontmatter, GraphtorError> {
    let raw: FrontmatterRaw =
        serde_yaml::from_str(raw_yaml).map_err(|e| GraphtorError::Contract {
            message: format!("malformed YAML frontmatter: {e}"),
            field: None,
        })?;

    // ── Required field: title ─────────────────────────────────────────────
    let title = require_non_empty(raw.title, "title")?;

    // ── Required field: source ────────────────────────────────────────────
    let source = require_non_empty(raw.source, "source")?;

    // ── Required field: ingested_at ───────────────────────────────────────
    let ingested_at = require_non_empty(raw.ingested_at, "ingested_at")?;

    // ── Required field: doc_type ──────────────────────────────────────────
    let doc_type = require_non_empty(raw.doc_type, "doc_type")?;

    // ── Required (graphtor-docs policy): source_path ──────────────────────
    let source_path = validate_source_path(raw.source_path.unwrap_or_default())?;

    // ── schema_version: must have major == 1 ─────────────────────────────
    let schema_version = raw.schema_version.unwrap_or_else(|| "1.0".to_string());
    let major = schema_version
        .split('.')
        .next()
        .unwrap_or("0")
        .parse::<u32>()
        .unwrap_or(0);
    if major != SUPPORTED_MAJOR_VERSION {
        return Err(GraphtorError::Contract {
            message: format!(
                "unsupported schema_version major '{major}'; only major version \
                 {SUPPORTED_MAJOR_VERSION} is accepted (got '{schema_version}')"
            ),
            field: Some("schema_version".to_string()),
        });
    }

    // ── content_sha256: verify when non-empty ─────────────────────────────
    // Hash is computed over the LF-normalised body so that Windows CRLF
    // checkouts produce the same digest as the originating docline emit.
    let content_sha256 = raw.content_sha256.unwrap_or_default();
    if !content_sha256.is_empty() {
        let normalised_body: std::borrow::Cow<str> = if body.contains('\r') {
            std::borrow::Cow::Owned(body.replace("\r\n", "\n"))
        } else {
            std::borrow::Cow::Borrowed(body)
        };
        let mut hasher = Sha256::new();
        hasher.update(normalised_body.as_bytes());
        let computed = format!("{:x}", hasher.finalize());
        if computed != content_sha256.to_ascii_lowercase() {
            return Err(GraphtorError::Contract {
                message: format!(
                    "content_sha256 mismatch: expected '{content_sha256}', computed '{computed}'"
                ),
                field: Some("content_sha256".to_string()),
            });
        }
    }

    Ok(ValidatedFrontmatter {
        title,
        source,
        ingested_at,
        doc_type,
        description: raw.description.unwrap_or_default(),
        content_sha256,
        source_path,
        chunk_strategy: raw.chunk_strategy.unwrap_or_else(|| "h1-h2-h3".to_string()),
        schema_version,
        canonical_url: normalize_optional(raw.canonical_url),
    })
}

/// Trim an optional string and collapse empty values to `None`.
fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Validate and normalise `source_path`.
///
/// Normalises back-slashes to forward slashes, then enforces:
/// - Non-empty after normalisation
/// - No leading `/` (must be relative)
/// - No Windows drive prefix (e.g. `C:/`)
/// - No `.` or `..` path components
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] on any violation.
fn validate_source_path(raw: impl Into<String>) -> Result<String, GraphtorError> {
    let path = raw.into().replace('\\', "/");

    if path.is_empty() {
        return Err(GraphtorError::Contract {
            message: "source_path is required and must not be empty".to_string(),
            field: Some("source_path".to_string()),
        });
    }

    // Reject absolute paths (leading '/').
    if path.starts_with('/') {
        return Err(GraphtorError::Contract {
            message: format!("source_path must be workspace-relative (no leading '/'): '{path}'"),
            field: Some("source_path".to_string()),
        });
    }

    // Reject Windows drive-prefixed paths (e.g. `C:/`, `c:/`, `C:\` normalised
    // to `C:/`).  A drive prefix is two chars: ASCII letter followed by `:`.
    if path.len() >= 2 {
        let mut chars = path.chars();
        let first = chars.next().unwrap_or('\0');
        let second = chars.next().unwrap_or('\0');
        if first.is_ascii_alphabetic() && second == ':' {
            return Err(GraphtorError::Contract {
                message: format!(
                    "source_path must be workspace-relative (no drive prefix): '{path}'"
                ),
                field: Some("source_path".to_string()),
            });
        }
    }

    // Reject `.`, `..`, and empty path components.
    //
    // Empty components arise from consecutive slashes (`docs//guide.md`) or a
    // trailing slash (`docs/guide.md/`). Neither form is canonical; rejecting
    // them ensures the persisted identity is unique and consistent.
    for component in path.split('/') {
        if component == "." || component == ".." {
            return Err(GraphtorError::Contract {
                message: format!("source_path must not contain '.' or '..' components: '{path}'"),
                field: Some("source_path".to_string()),
            });
        }
        if component.is_empty() {
            return Err(GraphtorError::Contract {
                message: format!(
                    "source_path must not contain empty path segments \
                     (consecutive or trailing slashes are not canonical): '{path}'"
                ),
                field: Some("source_path".to_string()),
            });
        }
    }

    Ok(path)
}

/// Extract the validated `source_path` from a docline v1 markdown file.
///
/// Reads the file, strips frontmatter, and returns the `source_path` value
/// after applying [`validate_source_path`] normalisation.  This is lighter
/// than full [`validate`] and is used for pre-scan duplicate-source-path
/// detection before the ingestion pipeline begins loading any chunks.
///
/// Files that cannot be read, have no frontmatter, contain malformed YAML, or
/// lack a `source_path` field are reported via the returned `Err` — callers
/// should treat them as "unknown path" and skip them in the duplicate check
/// (the subsequent reingest / parse pass will emit the definitive error).
///
/// # Errors
///
/// Returns [`GraphtorError`] if the file cannot be read, has no frontmatter,
/// contains malformed YAML, or the `source_path` field is absent or invalid.
pub(crate) fn extract_source_path_from_file(
    path: &std::path::Path,
) -> Result<String, GraphtorError> {
    use crate::parse::frontmatter;

    /// Minimal deserialization target — we only need `source_path`.
    #[derive(serde::Deserialize)]
    struct MinimalFm {
        source_path: Option<String>,
    }

    let content = std::fs::read_to_string(path)?;
    let (fm_data, _body) = frontmatter::strip(&content);

    let raw_yaml = match fm_data {
        Some(fm) => fm.raw_yaml,
        None => {
            return Err(GraphtorError::Contract {
                message: "no frontmatter found; docline v1 contract is required".to_string(),
                field: None,
            });
        }
    };

    let minimal: MinimalFm =
        serde_yaml::from_str(&raw_yaml).map_err(|e| GraphtorError::Contract {
            message: format!("malformed YAML frontmatter: {e}"),
            field: None,
        })?;

    validate_source_path(minimal.source_path.unwrap_or_default())
}

/// Helper: unwrap `opt`, failing with a Contract error when None or empty.
fn require_non_empty(opt: Option<String>, field: &str) -> Result<String, GraphtorError> {
    match opt {
        Some(v) if !v.is_empty() => Ok(v),
        Some(_) => Err(GraphtorError::Contract {
            message: format!("required field '{field}' must not be empty"),
            field: Some(field.to_string()),
        }),
        None => Err(GraphtorError::Contract {
            message: format!("required field '{field}' is missing"),
            field: Some(field.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_yaml(source_path: &str) -> String {
        format!(
            "title: My Guide\nsource: /repo/docs/my-guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n"
        )
    }

    fn valid_body() -> &'static str {
        "# My Guide\n\nHello world.\n"
    }

    // ── Required fields ───────────────────────────────────────────────────

    #[test]
    fn valid_frontmatter_parses_correctly() {
        let yaml = valid_yaml("docs/guide.md");
        let fm = validate(&yaml, valid_body()).expect("valid frontmatter should pass");
        assert_eq!(fm.title, "My Guide");
        assert_eq!(fm.source_path, "docs/guide.md");
        assert_eq!(fm.schema_version, "1.0");
        assert_eq!(fm.chunk_strategy, "h1-h2-h3");
    }

    #[test]
    fn canonical_url_is_none_when_absent() {
        let yaml = valid_yaml("docs/guide.md");
        let fm = validate(&yaml, valid_body()).expect("valid frontmatter should pass");
        assert_eq!(fm.canonical_url, None);
    }

    #[test]
    fn canonical_url_is_read_and_trimmed_when_present() {
        let yaml = format!(
            "{}canonical_url: \"  /fabric/admin/foo  \"\n",
            valid_yaml("docs/guide.md")
        );
        let fm = validate(&yaml, valid_body()).expect("valid frontmatter should pass");
        assert_eq!(fm.canonical_url.as_deref(), Some("/fabric/admin/foo"));
    }

    #[test]
    fn canonical_url_blank_collapses_to_none() {
        let yaml = format!("{}canonical_url: \"   \"\n", valid_yaml("docs/guide.md"));
        let fm = validate(&yaml, valid_body()).expect("valid frontmatter should pass");
        assert_eq!(fm.canonical_url, None);
    }

    #[test]
    fn missing_title_fails_closed() {
        let yaml = "source: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\n";
        let err = validate(yaml, valid_body()).expect_err("missing title must fail");
        assert!(
            err.to_string().contains("[contract]"),
            "wrong error type: {err}"
        );
        assert!(
            err.to_string().contains("title"),
            "should mention field: {err}"
        );
    }

    #[test]
    fn empty_title_fails_closed() {
        let yaml = "title: \"\"\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\n";
        let err = validate(yaml, valid_body()).expect_err("empty title must fail");
        assert!(
            err.to_string().contains("[contract]"),
            "wrong error type: {err}"
        );
    }

    #[test]
    fn missing_source_fails_closed() {
        let yaml = "title: Guide\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\n";
        let err = validate(yaml, valid_body()).expect_err("missing source must fail");
        assert!(err.to_string().contains("source"), "{err}");
    }

    #[test]
    fn missing_ingested_at_fails_closed() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ndoc_type: markdown\nsource_path: docs/guide.md\n";
        let err = validate(yaml, valid_body()).expect_err("missing ingested_at must fail");
        assert!(err.to_string().contains("ingested_at"), "{err}");
    }

    #[test]
    fn missing_doc_type_fails_closed() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\nsource_path: docs/guide.md\n";
        let err = validate(yaml, valid_body()).expect_err("missing doc_type must fail");
        assert!(err.to_string().contains("doc_type"), "{err}");
    }

    // ── source_path validation ────────────────────────────────────────────

    #[test]
    fn missing_source_path_fails_closed() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\n";
        let err = validate(yaml, valid_body()).expect_err("missing source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn empty_source_path_fails_closed() {
        let yaml = valid_yaml("");
        let err = validate(&yaml, valid_body()).expect_err("empty source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn absolute_source_path_fails_closed() {
        let yaml = valid_yaml("/absolute/path.md");
        let err = validate(&yaml, valid_body()).expect_err("absolute source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn backslash_source_path_normalized_to_forward_slash() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs\\guide.md\n";
        let fm = validate(yaml, valid_body()).expect("backslash path should normalize");
        assert_eq!(fm.source_path, "docs/guide.md");
    }

    // ── schema_version ────────────────────────────────────────────────────

    #[test]
    fn unsupported_major_version_fails_closed() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\nschema_version: \"2.0\"\n";
        let err = validate(yaml, valid_body()).expect_err("major version 2 must fail");
        assert!(err.to_string().contains("schema_version"), "{err}");
    }

    #[test]
    fn minor_version_bump_within_v1_is_accepted() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\nschema_version: \"1.5\"\n";
        let fm = validate(yaml, valid_body()).expect("1.x minor bump should be accepted");
        assert_eq!(fm.schema_version, "1.5");
    }

    #[test]
    fn default_schema_version_is_1_0() {
        let yaml = valid_yaml("docs/guide.md");
        let fm = validate(&yaml, valid_body()).expect("no schema_version defaults to 1.0");
        assert_eq!(fm.schema_version, "1.0");
    }

    // ── content_sha256 ────────────────────────────────────────────────────

    #[test]
    fn matching_content_sha256_passes() {
        use sha2::{Digest, Sha256};
        let body = "# Guide\n\nContent here.\n";
        let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
        let yaml = format!(
            "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\ncontent_sha256: \"{hash}\"\n"
        );
        let fm = validate(&yaml, body).expect("matching hash should pass");
        assert_eq!(fm.content_sha256, hash);
    }

    #[test]
    fn mismatched_content_sha256_fails_closed() {
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\ncontent_sha256: \"0000000000000000000000000000000000000000000000000000000000000000\"\n";
        let err = validate(yaml, valid_body()).expect_err("wrong hash must fail");
        assert!(err.to_string().contains("content_sha256"), "{err}");
    }

    #[test]
    fn empty_content_sha256_skips_hash_check() {
        // When content_sha256 is absent or empty, no hash check is performed.
        let yaml = "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: docs/guide.md\ncontent_sha256: \"\"\n";
        let fm = validate(yaml, valid_body()).expect("empty hash should skip check");
        assert_eq!(fm.content_sha256, "");
    }

    // ── Malformed YAML ────────────────────────────────────────────────────

    #[test]
    fn malformed_yaml_fails_closed() {
        let bad_yaml = "title: [unterminated\n";
        let err = validate(bad_yaml, valid_body()).expect_err("malformed yaml must fail");
        assert!(err.to_string().starts_with("[contract]"), "{err}");
    }

    // ── Schema JSON embedded ──────────────────────────────────────────────

    #[test]
    fn embedded_schema_json_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(SCHEMA_V1_JSON).expect("embedded schema must be valid JSON");
    }

    // ── source_path: drive prefix + dot components ────────────────────────

    #[test]
    fn drive_prefixed_source_path_fails_closed() {
        let yaml = valid_yaml("C:/users/docs/guide.md");
        let err = validate(&yaml, valid_body()).expect_err("drive-prefixed source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
        assert!(
            err.to_string().contains("drive"),
            "error should mention drive prefix: {err}"
        );
    }

    #[test]
    fn lowercase_drive_prefixed_source_path_fails_closed() {
        let yaml = valid_yaml("c:/users/docs/guide.md");
        let err = validate(&yaml, valid_body())
            .expect_err("lowercase drive-prefixed source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn backslash_drive_prefixed_source_path_fails_closed() {
        // Backslash is normalised to forward slash before the drive-prefix check.
        let yaml = valid_yaml("C:\\users\\docs\\guide.md");
        let err = validate(&yaml, valid_body())
            .expect_err("backslash drive-prefixed source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn dot_component_in_source_path_fails_closed() {
        let yaml = valid_yaml("docs/./guide.md");
        let err =
            validate(&yaml, valid_body()).expect_err("'.' component in source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn dotdot_component_in_source_path_fails_closed() {
        let yaml = valid_yaml("docs/../guide.md");
        let err =
            validate(&yaml, valid_body()).expect_err("'..' component in source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn leading_dotdot_source_path_fails_closed() {
        let yaml = valid_yaml("../escape.md");
        let err = validate(&yaml, valid_body()).expect_err("leading '..' source_path must fail");
        assert!(err.to_string().contains("source_path"), "{err}");
    }

    #[test]
    fn double_slash_in_source_path_fails_closed() {
        // `docs//guide.md` splits into ["docs", "", "guide.md"]: the empty
        // component is non-canonical and must be rejected.
        let yaml = valid_yaml("docs//guide.md");
        let err = validate(&yaml, valid_body())
            .expect_err("double-slash source_path must be rejected as non-canonical");
        assert!(err.to_string().contains("source_path"), "{err}");
        assert!(
            err.to_string().contains("empty path segment"),
            "error should describe empty segment: {err}"
        );
    }

    #[test]
    fn trailing_slash_in_source_path_fails_closed() {
        // `docs/guide.md/` has a trailing empty component and must be rejected.
        let yaml = valid_yaml("docs/guide.md/");
        let err = validate(&yaml, valid_body())
            .expect_err("trailing-slash source_path must be rejected as non-canonical");
        assert!(err.to_string().contains("source_path"), "{err}");
        assert!(
            err.to_string().contains("empty path segment"),
            "error should describe empty segment: {err}"
        );
    }

    // ── CRLF body hash normalization ──────────────────────────────────────

    #[test]
    fn crlf_body_hash_validates_against_lf_hash() {
        // Docline computes the hash over LF-normalised body.
        // A Windows checkout may deliver CRLF body bytes; validation must
        // normalise before hashing to match the stored digest.
        use sha2::{Digest, Sha256};
        let lf_body = "# Guide\n\nContent here.\n";
        let crlf_body = "# Guide\r\n\r\nContent here.\r\n";
        // Hash stored in the contract is over LF-normalised bytes.
        let stored_hash = format!("{:x}", Sha256::digest(lf_body.as_bytes()));
        let yaml = format!(
            "title: Guide\nsource: /repo/docs/guide.md\ningested_at: 2026-01-01T00:00:00Z\
             \ndoc_type: markdown\nsource_path: docs/guide.md\ncontent_sha256: \"{stored_hash}\"\n"
        );
        // Validate with the CRLF body; should pass because we normalise before hashing.
        let fm =
            validate(&yaml, crlf_body).expect("CRLF body must validate against LF-normalised hash");
        assert_eq!(fm.content_sha256, stored_hash);
    }

    #[test]
    fn crlf_frontmatter_yaml_parses_correctly() {
        // YAML with CRLF line endings must be accepted (normalised by the strip layer).
        let yaml = "title: CRLF Guide\r\nsource: /repo/docs/guide.md\r\ningested_at: 2026-01-01T00:00:00Z\r\ndoc_type: markdown\r\nsource_path: docs/guide.md\r\n";
        let fm = validate(yaml, valid_body()).expect("CRLF YAML must be accepted");
        assert_eq!(fm.title, "CRLF Guide");
        assert_eq!(fm.source_path, "docs/guide.md");
    }
}
