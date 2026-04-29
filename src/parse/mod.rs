//! `parse` module — markdown parsing, chunking, and extraction pipeline.
//!
//! This module converts raw `.md` file content into structured
//! [`ParsedDocument`] values ready for the embedding and storage stages.
//!
//! # Pipeline
//!
//! ```text
//! raw markdown
//!     └─ frontmatter::strip   — detect and extract YAML frontmatter
//!     └─ ast::parse_ast       — walk pulldown-cmark event stream → AstNode list
//!     └─ chunker::chunk       — split AstNodes at H2/H3 boundaries → Chunks
//!     └─ links::extract       — extract References from AstNode::Link events
//!     └─ code::extract        — extract CodeSnippets from AstNode::CodeBlock events
//!     └─ ParsedDocument       — assembled output
//! ```

pub mod ast;
pub mod chunker;
pub mod code;
pub mod frontmatter;
pub mod links;
pub mod types;

pub use ast::parse_ast;
pub use types::{AstNode, Chunk, CodeSnippet, FrontmatterData, ParsedDocument, Reference};

use crate::error::GraphtorError;

/// Parse a raw markdown document into a fully assembled [`ParsedDocument`].
///
/// This is the primary entry point for the parsing pipeline. It:
///
/// 1. Strips YAML frontmatter (if present) and extracts metadata.
/// 2. Walks the markdown body with `pulldown-cmark` to produce an
///    [`AstNode`] stream.
/// 3. Splits the node stream into [`Chunk`]s at H2/H3 boundaries.
/// 4. Extracts [`Reference`]s and [`CodeSnippet`]s per chunk.
/// 5. Assembles everything into a [`ParsedDocument`].
///
/// # Errors
///
/// Returns [`GraphtorError`] if chunk or snippet ID generation fails.
///
/// # Panics
///
/// Does not panic.
pub fn parse_document(content: &str, source_path: &str) -> Result<ParsedDocument, GraphtorError> {
    // Step 1 — frontmatter.
    let (fm, body) = frontmatter::strip(content);

    // Step 2 — AST walk.
    let nodes = ast::parse_ast(body);

    // Step 3 — heading-based chunking.
    let chunks = chunker::chunk(&nodes, source_path)?;

    // Step 4 — per-chunk extraction.
    let mut all_references: Vec<Reference> = Vec::new();
    let mut all_snippets: Vec<CodeSnippet> = Vec::new();

    for c in &chunks {
        // Re-parse just this chunk's content to get its node list for extraction.
        let chunk_nodes = ast::parse_ast(&c.content);
        let refs = links::extract(&chunk_nodes, &c.chunk_id);
        let snips = code::extract(&chunk_nodes, &c.chunk_id)?;
        all_references.extend(refs);
        all_snippets.extend(snips);
    }

    // Step 5 — derive document title.
    let title = fm.as_ref().and_then(|f| f.title.clone()).or_else(|| {
        // Fall back to the first H1 from the original body.
        nodes.iter().find_map(|n| {
            if let AstNode::Heading { level: 1, text } = n {
                Some(text.clone())
            } else {
                None
            }
        })
    });

    Ok(ParsedDocument {
        path: source_path.to_string(),
        title,
        frontmatter: fm,
        chunks,
        references: all_references,
        code_snippets: all_snippets,
    })
}
