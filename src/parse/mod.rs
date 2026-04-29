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
