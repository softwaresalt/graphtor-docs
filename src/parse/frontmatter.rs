//! YAML frontmatter detection and extraction.
//!
//! Markdown files commonly open with a `---`-delimited YAML block.
//! [`strip`] detects this pattern, extracts the YAML text, and returns the
//! document body without the frontmatter preamble.

use serde::Deserialize;

use crate::parse::types::FrontmatterData;

/// Minimal schema for deserialising the fields we care about from frontmatter.
#[derive(Deserialize, Default)]
struct FrontmatterRaw {
    title: Option<String>,
    description: Option<String>,
}

/// Detect and strip YAML frontmatter from `content`.
///
/// Returns `(Some(frontmatter), body)` when the document starts with a `---\n`
/// delimiter at byte 0, otherwise `(None, content)` with the original text unchanged.
///
/// Frontmatter must begin at the very first byte of `content`. The closing
/// delimiter may be `---` or `...` (YAML end-of-document marker), followed by
/// a newline or end-of-string.
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn strip(content: &str) -> (Option<FrontmatterData>, &str) {
    // Frontmatter must start at byte 0 — no leading whitespace or newlines
    // allowed, which also avoids any offset-calculation ambiguity.
    if !content.starts_with("---\n") {
        return (None, content);
    }

    let after_open = &content["---\n".len()..];

    // Find the closing delimiter and compute the exact byte offset of the body
    // start, both relative to `content`.
    let (close_pos, delim_end) = if let Some(p) = after_open.find("\n---\n") {
        (p, p + 5) // \n + --- + \n
    } else if let Some(p) = after_open.find("\n...\n") {
        (p, p + 5) // \n + ... + \n
    } else if let Some(p) = after_open.find("\n---") {
        (p, p + 4) // \n + --- (EOF, no trailing newline)
    } else if let Some(p) = after_open.find("\n...") {
        (p, p + 4) // \n + ... (EOF, no trailing newline)
    } else {
        // Malformed — no closing delimiter.
        return (None, content);
    };

    let raw_yaml = &after_open[..close_pos];

    // body_start is relative to `content` (= opening "---\n" length + delim_end).
    let body_start = ("---\n".len() + delim_end).min(content.len());

    let raw: FrontmatterRaw = serde_yaml::from_str(raw_yaml).unwrap_or_default();

    let data = FrontmatterData {
        title: raw.title,
        description: raw.description,
        raw_yaml: raw_yaml.to_string(),
    };

    (Some(data), &content[body_start..])
}
