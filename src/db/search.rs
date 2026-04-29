//! Text-based search operations over stored document chunks.
//!
//! Provides [`search_by_text`], which returns all chunks whose content
//! contains the given query string (case-insensitive). The matching uses
//! `CozoDB`'s built-in [`str_includes`] and [`lowercase`] string functions so
//! the filtering happens inside the database engine.
//!
//! A placeholder [`search_similar`] function is provided for future
//! embedding-based semantic search and currently returns an error indicating
//! that the feature is not yet implemented.

use std::collections::BTreeMap;

use cozo::DataValue;

use super::store::DataStore;
use crate::error::GraphtorError;

/// A search result containing the chunk identifier and a content snippet.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    /// Stable SHA-256 chunk identifier.
    pub chunk_id: String,
    /// Relative document path within the source.
    pub path: String,
    /// Heading hierarchy of the matching chunk, ordered from H1 downward.
    pub heading_hierarchy: Vec<String>,
    /// Content of the matching chunk.
    pub content: String,
}

/// Search chunks by keyword using case-insensitive substring matching.
///
/// Returns all chunks whose content contains `query` (case-insensitive).
/// Results are not ranked; callers may re-rank by relevance if desired.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialization failure.
pub fn search_by_text(store: &DataStore, query: &str) -> Result<Vec<SearchResult>, GraphtorError> {
    let script = r"
        ?[chunk_id, path, headings, content]
            := *doc_chunks{ chunk_id, path, headings, content },
               str_includes(lowercase(content), lowercase($query))
    ";
    let mut params = BTreeMap::new();
    params.insert("query".to_string(), DataValue::Str(query.into()));
    let rows = store.query(script, params)?;
    rows.rows.iter().map(|row| row_to_result(row)).collect()
}

/// Placeholder for embedding-based semantic similarity search.
///
/// This feature is not yet implemented. The function always returns
/// [`GraphtorError::Database`] with a descriptive message.
///
/// # Errors
///
/// Always returns [`GraphtorError::Database`] until implemented.
pub fn search_similar(
    _store: &DataStore,
    _query_text: &str,
    _limit: usize,
) -> Result<Vec<SearchResult>, GraphtorError> {
    Err(GraphtorError::Database {
        message: "semantic similarity search is not yet implemented".to_string(),
        operation: "search_similar".to_string(),
    })
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_result(row: &[DataValue]) -> Result<SearchResult, GraphtorError> {
    let chunk_id = require_str(row, 0, "chunk_id")?;
    let path = require_str(row, 1, "path")?;
    let headings_json = require_str(row, 2, "headings")?;
    let heading_hierarchy: Vec<String> =
        serde_json::from_str(&headings_json).map_err(|e| GraphtorError::Database {
            message: e.to_string(),
            operation: "deserialize_headings".to_string(),
        })?;
    let content = require_str(row, 3, "content")?;
    Ok(SearchResult {
        chunk_id,
        path,
        heading_hierarchy,
        content,
    })
}

// ── Value helpers ─────────────────────────────────────────────────────────────

fn require_str(row: &[DataValue], idx: usize, field: &str) -> Result<String, GraphtorError> {
    row.get(idx)
        .and_then(|v| v.get_str())
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Database {
            message: format!("missing or non-string field '{field}' at column {idx}"),
            operation: "row_decode".to_string(),
        })
}
