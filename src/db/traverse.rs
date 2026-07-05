//! Graph traversal operations over the document chunk graph.
//!
//! Provides [`find_related_chunks`], which performs a breadth-first search
//! starting from a seed `chunk_id`, following outgoing `doc_edges` links to
//! discover related chunks up to a configurable depth.
//!
//! Resolution is **two-tier**:
//!
//! - **Tier 1 (intra-source):** a `target_path` is matched against
//!   `doc_chunks` scoped to the *current* chunk's `source_id`. This keeps
//!   within-repo relative links fast and prevents identical `source_path`
//!   values from different sources from being incorrectly cross-linked.
//! - **Tier 2 (cross-source):** when a `target_path` is **absolute**
//!   (`/fabric/admin/foo`), the target is resolved globally through
//!   `doc_url_index` by its `canonical_url`. The resolved chunk is enqueued
//!   with *its own* `source_id`, so subsequent hops are re-scoped to the target
//!   document's home source. Tier 2 fires only for absolute targets so a broken
//!   *relative* link can never coincidentally cross-link into another source.
//!
//! Unresolved targets stay dangling — they produce no result and never a false
//! match.

use std::collections::{BTreeMap, HashSet, VecDeque};

use cozo::DataValue;
use tracing::debug;

use super::store::DataStore;
use super::urls::resolve_canonical_url;
use crate::error::GraphtorError;

/// A chunk reached during graph traversal.
#[derive(Debug, Clone, PartialEq)]
pub struct TraversalResult {
    /// Stable SHA-256 chunk identifier.
    pub chunk_id: String,
    /// Identifier of the source this chunk belongs to. May differ from the
    /// seed's source when the chunk was reached via a cross-source hop.
    pub source_id: String,
    /// Relative document path within the source.
    pub path: String,
    /// BFS depth at which this chunk was first discovered (seed = 0).
    pub depth: usize,
    /// `true` when this chunk belongs to a **different source than the seed**
    /// chunk (i.e. it was reached by crossing a source boundary). Computed
    /// relative to the seed so the flag is deterministic and independent of the
    /// path taken to reach the chunk.
    pub cross_source: bool,
}

/// Find chunks related to `start_chunk_id` via BFS over outgoing link edges.
///
/// The `max_depth` parameter controls how many hops the traversal follows.
/// The seed chunk itself is excluded from the results. If a target document
/// path resolves to multiple chunks, all of them are included.
///
/// Resolution is **two-tier** (see the module docs): Tier 1 matches
/// `target_path` intra-source; Tier 2 resolves absolute or otherwise-unmatched
/// targets globally via `canonical_url`, re-scoping each hop to the resolved
/// chunk's own source. Multi-source databases with identical `source_path`
/// values do not cause cross-source link pollution, while genuine cross-product
/// links resolve when a matching `canonical_url` exists.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on any query failure.
pub fn find_related_chunks(
    store: &DataStore,
    start_chunk_id: &str,
    max_depth: usize,
) -> Result<Vec<TraversalResult>, GraphtorError> {
    // Resolve the seed chunk's source so the first hop is scoped correctly.
    let seed_source = source_id_for_chunk(store, start_chunk_id)?.unwrap_or_default();

    let mut results: Vec<TraversalResult> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start_chunk_id.to_owned());

    // Queue entries: (chunk_id, source_id, current_depth). Each chunk carries
    // its own source_id so that after a cross-source hop, further hops are
    // scoped to the target document's home source rather than the seed's.
    let mut queue: VecDeque<(String, String, usize)> = VecDeque::new();
    queue.push_back((start_chunk_id.to_owned(), seed_source.clone(), 0));

    while let Some((current_id, current_source, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let target_paths = edges_from(store, &current_id)?;
        for target_path in target_paths {
            for resolved in resolve_targets(store, &current_source, &target_path)? {
                if visited.contains(&resolved.chunk_id) {
                    continue;
                }
                visited.insert(resolved.chunk_id.clone());
                // cross_source is measured relative to the SEED (not the last
                // hop), so it is deterministic and answers "is this result
                // external to where I started?".
                let cross_source = resolved.source_id != seed_source;
                debug!(
                    chunk_id = %resolved.chunk_id,
                    depth = depth + 1,
                    cross_source,
                    "traversal discovered chunk"
                );
                results.push(TraversalResult {
                    chunk_id: resolved.chunk_id.clone(),
                    source_id: resolved.source_id.clone(),
                    path: resolved.path,
                    depth: depth + 1,
                    cross_source,
                });
                queue.push_back((resolved.chunk_id, resolved.source_id, depth + 1));
            }
        }
    }
    Ok(results)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// A single resolved traversal target with the source it belongs to.
struct ResolvedTarget {
    chunk_id: String,
    path: String,
    source_id: String,
}

/// Resolve one `target_path` from a chunk in `current_source` into concrete
/// target chunks, applying the two-tier strategy.
fn resolve_targets(
    store: &DataStore,
    current_source: &str,
    target_path: &str,
) -> Result<Vec<ResolvedTarget>, GraphtorError> {
    let mut out: Vec<ResolvedTarget> = Vec::new();

    // Tier 1 — intra-source exact path match (unchanged behaviour). Preserves
    // the source-pollution guard for within-repo relative links.
    for (chunk_id, path) in chunks_at_path(store, current_source, target_path)? {
        out.push(ResolvedTarget {
            chunk_id,
            path,
            source_id: current_source.to_owned(),
        });
    }

    // Tier 2 — global canonical_url lookup, gated strictly on ABSOLUTE targets.
    // Absolute Learn URLs (`/fabric/admin/foo`) never match a relative
    // intra-source path, so Tier 1 is always empty for them; restricting Tier 2
    // to absolute targets means a broken *relative* link stays dangling instead
    // of coincidentally matching some other source's canonical_url. The BFS
    // `visited` set dedupes if Tier 2 resolves an already-seen chunk.
    if target_path.starts_with('/') {
        if let Some(res) = resolve_canonical_url(store, target_path)? {
            out.push(ResolvedTarget {
                chunk_id: res.chunk_id,
                path: res.path,
                source_id: res.source_id,
            });
        }
    }

    Ok(out)
}

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
