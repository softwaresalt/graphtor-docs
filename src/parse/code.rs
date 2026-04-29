//! Code block extraction from an [`AstNode`] stream.
//!
//! [`extract`] collects every [`AstNode::CodeBlock`] into a [`CodeSnippet`]
//! record associated with a parent `chunk_id`.

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{AstNode, CodeSnippet};

/// Extract all code blocks from `nodes` as [`CodeSnippet`] records.
///
/// Each snippet receives a stable SHA-256 `id` derived from its language tag,
/// content, and parent `chunk_id`. Using a composite key
/// (`chunk_id + "\0" + language + "\0" + content`) means empty fenced code
/// blocks — valid Markdown — produce a unique, stable ID rather than
/// triggering an error.
///
/// # Errors
///
/// Returns [`GraphtorError`] if snippet ID generation fails.
///
/// # Panics
///
/// Does not panic.
pub fn extract(nodes: &[AstNode], chunk_id: &str) -> Result<Vec<CodeSnippet>, GraphtorError> {
    let mut snippets = Vec::new();
    for node in nodes {
        if let AstNode::CodeBlock { language, content } = node {
            // Composite key: chunk_id + \0 + language + \0 + content.
            // This is always non-empty (chunk_id is a 64-char hex string) and
            // unambiguous even when content is an empty fenced code block.
            let composite_key = format!(
                "{chunk_id}\0{}\0{content}",
                language.as_deref().unwrap_or("")
            );
            let id = generate_chunk_id(&composite_key, chunk_id)?;
            snippets.push(CodeSnippet {
                id,
                chunk_id: chunk_id.to_string(),
                language: language.clone(),
                content: content.clone(),
            });
        }
    }
    Ok(snippets)
}
