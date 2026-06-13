//! Graph traversal operations over the document chunk graph.
//!
//! Provides [`find_related_chunks`], which performs a breadth-first search
//! starting from a seed `chunk_id`, following outgoing `doc_edges` links to
//! discover related chunks up to a configurable depth.
//!
//! Traversal is **source-scoped**: links are resolved within the same
//! `source_id` as the seed chunk.  This prevents identical `source_path`
//! values from different sources from being incorrectly cross-linked.

use std::collections::{BTreeMap, HashSet, VecDeque};

use cozo::DataValue;
use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;

/// A chunk reached during graph traversal.
#[derive(Debug, Clone, PartialEq)]
pub struct TraversalResult {
    /// Stable SHA-256 chunk identifier.
    pub chunk_id: String,
    /// Relative document path within the source.
    pub path: String,
    /// BFS depth at which this chunk was first discovered (seed = 0).
    pub depth: usize,
}

/// Find chunks related to `start_chunk_id` via BFS over outgoing link edges.
///
/// The `max_depth` parameter controls how many hops the traversal follows.
/// The seed chunk itself is excluded from the results. If a target document
/// path resolves to multiple chunks, all of them are included.
///
/// Traversal is **source-scoped**: only chunks belonging to the same
/// `source_id` as the seed are considered when resolving `target_path`
/// values.  Multi-source databases with identical `source_path` values will
/// not cause cross-source link pollution.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query failure.
pub fn find_related_chunks(
    store: &DataStore,
    start_chunk_id: &str,
    max_depth: usize,
) -> Result<Vec<TraversalResult>, GraphtorError> {
    // Resolve the source_id of the seed chunk so we can scope all path
    // lookups to the same source.
    let source_id = source_id_for_chunk(store, start_chunk_id)?.unwrap_or_default();

    let mut results: Vec<TraversalResult> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start_chunk_id.to_owned());

    // Queue entries: (chunk_id, current_depth)
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((start_chunk_id.to_owned(), 0));

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let target_paths = edges_from(store, &current_id)?;
        for target_path in target_paths {
            // Resolve only within the same source to avoid cross-source pollution.
            let chunks = chunks_at_path(store, &source_id, &target_path)?;
            for (chunk_id, path) in chunks {
                if visited.contains(&chunk_id) {
                    continue;
                }
                visited.insert(chunk_id.clone());
                debug!(chunk_id = %chunk_id, depth = depth + 1, "traversal discovered chunk");
                results.push(TraversalResult {
                    chunk_id: chunk_id.clone(),
                    path,
                    depth: depth + 1,
                });
                queue.push_back((chunk_id, depth + 1));
            }
        }
    }
    Ok(results)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Return the `source_id` for the given `chunk_id`, or `None` when not found.
fn source_id_for_chunk(store: &DataStore, chunk_id: &str) -> Result<Option<String>, GraphtorError> {
    let script = r"
        ?[source_id] := *doc_chunks{ chunk_id: $cid, source_id }
    ";
    let mut params = BTreeMap::new();
    params.insert("cid".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;
    Ok(rows
        .rows
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.get_str().map(str::to_owned)))
}

/// Return all `target_path` values for outgoing edges from `chunk_id`.
fn edges_from(store: &DataStore, chunk_id: &str) -> Result<Vec<String>, GraphtorError> {
    let script = r"
        ?[target_path] := *doc_edges{ src_chunk_id: $src, target_path }
    ";
    let mut params = BTreeMap::new();
    params.insert("src".to_string(), DataValue::Str(chunk_id.into()));
    let rows = store.query(script, params)?;
    rows.rows
        .iter()
        .filter_map(|row| row.first().and_then(|v| v.get_str()).map(str::to_owned))
        .map(Ok)
        .collect()
}

/// Return all `(chunk_id, path)` pairs where `source_id` and `path` match.
///
/// Scoping by `source_id` ensures that chunks from different sources with
/// identical paths do not cross-pollinate during traversal.
fn chunks_at_path(
    store: &DataStore,
    source_id: &str,
    target_path: &str,
) -> Result<Vec<(String, String)>, GraphtorError> {
    let script = r"
        ?[chunk_id, path] := *doc_chunks{ chunk_id, source_id, path },
                             source_id = $sid,
                             path = $path
    ";
    let mut params = BTreeMap::new();
    params.insert("sid".to_string(), DataValue::Str(source_id.into()));
    params.insert("path".to_string(), DataValue::Str(target_path.into()));
    let rows = store.query(script, params)?;
    rows.rows
        .iter()
        .map(|row| {
            let chunk_id = row
                .first()
                .and_then(|v| v.get_str())
                .map(str::to_owned)
                .ok_or_else(|| GraphtorError::Database {
                    message: "missing chunk_id in traversal result".to_string(),
                    operation: "chunks_at_path".to_string(),
                })?;
            let path = row
                .get(1)
                .and_then(|v| v.get_str())
                .map(str::to_owned)
                .ok_or_else(|| GraphtorError::Database {
                    message: "missing path in traversal result".to_string(),
                    operation: "chunks_at_path".to_string(),
                })?;
            Ok((chunk_id, path))
        })
        .collect()
}
