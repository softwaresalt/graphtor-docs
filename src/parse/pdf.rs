//! PDF document parsing pipeline.
//!
//! Converts raw PDF byte content into a [`ParsedDocument`] using
//! `pdf-extract` for text extraction followed by page-based chunking.
//!
//! # Pipeline
//!
//! ```text
//! raw PDF bytes
//!     └─ pdf_extract::extract_text_from_mem  — text extraction per page
//!     └─ chunk_pdf_text                      — split at page / paragraph boundaries → Chunks
//!     └─ extract_title                       — first meaningful text line → title
//!     └─ ParsedDocument                      — assembled output
//! ```
//!
//! Graph link types (`references`, `code_snippets`) are not produced for
//! PDF sources — structure cannot be recovered deterministically from
//! rendered text output.

use std::path::Path;

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{Chunk, ParsedDocument};

/// Maximum characters in a single chunk before splitting at paragraph boundaries.
const MAX_CHUNK_CHARS: usize = 2_000;

/// Parse raw PDF bytes into a fully assembled [`ParsedDocument`].
///
/// Extracts text using `pdf-extract`, splits at page (`\x0c` form-feed)
/// and paragraph (`\n\n`) boundaries, and assembles the result.
///
/// Graph link types (`references`, `code_snippets`) are empty — PDF
/// rendering does not preserve hyperlink or code-block structure.
///
/// # Errors
///
/// Returns [`GraphtorError::Parse`] if `pdf-extract` fails to decode the
/// bytes as a valid PDF, or if chunk ID generation fails.
pub fn parse_pdf_document(
    bytes: &[u8],
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|e| GraphtorError::Parse {
        message: format!("pdf text extraction failed: {e}"),
        path: Some(source_path.into()),
    })?;

    let chunks = chunk_pdf_text(&text, source_path)?;
    let title = extract_title(&text, source_path);

    Ok(ParsedDocument {
        path: source_path.to_string(),
        title,
        frontmatter: None,
        chunks,
        references: Vec::new(),
        code_snippets: Vec::new(),
    })
}

/// Split extracted PDF text into [`Chunk`]s at page and paragraph boundaries.
///
/// Pages are delimited by form-feed (`\x0c`) characters emitted by
/// `pdf-extract`. Pages longer than [`MAX_CHUNK_CHARS`] are further split
/// at double-newline paragraph boundaries. Empty pages are skipped.
fn chunk_pdf_text(text: &str, source_path: &str) -> Result<Vec<Chunk>, GraphtorError> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut position = 0_usize;
    let mut char_offset = 0_usize;

    for (page_idx, page_text) in text.split('\x0c').enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let page_label = format!("Page {}", page_idx + 1);
        let segments = split_long_text(trimmed);

        for segment in segments {
            let chunk_id = generate_chunk_id(&segment, source_path)?;
            let content_len = segment.len();
            chunks.push(Chunk {
                chunk_id,
                content: segment,
                heading_hierarchy: vec![page_label.clone()],
                position,
                char_offset,
                source_path: source_path.to_string(),
            });
            position += 1;
            char_offset += content_len;
        }
    }

    Ok(chunks)
}

/// Split a text segment at paragraph (`\n\n`) boundaries when it exceeds
/// [`MAX_CHUNK_CHARS`].
///
/// Short segments are returned unchanged in a single-element `Vec`.
fn split_long_text(text: &str) -> Vec<String> {
    if text.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in text.split("\n\n") {
        if current.is_empty() {
            current.push_str(para);
        } else if current.len() + 2 + para.len() > MAX_CHUNK_CHARS {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
            current = para.to_string();
        } else {
            current.push_str("\n\n");
            current.push_str(para);
        }
    }

    let tail = current.trim().to_string();
    if !tail.is_empty() {
        segments.push(tail);
    }

    if segments.is_empty() {
        vec![text.to_string()]
    } else {
        segments
    }
}

