//! Code block extraction from an [`AstNode`] stream.
//!
//! [`extract`] collects every [`AstNode::CodeBlock`] into a [`CodeSnippet`]
//! record associated with a parent `chunk_id`.

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{AstNode, CodeSnippet};

/// Extract all code blocks from `nodes` as [`CodeSnippet`] records.
///
/// Each snippet receives a stable SHA-256 `id` derived from its content and
/// the parent `chunk_id`, and a back-reference to that `chunk_id`.
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
            let id = generate_chunk_id(content, chunk_id)?;
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
