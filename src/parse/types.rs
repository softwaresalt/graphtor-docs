//! Canonical data types for the markdown parsing pipeline.
//!
//! These types form the contract between all `parse` submodules.
//! Every stage reads from or writes to these structures.

use serde::{Deserialize, Serialize};

/// A document that has been fully parsed and decomposed into its components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    /// File-system path (relative to the repo root) of the source document.
    pub path: String,
    /// Title extracted from frontmatter, or the first H1 heading if absent.
    pub title: Option<String>,
    /// Structured metadata extracted from YAML frontmatter, if present.
    pub frontmatter: Option<FrontmatterData>,
    /// Ordered list of content chunks split at heading boundaries.
    pub chunks: Vec<Chunk>,
    /// Hyperlink references extracted across all chunks.
    pub references: Vec<Reference>,
    /// Fenced code blocks extracted across all chunks.
    pub code_snippets: Vec<CodeSnippet>,
}

/// A self-contained content chunk split at an H2 or H3 heading boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Stable SHA-256 identifier (`sha256(content + "|" + source_path)`).
    pub chunk_id: String,
    /// Raw markdown content of this chunk (may include sub-headings H4+).
    pub content: String,
    /// Ordered heading ancestry from H1 down to this chunk's own heading.
    /// Empty for the document intro chunk (text before the first heading).
    pub heading_hierarchy: Vec<String>,
    /// Zero-based position of this chunk within the document.
    pub position: usize,
    /// Character offset of the chunk's first byte within the source document.
    pub char_offset: usize,
    /// Path of the source document; mirrors [`ParsedDocument::path`].
    pub source_path: String,
}

/// A hyperlink reference found inside a [`Chunk`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// Identifier of the chunk where the hyperlink appears.
    pub source_chunk_id: String,
    /// Raw link target as written in the source document.
    pub target_path: String,
    /// Visible anchor text of the hyperlink.
    pub link_text: String,
    /// URL fragment (`#section`) if present.
    pub anchor: Option<String>,
}

/// A fenced code block extracted from a [`Chunk`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    /// Stable SHA-256 identifier for this snippet.
    pub id: String,
    /// Identifier of the parent [`Chunk`].
    pub chunk_id: String,
    /// Fenced language tag, e.g. `"rust"` or `"json"`. `None` when absent.
    pub language: Option<String>,
    /// Raw source code content (trailing newline stripped).
    pub content: String,
}

/// YAML frontmatter extracted from the document preamble.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontmatterData {
    /// `title:` field value.
    pub title: Option<String>,
    /// `description:` field value.
    pub description: Option<String>,
    /// Full raw YAML text between the `---` delimiters.
    pub raw_yaml: String,
}

/// A node produced by the low-level AST walker over a markdown event stream.
///
/// Consumers iterate this flat list and fold it into higher-level structures
/// (chunks, references, code snippets).
#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    /// A heading found at `level` (1–6) with concatenated text content.
    Heading {
        /// ATX heading level (1 = H1, 6 = H6).
        level: u32,
        /// Concatenated plain text of the heading.
        text: String,
    },
    /// An inline hyperlink.
    Link {
        /// Link target URL or path.
        url: String,
        /// Concatenated anchor text.
        text: String,
    },
    /// A fenced or indented code block.
    CodeBlock {
        /// Language identifier from the opening fence, if present.
        language: Option<String>,
        /// Raw code content.
        content: String,
    },
    /// A block of non-heading, non-code paragraph text.
    Paragraph {
        /// Plain text content of the paragraph.
        text: String,
    },
}