/// Extract a document title from the first meaningful line of text.
///
/// A "meaningful" line is between 4 and 200 characters (exclusive). Falls
/// back to the file stem of `source_path` when no candidate is found.
fn extract_title(text: &str, source_path: &str) -> Option<String> {
    let candidate = text
        .lines()
        .map(str::trim)
        .find(|line| line.len() > 3 && line.len() < 200);

    candidate.map(String::from).or_else(|| {
        Path::new(source_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    })
}

#[cfg(test)]
mod tests {
    use super::{chunk_pdf_text, extract_title, split_long_text, MAX_CHUNK_CHARS};

    // ── chunk_pdf_text ───────────────────────────────────────────────────────

    #[test]
    fn chunk_empty_text_produces_no_chunks() {
        let result = chunk_pdf_text("", "test.pdf").expect("should not fail on empty text");
        assert!(result.is_empty(), "empty text must produce zero chunks");
    }

    #[test]
    fn chunk_single_page_produces_one_chunk() {
        let result =
            chunk_pdf_text("Hello, world!", "test.pdf").expect("single page should succeed");
        assert_eq!(result.len(), 1, "single page must produce one chunk");
        assert_eq!(result[0].heading_hierarchy, vec!["Page 1"]);
        assert_eq!(result[0].position, 0);
        assert_eq!(result[0].source_path, "test.pdf");
    }

    #[test]
    fn chunk_two_pages_produces_two_chunks() {
        let text = "Page one content\x0cPage two content";
        let result = chunk_pdf_text(text, "two_pages.pdf").expect("two pages should succeed");
        assert_eq!(result.len(), 2, "two pages must produce two chunks");
        assert_eq!(result[0].heading_hierarchy, vec!["Page 1"]);
        assert_eq!(result[1].heading_hierarchy, vec!["Page 2"]);
    }

    #[test]
    fn chunk_trailing_formfeed_produces_no_extra_chunk() {
        let text = "Only page\x0c";
        let result =
            chunk_pdf_text(text, "trailing.pdf").expect("trailing form-feed should succeed");
        assert_eq!(
            result.len(),
            1,
            "trailing form-feed must not produce an empty chunk"
        );
    }

    #[test]
    fn chunk_positions_are_sequential() {
        let text = "Alpha\x0cBeta\x0cGamma";
        let result = chunk_pdf_text(text, "seq.pdf").expect("three pages should succeed");
        assert_eq!(result.len(), 3, "three pages must produce three chunks");
        for (i, chunk) in result.iter().enumerate() {
            assert_eq!(chunk.position, i, "chunk position must equal its index");
        }
    }

    #[test]
    fn chunk_long_page_splits_at_paragraphs() {
        let para_a = "A".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let para_b = "B".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let text = format!("{para_a}\n\n{para_b}");
        let result = chunk_pdf_text(&text, "long.pdf").expect("long page should succeed");
        assert!(
            result.len() >= 2,
            "a long page must be split into at least two chunks"
        );
    }

    #[test]
    fn chunk_ids_are_stable_and_unique() {
        let text = "First page\x0cSecond page";
        let result = chunk_pdf_text(text, "unique.pdf").expect("should succeed");
        assert_eq!(result.len(), 2);
        assert_ne!(
            result[0].chunk_id, result[1].chunk_id,
            "different pages must produce different chunk IDs"
        );
        // IDs are 64-char SHA-256 hex strings.
        for chunk in &result {
            assert_eq!(
                chunk.chunk_id.len(),
                64,
                "chunk ID must be 64 hex characters"
            );
        }
    }

    // ── split_long_text ──────────────────────────────────────────────────────

    #[test]
    fn split_short_input_unchanged() {
        let input = "short text";
        let segs = split_long_text(input);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], input);
    }

    #[test]
    fn split_long_text_at_paragraphs() {
        let para_a = "A".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let para_b = "B".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let input = format!("{para_a}\n\n{para_b}");
        let segs = split_long_text(&input);
        assert!(
            segs.len() >= 2,
            "long text must be split into at least 2 segments"
        );
    }

    // ── extract_title ────────────────────────────────────────────────────────

    #[test]
    fn title_returns_first_meaningful_line() {
        let text = "\n\nDocument Title\nSome other content";
        let title = extract_title(text, "my_doc.pdf");
        assert_eq!(title, Some("Document Title".to_string()));
    }

    #[test]
    fn title_skips_lines_shorter_than_four_chars() {
        let text = "a\nb\nc\nA real title here";
        let title = extract_title(text, "my_doc.pdf");
        assert_eq!(title, Some("A real title here".to_string()));
    }

    #[test]
    fn title_falls_back_to_file_stem_when_no_candidate() {
        // Three lines, each ≤ 3 chars — all skipped.
        let text = "a\nb\nc";
        let title = extract_title(text, "some_document.pdf");
        assert_eq!(title, Some("some_document".to_string()));
    }

    #[test]
    fn title_empty_text_falls_back_to_file_stem() {
        let title = extract_title("", "readme.pdf");
        assert_eq!(title, Some("readme".to_string()));
    }

    #[test]
    fn title_dotfile_returns_filename_as_stem() {
        // Rust's Path::file_stem() treats dotfiles (no extension) as having
        // their full name as the stem — there is no "no stem" case for dotfiles.
        let title = extract_title("", ".hidden");
        assert_eq!(
            title,
            Some(".hidden".to_string()),
            "dotfile full name is its own stem"
        );
    }
}
