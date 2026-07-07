//! Chunk storage and retrieval operations.
//!
//! Manages `doc_chunks`, the primary relation that stores document content
//! chunks with their metadata. Chunks are keyed by a stable SHA-256
//! `chunk_id` and linked to their source via `source_id`.
//!
//! The `doc_chunks` relation includes an `embedding: <F32; 384>?` column
//! that is indexed by the `doc_chunks:embedding_idx` HNSW index for
//! semantic search. [`upsert_chunk`] preserves any existing non-null
//! embedding — call [`crate::db::vectors::upsert_vector`] to store or
//! update embeddings explicitly.

use std::collections::{BTreeMap, HashMap};

use cozo::{DataValue, Num};
use tracing::debug;

use super::{store::DataStore, vectors::get_vector};
use crate::error::GraphtorError;
use crate::parse::types::Chunk;

/// A stored document chunk record.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkRecord {
    /// Stable SHA-256 identifier.
    pub chunk_id: String,
    /// Identifier of the source this chunk belongs to.
    pub source_id: String,
    /// Relative document path within the source.
    pub path: String,
    /// Document or chunk title, if present.
    pub title: Option<String>,
    /// Zero-based position within the document.
    pub position: usize,
    /// Approximate character offset of this chunk within the document.
    pub char_offset: usize,
    /// Ordered heading ancestry from H1 down to the chunk heading.
    pub heading_hierarchy: Vec<String>,
    /// Normalised markdown content.
    pub content: String,
}

/// Upsert a chunk derived from a parsed [`Chunk`].
///
/// Updates chunk metadata for any existing record with the same `chunk_id`.
/// Any existing non-null `embedding` is preserved — this function only manages
/// chunk metadata. Use [`crate::db::vectors::upsert_vector`] to store or
/// update embeddings explicitly.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on serialization or query failure.
pub fn upsert_chunk(
    store: &DataStore,
    source_id: &str,
    chunk: &Chunk,
) -> Result<(), GraphtorError> {
    let headings_json =
        serde_json::to_string(&chunk.heading_hierarchy).map_err(|e| GraphtorError::Database {
            message: e.to_string(),
            operation: "serialize_headings".to_string(),
        })?;
    let position_i64 = i64::try_from(chunk.position).map_err(|_| GraphtorError::Database {
        message: format!("position value {} is out of i64 range", chunk.position),
        operation: "upsert_chunk".to_string(),
    })?;
    let char_offset_i64 =
        i64::try_from(chunk.char_offset).map_err(|_| GraphtorError::Database {
            message: format!(
                "char_offset value {} is out of i64 range",
                chunk.char_offset
            ),
            operation: "upsert_chunk".to_string(),
        })?;

    // Preserve any existing non-null embedding. If none exists (new chunk or
    // embedding not yet computed), pass null. This prevents a model=None
    // re-sync from erasing embeddings stored by a prior embed pass.
    let existing_embedding = get_vector(store, chunk.chunk_id.as_str())?;
    let embedding_val = match existing_embedding {
        Some(floats) => DataValue::List(
            floats
                .into_iter()
                .map(|x| DataValue::Num(Num::Float(f64::from(x))))
                .collect(),
        ),
        None => DataValue::Null,
    };

    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
            <- [[$chunk_id, $source_id, $path, $title, $position, $char_offset, $headings, $content, $embedding]]
        :put doc_chunks { chunk_id => source_id, path, title, position, char_offset, headings, content, embedding }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "chunk_id".to_string(),
        DataValue::Str(chunk.chunk_id.as_str().into()),
    );
    params.insert("source_id".to_string(), DataValue::Str(source_id.into()));
    params.insert(
        "path".to_string(),
        DataValue::Str(chunk.source_path.as_str().into()),
    );
    params.insert("title".to_string(), DataValue::Null);
    params.insert(
        "position".to_string(),
        DataValue::Num(Num::Int(position_i64)),
    );
    params.insert(
        "char_offset".to_string(),
        DataValue::Num(Num::Int(char_offset_i64)),
    );
    params.insert(
        "headings".to_string(),
        DataValue::Str(headings_json.as_str().into()),
    );
    params.insert(
        "content".to_string(),
        DataValue::Str(chunk.content.as_str().into()),
    );
    params.insert("embedding".to_string(), embedding_val);
    store.mutate(script, params)?;
    debug!(chunk_id = %chunk.chunk_id, "upserted doc_chunks record");
    Ok(())
}

