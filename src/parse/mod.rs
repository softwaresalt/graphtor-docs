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

use std::path::Path;

pub mod ast;
pub mod chunker;
pub mod code;
pub mod docx;
pub mod frontmatter;
pub mod links;
pub mod pdf;
pub mod types;

pub use ast::parse_ast;
pub use docx::parse_docx_document;
pub use pdf::parse_pdf_document;
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

/// Normalize a file extension to the parser's canonical form.
///
/// Currently `.markdown` is treated the same as `.md`.
#[must_use]
pub(crate) fn normalized_document_extension(path: &Path) -> Option<String> {
    let raw = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?;

    if raw == "markdown" {
        Some("md".to_string())
    } else {
        Some(raw)
    }
}

/// Return `true` when `ext` is one of the parser-supported document formats.
#[must_use]
pub(crate) fn is_supported_document_extension(ext: &str) -> bool {
    matches!(ext, "md" | "pdf" | "docx")
}

/// Parse a document file by dispatching to the correct format parser.
///
/// `file` is the on-disk path and `source_path` is the source-relative path
/// recorded in the parsed document.
///
/// # Errors
///
/// Returns [`GraphtorError::Parse`] if the file cannot be read, uses an
/// unsupported extension, or fails to parse.
pub fn parse_file(file: &Path, source_path: &str) -> Result<ParsedDocument, GraphtorError> {
    let Some(ext) = normalized_document_extension(file) else {
        return Err(GraphtorError::Parse {
            message: "file has no supported extension".to_string(),
            path: Some(file.to_path_buf()),
        });
    };

    match ext.as_str() {
        "md" => {
            let content = std::fs::read_to_string(file).map_err(|error| GraphtorError::Parse {
                message: format!("failed to read file: {error}"),
                path: Some(file.to_path_buf()),
            })?;

            parse_document(&content, source_path).map_err(|error| GraphtorError::Parse {
                message: format!("markdown parse failed: {error}"),
                path: Some(file.to_path_buf()),
            })
        }
        "pdf" => {
            let bytes = std::fs::read(file).map_err(|error| GraphtorError::Parse {
                message: format!("failed to read file: {error}"),
                path: Some(file.to_path_buf()),
            })?;

            let parse_path = source_path.to_string();
            let panic_path = source_path.to_string();
            std::panic::catch_unwind(move || parse_pdf_document(&bytes, &parse_path))
                .unwrap_or_else(|_| {
                    Err(GraphtorError::Parse {
                        message: "pdf-extract panicked (malformed or unsupported PDF content; \
                                  likely an empty glyph array or corrupted font table)"
                            .to_string(),
                        path: Some(panic_path.into()),
                    })
                })
        }
        "docx" => {
            let bytes = std::fs::read(file).map_err(|error| GraphtorError::Parse {
                message: format!("failed to read file: {error}"),
                path: Some(file.to_path_buf()),
            })?;

            parse_docx_document(&bytes, source_path)
        }
        _ => Err(GraphtorError::Parse {
            message: format!("unsupported file extension '{ext}'"),
            path: Some(file.to_path_buf()),
        }),
    }
}
