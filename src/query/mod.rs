//! Reusable multi-store query layer shared by the MCP server and the CLI.
//!
//! The functions in this module operate on a slice of already-opened
//! [`DataStore`] handles (plus an optional [`EmbeddingModel`] for semantic
//! search) and return the typed result structs produced by the `db` layer.
//! They encapsulate the cross-store aggregation semantics — merge ordering,
//! deduplication, source filtering, and result limits — so that both the MCP
//! tool handlers in [`crate::mcp::server`] and the `graphtor-docs` CLI
//! subcommands execute *identical* query logic.
//!
//! # Design
//!
//! Each public function is a thin, pure orchestration over the `db` free
//! functions:
//!
//! - [`search_text`] — full-text keyword search via [`search_by_text`], merged
//!   across all stores, optionally filtered by `source_id`, then truncated to
//!   `top_k`.
//! - [`search_semantic`] — embedding search via [`search_similar`], merged with
//!   an early-stopping limit of `top_k`.
//! - [`research`] — composite search (semantic when a model is supplied, else
//!   keyword) followed by BFS graph traversal from the top seeds.
//! - [`traverse`] — BFS graph traversal via [`find_related_chunks`], scoped to
//!   the store that owns the seed chunk.
//! - [`list_sources`] — source registry enumeration, deduplicated by
//!   `source_id` across stores.
//! - [`get_chunk`] — single-chunk lookup, returning the first match across
//!   stores.
//! - [`get_document`] — all chunks for a path, deduplicated and sorted into
//!   reading order, optionally filtered by `source_id`.
//! - [`status`] — aggregate [`DbStatus`] counts summed across all stores.
//!
//! Callers remain responsible for input validation (rejecting empty queries),
//! parameter defaulting, and output formatting. Result-size limits, however,
//! are clamped *internally* to the shared maxima [`MAX_SEARCH_TOP_K`],
//! [`MAX_RESEARCH_TOP_K`], [`MAX_RESEARCH_DEPTH`], and [`MAX_TRAVERSE_DEPTH`],
//! so the CLI and MCP query surfaces enforce identical upper bounds from a
//! single source of truth.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::db::{
    chunks::{get_chunk as db_get_chunk, list_chunks_by_path, ChunkRecord},
    nodes::{list_sources as db_list_sources, SourceRecord},
    search::{search_by_text, search_similar, SearchResult},
    store::DbStatus,
    traverse::{find_related_chunks, TraversalResult},
    DataStore,
};
use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;

/// Maximum number of results returned by [`search_text`] and
/// [`search_semantic`].
///
/// Requests larger than this are clamped down. This is the single source of
/// truth for the keyword/semantic result cap shared by the MCP tools and the
/// CLI subcommands.
pub const MAX_SEARCH_TOP_K: usize = 50;

/// Maximum initial-search breadth (`top_k`) for [`research`].
///
/// Requests larger than this are clamped down, keeping the MCP `research_topic`
/// tool and the CLI `research` command in lockstep.
pub const MAX_RESEARCH_TOP_K: usize = 20;

/// Maximum BFS traversal depth (`max_depth`) for [`research`].
///
/// Requests larger than this are clamped down.
pub const MAX_RESEARCH_DEPTH: usize = 3;

/// Maximum BFS traversal depth (`max_depth`) for [`traverse`].
///
/// Requests larger than this are clamped down.
pub const MAX_TRAVERSE_DEPTH: usize = 5;

/// Combined output of [`research`]: initial search hits plus BFS-discovered
/// related chunks.
///
/// The `related` chunks are globally deduplicated against both the initial hits
/// and one another, so each chunk appears at most once across the whole
/// structure.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchResults {
    /// Initial search results (semantic when a model was supplied, else
    /// keyword), already truncated to the requested breadth.
    pub initial: Vec<SearchResult>,
    /// Related chunks discovered by BFS traversal from the top seeds.
    pub related: Vec<TraversalResult>,
}

