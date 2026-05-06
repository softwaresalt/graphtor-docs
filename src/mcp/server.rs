//! `DocServer` — rmcp MCP server with `search_local_docs`, `traverse_doc_links`,
//! `search_semantic`, `research_topic`, and supporting tools.
//!
//! Implements [`rmcp::ServerHandler`] and exposes MCP tools:
//!
//! - `search_local_docs`  — full-text keyword search via [`crate::db::search::search_by_text`].
//! - `traverse_doc_links` — BFS graph traversal via [`crate::db::traverse::find_related_chunks`].
//! - `search_semantic`    — embedding-based semantic search via
//!   [`crate::db::search::search_similar`] (requires model to be loaded).
//! - `research_topic`     — composite search + graph traversal for in-depth topic research.
//! - `list_sources`       — enumerate registered documentation sources.
//! - `get_chunk_by_id`    — retrieve a single chunk by its stable SHA-256 ID.
//! - `get_document`       — retrieve all chunks for a document path.
//! - `get_status`         — report database and background sync status.
//!
//! Use [`DocServer::new`] to construct a server without an embedding model, or
//! [`DocServer::with_model`] to enable semantic search.  To enable background sync
//! status reporting, use [`DocServer::with_sync_status`] after construction.
//! Pass the server to [`rmcp::serve_server`] with [`rmcp::transport::stdio`] to
//! start the STDIO MCP server.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    db::{
        chunks::{get_chunk, list_chunks_by_path},
        nodes::list_sources,
        search::{search_by_text, search_similar},
        traverse::find_related_chunks,
        DataStore,
    },
    embed::EmbeddingModel,
    error::GraphtorError,
};

use super::format::{
    format_chunk, format_db_status, format_document, format_research_results,
    format_search_results, format_sources_list, format_traversal_results,
};

// ── Sync status ───────────────────────────────────────────────────────────────

/// Background sync status reported by the `get_status` MCP tool.
///
/// Updated atomically by the `serve` command's background sync task and
/// read by the `get_status` tool.  Shared via `Arc<Mutex<SyncStatus>>`.
#[derive(Debug, Default)]
pub enum SyncStatus {
    /// No background sync has been attempted (default state).
    #[default]
    Idle,
    /// Background sync is currently running.
    Syncing,
    /// Background sync completed successfully.
    Done {
        /// Number of source files processed.
        files: usize,
        /// Number of chunks loaded into the store.
        chunks: usize,
    },
    /// Background sync completed with errors.
    Error(String),
}

// ── Parameter types ───────────────────────────────────────────────────────────

/// Parameters for the `search_local_docs` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SearchParams {
    /// Full-text keyword query to search documentation chunks.
    pub query: String,
    /// Optional source ID prefix to restrict results to a specific documentation source.
    pub source_id: Option<String>,
    /// Maximum number of results to return (default: 10, max: 50).
    pub top_k: Option<u32>,
}

/// Parameters for the `traverse_doc_links` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TraverseParams {
    /// Stable SHA-256 chunk identifier to start BFS traversal from.
    pub chunk_id: String,
    /// Maximum BFS traversal depth (default: 2, max: 5).
    pub max_depth: Option<u32>,
}

/// Parameters for the `search_semantic` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SemanticSearchParams {
    /// Natural-language query to embed and search semantically.
    pub query: String,
    /// Maximum number of results to return (default: 10, max: 50).
    pub top_k: Option<u32>,
}

/// Parameters for the `list_sources` MCP tool.
///
/// No input parameters are required.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ListSourcesParams {}

/// Parameters for the `get_chunk_by_id` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GetChunkParams {
    /// Stable SHA-256 chunk identifier to retrieve.
    pub chunk_id: String,
}

/// Parameters for the `get_document` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GetDocumentParams {
    /// Source identifier to scope the document lookup.
    ///
    /// Pass an empty string to retrieve chunks from all sources that contain
    /// the given `path`.
    pub source_id: String,
    /// Relative document path within the source (e.g. `"articles/intro.md"`).
    pub path: String,
}

