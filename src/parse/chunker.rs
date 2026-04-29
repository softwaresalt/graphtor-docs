//! Heading-based document chunker — splits an [`AstNode`] stream at H1, H2,
//! and H3 boundaries into self-contained [`Chunk`] records.
//!
//! See [`chunk`] for the public entry point.

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{AstNode, Chunk};

/// Split an ordered list of [`AstNode`]s into [`Chunk`]s.
///
/// Splitting rules:
/// - Content before the first H1/H2/H3 becomes an **intro chunk** with an
///   empty `heading_hierarchy`.
/// - Each H1, H2, or H3 heading starts a new chunk. Its heading and all
///   content until the next H1/H2/H3 (or end of document) belong to that chunk.
/// - H4–H6 headings and their content remain inside the enclosing H1/H2/H3
///   chunk.
/// - The `source_path` is attached to every chunk as provenance.
///
/// # Errors
///
/// Returns [`GraphtorError`] if chunk ID generation fails (e.g. empty content).
///
/// # Panics
///
/// Does not panic.
pub fn chunk(nodes: &[AstNode], source_path: &str) -> Result<Vec<Chunk>, GraphtorError> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current_lines: Vec<String> = Vec::new();
    let mut heading_hierarchy: Vec<String> = Vec::new();
    let mut char_offset: usize = 0;

    for node in nodes {
        match node {
            AstNode::Heading { level, text } if *level <= 3 => {
                // Flush the current accumulated block before starting a new chunk.
                let flushed = flush_chunk(
                    &current_lines,
                    heading_hierarchy.clone(),
                    char_offset,
                    chunks.len(),
                    source_path,
                )?;
                if let Some(c) = flushed {
                    let advance: usize = current_lines.iter().map(|l| l.len() + 1).sum();
                    char_offset += advance;
                    chunks.push(c);
                }
                current_lines.clear();

                // Update hierarchy.
                match level {
                    1 => {
                        heading_hierarchy.clear();
                        heading_hierarchy.push(text.clone());
                    }
                    2 => {
                        heading_hierarchy.truncate(1);
                        heading_hierarchy.push(text.clone());
                    }
                    _ => {
                        heading_hierarchy.truncate(2);
                        heading_hierarchy.push(text.clone());
                    }
                }

                let hashes = "#".repeat(*level as usize);
                current_lines.push(format!("{hashes} {text}"));
            }
            AstNode::Heading { level, text } => {
                // H4–H6: fold into the current chunk.
                let hashes = "#".repeat(*level as usize);
                current_lines.push(format!("{hashes} {text}"));
            }
            AstNode::Link { url, text } => {
                current_lines.push(format!("[{text}]({url})"));
            }
            AstNode::CodeBlock { language, content } => {
                let lang = language.as_deref().unwrap_or("");
                current_lines.push(format!("```{lang}\n{content}\n```"));
            }
            AstNode::Paragraph { text } => {
                current_lines.push(text.clone());
            }
        }
    }

    // Flush the final chunk.
    if let Some(c) = flush_chunk(
        &current_lines,
        heading_hierarchy,
        char_offset,
        chunks.len(),
        source_path,
    )? {
        chunks.push(c);
    }

    Ok(chunks)
}

/// Build a [`Chunk`] from accumulated lines, or return `None` if empty.
fn flush_chunk(
    lines: &[String],
    heading_hierarchy: Vec<String>,
    char_offset: usize,
    position: usize,
    source_path: &str,
) -> Result<Option<Chunk>, GraphtorError> {
    let content = lines.join("\n").trim_end().to_string();
    if content.is_empty() {
        return Ok(None);
    }
    let chunk_id = generate_chunk_id(&content, source_path)?;
    Ok(Some(Chunk {
        chunk_id,
        content,
        heading_hierarchy,
        position,
        char_offset,
        source_path: source_path.to_string(),
    }))
}
