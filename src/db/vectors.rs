//! Vector embedding storage and brute-force cosine-similarity search.
//!
//! Stores 384-dimensional embeddings produced by `all-MiniLM-L6-v2` in the
//! `doc_vectors` relation keyed on `chunk_id`, and exposes nearest-neighbour
//! lookup via dot-product similarity computed in Rust.
//!
//! # Storage
//!
//! Embeddings are serialised as compact JSON arrays and stored in the
//! `doc_vectors` relation.  The JSON representation is portable; a future
//! migration to a native `CozoDB` HNSW index requires only a schema change and
//! a re-ingestion pass.
//!
//! # Search
//!
//! [`search_by_vector`] loads all stored embeddings, computes dot-product
//! similarity in Rust, and returns the top-`limit` results joined with their
//! chunk metadata from `doc_chunks`.  `all-MiniLM-L6-v2` produces L2-
//! normalised vectors, so dot product equals cosine similarity.  This
//! brute-force O(n) approach is adequate for documentation collections of up
//! to ~100 k chunks (< 10 ms on commodity hardware); upgrade to `CozoDB` HNSW
//! when the collection grows beyond that.

use std::collections::BTreeMap;

use cozo::DataValue;
use tracing::debug;

use super::{search::SearchResult, store::DataStore};
use crate::error::GraphtorError;

// ── Write operations ──────────────────────────────────────────────────────────

/// Persist or update the embedding for a chunk.
///
/// The `embedding` slice is serialised as a JSON array and stored in
/// `doc_vectors`.  Replaces any existing record with the same `chunk_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on serialisation or query failure.
pub fn upsert_vector(
    store: &DataStore,
    chunk_id: &str,
    embedding: &[f32],
) -> Result<(), GraphtorError> {
    let json = serde_json::to_string(embedding).map_err(|e| GraphtorError::Database {
        message: format!("failed to serialize embedding: {e}"),
        operation: "upsert_vector".to_string(),
    })?;
    let script = r"
        ?[chunk_id, embedding] <- [[$chunk_id, $embedding]]
        :put doc_vectors { chunk_id => embedding }
    ";
    let mut params = BTreeMap::new();
    params.insert("chunk_id".to_string(), DataValue::Str(chunk_id.into()));
    params.insert(
        "embedding".to_string(),
        DataValue::Str(json.as_str().into()),
    );
    store.mutate(script, params)?;
    debug!(chunk_id, "upserted doc_vectors record");
    Ok(())
}

/// Delete the stored embeddings for a slice of chunk IDs.
///
/// Silently skips chunk IDs that have no stored vector.  Returns the number
/// of deletion operations attempted (one per non-empty `chunk_ids` entry).
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
            ?[chunk_id] := *doc_vectors{ chunk_id }, chunk_id = $id
            :rm doc_vectors { chunk_id }
        ";
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), DataValue::Str(chunk_id.as_str().into()));
        store.mutate(script, params)?;
        deleted += 1;
    }
    debug!(count = deleted, "deleted doc_vectors records");
    Ok(deleted)
}

// ── Read operations ───────────────────────────────────────────────────────────