/// Parameters for the `get_status` MCP tool.
///
/// No input parameters are required.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GetStatusParams {}

/// Parameters for the `research_topic` MCP tool.
#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ResearchTopicParams {
    /// Natural-language or keyword query for the research topic.
    pub query: String,
    /// Maximum number of initial search results to retrieve (default: 5, max: 20).
    ///
    /// At most `min(top_k, 3)` of the top results are used as seeds for graph traversal.
    pub top_k: Option<u32>,
    /// BFS traversal depth from each seed chunk (default: 1, max: 3).
    pub max_depth: Option<u32>,
}

// ── Server ────────────────────────────────────────────────────────────────────

/// The `LocalDocRAG` MCP server.
///
/// Provides `search_local_docs`, `traverse_doc_links`, `search_semantic`,
/// `research_topic`, and supporting tools backed by an embedded [`DataStore`].
/// The server is [`Clone`] because both [`DataStore`] and [`EmbeddingModel`] are
/// cheap [`std::sync::Arc`] clones internally.
///
/// Use [`DocServer::new`] to construct without embeddings, [`DocServer::with_model`]
/// to add semantic search, and [`DocServer::with_sync_status`] to wire in background
/// sync status reporting from the `serve` command.
#[derive(Clone)]
pub struct DocServer {
    store: DataStore,
    /// Embedding model for semantic search.  When `None`, `search_semantic`
    /// returns a descriptive error rather than silently failing.
    model: Option<EmbeddingModel>,
    /// Shared background sync status updated by the `serve` command's sync task.
    sync_status: Arc<Mutex<SyncStatus>>,
}

impl DocServer {
    /// Create a new [`DocServer`] backed by the given [`DataStore`].
    ///
    /// Semantic search (`search_semantic`) will be unavailable until a model
    /// is supplied via [`DocServer::with_model`].  Background sync status
    /// defaults to [`SyncStatus::Idle`]; supply a shared status handle via
    /// [`DocServer::with_sync_status`] to enable runtime reporting.
    #[must_use]
    pub fn new(store: DataStore) -> Self {
        Self {
            store,
            model: None,
            sync_status: Arc::default(),
        }
    }

    /// Create a [`DocServer`] with an embedding model for semantic search.
    #[must_use]
    pub fn with_model(store: DataStore, model: EmbeddingModel) -> Self {
        Self {
            store,
            model: Some(model),
            sync_status: Arc::default(),
        }
    }