/// Merge per-store search results round-robin, deduplicating by `chunk_id`.
///
/// Results are interleaved one-per-store per round so that no single store
/// dominates the head of the merged list. When `limit` is `Some(max)`, merging
/// stops as soon as `max` unique results have been collected.
fn merge_search_results(
    per_store_results: &[Vec<SearchResult>],
    limit: Option<usize>,
) -> Vec<SearchResult> {
    let mut merged = Vec::new();
    let mut seen_chunk_ids = BTreeSet::new();
    let mut round_index = 0;

    loop {
        let mut progressed = false;

        for results in per_store_results {
            if let Some(result) = results.get(round_index) {
                progressed = true;
                if seen_chunk_ids.insert(result.chunk_id.clone()) {
                    merged.push(result.clone());
                    if limit.is_some_and(|max| merged.len() >= max) {
                        return merged;
                    }
                }
            }
        }

        if !progressed {
            break;
        }

        round_index += 1;
    }

    merged
}

/// Return the first store that contains `chunk_id`, or `None` when no store
/// holds it.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from the underlying chunk lookup.
fn store_for_chunk_id<'a>(
    stores: &'a [DataStore],
    chunk_id: &str,
) -> Result<Option<&'a DataStore>, GraphtorError> {
    for store in stores {
        if db_get_chunk(store, chunk_id)?.is_some() {
            return Ok(Some(store));
        }
    }

    Ok(None)
}

/// Collect all chunks for `path` across every store, deduplicated by `chunk_id`
/// and sorted into a deterministic reading order.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from the per-store chunk queries.
fn list_chunks_by_path_all(
    stores: &[DataStore],
    path: &str,
) -> Result<Vec<ChunkRecord>, GraphtorError> {
    let mut all_chunks = Vec::new();
    let mut seen_chunk_ids = BTreeSet::new();

    for store in stores {
        for chunk in list_chunks_by_path(store, path)? {
            if seen_chunk_ids.insert(chunk.chunk_id.clone()) {
                all_chunks.push(chunk);
            }
        }
    }

    all_chunks.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then(left.position.cmp(&right.position))
            .then(left.char_offset.cmp(&right.char_offset))
            .then(left.chunk_id.cmp(&right.chunk_id))
    });

    Ok(all_chunks)
}

/// Full-text keyword search across all stores.
///
/// Runs [`search_by_text`] against every store, merges the results round-robin
/// (deduplicating by `chunk_id`), applies an optional `source_id` filter
/// (a blank/whitespace-only `source_id` is treated as no filter; surrounding
/// whitespace is trimmed before comparison), and finally truncates to at most
/// `top_k` results. `top_k` is clamped to [`MAX_SEARCH_TOP_K`].
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from any per-store text query.
pub fn search_text(
    stores: &[DataStore],
    query: &str,
    source_id: Option<&str>,
    top_k: usize,
) -> Result<Vec<SearchResult>, GraphtorError> {
    let top_k = top_k.min(MAX_SEARCH_TOP_K);
    let per_store_results = stores
        .iter()
        .map(|store| search_by_text(store, query))
        .collect::<Result<Vec<_>, _>>()?;
    let merged = merge_search_results(&per_store_results, None);

    // Normalize source_id: trim surrounding whitespace and treat blank as no
    // filter. Compare against the trimmed value so a padded id (e.g. "docs ")
    // behaves identically to `get_document`.
    let sid_filter = source_id.map(str::trim).filter(|s| !s.is_empty());
    let filtered: Vec<SearchResult> = if let Some(sid) = sid_filter {
        merged.into_iter().filter(|r| r.source_id == sid).collect()
    } else {
        merged
    };

    Ok(filtered.into_iter().take(top_k).collect())
}

