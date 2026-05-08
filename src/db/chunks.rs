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

use std::collections::BTreeMap;

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