/// Upsert many chunks in a **single** multi-row `:put` mutation.
///
/// Amortizes `CozoScript` compilation and transaction commit across an entire
/// batch: one script compile and one `:put` for `chunks.len()` rows instead of
/// one compile per chunk as with [`upsert_chunk`]. This is the hot path for a
/// full ingest of thousands of files.
///
/// Each row is serialized identically to [`upsert_chunk`]: `title` is always
/// `Null`, `position`/`char_offset` are stored as `Int`, `headings` is the
/// JSON-encoded heading hierarchy, and `content` is stored as `Str`. The
/// `embedding` column for a chunk is the vector from `embeddings` (as a `List`
/// of `Float`) when the map contains its `chunk_id`, or `Null` otherwise.
///
/// Unlike [`upsert_chunk`], this function does **not** read existing embeddings
/// per chunk. Callers are responsible for supplying an `embeddings` map that
/// already merges freshly computed vectors with any previously stored ones
/// (see [`get_vectors_for`]) so a model-less re-sync does not erase embeddings.
///
/// When two rows in `chunks` share a `chunk_id`, only the **last** slice
/// occurrence is written. A raw multi-row `:put` resolves duplicate keys by
/// `CozoDB`'s sorted tuple order (not slice order), so this function first
/// collapses duplicates in Rust — keeping the last occurrence — making the
/// batch provably identical to calling [`upsert_chunk`] once per chunk in
/// slice order.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on serialization or mutation failure.
// The `embeddings` map is always constructed with the default hasher by the
// Load stage, so a concrete `HashMap` keeps the prescribed API shape without a
// spurious generic hasher parameter.
#[allow(clippy::implicit_hasher)]
pub fn upsert_chunks_batch(
    store: &DataStore,
    source_id: &str,
    chunks: &[&Chunk],
    embeddings: &HashMap<String, Vec<f32>>,
) -> Result<(), GraphtorError> {
    if chunks.is_empty() {
        return Ok(());
    }
    // De-duplicate by `chunk_id`, keeping the LAST slice occurrence. A raw
    // multi-row `:put` resolves duplicate keys by CozoDB's sorted tuple order,
    // not slice order, so collapsing here first makes the batch provably
    // identical to calling `upsert_chunk` once per chunk in slice order.
    let mut by_key: HashMap<&str, &Chunk> = HashMap::with_capacity(chunks.len());
    for &chunk in chunks {
        by_key.insert(chunk.chunk_id.as_str(), chunk);
    }
    let mut rows: Vec<DataValue> = Vec::with_capacity(by_key.len());
    for chunk in by_key.into_values() {
        rows.push(chunk_row_value(source_id, chunk, embeddings)?);
    }
    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
            <- $rows
        :put doc_chunks { chunk_id => source_id, path, title, position, char_offset, headings, content, embedding }
    ";
    let mut params = BTreeMap::new();
    let row_count = rows.len();
    params.insert("rows".to_string(), DataValue::List(rows));
    store.mutate(script, params)?;
    debug!(count = row_count, "batch-upserted doc_chunks records");
    Ok(())
}

