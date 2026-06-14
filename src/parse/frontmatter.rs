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
/// (or `---\r\n`) delimiter at byte 0, otherwise `(None, content)` with the
/// original text unchanged.
///
/// Frontmatter must begin at the very first byte of `content`. The closing
/// delimiter may be `---` or `...` (YAML end-of-document marker), followed by
/// a newline, CRLF, or end-of-string.
///
/// Both LF-only and CRLF working-tree line endings are accepted so that
/// Windows checkouts do not cause detection failures.  The YAML text stored
/// in [`FrontmatterData::raw_yaml`] is always LF-normalised.
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn strip(content: &str) -> (Option<FrontmatterData>, &str) {
    // Detect the line-ending style from the opening delimiter.
    // Frontmatter must start at byte 0 — no leading whitespace or newlines
    // allowed, which also avoids any offset-calculation ambiguity.
    let (open_len, crlf) = if content.starts_with("---\r\n") {
        ("---\r\n".len(), true)
    } else if content.starts_with("---\n") {
        ("---\n".len(), false)
    } else {
        return (None, content);
    };

    let after_open = &content[open_len..];

    // Find the closing delimiter and compute the exact byte offset of the body
    // start, both relative to `content`.
    // Accept both CRLF and LF variants regardless of the opening EOL style so
    // that mixed-EOL documents (rare but possible) are handled gracefully.
    let (close_pos, delim_end) = {
        // Helper: search for each candidate pattern and return the earliest.
        let candidates: &[&str] = if crlf {
            &[
                "\r\n---\r\n",
                "\r\n...\r\n",
                "\n---\n",
                "\n...\n",
                "\r\n---",
                "\r\n...",
                "\n---",
                "\n...",
            ]
        } else {
            &[
                "\n---\n",
                "\n...\n",
                "\r\n---\r\n",
                "\r\n...\r\n",
                "\n---",
                "\n...",
                "\r\n---",
                "\r\n...",
            ]
        };

        let mut best: Option<(usize, usize)> = None;
        for pattern in candidates {
            if let Some(pos) = after_open.find(pattern) {
                let end = pos + pattern.len();
                // Keep the earliest match (smallest `pos`).
                match best {
                    None => best = Some((pos, end)),
                    Some((prev_pos, _)) if pos < prev_pos => best = Some((pos, end)),
                    _ => {}
                }
            }
        }
        match best {
            Some(r) => r,
            None => return (None, content), // Malformed — no closing delimiter.
        }
    };

    let raw_yaml_bytes = &after_open[..close_pos];

    // body_start is relative to `content` (= open_len + delim_end).
    let body_start = (open_len + delim_end).min(content.len());

    // Normalise CRLF→LF in the YAML text so the YAML parser always sees clean
    // LF-only input regardless of the working-tree line-ending convention.
    let yaml_normalized: std::borrow::Cow<str> = if raw_yaml_bytes.contains('\r') {
        std::borrow::Cow::Owned(raw_yaml_bytes.replace("\r\n", "\n"))
    } else {
        std::borrow::Cow::Borrowed(raw_yaml_bytes)
    };

    let raw: FrontmatterRaw = serde_yaml::from_str(yaml_normalized.as_ref()).unwrap_or_default();

    let data = FrontmatterData {
        title: raw.title,
        description: raw.description,
        // Store the LF-normalised YAML for downstream consumers.
        raw_yaml: yaml_normalized.into_owned(),
    };

    (Some(data), &content[body_start..])
}
