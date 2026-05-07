//! HNSW vector search via the native `CozoDB` index on `doc_chunks`.
//!
//! Stores 384-dimensional embeddings produced by `all-MiniLM-L6-v2` directly
//! in the `doc_chunks` relation as the `embedding: <F32; 384>?` column and
//! exposes approximate nearest-neighbour lookup via the `doc_chunks:embedding_idx`
//! HNSW index maintained automatically by `CozoDB`.
//!
//! # Storage
//!
//! [`upsert_vector`] performs a join-put: it reads the existing `doc_chunks`
//! row and writes it back with the new embedding.  The HNSW index is updated
//! automatically on every `:put`.  The chunk **must** already exist in
//! `doc_chunks` before `upsert_vector` is called.
//!
//! # Search
//!
//! [`search_by_vector`] issues a tilde-query against the HNSW index and joins
//! the results with `doc_chunks` metadata.  Approximate k-nearest-neighbour
//! lookup scales to millions of vectors with sub-millisecond latency.

use std::collections::BTreeMap;

use cozo::{DataValue, Num};
use tracing::debug;

use super::{search::SearchResult, store::DataStore};
use crate::error::GraphtorError;

// ── Write operations ──────────────────────────────────────────────────────────

/// Persist or update the embedding for an existing chunk.
///
/// Reads the current `doc_chunks` row for `chunk_id` and writes it back with
/// the supplied `embedding`.  The HNSW index is updated automatically.
///
/// The chunk **must** already exist in `doc_chunks` — call
/// [`crate::db::upsert_chunk`] before calling this function.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] if the chunk does not exist or on any
/// query or mutation failure.
pub fn upsert_vector(
    store: &DataStore,
    chunk_id: &str,
    embedding: &[f32],
) -> Result<(), GraphtorError> {
    let floats: Vec<DataValue> = embedding
        .iter()
        .map(|&x| DataValue::Num(Num::Float(f64::from(x))))
        .collect();
    let emb_val = DataValue::List(floats);

    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
            := *doc_chunks{ chunk_id, source_id, path, title, position, char_offset, headings, content },
               chunk_id = $chunk_id,
               embedding = vec($emb)
        :put doc_chunks {
            chunk_id => source_id, path, title, position, char_offset, headings, content, embedding
        }
    ";
    let mut params = BTreeMap::new();
    params.insert("chunk_id".to_string(), DataValue::Str(chunk_id.into()));
    params.insert("emb".to_string(), emb_val);
    store.mutate(script, params)?;
    debug!(chunk_id, "upserted embedding into doc_chunks");
    Ok(())
}

/// Null out the stored embeddings for a slice of chunk IDs.
///
/// Sets the `embedding` column to `null` for each matching `doc_chunks` row.
/// Silently skips chunk IDs that do not exist.  Returns the number of
/// operations attempted (one per entry in `chunk_ids`).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on mutation failure.
pub fn delete_vectors_by_chunk_ids(
    store: &DataStore,
    chunk_ids: &[String],
) -> Result<usize, GraphtorError> {
    if chunk_ids.is_empty() {
        return Ok(0);
    }
    let mut deleted = 0_usize;
    for chunk_id in chunk_ids {
        let script = r"
            ?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
                := *doc_chunks{ chunk_id, source_id, path, title, position, char_offset, headings, content },
                   chunk_id = $id,
                   embedding = null
            :put doc_chunks {
                chunk_id => source_id, path, title, position, char_offset, headings, content, embedding
            }
        ";
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), DataValue::Str(chunk_id.as_str().into()));
        store.mutate(script, params)?;
        deleted += 1;
    }
    debug!(count = deleted, "nulled embeddings in doc_chunks");
    Ok(deleted)
}

// ── Read operations ───────────────────────────────────────────────────────────

/// Return the stored embedding for a chunk, or `None` if not found.
///
/// Returns `None` when the chunk does not exist or its `embedding` column is
/// `null` (not yet embedded).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialisation failure.
pub fn get_vector(store: &DataStore, chunk_id: &str) -> Result<Option<Vec<f32>>, GraphtorError> {
    let script = r"
        ?[embedding] := *doc_chunks{ chunk_id, embedding }, chunk_id = $id
    ";
    let mut params = BTreeMap::new();
    params.insert("id".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;

    let Some(row) = rows.rows.into_iter().next() else {
        return Ok(None);
    };
    let Some(val) = row.into_iter().next() else {
        return Ok(None);
    };
    match val {
        DataValue::Vec(cozo::Vector::F32(arr)) => Ok(Some(arr.to_vec())),
        DataValue::Null => Ok(None),
        _ => Err(GraphtorError::Database {
            message: "unexpected embedding type in doc_chunks".to_string(),
            operation: "get_vector".to_string(),
        }),
    }
}

/// Search for the `limit` most similar stored chunks to `query_vec` using the
/// `CozoDB` HNSW index on `doc_chunks`.
///
/// Issues a tilde-query against `doc_chunks:embedding_idx` and joins results
/// with chunk metadata.  Returns an empty [`Vec`] when `limit == 0` or no
/// embeddings are indexed.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialisation failure.
pub fn search_by_vector(
    store: &DataStore,
    query_vec: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>, GraphtorError> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let floats: Vec<DataValue> = query_vec
        .iter()
        .map(|&x| DataValue::Num(Num::Float(f64::from(x))))
        .collect();
    let query_val = DataValue::List(floats);

    let limit_i64 = i64::try_from(limit).map_err(|_| GraphtorError::Database {
        message: format!("limit {limit} is out of i64 range"),
        operation: "search_by_vector".to_string(),
    })?;

    let script = format!(
        "?[chunk_id, source_id, path, headings, content] \
         := q = vec($query), \
            ~doc_chunks:embedding_idx{{ chunk_id | query: q, k: {limit_i64}, ef: 50 }}, \
            *doc_chunks{{ chunk_id, source_id, path, headings, content }}"
    );
    let mut params = BTreeMap::new();
    params.insert("query".to_string(), query_val);
    let rows = store.query(&script, params)?;

    rows.rows
        .iter()
        .map(|row| decode_search_result_row(row))
        .collect()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Decode a projected `doc_chunks` row as a [`SearchResult`].
fn decode_search_result_row(row: &[DataValue]) -> Result<SearchResult, GraphtorError> {
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

/// Extract a required `String` from a `DataValue` row at `idx`.
fn require_str(row: &[DataValue], idx: usize, field: &str) -> Result<String, GraphtorError> {
    row.get(idx)
        .and_then(DataValue::get_str)
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Database {
            message: format!("missing or non-string field '{field}' at column {idx}"),
            operation: "row_decode".to_string(),
        })
}