/// Build one `doc_chunks` row as a `DataValue::List`, reusing the exact
/// per-column serialization of [`upsert_chunk`].
fn chunk_row_value(
    source_id: &str,
    chunk: &Chunk,
    embeddings: &HashMap<String, Vec<f32>>,
) -> Result<DataValue, GraphtorError> {
    let headings_json =
        serde_json::to_string(&chunk.heading_hierarchy).map_err(|e| GraphtorError::Database {
            message: e.to_string(),
            operation: "serialize_headings".to_string(),
        })?;
    let position_i64 = i64::try_from(chunk.position).map_err(|_| GraphtorError::Database {
        message: format!("position value {} is out of i64 range", chunk.position),
        operation: "upsert_chunks_batch".to_string(),
    })?;
    let char_offset_i64 =
        i64::try_from(chunk.char_offset).map_err(|_| GraphtorError::Database {
            message: format!(
                "char_offset value {} is out of i64 range",
                chunk.char_offset
            ),
            operation: "upsert_chunks_batch".to_string(),
        })?;
    let embedding_val = match embeddings.get(chunk.chunk_id.as_str()) {
        Some(floats) => DataValue::List(
            floats
                .iter()
                .map(|&x| DataValue::Num(Num::Float(f64::from(x))))
                .collect(),
        ),
        None => DataValue::Null,
    };
    Ok(DataValue::List(vec![
        DataValue::Str(chunk.chunk_id.as_str().into()),
        DataValue::Str(source_id.into()),
        DataValue::Str(chunk.source_path.as_str().into()),
        DataValue::Null,
        DataValue::Num(Num::Int(position_i64)),
        DataValue::Num(Num::Int(char_offset_i64)),
        DataValue::Str(headings_json.as_str().into()),
        DataValue::Str(chunk.content.as_str().into()),
        embedding_val,
    ]))
}

/// Fetch the stored embeddings for many `chunk_ids` in a **single** query.
///
/// Generalizes [`crate::db::vectors::get_vector`] to a batch: issues ONE query
/// using an `is_in` membership predicate instead of one query per id. Only
/// chunks that both exist and carry a non-null embedding appear in the returned
/// map — ids that are absent or unembedded are simply omitted, exactly as
/// [`get_vector`] returns `None` for them.
///
/// Used by the Load stage to preserve previously stored embeddings across a
/// model-less re-sync without paying a per-chunk query.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialisation failure.
pub fn get_vectors_for(
    store: &DataStore,
    chunk_ids: &[&str],
) -> Result<HashMap<String, Vec<f32>>, GraphtorError> {
    if chunk_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let script =
        r"?[chunk_id, embedding] := *doc_chunks{ chunk_id, embedding }, is_in(chunk_id, $ids)";
    let ids = DataValue::List(
        chunk_ids
            .iter()
            .map(|id| DataValue::Str((*id).into()))
            .collect(),
    );
    let mut params = BTreeMap::new();
    params.insert("ids".to_string(), ids);
    let rows = store.query(script, params)?;

    let mut map: HashMap<String, Vec<f32>> = HashMap::with_capacity(rows.rows.len());
    for row in rows.rows {
        let mut cols = row.into_iter();
        let (Some(id_val), Some(emb_val)) = (cols.next(), cols.next()) else {
            continue;
        };
        let Some(chunk_id) = id_val.get_str().map(str::to_owned) else {
            continue;
        };
        match emb_val {
            DataValue::Vec(cozo::Vector::F32(arr)) => {
                map.insert(chunk_id, arr.to_vec());
            }
            // Chunk exists but has no embedding yet — omit, like `get_vector`.
            DataValue::Null => {}
            _ => {
                return Err(GraphtorError::Database {
                    message: "unexpected embedding type in doc_chunks".to_string(),
                    operation: "get_vectors_for".to_string(),
                });
            }
        }
    }
    Ok(map)
}

/// Retrieve a single chunk by its identifier.
///
/// Returns `Ok(None)` if no matching chunk exists.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialization failure.
pub fn get_chunk(store: &DataStore, chunk_id: &str) -> Result<Option<ChunkRecord>, GraphtorError> {
    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content]
            := *doc_chunks{
                chunk_id, source_id, path, title,
                position, char_offset, headings, content
               },
               chunk_id = $id
    ";
    let mut params = BTreeMap::new();
    params.insert("id".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;
    rows.rows
        .into_iter()
        .next()
        .map(|row| row_to_chunk(&row))
        .transpose()
}

/// List all chunks belonging to a given source, ordered by document position.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialization failure.
pub fn list_chunks_for_source(
    store: &DataStore,
    source_id: &str,
) -> Result<Vec<ChunkRecord>, GraphtorError> {
    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content]
            := *doc_chunks{
                chunk_id, source_id, path, title,
                position, char_offset, headings, content
               },
               source_id = $sid
    ";
    let mut params = BTreeMap::new();
    params.insert("sid".to_string(), DataValue::Str(source_id.into()));
    let rows = store.query(script, params)?;
    rows.rows.iter().map(|row| row_to_chunk(row)).collect()
}

