//! DOCX document parsing pipeline.
//!
//! Converts raw DOCX bytes into a [`ParsedDocument`] by:
//! 1. Deserialising the DOCX archive with `docx-rs`.
//! 2. Walking `Document.children`, extracting text from `Paragraph` nodes.
//! 3. Splitting at `Heading1`–`Heading3` paragraph-style boundaries.
//! 4. Producing one [`Chunk`] per section, with a stable SHA-256 chunk ID.
//!
//! # Limitations
//!
//! - Tables and inline images are skipped (plain-text extraction only).
//! - `references` and `code_snippets` are always empty — DOCX rendering does
//!   not preserve hyperlink or code-block structure deterministically.

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{Chunk, ParsedDocument};

/// Parse raw DOCX bytes into a fully assembled [`ParsedDocument`].
///
/// # Errors
///
/// Returns [`GraphtorError::Parse`] if:
/// - `bytes` is not a valid DOCX archive.
/// - Chunk ID generation fails.
pub fn parse_docx_document(
    bytes: &[u8],
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    let docx = docx_rs::read_docx(bytes).map_err(|e| GraphtorError::Parse {
        message: format!("docx parse failed: {e:?}"),
        path: Some(source_path.into()),
    })?;

    let chunks = chunk_docx(&docx.document, source_path)?;
    let title = extract_docx_title(&docx.document);

    Ok(ParsedDocument {
        path: source_path.to_string(),
        title,
        frontmatter: None,
        chunks,
        references: Vec::new(),
        code_snippets: Vec::new(),
    })
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Walk the document body and split into [`Chunk`]s at heading boundaries.
///
/// Text appearing before the first heading is collected into an intro chunk
/// with an empty heading hierarchy (consistent with the Markdown chunker).
fn chunk_docx(
    document: &docx_rs::Document,
    source_path: &str,
) -> Result<Vec<Chunk>, GraphtorError> {
    // Each pending section is (heading_hierarchy, accumulated_text).
    let mut sections: Vec<(Vec<String>, String)> = Vec::new();
    let mut current_headings: Vec<String> = Vec::new();
    let mut current_text = String::new();

    for child in &document.children {
        let docx_rs::DocumentChild::Paragraph(para) = child else {
            continue;
        };

        let text = extract_para_text(para);
        let heading_level = style_to_heading_level(para);

        if let Some(level) = heading_level {
            // Flush current section before starting new heading.
            if !current_text.trim().is_empty() {
                sections.push((current_headings.clone(), current_text.trim().to_string()));
            }
            current_headings = update_heading_hierarchy(&current_headings, &text, level);
            current_text = String::new();
        } else if !text.is_empty() {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(&text);
        }
    }

    // Flush the last pending section.
    if !current_text.trim().is_empty() {
        sections.push((current_headings, current_text.trim().to_string()));
    }

    // Convert sections to Chunks.
    let mut chunks = Vec::new();
    for (position, (headings, content)) in sections.into_iter().enumerate() {
        if content.is_empty() {
            continue;
        }
        let chunk_id = generate_chunk_id(&content, source_path)?;
        let content_len = content.len();
        let char_offset: usize = chunks.iter().map(|c: &Chunk| c.content.len()).sum();
        chunks.push(Chunk {
            chunk_id,
            content,
            heading_hierarchy: headings,
            position,
            char_offset,
            source_path: source_path.to_string(),
        });
        let _ = content_len;
    }

    Ok(chunks)
}

/// Extract the plain-text content of a paragraph by concatenating its runs.
fn extract_para_text(para: &docx_rs::Paragraph) -> String {
    let mut text = String::new();
    for child in &para.children {
        let docx_rs::ParagraphChild::Run(run) = child else {
            continue;
        };
        for run_child in &run.children {
            if let docx_rs::RunChild::Text(t) = run_child {
                text.push_str(&t.text);
            }
        }
    }
    text
}

/// Map a DOCX paragraph style name to a heading level (1–3), or `None` for body text.
///
/// DOCX heading styles are conventionally named `"Heading1"`, `"Heading2"`, etc.
/// This function accepts the normalised (whitespace-stripped) style value.
fn style_to_heading_level(para: &docx_rs::Paragraph) -> Option<usize> {
    let style_val = para.property.style.as_ref().map(|s| s.val.as_str())?;
    match style_val {
        "Heading1" | "heading1" | "Heading 1" => Some(1),
        "Heading2" | "heading2" | "Heading 2" => Some(2),
        "Heading3" | "heading3" | "Heading 3" => Some(3),
        _ => None,
    }
}

/// Update the heading hierarchy stack when a new heading is encountered.
///
/// Truncates the stack to `level - 1` entries and appends `text`, so the
/// hierarchy always reflects the current nesting depth.
fn update_heading_hierarchy(current: &[String], text: &str, level: usize) -> Vec<String> {
    let depth = level.saturating_sub(1);
    let mut updated: Vec<String> = current.iter().take(depth).cloned().collect();
    if !text.is_empty() {
        updated.push(text.to_string());
    }
    updated
}

/// Extract a document title from the first heading or non-empty paragraph.
///
/// Returns `None` if the document is empty.
fn extract_docx_title(document: &docx_rs::Document) -> Option<String> {
    for child in &document.children {
        let docx_rs::DocumentChild::Paragraph(para) = child else {
            continue;
        };
        let text = extract_para_text(para);
        let trimmed = text.trim().to_string();
        if trimmed.len() > 3 {
            return Some(trimmed);
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{style_to_heading_level, update_heading_hierarchy};

    // Construct a minimal Paragraph with a given style for testing.
    fn para_with_style(style_val: &str) -> docx_rs::Paragraph {
        let mut para = docx_rs::Paragraph::new();
        para.property.style = Some(docx_rs::ParagraphStyle::new(Some(style_val)));
        para
    }

    fn para_no_style() -> docx_rs::Paragraph {
        docx_rs::Paragraph::new()
    }

    // ── style_to_heading_level ───────────────────────────────────────────────

    #[test]
    fn heading1_maps_to_level_1() {
        let para = para_with_style("Heading1");
        assert_eq!(style_to_heading_level(&para), Some(1));
    }

    #[test]
    fn heading2_maps_to_level_2() {
        let para = para_with_style("Heading2");
        assert_eq!(style_to_heading_level(&para), Some(2));
    }

    #[test]
    fn heading3_maps_to_level_3() {
        let para = para_with_style("Heading3");
        assert_eq!(style_to_heading_level(&para), Some(3));
    }

    #[test]
    fn normal_style_maps_to_none() {
        let para = para_with_style("Normal");
        assert_eq!(style_to_heading_level(&para), None);
    }

    #[test]
    fn no_style_maps_to_none() {
        let para = para_no_style();
        assert_eq!(style_to_heading_level(&para), None);
    }

    // ── update_heading_hierarchy ─────────────────────────────────────────────

    #[test]
    fn h1_resets_hierarchy_to_single_entry() {
        let current = vec!["Old H1".to_string(), "Old H2".to_string()];
        let updated = update_heading_hierarchy(&current, "New H1", 1);
        assert_eq!(updated, vec!["New H1".to_string()]);
    }

    #[test]
    fn h2_appends_under_existing_h1() {
        let current = vec!["Chapter 1".to_string()];
        let updated = update_heading_hierarchy(&current, "Section 1.1", 2);
        assert_eq!(
            updated,
            vec!["Chapter 1".to_string(), "Section 1.1".to_string()]
        );
    }

    #[test]
    fn h3_appends_under_h1_and_h2() {
        let current = vec!["Chapter 1".to_string(), "Section 1.1".to_string()];
        let updated = update_heading_hierarchy(&current, "Subsection 1.1.1", 3);
        assert_eq!(
            updated,
            vec![
                "Chapter 1".to_string(),
                "Section 1.1".to_string(),
                "Subsection 1.1.1".to_string()
            ]
        );
    }

    #[test]
    fn h2_after_h3_truncates_stack_correctly() {
        // After a H3 we jump back to H2 — hierarchy must be truncated.
        let current = vec!["Chap".to_string(), "Sec".to_string(), "Subsec".to_string()];
        let updated = update_heading_hierarchy(&current, "New Sec", 2);
        assert_eq!(updated, vec!["Chap".to_string(), "New Sec".to_string()]);
    }

    #[test]
    fn empty_text_does_not_add_to_hierarchy() {
        let current = vec!["H1".to_string()];
        let updated = update_heading_hierarchy(&current, "", 2);
        assert_eq!(updated, vec!["H1".to_string()]);
    }

    #[test]
    fn initial_h1_with_empty_current_produces_single_entry() {
        let updated = update_heading_hierarchy(&[], "Introduction", 1);
        assert_eq!(updated, vec!["Introduction".to_string()]);
    }
}
