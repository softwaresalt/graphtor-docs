//! `parse` module — Markdown parsing, chunking, and extraction pipeline.
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
pub mod frontmatter;
pub mod links;
pub mod types;

pub use ast::parse_ast;
pub use types::{AstNode, Chunk, CodeSnippet, FrontmatterData, ParsedDocument, Reference};

use crate::error::GraphtorError;

/// Parse a raw Markdown document into a fully assembled [`ParsedDocument`].
///
/// This is the primary entry point for the parsing pipeline. It:
///
/// 1. Strips YAML frontmatter (if present) and extracts metadata.
/// 2. Walks the Markdown body with `pulldown-cmark` to produce an
///    [`AstNode`] stream.
/// 3. Splits the node stream into [`Chunk`]s at H2/H3 boundaries, namespaced
///    by `source_id` to prevent cross-source chunk ID collisions.
/// 4. Extracts [`Reference`]s and [`CodeSnippet`]s per chunk.
/// 5. Assembles everything into a [`ParsedDocument`].
///
/// # Parameters
///
/// - `content` — raw file content (UTF-8).
/// - `source_id` — source registry identifier (from `sources.yaml`).
/// - `source_path` — canonical source-relative path recorded in parsed document
///   provenance.  For runtime ingestion use [`parse_file`], which derives this
///   value from the validated docline contract rather than accepting it from
///   the caller.
///
/// # Errors
///
/// Returns [`GraphtorError`] if chunk or snippet ID generation fails.
///
/// # Panics
///
/// Does not panic.
pub fn parse_document(
    content: &str,
    source_id: &str,
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    // Step 1 — frontmatter.
    let (fm, body) = frontmatter::strip(content);

    // Step 2 — AST walk.
    let nodes = ast::parse_ast(body);

    // Step 3 — heading-based chunking with source namespace.
    let chunks = chunker::chunk(&nodes, source_id, source_path)?;

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
/// `.markdown` is treated the same as `.md`.
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

/// Return `true` when `ext` is a parser-supported document format.
///
/// Only Markdown (`.md`) is supported after the docline pivot.
#[must_use]
pub(crate) fn is_supported_document_extension(ext: &str) -> bool {
    matches!(ext, "md")
}

/// Parse a document file by dispatching to the Markdown parser.
///
/// `file` is the on-disk path and `source_id` is the source registry identifier.
/// The canonical document path (`source_path`) is **not** accepted from the caller
/// — it is derived from the validated docline v1 frontmatter contract embedded in
/// the file.  This enforces the canonical identity model: the document's logical
/// identity is `{source_id, contract.source_path}`, independent of where the file
/// lives on the filesystem.
///
/// # Contract enforcement
///
/// All Markdown files must carry a valid docline v1 frontmatter block. The
/// following conditions cause a [`GraphtorError::Contract`] failure (fail-closed):
///
/// - No frontmatter present (missing `---` delimiter)
/// - Malformed YAML in the frontmatter block
/// - Missing required contract fields (`title`, `source`, `ingested_at`,
///   `doc_type`, `source_path`)
/// - Unsupported `schema_version` major component (only major `1` is accepted)
/// - `content_sha256` present but does not match the SHA-256 of the body
/// - `source_path` empty, absolute, or resolves to empty after normalisation
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] for any contract validation failure.
/// Returns [`GraphtorError::Parse`] if the file cannot be read or uses an
/// unsupported extension.
pub fn parse_file(file: &Path, source_id: &str) -> Result<ParsedDocument, GraphtorError> {
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

            parse_markdown_with_contract(&content, source_id).map_err(|error| {
                GraphtorError::Parse {
                    message: format!("Markdown contract validation failed: {error}"),
                    path: Some(file.to_path_buf()),
                }
            })
        }
        _ => Err(GraphtorError::Parse {
            message: format!("unsupported file extension '{ext}'; only .md is accepted"),
            path: Some(file.to_path_buf()),
        }),
    }
}

/// Parse a Markdown string with mandatory docline v1 contract validation.
///
/// This is the runtime ingestion entry point for Markdown content.  Callers
/// that need permissive parsing for tests or tooling should use
/// [`parse_document`] instead.
///
/// `source_id` is used to namespace chunk identifiers.  The canonical
/// `source_path` is taken from the validated frontmatter, not from the caller.
///
/// # Errors
///
/// Returns [`GraphtorError::Contract`] for any contract violation.
/// Returns [`GraphtorError::Parse`] for chunk/snippet generation failures.
pub fn parse_markdown_with_contract(
    content: &str,
    source_id: &str,
) -> Result<ParsedDocument, GraphtorError> {
    use crate::ingest_contract;

    // Step 1 — extract frontmatter block.
    let (fm_data, body) = frontmatter::strip(content);
    let raw_yaml = match &fm_data {
        Some(fm) => fm.raw_yaml.as_str(),
        None => {
            return Err(GraphtorError::Contract {
                message: "markdown file has no frontmatter — docline v1 contract is required for \
                          runtime ingestion; documents must be produced by docline and carry a \
                          valid ---...--- YAML block"
                    .to_string(),
                field: None,
            });
        }
    };

    // Step 2 — validate against the docline v1 contract (fail-closed).
    let validated = ingest_contract::validate(raw_yaml, body)?;

    // Step 3 — build ParsedDocument using the validated contract fields as
    //           the authoritative source of the document's identity.
    let source_path = &validated.source_path;

    let nodes = ast::parse_ast(body);
    let chunks = chunker::chunk(&nodes, source_id, source_path)?;

    let mut all_references: Vec<Reference> = Vec::new();
    let mut all_snippets: Vec<CodeSnippet> = Vec::new();
    for c in &chunks {
        let chunk_nodes = ast::parse_ast(&c.content);
        let refs = links::extract(&chunk_nodes, &c.chunk_id);
        let snips = code::extract(&chunk_nodes, &c.chunk_id)?;
        all_references.extend(refs);
        all_snippets.extend(snips);
    }

    // Build a FrontmatterData from the validated fields so downstream code
    // that inspects `doc.frontmatter` still works correctly.
    let frontmatter_data = Some(FrontmatterData {
        title: Some(validated.title.clone()),
        description: if validated.description.is_empty() {
            None
        } else {
            Some(validated.description.clone())
        },
        canonical_url: validated.canonical_url.clone(),
        raw_yaml: raw_yaml.to_string(),
    });

    Ok(ParsedDocument {
        path: source_path.clone(),
        title: Some(validated.title),
        frontmatter: frontmatter_data,
        chunks,
        references: all_references,
        code_snippets: all_snippets,
    })
}