/// List all chunks associated with a given document path, ordered by position.
///
/// Chunks stored for the same `path` may originate from different sources.
/// The returned slice is sorted ascending by [`ChunkRecord::position`] so
/// callers always receive the natural reading order of the document.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or deserialization failure.
pub fn list_chunks_by_path(
    store: &DataStore,
    path: &str,
) -> Result<Vec<ChunkRecord>, GraphtorError> {
    let script = r"
        ?[chunk_id, source_id, path, title, position, char_offset, headings, content]
            := *doc_chunks{
                chunk_id, source_id, path, title,
                position, char_offset, headings, content
               },
               path = $path
    ";
    let mut params = BTreeMap::new();
    params.insert("path".to_string(), DataValue::Str(path.into()));
    let rows = store.query(script, params)?;
    let mut chunks: Vec<ChunkRecord> = rows
        .rows
        .iter()
        .map(|row| row_to_chunk(row))
        .collect::<Result<Vec<_>, _>>()?;
    chunks.sort_by_key(|c| c.position);
    Ok(chunks)
}

/// Delete all chunks associated with the given `source_id` and `path`.
///
/// Unlike [`delete_chunks_by_path`], this variant scopes the deletion to a
/// specific source so cross-source records at identical paths are not affected.
///
/// Returns the list of deleted `chunk_id` values so callers can cascade the
/// deletion to `doc_edges` and `doc_code`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or mutation failure.
pub fn delete_chunks_by_source_and_path(
    store: &DataStore,
    source_id: &str,
    path: &str,
) -> Result<Vec<String>, GraphtorError> {
    // Collect chunk_ids for this (source_id, path) pair first.
    let select = r"
        ?[chunk_id] := *doc_chunks{ chunk_id, source_id, path },
                       source_id = $sid,
                       path = $path
    ";
    let mut params = BTreeMap::new();
    params.insert("sid".to_string(), DataValue::Str(source_id.into()));
    params.insert("path".to_string(), DataValue::Str(path.into()));
    let rows = store.query(select, params)?;

    let ids: Vec<String> = rows
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(|v| v.get_str()).map(str::to_owned))
        .collect();

    if ids.is_empty() {
        return Ok(ids);
    }

    // Delete by primary key, scoped to this source.
    let rm = r"
        ?[chunk_id] := *doc_chunks{ chunk_id, source_id, path },
                       source_id = $sid,
                       path = $path
        :rm doc_chunks { chunk_id }
    ";
    let mut params = BTreeMap::new();
    params.insert("sid".to_string(), DataValue::Str(source_id.into()));
    params.insert("path".to_string(), DataValue::Str(path.into()));
    store.mutate(rm, params)?;
    debug!(
        count = ids.len(),
        source_id, path, "deleted doc_chunks records by source and path"
    );
    Ok(ids)
}