/// Return the stored embedding for a chunk, or `None` if not found.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialisation failure.
pub fn get_vector(store: &DataStore, chunk_id: &str) -> Result<Option<Vec<f32>>, GraphtorError> {
    let script = r"
        ?[embedding] := *doc_vectors{ chunk_id, embedding }, chunk_id = $id
    ";
    let mut params = BTreeMap::new();
    params.insert("id".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;
    rows.rows
        .into_iter()
        .next()
        .map(|row| decode_embedding_col(&row, 0, "get_vector"))
        .transpose()
}

/// Search for the `limit` most similar stored chunks to `query_vec`.
///
/// Loads all vectors from `doc_vectors`, computes dot-product similarity
/// (equivalent to cosine similarity for L2-normalised vectors), sorts
/// descending, and returns the top-`limit` results joined with chunk
/// metadata from `doc_chunks`.
///
/// Returns an empty [`Vec`] when no vectors are stored or `limit == 0`.
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

    // Load all (chunk_id, embedding) pairs.
    let all = load_all_vectors(store)?;
    if all.is_empty() {
        return Ok(Vec::new());
    }

    let query_norm = l2_norm(query_vec);
    if query_norm == 0.0 {
        return Ok(Vec::new());
    }

    // Score every stored vector.
    let mut scored: Vec<(String, f32)> = all
        .into_iter()
        .map(|(cid, vec)| {
            let sim = cosine_similarity(query_vec, query_norm, &vec);
            (cid, sim)
        })
        .collect();

    // Sort descending by similarity.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(limit);

    // Resolve chunk metadata for each top-k result.
    let mut results = Vec::with_capacity(scored.len());
    for (chunk_id, _score) in scored {
        if let Some(sr) = fetch_chunk_as_result(store, &chunk_id)? {
            results.push(sr);
        }
    }

    Ok(results)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Load all `(chunk_id, embedding)` pairs stored in `doc_vectors`.
fn load_all_vectors(store: &DataStore) -> Result<Vec<(String, Vec<f32>)>, GraphtorError> {
    let script = "?[chunk_id, embedding] := *doc_vectors{ chunk_id, embedding }";
    let rows = store.query(script, BTreeMap::new())?;
    rows.rows
        .iter()
        .map(|row| {
            let chunk_id = row
                .first()
                .and_then(DataValue::get_str)
                .map(str::to_owned)
                .ok_or_else(|| GraphtorError::Database {
                    message: "missing chunk_id in doc_vectors row".to_string(),
                    operation: "load_all_vectors".to_string(),
                })?;
            let embedding = decode_embedding_col(row, 1, "load_all_vectors")?;
            Ok((chunk_id, embedding))
        })
        .collect()
}

/// Deserialise an embedding from a specific column of a `doc_vectors` row.
fn decode_embedding_col(
    row: &[DataValue],
    col: usize,
    op: &str,
) -> Result<Vec<f32>, GraphtorError> {
    let json_str =
        row.get(col)
            .and_then(DataValue::get_str)
            .ok_or_else(|| GraphtorError::Database {
                message: format!("missing or non-string embedding at column {col}"),
                operation: op.to_string(),
            })?;
    serde_json::from_str::<Vec<f32>>(json_str).map_err(|e| GraphtorError::Database {
        message: format!("failed to deserialize embedding: {e}"),
        operation: op.to_string(),
    })
}

/// Fetch a chunk's metadata as a [`SearchResult`] by joining `doc_chunks`.
///
/// Returns `Ok(None)` when the chunk has no corresponding `doc_chunks` record.
fn fetch_chunk_as_result(
    store: &DataStore,
    chunk_id: &str,
) -> Result<Option<SearchResult>, GraphtorError> {
    let script = r"
        ?[chunk_id, source_id, path, headings, content]
            := *doc_chunks{ chunk_id, source_id, path, headings, content },
               chunk_id = $id
    ";
    let mut params = BTreeMap::new();
    params.insert("id".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;

    rows.rows
        .into_iter()
        .next()
        .map(|row| decode_search_result_row(&row))
        .transpose()
}

/// Decode a `doc_chunks` row as a [`SearchResult`].
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

// ── Math helpers ──────────────────────────────────────────────────────────────

/// Compute the L2 norm (Euclidean magnitude) of a float slice.
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Compute cosine similarity between `a` (with pre-computed norm `a_norm`) and `b`.
///
/// Returns `0.0` when `b` is the zero vector.
fn cosine_similarity(a: &[f32], a_norm: f32, b: &[f32]) -> f32 {
    let b_norm = l2_norm(b);
    if b_norm == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    dot / (a_norm * b_norm)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_norm_unit_vector_is_one() {
        let v = [1.0_f32, 0.0, 0.0, 0.0];
        assert!((l2_norm(&v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn l2_norm_zero_vector_is_zero() {
        let v = [0.0_f32, 0.0, 0.0, 0.0];
        assert!(l2_norm(&v) < f32::EPSILON);
    }

    #[test]
    fn cosine_similarity_identical_vectors_is_one() {
        let v = [1.0_f32, 0.0, 0.0];
        let norm = l2_norm(&v);
        assert!((cosine_similarity(&v, norm, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors_is_zero() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [0.0_f32, 1.0, 0.0];
        let norm_a = l2_norm(&a);
        assert!((cosine_similarity(&a, norm_a, &b)).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_zero_b_returns_zero() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [0.0_f32, 0.0, 0.0];
        let norm_a = l2_norm(&a);
        assert!(cosine_similarity(&a, norm_a, &b) < f32::EPSILON);
    }
}
