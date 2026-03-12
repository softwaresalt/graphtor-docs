//! Deterministic chunk identifier generation via SHA-256.
//!
//! Provides [`generate_chunk_id`] which computes a stable, 64-character
//! lowercase hexadecimal identifier for a documentation chunk. The
//! identifier serves as the cross-database correlation key linking
//! `LanceDB` vectors to Kùzu graph nodes.

use sha2::{Digest, Sha256};

use crate::error::GraphtorError;

/// Generate a deterministic SHA-256-based chunk identifier.
///
/// Computes `SHA-256(content + "\0" + source_path)` and returns a
/// 64-character lowercase hexadecimal string.
///
/// # Errors
///
/// Returns [`GraphtorError::Parse`] if `content` or `source_path` is empty.
///
/// # Examples
///
/// ```
/// # use graphtor_core::chunk::id::generate_chunk_id;
/// let id = generate_chunk_id("hello world", "docs/guide.md").unwrap();
/// assert_eq!(id.len(), 64);
/// assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
/// ```
pub fn generate_chunk_id(content: &str, source_path: &str) -> Result<String, GraphtorError> {
    if content.is_empty() {
        return Err(GraphtorError::Parse {
            message: "chunk content must not be empty".to_string(),
            path: None,
        });
    }
    if source_path.is_empty() {
        return Err(GraphtorError::Parse {
            message: "source path must not be empty".to_string(),
            path: None,
        });
    }

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_path.as_bytes());
    let result = hasher.finalize();

    Ok(format!("{result:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T023: determinism ─────────────────────────────────────────────────

    #[test]
    fn same_input_produces_same_id() {
        let id1 = generate_chunk_id("hello world", "docs/guide.md").unwrap();
        let id2 = generate_chunk_id("hello world", "docs/guide.md").unwrap();
        assert_eq!(id1, id2, "same input must produce the same chunk id");
    }

    #[test]
    fn deterministic_across_calls() {
        for _ in 0..10 {
            let id = generate_chunk_id("stable content", "path/to/doc.md").unwrap();
            assert_eq!(
                id,
                generate_chunk_id("stable content", "path/to/doc.md").unwrap(),
                "chunk id must be deterministic"
            );
        }
    }

    // ── T024: uniqueness ──────────────────────────────────────────────────

    #[test]
    fn different_content_same_path_produces_different_id() {
        let id1 = generate_chunk_id("content A", "docs/guide.md").unwrap();
        let id2 = generate_chunk_id("content B", "docs/guide.md").unwrap();
        assert_ne!(
            id1, id2,
            "different content at same path must produce different ids"
        );
    }

    #[test]
    fn same_content_different_path_produces_different_id() {
        let id1 = generate_chunk_id("identical content", "docs/a.md").unwrap();
        let id2 = generate_chunk_id("identical content", "docs/b.md").unwrap();
        assert_ne!(
            id1, id2,
            "same content at different paths must produce different ids"
        );
    }

    #[test]
    fn content_path_separator_prevents_ambiguity() {
        // Without null-byte separator, "ab" + "c" == "a" + "bc"
        let id1 = generate_chunk_id("ab", "c").unwrap();
        let id2 = generate_chunk_id("a", "bc").unwrap();
        assert_ne!(id1, id2, "null-byte separator must prevent hash collisions");
    }

    // ── T025: format ──────────────────────────────────────────────────────

    #[test]
    fn id_is_64_hex_characters() {
        let id = generate_chunk_id("some chunk text", "docs/api.md").unwrap();
        assert_eq!(
            id.len(),
            64,
            "chunk id must be exactly 64 characters, got {}",
            id.len()
        );
    }

    #[test]
    fn id_is_lowercase_hex() {
        let id = generate_chunk_id("text", "path.md").unwrap();
        assert!(
            id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "chunk id must be lowercase hexadecimal: {id}"
        );
    }

    #[test]
    fn id_matches_expected_hex_regex_pattern() {
        let id = generate_chunk_id("chunk", "src/doc.md").unwrap();
        // Verify format: exactly 64 lowercase hex chars
        let valid = id.len() == 64 && id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
        assert!(valid, "id '{id}' does not match ^[0-9a-f]{{64}}$");
    }

    // ── T026: edge cases ──────────────────────────────────────────────────

    #[test]
    fn unicode_content_produces_valid_id() {
        let id = generate_chunk_id("日本語のドキュメント — résumé — Ω", "docs/unicode.md").unwrap();
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    }

    #[test]
    fn large_content_produces_valid_id() {
        let large = "x".repeat(1_000_000);
        let id = generate_chunk_id(&large, "docs/large.md").unwrap();
        assert_eq!(id.len(), 64);
    }

    #[test]
    fn empty_content_returns_parse_error() {
        let result = generate_chunk_id("", "docs/guide.md");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.starts_with("[parse]"),
            "empty content should produce Parse error: {msg}"
        );
    }

    #[test]
    fn empty_path_returns_parse_error() {
        let result = generate_chunk_id("some content", "");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.starts_with("[parse]"),
            "empty path should produce Parse error: {msg}"
        );
    }

    #[test]
    fn known_input_produces_deterministic_64_char_hex() {
        // Regression anchor: SHA-256("hello\0world") pinned to a known value.
        // Algorithm: SHA-256(content_bytes + b"\0" + source_path_bytes).
        // To recompute: echo -n $'hello\x00world' | sha256sum
        // Expected: b206899bc103669c8e7b36de29d73f95b46795b508aa87d612b2ce84bfb29df2
        let id = generate_chunk_id("hello", "world").unwrap();
        assert_eq!(
            id, "b206899bc103669c8e7b36de29d73f95b46795b508aa87d612b2ce84bfb29df2",
            "SHA-256 regression anchor must not change — algorithm or separator drift detected"
        );
        assert_eq!(id.len(), 64);
    }
}