/// Delete all chunks associated with the given source-root-relative `path`.
///
/// Returns the list of deleted `chunk_id` values so callers can cascade the
/// deletion to `doc_edges` and `doc_code`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or mutation failure.
pub fn delete_chunks_by_path(store: &DataStore, path: &str) -> Result<Vec<String>, GraphtorError> {
    // Collect chunk_ids for this path first.
    let select = r"
        ?[chunk_id] := *doc_chunks{ chunk_id, path }, path = $path
    ";
    let mut params = BTreeMap::new();
    params.insert("path".to_string(), DataValue::Str(path.into()));
    let rows = store.query(select, params)?;

    let ids: Vec<String> = rows
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(|v| v.get_str()).map(str::to_owned))
        .collect();

    if ids.is_empty() {
        return Ok(ids);
    }

    // Delete by primary key.
    let rm = r"
        ?[chunk_id] := *doc_chunks{ chunk_id, path }, path = $path
        :rm doc_chunks { chunk_id }
    ";
    let mut params = BTreeMap::new();
    params.insert("path".to_string(), DataValue::Str(path.into()));
    store.mutate(rm, params)?;
    debug!(
        count = ids.len(),
        path, "deleted doc_chunks records by path"
    );
    Ok(ids)
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_chunk(row: &[DataValue]) -> Result<ChunkRecord, GraphtorError> {
    let chunk_id = require_str(row, 0, "chunk_id")?;
    let source_id = require_str(row, 1, "source_id")?;
    let path = require_str(row, 2, "path")?;
    let title = opt_col_str(row, 3);
    let position = require_usize(row, 4, "position")?;
    let char_offset = require_usize(row, 5, "char_offset")?;
    let headings_json = require_str(row, 6, "headings")?;
    let heading_hierarchy: Vec<String> =
        serde_json::from_str(&headings_json).map_err(|e| GraphtorError::Database {
            message: e.to_string(),
            operation: "deserialize_headings".to_string(),
        })?;
    let content = require_str(row, 7, "content")?;
    Ok(ChunkRecord {
        chunk_id,
        source_id,
        path,
        title,
        position,
        char_offset,
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

fn require_usize(row: &[DataValue], idx: usize, field: &str) -> Result<usize, GraphtorError> {
    let int_val =
        row.get(idx)
            .and_then(DataValue::get_int)
            .ok_or_else(|| GraphtorError::Database {
                message: format!("missing or non-integer field '{field}' at column {idx}"),
                operation: "row_decode".to_string(),
            })?;
    usize::try_from(int_val).map_err(|_| GraphtorError::Database {
        message: format!("value {int_val} for field '{field}' is out of usize range"),
        operation: "row_decode".to_string(),
    })
}

fn opt_col_str(row: &[DataValue], idx: usize) -> Option<String> {
    row.get(idx).and_then(|v| v.get_str()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DataStore {
        let s = DataStore::open_mem().expect("open_mem");
        s.ensure_schema().expect("ensure_schema");
        s
    }

    fn chunk(id: &str, path: &str, position: usize, content: &str) -> Chunk {
        Chunk {
            chunk_id: id.to_owned(),
            content: content.to_owned(),
            heading_hierarchy: vec!["H1".to_owned(), id.to_owned()],
            position,
            char_offset: position * 10,
            source_path: path.to_owned(),
        }
    }

    #[test]
    fn batch_put_stores_same_rows_as_single_row_puts() {
        // A batched put of N chunks must be byte-for-byte equivalent to calling
        // `upsert_chunk` once per chunk.
        let batched = store();
        let single = store();

        let c0 = chunk("chunk-0", "guide.md", 0, "intro body");
        let c1 = chunk("chunk-1", "guide.md", 1, "second body");
        let c2 = chunk("chunk-2", "other.md", 0, "other body");
        let chunks = [&c0, &c1, &c2];

        upsert_chunks_batch(&batched, "src", &chunks, &HashMap::new()).expect("batch put");
        for c in chunks {
            upsert_chunk(&single, "src", c).expect("single put");
        }

        for c in chunks {
            let from_batch = get_chunk(&batched, &c.chunk_id)
                .expect("query batch")
                .expect("row present in batch store");
            let from_single = get_chunk(&single, &c.chunk_id)
                .expect("query single")
                .expect("row present in single store");
            assert_eq!(from_batch, from_single, "row mismatch for {}", c.chunk_id);
            // Cross-check the decoded record against the source chunk.
            assert_eq!(from_batch.source_id, "src");
            assert_eq!(from_batch.path, c.source_path);
            assert_eq!(from_batch.position, c.position);
            assert_eq!(from_batch.char_offset, c.char_offset);
            assert_eq!(from_batch.heading_hierarchy, c.heading_hierarchy);
            assert_eq!(from_batch.content, c.content);
            assert_eq!(from_batch.title, None);
        }
    }

    #[test]
    fn batch_put_persists_supplied_embeddings_and_nulls_the_rest() {
        let s = store();
        let c0 = chunk("emb-0", "g.md", 0, "with embedding");
        let c1 = chunk("emb-1", "g.md", 1, "without embedding");

        let mut embeddings = HashMap::new();
        embeddings.insert("emb-0".to_owned(), vec![0.5_f32; 384]);

        upsert_chunks_batch(&s, "src", &[&c0, &c1], &embeddings).expect("batch put");

        let v0 = get_vector(&s, "emb-0").expect("get emb-0");
        assert_eq!(v0.as_ref().map(Vec::len), Some(384));
        assert!((v0.expect("some")[0] - 0.5).abs() < f32::EPSILON);
        assert_eq!(get_vector(&s, "emb-1").expect("get emb-1"), None);
    }

    #[test]
    fn get_vectors_for_returns_only_requested_embedded_ids() {
        let s = store();
        let embedded = chunk("has-vec", "g.md", 0, "a");
        let bare = chunk("no-vec", "g.md", 1, "b");
        let unrequested = chunk("has-vec-2", "g.md", 2, "c");

        let mut embeddings = HashMap::new();
        embeddings.insert("has-vec".to_owned(), vec![0.1_f32; 384]);
        embeddings.insert("has-vec-2".to_owned(), vec![0.2_f32; 384]);
        upsert_chunks_batch(&s, "src", &[&embedded, &bare, &unrequested], &embeddings)
            .expect("batch put");

        // Request one embedded id, one unembedded id, one non-existent id.
        let got = get_vectors_for(&s, &["has-vec", "no-vec", "ghost"]).expect("get_vectors_for");
        assert_eq!(got.len(), 1, "only the embedded, requested id is returned");
        assert!(got.contains_key("has-vec"));
        assert!(!got.contains_key("no-vec"));
        assert!(!got.contains_key("ghost"));
        assert_eq!(got["has-vec"].len(), 384);
        // The embedded chunk we did NOT request must be excluded.
        assert!(!got.contains_key("has-vec-2"));
    }

    #[test]
    fn get_vectors_for_empty_input_is_noop() {
        let s = store();
        assert!(get_vectors_for(&s, &[]).expect("empty").is_empty());
    }

    #[test]
    fn batch_put_empty_is_noop() {
        let s = store();
        upsert_chunks_batch(&s, "src", &[], &HashMap::new()).expect("empty batch");
        assert_eq!(list_chunks_for_source(&s, "src").expect("list").len(), 0);
    }

    #[test]
    fn batch_and_single_reject_out_of_range_position_consistently() {
        // The per-row fallback in the Load stage relies on `upsert_chunk` and
        // `upsert_chunks_batch` rejecting the same invalid row. A position that
        // exceeds i64 range is such a row. Skip on targets where usize cannot
        // exceed i64::MAX (e.g. 32-bit), where the conversion never overflows.
        let Ok(big) = usize::try_from(u64::try_from(i64::MAX).expect("i64::MAX >= 0") + 1) else {
            return;
        };
        let s = store();
        let mut bad = chunk("bad", "p.md", 0, "body");
        bad.position = big;
        assert!(
            upsert_chunk(&s, "src", &bad).is_err(),
            "single-row upsert must reject out-of-range position"
        );
        assert!(
            upsert_chunks_batch(&s, "src", &[&bad], &HashMap::new()).is_err(),
            "batch upsert must reject out-of-range position too"
        );
    }

    #[test]
    fn batch_put_duplicate_chunk_id_keeps_last_occurrence() {
        // Two rows sharing a chunk_id within one batch must resolve to the LAST
        // slice occurrence's columns, matching per-row slice-order semantics.
        // (A raw multi-row `:put` would otherwise resolve dup keys by CozoDB's
        // sorted tuple order, so the batch path de-duplicates in Rust first.)
        let s = store();
        let first = chunk("dup", "first.md", 3, "first body");
        let second = chunk("dup", "second.md", 7, "second body");

        upsert_chunks_batch(&s, "src", &[&first, &second], &HashMap::new()).expect("batch put");

        let stored = list_chunks_for_source(&s, "src").expect("list");
        assert_eq!(stored.len(), 1, "duplicate chunk_id collapses to one row");

        let row = get_chunk(&s, "dup")
            .expect("query")
            .expect("row present after batch put");
        assert_eq!(row.path, "second.md", "last occurrence's path wins");
        assert_eq!(row.position, 7, "last occurrence's position wins");
        assert_eq!(row.char_offset, 70, "last occurrence's char_offset wins");
        assert_eq!(row.content, "second body", "last occurrence's content wins");
    }
}
