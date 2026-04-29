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
/// Returns `(Some(frontmatter), body)` when the document starts with a `---`
/// delimiter, otherwise `(None, content)` with the original text unchanged.
///
/// The frontmatter block must start on the very first line. The closing `---`
/// may appear as `---` or `...` (YAML end-of-document marker).
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn strip(content: &str) -> (Option<FrontmatterData>, &str) {
    let trimmed = content.trim_start_matches('\n');

    if !trimmed.starts_with("---\n") && trimmed != "---" {
        return (None, content);
    }

    let after_open = &trimmed["---\n".len()..];

    // Find the closing delimiter and compute the byte offset of the body start
    // (relative to `after_open`).
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

    // body_start is relative to the original `content` string.
    let open_len = "---\n".len();
    let body_start = (open_len + delim_end).min(content.len());

    let raw: FrontmatterRaw = serde_yaml::from_str(raw_yaml).unwrap_or_default();

    let data = FrontmatterData {
        title: raw.title,
        description: raw.description,
        raw_yaml: raw_yaml.to_string(),
    };

    (Some(data), &content[body_start..])
}
