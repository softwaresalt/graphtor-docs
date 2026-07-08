//! Exact brute-force cosine k-nearest-neighbour vector search over `doc_chunks`.
//!
//! Stores 384-dimensional embeddings produced by `all-MiniLM-L6-v2` directly
//! in the `doc_chunks` relation as the `embedding: <F32; 384>?` column and
//! exposes **exact** nearest-neighbour lookup by scanning every stored
//! embedding at query time. No vector index is maintained.
//!
//! # Storage
//!
//! [`upsert_vector`] performs a join-put: it reads the existing `doc_chunks`
//! row and writes it back with the new embedding.  The chunk **must** already
//! exist in `doc_chunks` before `upsert_vector` is called.  Because there is no
//! auxiliary index, writes pay no per-`:put` index-maintenance cost — this is
//! what keeps embedded ingest fast.
//!
//! # Search
//!
//! [`search_by_vector`] computes `cos_dist` between the query vector and every
//! non-null stored embedding, orders by distance, and takes the top `limit`.
//! The scan is `O(N)` in the number of embedded chunks and returns 100% recall
//! (the true nearest neighbours). This is well suited to corpora up to a few
//! hundred thousand vectors; beyond that, query latency grows linearly.

use std::collections::BTreeMap;

use cozo::{DataValue, Num};
use tracing::debug;

use super::{search::SearchResult, store::DataStore};
use crate::error::GraphtorError;

// ── Write operations ──────────────────────────────────────────────────────────

/// Persist or update the embedding for an existing chunk.
///
/// Reads the current `doc_chunks` row for `chunk_id` and writes it back with
/// the supplied `embedding`.  No auxiliary vector index is maintained, so the
/// write pays no index-maintenance cost.
///
/// The chunk **must** already exist in `doc_chunks` — call
/// [`crate::db::upsert_chunk`] before calling this function.  If the chunk
/// does not exist, the join produces zero rows and the call is a silent no-op
/// (no error is returned, no embedding is stored).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query or mutation failure.
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

/// Search for the `limit` most similar stored chunks to `query_vec` using an
/// **exact** brute-force cosine k-nearest-neighbour scan over `doc_chunks`.
///
/// Computes `cos_dist` between the query vector and every non-null stored
/// embedding, orders by ascending distance, and returns the top `limit` rows
/// joined with chunk metadata.  The scan is `O(N)` in the number of embedded
/// chunks and yields the true nearest neighbours (100% recall).  Returns an
/// empty [`Vec`] when `limit == 0` or no embeddings are stored.
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
        "?[chunk_id, source_id, path, headings, content, dist] \
         := q = vec($query), \
            *doc_chunks{{ chunk_id, source_id, path, headings, content, embedding }}, \
            !is_null(embedding), \
            dist = cos_dist(q, embedding) \
         :order dist \
         :limit {limit_i64}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::upsert_chunk;
    use crate::parse::types::Chunk;

    /// Open an in-memory store with schema applied.
    fn store() -> DataStore {
        let s = DataStore::open_mem().expect("open_mem");
        s.ensure_schema().expect("ensure_schema");
        s
    }

    /// Build a 384-dimensional unit vector with `1.0` at `pos`, `0.0` elsewhere.
    fn unit_vec(pos: usize) -> Vec<f32> {
        let mut v = vec![0.0_f32; 384];
        v[pos] = 1.0;
        v
    }

    /// Insert a minimal chunk so `upsert_vector`'s join-put finds a row.
    fn insert_chunk(store: &DataStore, chunk_id: &str, path: &str) {
        let chunk = Chunk {
            chunk_id: chunk_id.to_owned(),
            content: format!("content for {chunk_id}"),
            heading_hierarchy: vec!["Heading".to_owned()],
            position: 0,
            char_offset: 0,
            source_path: path.to_owned(),
        };
        upsert_chunk(store, "test-src", &chunk).expect("chunk upsert");
    }

    #[test]
    fn brute_force_search_returns_exact_nearest_first() {
        let s = store();

        // Three orthogonal unit vectors in 384-D.
        insert_chunk(&s, "chunk-a", "docs/a.md");
        insert_chunk(&s, "chunk-b", "docs/b.md");
        insert_chunk(&s, "chunk-c", "docs/c.md");

        upsert_vector(&s, "chunk-a", &unit_vec(0)).expect("upsert a");
        upsert_vector(&s, "chunk-b", &unit_vec(1)).expect("upsert b");
        upsert_vector(&s, "chunk-c", &unit_vec(2)).expect("upsert c");

        // Query exactly matches chunk-b — the exact nearest neighbour.
        let results = search_by_vector(&s, &unit_vec(1), 3).expect("search should succeed");

        assert_eq!(results.len(), 3, "all three embeddings should be scanned");
        assert_eq!(
            results[0].chunk_id, "chunk-b",
            "brute-force scan must return the exact nearest neighbour first"
        );
    }

    #[test]
    fn brute_force_search_excludes_unembedded_chunks() {
        let s = store();

        // Two embedded chunks plus one chunk left un-embedded (null embedding),
        // as produced by `upsert_chunk` before `upsert_vector` runs — a realistic
        // partially-embedded state.
        insert_chunk(&s, "chunk-a", "docs/a.md");
        insert_chunk(&s, "chunk-b", "docs/b.md");
        insert_chunk(&s, "chunk-unembedded", "docs/c.md");

        upsert_vector(&s, "chunk-a", &unit_vec(0)).expect("upsert a");
        upsert_vector(&s, "chunk-b", &unit_vec(1)).expect("upsert b");
        // chunk-unembedded intentionally has a null embedding (no upsert_vector).

        let results = search_by_vector(&s, &unit_vec(0), 10).expect("search should succeed");

        assert_eq!(
            results.len(),
            2,
            "only the two embedded chunks are scored; the null-embedding chunk is excluded"
        );
        assert!(
            results.iter().all(|r| r.chunk_id != "chunk-unembedded"),
            "a chunk with a null embedding must never appear in brute-force results"
        );
    }
}
