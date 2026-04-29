//! Edge and code-snippet storage operations.
//!
//! Manages two stored relations:
//!
//! - `doc_edges` — directed hyperlink edges between chunks
//! - `doc_code` — code snippets extracted from document chunks

use std::collections::BTreeMap;

use cozo::DataValue;
use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;
use crate::parse::types::{CodeSnippet, Reference};

/// A stored hyperlink edge from one chunk to a document path.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRecord {
    /// Identifier of the source chunk containing the link.
    pub src_chunk_id: String,
    /// Target document path (may be relative or absolute within the corpus).
    pub target_path: String,
    /// Human-readable link text.
    pub link_text: String,
    /// Optional anchor fragment (e.g. `"#heading-id"`).
    pub anchor: Option<String>,
}

/// A stored code snippet extracted from a chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRecord {
    /// Stable identifier for this snippet (SHA-256 of content + `chunk_id`).
    pub snippet_id: String,
    /// Identifier of the parent chunk.
    pub chunk_id: String,
    /// Programming language tag, if detected.
    pub language: Option<String>,
    /// Raw code content.
    pub content: String,
}

/// Upsert a hyperlink edge from a parsed [`Reference`].
///
/// Replaces any existing record with the same `(src_chunk_id, target_path)` key.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_edge(store: &DataStore, reference: &Reference) -> Result<(), GraphtorError> {
    let script = r"
        ?[src_chunk_id, target_path, link_text, anchor]
            <- [[$src, $target, $link_text, $anchor]]
        :put doc_edges { src_chunk_id, target_path => link_text, anchor }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "src".to_string(),
        DataValue::Str(reference.source_chunk_id.as_str().into()),
    );
    params.insert(
        "target".to_string(),
        DataValue::Str(reference.target_path.as_str().into()),
    );
    params.insert(
        "link_text".to_string(),
        DataValue::Str(reference.link_text.as_str().into()),
    );
    params.insert("anchor".to_string(), opt_str(reference.anchor.as_deref()));
    store.mutate(script, params)?;
    debug!(
        src = %reference.source_chunk_id,
        target = %reference.target_path,
        "upserted doc_edges record"
    );
    Ok(())
}

/// List all outgoing edges from the given chunk.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or row-decode failure.
pub fn list_edges_from_chunk(
    store: &DataStore,
    src_chunk_id: &str,
) -> Result<Vec<EdgeRecord>, GraphtorError> {
    let script = r"
        ?[src_chunk_id, target_path, link_text, anchor]
            := *doc_edges{ src_chunk_id, target_path, link_text, anchor },
               src_chunk_id = $src
    ";
    let mut params = BTreeMap::new();
    params.insert("src".to_string(), DataValue::Str(src_chunk_id.into()));
    let rows = store.query(script, params)?;
    rows.rows.iter().map(|row| row_to_edge(row)).collect()
}

/// Upsert a code snippet from a parsed [`CodeSnippet`].
///
/// Replaces any existing record with the same `snippet_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_code_snippet(store: &DataStore, snippet: &CodeSnippet) -> Result<(), GraphtorError> {
    let script = r"
        ?[snippet_id, chunk_id, language, content]
            <- [[$snippet_id, $chunk_id, $language, $content]]
        :put doc_code { snippet_id => chunk_id, language, content }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "snippet_id".to_string(),
        DataValue::Str(snippet.id.as_str().into()),
    );
    params.insert(
        "chunk_id".to_string(),
        DataValue::Str(snippet.chunk_id.as_str().into()),
    );
    params.insert("language".to_string(), opt_str(snippet.language.as_deref()));
    params.insert(
        "content".to_string(),
        DataValue::Str(snippet.content.as_str().into()),
    );
    store.mutate(script, params)?;
    debug!(snippet_id = %snippet.id, "upserted doc_code record");
    Ok(())
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_edge(row: &[DataValue]) -> Result<EdgeRecord, GraphtorError> {
    let src_chunk_id = require_str(row, 0, "src_chunk_id")?;
    let target_path = require_str(row, 1, "target_path")?;
    let link_text = require_str(row, 2, "link_text")?;
    let anchor = opt_col_str(row, 3);
    Ok(EdgeRecord {
        src_chunk_id,
        target_path,
        link_text,
        anchor,
    })
}

// ── Value helpers ─────────────────────────────────────────────────────────────

fn opt_str(v: Option<&str>) -> DataValue {
    match v {
        Some(s) => DataValue::Str(s.into()),
        None => DataValue::Null,
    }
}

fn require_str(row: &[DataValue], idx: usize, field: &str) -> Result<String, GraphtorError> {
    row.get(idx)
        .and_then(|v| v.get_str())
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Database {
            message: format!("missing or non-string field '{field}' at column {idx}"),
            operation: "row_decode".to_string(),
        })
}

fn opt_col_str(row: &[DataValue], idx: usize) -> Option<String> {
    row.get(idx).and_then(|v| v.get_str()).map(str::to_owned)
}