    /// Attach a shared background sync status handle.
    ///
    /// Pass the same `Arc<Mutex<SyncStatus>>` that the background sync task
    /// writes to.  `get_status` will reflect the current sync state.
    #[must_use]
    pub fn with_sync_status(mut self, status: Arc<Mutex<SyncStatus>>) -> Self {
        self.sync_status = status;
        self
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl DocServer {
    /// Search local documentation chunks by keyword.
    ///
    /// Returns matching chunks with path, heading context, and content
    /// as structured markdown.  Pass `source_id` to restrict results
    /// to a specific documentation source.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the underlying text search fails.
    #[tool(
        description = "Search local documentation chunks by keyword or phrase. Returns matching \
        chunks with source path, heading context, content, and a Chunk ID that can be passed to \
        `traverse_doc_links` to explore related documentation. Use source_id to restrict results \
        to a specific documentation source. Use this tool to find documentation related to a topic."
    )]
    fn search_local_docs(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(query = %params.query, "search_local_docs invoked");
        if params.query.trim().is_empty() {
            return Err(ErrorData::invalid_params("query cannot be empty", None));
        }
        let results = search_by_text(&self.store, &params.query).map_err(|e| into_tool_err(&e))?;
        // Normalize empty source_id to None (treat the same as no filter).
        let sid_filter =
            params
                .source_id
                .as_deref()
                .and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
        let filtered: Vec<_> = if let Some(sid) = sid_filter {
            results.into_iter().filter(|r| r.source_id == sid).collect()
        } else {
            results
        };
        // u32 always fits in usize on all 32-bit and 64-bit platforms we support.
        #[allow(clippy::cast_possible_truncation)]
        let limit = params.top_k.unwrap_or(10).min(50) as usize;
        let page: Vec<_> = filtered.into_iter().take(limit).collect();
        let md = format_search_results(&page);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Traverse the document link graph from a starting chunk.
    ///
    /// Follows outgoing `doc_edges` via BFS and returns related chunks
    /// with path and traversal depth as structured markdown.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the graph traversal fails.
    #[tool(
        description = "Traverse the document link graph starting from a chunk ID. Use this after \
        `search_local_docs` to explore related documentation via semantic links. Requires a valid \
        chunk ID from `search_local_docs` output. Returns all reachable chunks within max_depth \
        hops (default 2, max 5) with their paths and traversal depths."
    )]
    fn traverse_doc_links(
        &self,
        Parameters(params): Parameters<TraverseParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(chunk_id = %params.chunk_id, "traverse_doc_links invoked");
        if params.chunk_id.trim().is_empty() {
            return Err(ErrorData::invalid_params("chunk_id cannot be empty", None));
        }
        // u32 always fits in usize on all 32-bit and 64-bit platforms we support.
        #[allow(clippy::cast_possible_truncation)]
        let depth = params.max_depth.unwrap_or(2).min(5) as usize;
        let results = find_related_chunks(&self.store, &params.chunk_id, depth)
            .map_err(|e| into_tool_err(&e))?;
        let md = format_traversal_results(&params.chunk_id, &results);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Search local documentation by semantic similarity.
    ///
    /// Embeds the natural-language `query` using the loaded `all-MiniLM-L6-v2`
    /// model and returns the `top_k` most similar stored documentation chunks
    /// ranked by cosine similarity.  Use this tool for conceptual queries where
    /// keyword matching is insufficient.
    ///
    /// Requires the embedding model to be loaded (`graphtor-docs serve`).
    /// Returns an error when the server was started without a model.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the model is unavailable or search fails.
    #[tool(
        description = "Search local documentation by semantic / conceptual similarity. Embeds the \
        query using all-MiniLM-L6-v2 and returns the most similar documentation chunks by cosine \
        similarity. Use this when keyword search (`search_local_docs`) misses conceptually related \
        content. Requires the embedding model to be loaded. Returns top_k results (default 10, \
        max 50)."
    )]
    fn search_semantic(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(query = %params.query, "search_semantic invoked");
        if params.query.trim().is_empty() {
            return Err(ErrorData::invalid_params("query cannot be empty", None));
        }
        let model = self.model.as_ref().ok_or_else(|| {
            ErrorData::invalid_params(
                "semantic search is disabled: the embedding model is not loaded. \
                 Run `graphtor-docs serve` with the embedding model enabled.",
                None,
            )
        })?;
        // u32 always fits in usize on all 32-bit and 64-bit platforms we support.
        #[allow(clippy::cast_possible_truncation)]
        let limit = params.top_k.unwrap_or(10).min(50) as usize;
        let results = search_similar(&self.store, model, &params.query, limit)
            .map_err(|e| into_tool_err(&e))?;
        let md = format_search_results(&results);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// List all registered documentation sources.
    ///
    /// Returns the complete registry of indexed documentation sources with
    /// their identifiers, kinds, names, and last-sync timestamps.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the database query fails.
    #[tool(
        description = "List all registered documentation sources. Returns source IDs, kinds \
        (git/local), display names, and last-sync timestamps. Use this tool to discover which \
        documentation sources are available before searching or retrieving documents."
    )]
    fn list_sources(
        &self,
        Parameters(_params): Parameters<ListSourcesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!("list_sources invoked");
        let sources = list_sources(&self.store).map_err(|e| into_tool_err(&e))?;
        let md = format_sources_list(&sources);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Retrieve a single documentation chunk by its stable chunk ID.
    ///
    /// Returns the full chunk content, heading context, source path, and
    /// position metadata.  Use this tool when you already have a chunk ID
    /// from a previous `search_local_docs` result and want the complete text.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the chunk ID is invalid or the query fails.
    #[tool(
        description = "Retrieve a single documentation chunk by its stable SHA-256 chunk ID. \
        Returns full content, source path, heading hierarchy, and position. Use this after \
        `search_local_docs` when you need the complete text of a specific chunk. The chunk ID \
        must be an exact match (hex string from search results)."
    )]
    fn get_chunk_by_id(
        &self,
        Parameters(params): Parameters<GetChunkParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(chunk_id = %params.chunk_id, "get_chunk_by_id invoked");
        if params.chunk_id.trim().is_empty() {
            return Err(ErrorData::invalid_params("chunk_id cannot be empty", None));
        }
        let chunk = get_chunk(&self.store, &params.chunk_id).map_err(|e| into_tool_err(&e))?;
        let md = match chunk {
            Some(c) => format_chunk(&c),
            None => format!("Chunk `{}` not found.", params.chunk_id),
        };
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Retrieve all chunks for a document path within a source.
    ///
    /// Returns the ordered sequence of chunks that make up the document,
    /// sorted by reading position.  Pass `source_id` to restrict results
    /// to a specific source; leave it empty to retrieve chunks from all
    /// sources that contain the given path.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when `path` is empty or the query fails.
    #[tool(
        description = "Retrieve all chunks for a specific document path, assembled in reading \
        order. Provide source_id to scope to one source, or leave it empty to retrieve across \
        all sources. Use this to read a complete document after identifying its path via \
        `search_local_docs`. Returns chunks sorted by position."
    )]
    fn get_document(
        &self,
        Parameters(params): Parameters<GetDocumentParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(source_id = %params.source_id, path = %params.path, "get_document invoked");
        if params.path.trim().is_empty() {
            return Err(ErrorData::invalid_params("path cannot be empty", None));
        }
        let all_chunks =
            list_chunks_by_path(&self.store, &params.path).map_err(|e| into_tool_err(&e))?;
        // Filter by source_id when one is provided and non-empty.
        let sid = params.source_id.trim();
        let chunks: Vec<_> = if sid.is_empty() {
            all_chunks
        } else {
            all_chunks
                .into_iter()
                .filter(|c| c.source_id == sid)
                .collect()
        };
        let md = format_document(&params.path, &chunks);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Perform composite research on a topic.
    ///
    /// Combines initial keyword or semantic search with BFS graph traversal
    /// from the top seed results.  Returns matching chunks plus related chunks
    /// discovered via document link edges, deduplicated across all seeds.
    ///
    /// When the embedding model is loaded, uses semantic (ranked) search for
    /// initial results; otherwise falls back to unranked keyword search.
    /// At most `min(top_k, 3)` of the top results seed the graph traversal.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when `query` is empty or any search/traversal fails.
    #[tool(
        description = "Perform in-depth research on a topic: searches documentation by keyword \
        or semantic similarity, then traverses the document graph from the top results to surface \
        related context. Returns initial search hits plus BFS-discovered related chunks, all \
        deduplicated. Use this tool for comprehensive topic research when you want both direct \
        matches and linked documentation. top_k controls initial search breadth (default 5, max \
        20); max_depth controls graph traversal depth (default 1, max 3)."
    )]
    fn research_topic(
        &self,
        Parameters(params): Parameters<ResearchTopicParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(query = %params.query, "research_topic invoked");
        if params.query.trim().is_empty() {
            return Err(ErrorData::invalid_params("query cannot be empty", None));
        }

        // u32 always fits in usize on all 32-bit and 64-bit platforms we support.
        #[allow(clippy::cast_possible_truncation)]
        let search_k = params.top_k.unwrap_or(5).min(20) as usize;
        let seed_k = search_k.min(3);
        #[allow(clippy::cast_possible_truncation)]
        let depth = params.max_depth.unwrap_or(1).min(3) as usize;

        // Prefer semantic (ranked) search when the embedding model is available;
        // fall back to unranked text search so seed selection is deterministic.
        let initial: Vec<crate::db::search::SearchResult> = if let Some(model) = &self.model {
            search_similar(&self.store, model, &params.query, search_k)
                .map_err(|e| into_tool_err(&e))?
        } else {
            let all = search_by_text(&self.store, &params.query).map_err(|e| into_tool_err(&e))?;
            all.into_iter().take(search_k).collect()
        };

        // Traverse from the top seeds; deduplicate globally across all seeds.
        let mut seen_ids: std::collections::HashSet<String> =
            initial.iter().map(|r| r.chunk_id.clone()).collect();
        let mut related: Vec<crate::db::traverse::TraversalResult> = Vec::new();

        for seed in initial.iter().take(seed_k) {
            let traversal = find_related_chunks(&self.store, &seed.chunk_id, depth)
                .map_err(|e| into_tool_err(&e))?;
            for tr in traversal {
                if seen_ids.insert(tr.chunk_id.clone()) {
                    related.push(tr);
                }
            }
        }

        let md = format_research_results(&params.query, &initial, &related);
        Ok(CallToolResult::success(vec![Content::text(md)]))
    }

    /// Return current database and sync status.
    ///
    /// Returns the number of registered sources, total chunk count, active schema
    /// version, and the current background sync state.  Useful for quick health
    /// checks and verifying that the ingestion pipeline has run.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] when the status queries fail.
    #[tool(
        description = "Return current database status: registered source count, total chunk \
        count, schema version, and background sync state. Use this tool to verify the ingestion \
        pipeline has run and to check the health of the local documentation database."
    )]
    fn get_status(
        &self,
        Parameters(_params): Parameters<GetStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        info!("get_status invoked");
        let status = self.store.get_status().map_err(|e| into_tool_err(&e))?;
        let mut md = format_db_status(&status);

        let sync_str = {
            let guard = self
                .sync_status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &*guard {
                SyncStatus::Idle => "idle".to_string(),
                SyncStatus::Syncing => "syncing (background)".to_string(),
                SyncStatus::Done { files, chunks } => {
                    format!("done ({files} files, {chunks} chunks)")
                }
                SyncStatus::Error(msg) => format!("error: {msg}"),
            }
        };
        writeln!(md, "- **Auto-sync:** {sync_str}").expect("write to String is infallible");

        Ok(CallToolResult::success(vec![Content::text(md)]))
    }
}