/// Embedding-based semantic search across all stores.
///
/// Runs [`search_similar`] against every store (each limited to `top_k`) and
/// merges the ranked results round-robin with an early-stopping limit of
/// `top_k`. `top_k` is clamped to [`MAX_SEARCH_TOP_K`].
///
/// # Errors
///
/// Propagates [`GraphtorError::Embed`] if the query fails to embed, or
/// [`GraphtorError::Database`] on any per-store vector lookup failure.
pub fn search_semantic(
    stores: &[DataStore],
    model: &EmbeddingModel,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>, GraphtorError> {
    let top_k = top_k.min(MAX_SEARCH_TOP_K);
    let per_store_results = stores
        .iter()
        .map(|store| search_similar(store, model, query, top_k))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(merge_search_results(&per_store_results, Some(top_k)))
}

/// Composite topic research: initial search plus BFS graph traversal.
///
/// When `model` is `Some`, the initial results come from [`search_semantic`]
/// (ranked); otherwise they fall back to [`search_text`] (unranked, so seed
/// selection stays deterministic). At most `min(top_k, 3)` of the top results
/// seed the graph traversal, which follows document link edges up to
/// `max_depth` hops. Related chunks are deduplicated globally against the
/// initial hits and one another. `top_k` is clamped to [`MAX_RESEARCH_TOP_K`]
/// and `max_depth` to [`MAX_RESEARCH_DEPTH`].
///
/// # Errors
///
/// Propagates [`GraphtorError`] from the initial search or any graph traversal.
pub fn research(
    stores: &[DataStore],
    model: Option<&EmbeddingModel>,
    query: &str,
    top_k: usize,
    max_depth: usize,
) -> Result<ResearchResults, GraphtorError> {
    let top_k = top_k.min(MAX_RESEARCH_TOP_K);
    let max_depth = max_depth.min(MAX_RESEARCH_DEPTH);
    let seed_k = top_k.min(3);

    // Prefer semantic (ranked) search when the embedding model is available;
    // fall back to unranked text search so seed selection is deterministic.
    let initial: Vec<SearchResult> = if let Some(model) = model {
        search_semantic(stores, model, query, top_k)?
    } else {
        search_text(stores, query, None, top_k)?
    };

    // Traverse from the top seeds; deduplicate globally across all seeds.
    let mut seen_ids: HashSet<String> = initial.iter().map(|r| r.chunk_id.clone()).collect();
    let mut related: Vec<TraversalResult> = Vec::new();

    for seed in initial.iter().take(seed_k) {
        if let Some(store) = store_for_chunk_id(stores, &seed.chunk_id)? {
            let traversal = find_related_chunks(store, &seed.chunk_id, max_depth)?;
            for tr in traversal {
                if seen_ids.insert(tr.chunk_id.clone()) {
                    related.push(tr);
                }
            }
        }
    }

    Ok(ResearchResults { initial, related })
}

/// BFS graph traversal from `chunk_id` up to `max_depth` hops.
///
/// Resolves the store that owns `chunk_id` and delegates to
/// [`find_related_chunks`]. Returns an empty vector when no store contains the
/// seed chunk. `max_depth` is clamped to [`MAX_TRAVERSE_DEPTH`].
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from chunk resolution or traversal.
pub fn traverse(
    stores: &[DataStore],
    chunk_id: &str,
    max_depth: usize,
) -> Result<Vec<TraversalResult>, GraphtorError> {
    let max_depth = max_depth.min(MAX_TRAVERSE_DEPTH);
    if let Some(store) = store_for_chunk_id(stores, chunk_id)? {
        find_related_chunks(store, chunk_id, max_depth)
    } else {
        Ok(Vec::new())
    }
}

/// Enumerate all registered documentation sources across every store.
///
/// Sources are deduplicated by `source_id` (first occurrence wins) and returned
/// in `source_id` order.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from any per-store source query.
pub fn list_sources(stores: &[DataStore]) -> Result<Vec<SourceRecord>, GraphtorError> {
    let mut sources_by_id = BTreeMap::new();

    for store in stores {
        for source in db_list_sources(store)? {
            sources_by_id
                .entry(source.source_id.clone())
                .or_insert(source);
        }
    }

    Ok(sources_by_id.into_values().collect())
}

/// Retrieve a single chunk by its stable identifier.
///
/// Returns the first match found across stores, or `None` when no store holds
/// the chunk.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from the per-store lookups.
pub fn get_chunk(
    stores: &[DataStore],
    chunk_id: &str,
) -> Result<Option<ChunkRecord>, GraphtorError> {
    for store in stores {
        if let Some(chunk) = db_get_chunk(store, chunk_id)? {
            return Ok(Some(chunk));
        }
    }

    Ok(None)
}

/// Retrieve all chunks for a document `path`, in reading order.
///
/// Aggregates chunks across every store (deduplicated and sorted by source,
/// position, offset, then id) and applies an optional `source_id` filter. A
/// blank/whitespace-only `source_id` is treated as no filter.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from the per-store chunk queries.
pub fn get_document(
    stores: &[DataStore],
    source_id: Option<&str>,
    path: &str,
) -> Result<Vec<ChunkRecord>, GraphtorError> {
    let all_chunks = list_chunks_by_path_all(stores, path)?;

    // Filter by source_id when one is provided and non-empty.
    let sid = source_id.map(str::trim).filter(|s| !s.is_empty());
    let chunks = if let Some(sid) = sid {
        all_chunks
            .into_iter()
            .filter(|c| c.source_id == sid)
            .collect()
    } else {
        all_chunks
    };

    Ok(chunks)
}

/// Aggregate database status across all stores.
///
/// Sums `source_count` and `chunk_count` and takes the maximum `schema_version`
/// over every store. Returns a zeroed [`DbStatus`] when `stores` is empty.
///
/// # Errors
///
/// Propagates [`GraphtorError::Database`] from any per-store status query.
pub fn status(stores: &[DataStore]) -> Result<DbStatus, GraphtorError> {
    let mut aggregate = DbStatus {
        source_count: 0,
        chunk_count: 0,
        schema_version: 0,
    };

    for store in stores {
        let next = store.get_status()?;
        aggregate.source_count += next.source_count;
        aggregate.chunk_count += next.chunk_count;
        aggregate.schema_version = aggregate.schema_version.max(next.schema_version);
    }

    Ok(aggregate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        schema::ensure_schema, upsert_chunk, upsert_edge, upsert_source, DataStore, SourceRecord,
    };
    use crate::embed::{resolve_embedding_model, ResolverCaller};
    use crate::parse::types::{Chunk, Reference};

    fn store() -> DataStore {
        let s = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&s).expect("schema");
        s
    }

    fn chunk(id: &str, path: &str, content: &str) -> Chunk {
        Chunk {
            chunk_id: id.to_owned(),
            content: content.to_owned(),
            heading_hierarchy: vec!["Introduction".to_owned()],
            position: 0,
            char_offset: 0,
            source_path: path.to_owned(),
        }
    }

    fn source(id: &str) -> SourceRecord {
        SourceRecord {
            source_id: id.to_owned(),
            url: format!("https://example.com/{id}"),
            kind: "local".to_owned(),
            name: id.to_owned(),
            synced_at: None,
        }
    }

    fn edge(from_chunk_id: &str, to_path: &str) -> Reference {
        Reference {
            source_chunk_id: from_chunk_id.to_owned(),
            target_path: to_path.to_owned(),
            link_text: "see also".to_owned(),
            anchor: None,
        }
    }

    // ── search_text ──────────────────────────────────────────────────────────

    #[test]
    fn search_text_aggregates_across_stores() {
        let primary = store();
        let secondary = store();
        upsert_chunk(
            &primary,
            "src-a",
            &chunk("chunk-a", "docs/a.md", "shared multi database query"),
        )
        .expect("upsert a");
        upsert_chunk(
            &secondary,
            "src-b",
            &chunk("chunk-b", "docs/b.md", "shared multi database query"),
        )
        .expect("upsert b");

        let stores = vec![primary, secondary];
        let results = search_text(&stores, "shared multi database", None, 10).expect("search");
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"docs/a.md"), "expected a.md, got {paths:?}");
        assert!(paths.contains(&"docs/b.md"), "expected b.md, got {paths:?}");
    }

    #[test]
    fn search_text_source_filter_restricts_results() {
        let s = store();
        upsert_chunk(
            &s,
            "source-alpha",
            &chunk("ca", "docs/a.md", "blob storage container migration"),
        )
        .expect("upsert a");
        upsert_chunk(
            &s,
            "source-beta",
            &chunk("cb", "docs/b.md", "blob storage container migration"),
        )
        .expect("upsert b");

        let stores = vec![s];
        let results =
            search_text(&stores, "blob storage", Some("source-alpha"), 10).expect("search");
        assert_eq!(results.len(), 1, "source filter should keep one result");
        assert_eq!(results[0].source_id, "source-alpha");
    }

    #[test]
    fn search_text_blank_source_filter_is_ignored() {
        let s = store();
        upsert_chunk(&s, "src-a", &chunk("ca", "docs/a.md", "keyword hit")).expect("upsert a");
        upsert_chunk(&s, "src-b", &chunk("cb", "docs/b.md", "keyword hit")).expect("upsert b");

        let stores = vec![s];
        let results = search_text(&stores, "keyword hit", Some("   "), 10).expect("search");
        assert_eq!(results.len(), 2, "blank source_id must not filter");
    }

    #[test]
    fn search_text_top_k_limits_results() {
        let s = store();
        for i in 0..8_usize {
            upsert_chunk(
                &s,
                "src-x",
                &chunk(
                    &format!("tk-{i}"),
                    &format!("docs/item-{i}.md"),
                    &format!("pagination rate limit throttle item {i}"),
                ),
            )
            .expect("upsert");
        }

        let stores = vec![s];
        let results = search_text(&stores, "pagination rate limit", None, 3).expect("search");
        assert!(results.len() <= 3, "top_k should cap results at 3");
    }

    #[test]
    fn search_text_empty_stores_returns_empty() {
        let stores: Vec<DataStore> = Vec::new();
        let results = search_text(&stores, "anything", None, 10).expect("search");
        assert!(results.is_empty());
    }

    #[test]
    fn search_text_padded_source_filter_matches() {
        let s = store();
        upsert_chunk(
            &s,
            "source-alpha",
            &chunk("ca", "docs/a.md", "padded filter content"),
        )
        .expect("upsert a");
        upsert_chunk(
            &s,
            "source-beta",
            &chunk("cb", "docs/b.md", "padded filter content"),
        )
        .expect("upsert b");

        let stores = vec![s];
        // A padded source_id must be trimmed before comparison and still match,
        // matching `get_document`'s behaviour (regression: previously compared
        // against the untrimmed value and returned nothing).
        let results =
            search_text(&stores, "padded filter", Some("source-alpha "), 10).expect("search");
        assert_eq!(
            results.len(),
            1,
            "padded source_id must match after trimming"
        );
        assert_eq!(results[0].source_id, "source-alpha");
    }

    #[test]
    fn search_text_clamps_top_k_to_max_search_top_k() {
        let s = store();
        // Insert more matching chunks than the clamp so an over-large top_k
        // cannot exceed MAX_SEARCH_TOP_K.
        for i in 0..(MAX_SEARCH_TOP_K + 10) {
            upsert_chunk(
                &s,
                "src-clamp",
                &chunk(
                    &format!("clamp-{i}"),
                    &format!("docs/clamp-{i}.md"),
                    "clampsearchkeyword shared body",
                ),
            )
            .expect("upsert");
        }

        let stores = vec![s];
        let results = search_text(&stores, "clampsearchkeyword", None, 100_000).expect("search");
        assert_eq!(
            results.len(),
            MAX_SEARCH_TOP_K,
            "over-large top_k must be clamped to MAX_SEARCH_TOP_K"
        );
    }

    // ── traverse ─────────────────────────────────────────────────────────────

    #[test]
    fn traverse_finds_related_chunk() {
        let s = store();
        upsert_chunk(&s, "src", &chunk("node-a", "a.md", "content a")).expect("upsert a");
        upsert_chunk(&s, "src", &chunk("node-b", "b.md", "content b")).expect("upsert b");
        upsert_edge(&s, &edge("node-a", "b.md")).expect("edge a->b");

        let stores = vec![s];
        let results = traverse(&stores, "node-a", 1).expect("traverse");
        assert!(
            results.iter().any(|r| r.chunk_id == "node-b"),
            "expected node-b, got {results:?}"
        );
    }

    #[test]
    fn traverse_unknown_chunk_returns_empty() {
        let stores = vec![store()];
        let results = traverse(&stores, "does-not-exist", 2).expect("traverse");
        assert!(results.is_empty());
    }

    #[test]
    fn traverse_clamps_max_depth_to_max_traverse_depth() {
        let s = store();
        // Build a linear chain node0 -> node1 -> ... -> node6 so the deepest
        // node sits at depth 6, beyond MAX_TRAVERSE_DEPTH (5).
        let chain_len = MAX_TRAVERSE_DEPTH + 2;
        for i in 0..chain_len {
            upsert_chunk(
                &s,
                "src-chain",
                &chunk(&format!("node{i}"), &format!("p{i}.md"), "chain body"),
            )
            .expect("upsert node");
        }
        for i in 0..(chain_len - 1) {
            upsert_edge(&s, &edge(&format!("node{i}"), &format!("p{}.md", i + 1))).expect("edge");
        }

        let stores = vec![s];
        let clamped = traverse(&stores, "node0", MAX_TRAVERSE_DEPTH).expect("traverse clamped");
        let over_large = traverse(&stores, "node0", 100_000).expect("traverse over-large");

        assert_eq!(
            over_large, clamped,
            "an over-large max_depth must behave identically to MAX_TRAVERSE_DEPTH"
        );
        assert!(
            over_large.iter().all(|r| r.depth <= MAX_TRAVERSE_DEPTH),
            "no result may exceed MAX_TRAVERSE_DEPTH, got {over_large:?}"
        );
        let last = format!("node{}", chain_len - 1);
        assert!(
            !over_large.iter().any(|r| r.chunk_id == last),
            "the node beyond the clamp ({last}) must not be reached"
        );
    }

    // ── list_sources ─────────────────────────────────────────────────────────

    #[test]
    fn list_sources_deduplicates_across_stores() {
        let primary = store();
        let secondary = store();
        upsert_source(&primary, &source("shared")).expect("upsert primary shared");
        upsert_source(&primary, &source("only-primary")).expect("upsert only-primary");
        upsert_source(&secondary, &source("shared")).expect("upsert secondary shared");
        upsert_source(&secondary, &source("only-secondary")).expect("upsert only-secondary");

        let stores = vec![primary, secondary];
        let sources = list_sources(&stores).expect("list");
        let ids: Vec<&str> = sources.iter().map(|s| s.source_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["only-primary", "only-secondary", "shared"],
            "sources must be deduplicated and sorted by id"
        );
    }

    #[test]
    fn list_sources_empty_stores_returns_empty() {
        let stores: Vec<DataStore> = Vec::new();
        assert!(list_sources(&stores).expect("list").is_empty());
    }

    // ── get_chunk ────────────────────────────────────────────────────────────

    #[test]
    fn get_chunk_returns_first_match_across_stores() {
        let primary = store();
        let secondary = store();
        upsert_chunk(
            &secondary,
            "src-b",
            &chunk("target-chunk", "docs/b.md", "unique retrieval content"),
        )
        .expect("upsert b");

        let stores = vec![primary, secondary];
        let found = get_chunk(&stores, "target-chunk").expect("get");
        let found = found.expect("chunk should be found in a secondary store");
        assert_eq!(found.path, "docs/b.md");
    }

    #[test]
    fn get_chunk_not_found_returns_none() {
        let stores = vec![store()];
        assert!(get_chunk(&stores, "missing").expect("get").is_none());
    }

    // ── get_document ─────────────────────────────────────────────────────────

    #[test]
    fn get_document_returns_chunks_in_reading_order() {
        let s = store();
        let mut c0 = chunk("doc-c0", "docs/guide.md", "first section content");
        c0.position = 0;
        let mut c1 = chunk("doc-c1", "docs/guide.md", "second section content");
        c1.position = 1;
        upsert_chunk(&s, "src-doc", &c1).expect("upsert c1 first");
        upsert_chunk(&s, "src-doc", &c0).expect("upsert c0 second");

        let stores = vec![s];
        let chunks = get_document(&stores, None, "docs/guide.md").expect("get_document");
        let positions: Vec<usize> = chunks.iter().map(|c| c.position).collect();
        assert_eq!(positions, vec![0, 1], "chunks must be in reading order");
    }

    #[test]
    fn get_document_source_filter_restricts_to_source() {
        let s = store();
        upsert_chunk(
            &s,
            "src-x",
            &chunk("gdx", "shared/doc.md", "content from source x"),
        )
        .expect("upsert x");
        upsert_chunk(
            &s,
            "src-y",
            &chunk("gdy", "shared/doc.md", "content from source y"),
        )
        .expect("upsert y");

        let stores = vec![s];
        let chunks = get_document(&stores, Some("src-x"), "shared/doc.md").expect("get_document");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source_id, "src-x");
    }

    #[test]
    fn get_document_unknown_path_returns_empty() {
        let stores = vec![store()];
        let chunks = get_document(&stores, None, "no/such/doc.md").expect("get_document");
        assert!(chunks.is_empty());
    }

    // ── status ───────────────────────────────────────────────────────────────

    #[test]
    fn status_aggregates_counts_across_stores() {
        let primary = store();
        let secondary = store();
        upsert_source(&primary, &source("s-p")).expect("upsert source p");
        upsert_chunk(&primary, "s-p", &chunk("cp", "docs/p.md", "p content")).expect("upsert cp");
        upsert_source(&secondary, &source("s-s")).expect("upsert source s");
        upsert_chunk(
            &secondary,
            "s-s",
            &chunk("cs1", "docs/s1.md", "s content one"),
        )
        .expect("upsert cs1");
        upsert_chunk(
            &secondary,
            "s-s",
            &chunk("cs2", "docs/s2.md", "s content two"),
        )
        .expect("upsert cs2");

        let stores = vec![primary, secondary];
        let aggregate = status(&stores).expect("status");
        assert_eq!(aggregate.source_count, 2, "sources summed across stores");
        assert_eq!(aggregate.chunk_count, 3, "chunks summed across stores");
    }

    #[test]
    fn status_empty_stores_is_zeroed() {
        let stores: Vec<DataStore> = Vec::new();
        let aggregate = status(&stores).expect("status");
        assert_eq!(aggregate.source_count, 0);
        assert_eq!(aggregate.chunk_count, 0);
        assert_eq!(aggregate.schema_version, 0);
    }

    // ── research ─────────────────────────────────────────────────────────────

    #[test]
    fn research_without_model_combines_search_and_traversal() {
        let s = store();
        upsert_chunk(
            &s,
            "src-rt",
            &chunk("rt-a", "docs/rt-a.md", "graph traversal depth first search"),
        )
        .expect("upsert rt-a");
        upsert_chunk(
            &s,
            "src-rt",
            &chunk("rt-b", "docs/rt-b.md", "related graph topology node edge"),
        )
        .expect("upsert rt-b");
        upsert_edge(&s, &edge("rt-a", "docs/rt-b.md")).expect("edge");

        let stores = vec![s];
        let result = research(&stores, None, "graph traversal", 5, 1).expect("research");
        assert!(
            result.initial.iter().any(|r| r.chunk_id == "rt-a"),
            "expected rt-a in initial results, got {:?}",
            result.initial
        );
        assert!(
            result.related.iter().any(|r| r.chunk_id == "rt-b"),
            "expected rt-b discovered via traversal, got {:?}",
            result.related
        );
    }

    #[test]
    fn research_deduplicates_related_against_initial() {
        let s = store();
        upsert_chunk(&s, "src", &chunk("seed", "seed.md", "alpha beta gamma")).expect("seed");
        upsert_chunk(&s, "src", &chunk("other", "other.md", "alpha beta gamma")).expect("other");
        // seed links to other.md; other is also an initial hit, so it must not
        // be duplicated into the related set.
        upsert_edge(&s, &edge("seed", "other.md")).expect("edge");

        let stores = vec![s];
        let result = research(&stores, None, "alpha beta", 5, 2).expect("research");
        assert!(
            !result.related.iter().any(|r| r.chunk_id == "other"),
            "related must not duplicate an initial hit, got {:?}",
            result.related
        );
    }

    #[test]
    fn research_clamps_top_k_to_max_research_top_k() {
        let s = store();
        // More matching chunks than the research breadth clamp.
        for i in 0..(MAX_RESEARCH_TOP_K + 5) {
            upsert_chunk(
                &s,
                "src-rk",
                &chunk(
                    &format!("rk-{i}"),
                    &format!("docs/rk-{i}.md"),
                    "researchbreadthkeyword body",
                ),
            )
            .expect("upsert");
        }

        let stores = vec![s];
        let result =
            research(&stores, None, "researchbreadthkeyword", 100_000, 1).expect("research");
        assert_eq!(
            result.initial.len(),
            MAX_RESEARCH_TOP_K,
            "over-large research top_k must be clamped to MAX_RESEARCH_TOP_K"
        );
    }

    #[test]
    fn research_clamps_max_depth_to_max_research_depth() {
        let s = store();
        // Only rd-0 matches the query, so it is the sole seed. A linear chain
        // rd-0 -> rd-1 -> ... places rd-(MAX_RESEARCH_DEPTH+1) beyond the clamp.
        let chain_len = MAX_RESEARCH_DEPTH + 2;
        upsert_chunk(
            &s,
            "src-rd",
            &chunk("rd-0", "docs/rd-0.md", "researchdepthkeyword seed body"),
        )
        .expect("upsert seed");
        for i in 1..chain_len {
            upsert_chunk(
                &s,
                "src-rd",
                &chunk(&format!("rd-{i}"), &format!("docs/rd-{i}.md"), "chain body"),
            )
            .expect("upsert chain node");
        }
        for i in 0..(chain_len - 1) {
            upsert_edge(
                &s,
                &edge(&format!("rd-{i}"), &format!("docs/rd-{}.md", i + 1)),
            )
            .expect("edge");
        }

        let stores = vec![s];
        let result = research(&stores, None, "researchdepthkeyword", 5, 100_000).expect("research");
        assert!(
            result.related.iter().all(|r| r.depth <= MAX_RESEARCH_DEPTH),
            "related depth must be clamped to MAX_RESEARCH_DEPTH, got {:?}",
            result.related
        );
        let beyond = format!("rd-{}", chain_len - 1);
        assert!(
            !result.related.iter().any(|r| r.chunk_id == beyond),
            "the node beyond the clamp ({beyond}) must not be reached"
        );
    }

    // ── search_semantic (model-gated) ────────────────────────────────────────

    /// Model-gated: only runs when `GRAPHTOR_EMBED_MODEL_DIR` points to a local
    /// embedding model directory, so it never triggers a network download in
    /// CI.  When the model is unavailable the test skips cleanly.
    #[test]
    fn search_semantic_merges_across_stores_when_model_available() {
        if std::env::var(crate::embed::resolver::MODEL_DIR_ENV).is_err() {
            // No local model configured — skip to keep CI offline and fast.
            return;
        }
        let Some(model) =
            resolve_embedding_model(ResolverCaller::Query, false).expect("resolver must not error")
        else {
            // Model directory set but load failed — skip rather than fail.
            return;
        };

        let primary = store();
        let secondary = store();
        upsert_chunk(
            &primary,
            "src-a",
            &chunk("sem-a", "docs/a.md", "authentication token validation flow"),
        )
        .expect("upsert a");
        upsert_chunk(
            &secondary,
            "src-b",
            &chunk("sem-b", "docs/b.md", "authentication token validation flow"),
        )
        .expect("upsert b");
        let vec_a = crate::embed::embed_text(&model, "authentication token validation flow")
            .expect("embed a");
        let vec_b = crate::embed::embed_text(&model, "authentication token validation flow")
            .expect("embed b");
        crate::db::vectors::upsert_vector(&primary, "sem-a", &vec_a).expect("vector a");
        crate::db::vectors::upsert_vector(&secondary, "sem-b", &vec_b).expect("vector b");

        let stores = vec![primary, secondary];
        let results =
            search_semantic(&stores, &model, "authentication flow", 10).expect("semantic search");
        let ids: Vec<&str> = results.iter().map(|r| r.chunk_id.as_str()).collect();
        assert!(
            ids.contains(&"sem-a") && ids.contains(&"sem-b"),
            "semantic search should merge hits across stores, got {ids:?}"
        );
    }
}
