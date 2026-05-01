//! Text-based and semantic search operations over stored document chunks.
//!
//! Provides:
//!
//! - [`search_by_text`] — case-insensitive keyword search via `CozoDB`'s
//!   built-in `str_includes` / `lowercase` functions.
//! - [`search_similar`] — embedding-based semantic search: embeds the query
//!   with [`crate::embed::EmbeddingModel`], then delegates to
//!   [`super::vectors::search_by_vector`] for cosine-similarity ranking.

use std::collections::BTreeMap;

use cozo::DataValue;

use super::store::DataStore;
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;

/// A search result containing the chunk identifier and a content snippet.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    /// Stable SHA-256 chunk identifier.
    pub chunk_id: String,
    /// Identifier of the source this chunk belongs to.
    pub source_id: String,
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
        ?[chunk_id, source_id, path, headings, content]
            := *doc_chunks{ chunk_id, source_id, path, headings, content },
               str_includes(lowercase(content), lowercase($query))
    ";
    let mut params = BTreeMap::new();
    params.insert("query".to_string(), DataValue::Str(query.into()));
    let rows = store.query(script, params)?;
    rows.rows.iter().map(|row| row_to_result(row)).collect()
}

/// Search chunks by embedding-based semantic similarity.
///
/// Embeds `query_text` using `model`, then retrieves the `limit` most
/// similar stored chunks via cosine similarity over `doc_vectors`.
///
/// Returns an empty [`Vec`] when no vectors have been stored yet.
///
/// # Errors
///
/// Returns [`GraphtorError::Embed`] if query embedding fails, or
/// [`GraphtorError::Database`] on vector lookup failure.
pub fn search_similar(
    store: &DataStore,
    model: &EmbeddingModel,
    query_text: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, GraphtorError> {
    let query_vec = crate::embed::embed_text(model, query_text)?;
    super::vectors::search_by_vector(store, &query_vec, limit)
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_result(row: &[DataValue]) -> Result<SearchResult, GraphtorError> {
    let chunk_id = require_str(row, 0, "chunk_id")?;
    let source_id = require_str(row, 1, "source_id")?;
    let path = require_str(row, 2, "path")?;
    let headings_json = require_str(row, 3, "headings")?;
    let heading_hierarchy: Vec<String> =
        serde_json::from_str(&headings_json).map_err(|e| GraphtorError::Database {
            message: e.to_string(),
            operation: "deserialize_headings".to_string(),
        })?;
    let content = require_str(row, 4, "content")?;
    Ok(SearchResult {
        chunk_id,
        source_id,
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