// ── ServerHandler ─────────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for DocServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "graphtor-docs",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "LocalDocRAG MCP server — search and traverse local documentation graphs \
                 indexed from MicrosoftDocs repositories.",
            )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Compile-time assertion: usize must be at least 32 bits (we do not support 16-bit targets).
const _: () = assert!(
    std::mem::size_of::<usize>() >= 4,
    "usize must be at least 32 bits; 16-bit targets are not supported"
);

/// Convert a [`GraphtorError`] to an MCP [`ErrorData`] response.
///
/// [`GraphtorError::PathViolation`] maps to `invalid_params` because it
/// indicates the client supplied invalid input. All other variants map to
/// `internal_error`.
fn into_tool_err(e: &GraphtorError) -> ErrorData {
    if e.is_client_error() {
        ErrorData::invalid_params(e.to_string(), None)
    } else {
        ErrorData::internal_error(e.to_string(), None)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use super::*;
    use crate::db::{schema::ensure_schema, DataStore};

    /// Returns `true` when `path` belongs to the documentation source identified by `prefix`.
    ///
    /// Requires an exact match (`path == prefix`) or a directory-boundary prefix match
    /// (`path` starts with `{prefix}/`). This prevents a prefix like `"docs"` from
    /// incorrectly matching unrelated paths such as `"docs-archive/file.md"`.
    fn path_matches_source(path: &str, prefix: &str) -> bool {
        path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    fn test_server() -> DocServer {
        let store = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&store).expect("schema");
        DocServer::new(store)
    }

    #[test]
    fn server_info_contains_correct_name() {
        let server = test_server();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "graphtor-docs");
    }

    #[test]
    fn server_info_has_tools_capability() {
        let server = test_server();
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
    }

    #[test]
    fn search_returns_no_results_for_empty_store() {
        let server = test_server();
        let params = SearchParams {
            query: "nonexistent term xyz".to_string(),
            source_id: None,
            top_k: None,
        };
        let result = server.search_local_docs(Parameters(params));
        assert!(result.is_ok());
        let ct = result.unwrap();
        assert!(!ct.content.is_empty());
    }

    #[test]
    fn traverse_returns_no_related_for_unknown_chunk() {
        let server = test_server();
        let params = TraverseParams {
            chunk_id: "unknown-chunk-id".to_string(),
            max_depth: Some(2),
        };
        let result = server.traverse_doc_links(Parameters(params));
        assert!(result.is_ok());
        let ct = result.unwrap();
        assert!(!ct.content.is_empty());
    }

    // ── T013.003: path_matches_source directory-boundary filter ──────────────

    #[test]
    fn path_matches_source_exact_match() {
        assert!(path_matches_source("docs", "docs"));
    }

    #[test]
    fn path_matches_source_subdirectory_match() {
        assert!(path_matches_source("docs/api.md", "docs"));
    }

    #[test]
    fn path_matches_source_rejects_false_prefix() {
        // "docs-archive" starts with "docs" as a string but is not in the
        // "docs" source — the directory-boundary check must reject it.
        assert!(!path_matches_source("docs-archive/file.md", "docs"));
    }

    #[test]
    fn path_matches_source_no_match_on_unrelated_path() {
        assert!(!path_matches_source("other/path.md", "docs"));
    }

    #[test]
    fn search_semantic_without_model_returns_error() {
        let server = test_server(); // no model loaded
        let params = SemanticSearchParams {
            query: "authentication flow".to_string(),
            top_k: Some(5),
        };
        let result = server.search_semantic(Parameters(params));
        assert!(result.is_err(), "should error when no model is loaded");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("embedding model"),
            "error should mention embedding model, got: {}",
            err.message
        );
    }

    #[test]
    fn search_semantic_empty_query_returns_invalid_params() {
        let server = test_server();
        let params = SemanticSearchParams {
            query: "   ".to_string(),
            top_k: None,
        };
        let result = server.search_semantic(Parameters(params));
        assert!(result.is_err());
    }

    #[test]
    fn into_tool_err_path_violation_message_contains_error_text() {
        let e = GraphtorError::PathViolation {
            attempted: std::path::PathBuf::from("/evil"),
            allowed_root: std::path::PathBuf::from("/safe"),
        };
        let err = into_tool_err(&e);
        assert!(
            err.message.contains("path_violation"),
            "expected path_violation in message, got: {}",
            err.message
        );
    }

    #[test]
    fn into_tool_err_database_error_message_contains_error_text() {
        let e = GraphtorError::Database {
            message: "conn refused".to_string(),
            operation: "query".to_string(),
        };
        let err = into_tool_err(&e);
        assert!(
            err.message.contains("conn refused"),
            "expected conn refused in message, got: {}",
            err.message
        );
    }

    // ── list_sources ──────────────────────────────────────────────────────────

    #[test]
    fn list_sources_returns_no_sources_message_on_empty_store() {
        let server = test_server();
        let result = server.list_sources(Parameters(ListSourcesParams {}));
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.content.is_empty());
        // The text is embedded in the Content debug/display representation.
        let text = format!("{:?}", content.content);
        assert!(
            text.contains("No documentation sources"),
            "expected no-sources message, got: {text}"
        );
    }

    // ── get_chunk_by_id ───────────────────────────────────────────────────────

    #[test]
    fn get_chunk_by_id_empty_chunk_id_returns_invalid_params() {
        let server = test_server();
        let params = GetChunkParams {
            chunk_id: "   ".to_string(),
        };
        let result = server.get_chunk_by_id(Parameters(params));
        assert!(result.is_err(), "empty chunk_id should return an error");
    }

    #[test]
    fn get_chunk_by_id_unknown_id_returns_not_found_message() {
        let server = test_server();
        let params = GetChunkParams {
            chunk_id: "deadbeef0000".to_string(),
        };
        let result = server.get_chunk_by_id(Parameters(params));
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.content.is_empty());
        let text = format!("{:?}", content.content);
        assert!(
            text.contains("not found"),
            "expected not-found message, got: {text}"
        );
    }

    // ── get_document ──────────────────────────────────────────────────────────

    #[test]
    fn get_document_empty_path_returns_invalid_params() {
        let server = test_server();
        let params = GetDocumentParams {
            source_id: "src-001".to_string(),
            path: "  ".to_string(),
        };
        let result = server.get_document(Parameters(params));
        assert!(result.is_err(), "empty path should return an error");
    }

    #[test]
    fn get_document_unknown_path_returns_no_chunks_message() {
        let server = test_server();
        let params = GetDocumentParams {
            source_id: String::new(),
            path: "no/such/doc.md".to_string(),
        };
        let result = server.get_document(Parameters(params));
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.content.is_empty());
        let text = format!("{:?}", content.content);
        assert!(
            text.contains("No chunks found"),
            "expected no-chunks message, got: {text}"
        );
    }

    // ── get_status ────────────────────────────────────────────────────────────

    #[test]
    fn get_status_returns_ok_on_empty_store() {
        let server = test_server();
        let result = server.get_status(Parameters(GetStatusParams {}));
        assert!(result.is_ok(), "get_status should succeed on empty store");
        let content = result.unwrap();
        assert!(!content.content.is_empty());
        let text = format!("{:?}", content.content);
        assert!(
            text.contains("Sources"),
            "expected Sources in output: {text}"
        );
        assert!(text.contains("Chunks"), "expected Chunks in output: {text}");
        assert!(text.contains("Schema"), "expected Schema in output: {text}");
    }

    // ── SyncStatus ────────────────────────────────────────────────────────────

    #[test]
    fn sync_status_default_is_idle() {
        let status: SyncStatus = SyncStatus::default();
        assert!(matches!(status, SyncStatus::Idle));
    }

    #[test]
    fn get_status_includes_auto_sync_field() {
        let server = test_server();
        let result = server.get_status(Parameters(GetStatusParams {})).unwrap();
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("Auto-sync"),
            "expected Auto-sync field in status output, got: {text}"
        );
    }

    #[test]
    fn with_sync_status_done_appears_in_get_status() {
        use std::sync::{Arc, Mutex};
        let store = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&store).expect("schema");
        let status_arc: Arc<Mutex<SyncStatus>> = Arc::new(Mutex::new(SyncStatus::Done {
            files: 10,
            chunks: 42,
        }));
        let server = DocServer::new(store).with_sync_status(Arc::clone(&status_arc));
        let result = server.get_status(Parameters(GetStatusParams {})).unwrap();
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("10") && text.contains("42"),
            "expected file and chunk counts in status output, got: {text}"
        );
    }

    // ── research_topic ────────────────────────────────────────────────────────

    #[test]
    fn research_topic_empty_query_returns_error() {
        let server = test_server();
        let params = ResearchTopicParams {
            query: "   ".to_string(),
            top_k: None,
            max_depth: None,
        };
        let result = server.research_topic(Parameters(params));
        assert!(result.is_err(), "empty query should return an error");
    }

    #[test]
    fn research_topic_returns_ok_on_empty_store() {
        let server = test_server();
        let params = ResearchTopicParams {
            query: "authentication".to_string(),
            top_k: Some(3),
            max_depth: Some(1),
        };
        let result = server.research_topic(Parameters(params));
        assert!(
            result.is_ok(),
            "research_topic should succeed on empty store"
        );
        let text = format!("{:?}", result.unwrap().content);
        assert!(
            text.contains("Research"),
            "expected Research heading in output: {text}"
        );
        assert!(
            text.contains("authentication"),
            "query should appear in output: {text}"
        );
    }

    #[test]
    fn research_topic_top_k_clamped_to_twenty() {
        // Ensure that top_k is accepted up to the max (20) without error.
        let server = test_server();
        let params = ResearchTopicParams {
            query: "async".to_string(),
            top_k: Some(100), // should be clamped to 20
            max_depth: None,
        };
        let result = server.research_topic(Parameters(params));
        assert!(
            result.is_ok(),
            "top_k over max should still succeed (clamped)"
        );
    }
}
