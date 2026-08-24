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

use crate::{db::DataStore, embed::EmbeddingModel, error::GraphtorError, query, sync::SyncMetrics};

use super::format::{
    format_chunk, format_db_status, format_document, format_research_results,
    format_search_results, format_sources_list, format_traversal_results,
};

// ── Sync status ───────────────────────────────────────────────────────────────

/// Background sync status reported by the `get_status` MCP tool.
///
/// Updated atomically by the `serve` command's background sync task and
/// read by the `get_status` tool.  Shared via `Arc<Mutex<SyncStatus>>`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// No background sync has been attempted (default state).
    #[default]
    Idle,
    /// Background sync is currently running.
    Syncing,
    /// Background sync is processing a specific source.
    InProgress {
        /// Source ID currently being synced.
        source: String,
        /// One-based source index currently in progress.
        current: usize,
        /// Total number of sources in this sync cycle.
        total: usize,
    },
    /// Background sync completed successfully.
    Done {
        /// Number of source files processed.
        files: usize,
        /// Number of chunks loaded into the store.
        chunks: usize,
    },
    /// Background sync completed and recorded structured metrics.
    Complete {
        /// Aggregate metrics for the completed sync cycle.
        metrics: SyncMetrics,
    },
    /// Background sync completed with errors.
    Error(String),
}

fn format_sync_status(status: &SyncStatus) -> String {
    match status {
        SyncStatus::Idle => "idle".to_string(),
        SyncStatus::Syncing => "syncing (background)".to_string(),
        SyncStatus::InProgress {
            source,
            current,
            total,
        } => format!("syncing: {source} ({current}/{total} sources)"),
        SyncStatus::Done { files, chunks } => format!("done ({files} files, {chunks} chunks)"),
        SyncStatus::Complete { metrics } => format!(
            "complete ({} files synced, {} chunks, {} deleted, {} errors, {} ms)",
            metrics.files_synced,
            metrics.chunks_created,
            metrics.files_deleted,
            metrics.errors,
            metrics.duration_ms
        ),
        SyncStatus::Error(msg) => format!("error: {msg}"),
    }
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
/// `research_topic`, and supporting tools backed by one or more embedded [`DataStore`]s.
/// The server is [`Clone`] because both [`DataStore`] and [`EmbeddingModel`] are
/// cheap [`std::sync::Arc`] clones internally.
///
/// Use [`DocServer::new`] or [`DocServer::with_stores`] to construct without
/// embeddings, [`DocServer::with_model`] or [`DocServer::with_stores_and_model`]
/// to add semantic search, and [`DocServer::with_sync_status`] to wire in
/// background sync status reporting from the `serve` command.
#[derive(Clone)]
pub struct DocServer {
    stores: Vec<DataStore>,
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
        Self::with_stores(store, Vec::new())
    }

    /// Create a new [`DocServer`] backed by multiple [`DataStore`] handles.
    ///
    /// `primary` is the first store and is used for operations that target a
    /// single database. `additional` may be empty.
    #[must_use]
    pub fn with_stores(primary: DataStore, additional: Vec<DataStore>) -> Self {
        let mut stores = Vec::with_capacity(additional.len() + 1);
        stores.push(primary);
        stores.extend(additional);
        Self {
            stores,
            model: None,
            sync_status: Arc::default(),
        }
    }

    /// Create a [`DocServer`] with an embedding model for semantic search.
    #[must_use]
    pub fn with_model(store: DataStore, model: EmbeddingModel) -> Self {
        Self::with_stores_and_model(store, Vec::new(), model)
    }

    /// Create a [`DocServer`] backed by multiple stores and a semantic model.
    #[must_use]
    pub fn with_stores_and_model(
        primary: DataStore,
        additional: Vec<DataStore>,
        model: EmbeddingModel,
    ) -> Self {
        let mut stores = Vec::with_capacity(additional.len() + 1);
        stores.push(primary);
        stores.extend(additional);
        Self {
            stores,
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
        // u32 always fits in usize on all 32-bit and 64-bit platforms we support.
        // The top_k maximum is clamped centrally in `query::search_text`.
        #[allow(clippy::cast_possible_truncation)]
        let limit = params.top_k.unwrap_or(10) as usize;
        let page = query::search_text(
            &self.stores,
            &params.query,
            params.source_id.as_deref(),
            limit,
        )
        .map_err(|e| into_tool_err(&e))?;
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
        // The max_depth maximum is clamped centrally in `query::traverse`.
        #[allow(clippy::cast_possible_truncation)]
        let depth = params.max_depth.unwrap_or(2) as usize;
        let results = query::traverse(&self.stores, &params.chunk_id, depth)
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
        // The top_k maximum is clamped centrally in `query::search_semantic`.
        #[allow(clippy::cast_possible_truncation)]
        let limit = params.top_k.unwrap_or(10) as usize;
        let results = query::search_semantic(&self.stores, model, &params.query, limit)
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
        let sources = query::list_sources(&self.stores).map_err(|e| into_tool_err(&e))?;
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
        let chunk =
            query::get_chunk(&self.stores, &params.chunk_id).map_err(|e| into_tool_err(&e))?;
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
        let chunks = query::get_document(&self.stores, Some(&params.source_id), &params.path)
            .map_err(|e| into_tool_err(&e))?;
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
        // The top_k and max_depth maxima are clamped centrally in `query::research`.
        #[allow(clippy::cast_possible_truncation)]
        let search_k = params.top_k.unwrap_or(5) as usize;
        #[allow(clippy::cast_possible_truncation)]
        let depth = params.max_depth.unwrap_or(1) as usize;

        // Prefer semantic (ranked) search when the embedding model is available;
        // fall back to unranked text search so seed selection is deterministic.
        // At most min(top_k, 3) of the top results seed the graph traversal.
        let results = query::research(
            &self.stores,
            self.model.as_ref(),
            &params.query,
            search_k,
            depth,
        )
        .map_err(|e| into_tool_err(&e))?;

        let md = format_research_results(&params.query, &results.initial, &results.related);
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
        let status = query::status(&self.stores).map_err(|e| into_tool_err(&e))?;
        let mut md = format_db_status(&status);

        let sync_str = {
            let guard = self
                .sync_status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            format_sync_status(&guard)
        };
        writeln!(md, "- **Auto-sync:** {sync_str}").expect("write to String is infallible");

        Ok(CallToolResult::success(vec![Content::text(md)]))
    }
}

// ── Manifest helpers ──────────────────────────────────────────────────────────

impl DocServer {
    /// Return all MCP tool definitions for the CLI `manifest` command.
    ///
    /// Constructs the tool router and extracts the [`rmcp::model::Tool`]
    /// attribute for every registered tool.  The returned list is sorted
    /// by name for deterministic, reproducible output.  Note that the
    /// live `tools/list` MCP response is unordered (`HashMap` iteration
    /// order), so the manifest mirrors the *set* of tools but not
    /// necessarily their order in a live server response.
    pub(crate) fn list_tools() -> Vec<rmcp::model::Tool> {
        let mut tools: Vec<rmcp::model::Tool> = Self::tool_router()
            .map
            .into_values()
            .map(|route| route.attr)
            .collect();
        // Sort by name for deterministic output order.
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }
}

// ── ServerHandler ─────────────────────────────────────────────────────────────

// The `#[tool_handler]` macro (rmcp-macros) generates `ServerHandler::call_tool`
// as an async trait method to satisfy the `ServerHandler` trait's async
// signature; its generated body dispatches synchronously through the tool
// router without ever reaching an `.await` point, which a newer clippy
// pedantic lint (added after this crate's original clippy baseline) flags on
// the macro-expanded code. The async signature is required by the trait
// contract, not chosen by this crate.
#[allow(clippy::unused_async_trait_impl)]
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
    use crate::db::{
        schema::ensure_schema, upsert_chunk, upsert_edge, upsert_source, DataStore, SourceRecord,
    };
    use crate::parse::types::{Chunk, Reference};

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

    #[test]
    fn with_sync_status_in_progress_appears_in_get_status() {
        use std::sync::{Arc, Mutex};
        let store = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&store).expect("schema");
        let status_arc: Arc<Mutex<SyncStatus>> = Arc::new(Mutex::new(SyncStatus::InProgress {
            source: "docs-source".to_string(),
            current: 2,
            total: 5,
        }));
        let server = DocServer::new(store).with_sync_status(Arc::clone(&status_arc));
        let result = server.get_status(Parameters(GetStatusParams {})).unwrap();
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("docs-source") && text.contains("2/5"),
            "expected source progress in status output, got: {text}"
        );
    }

    #[test]
    fn with_sync_status_complete_appears_in_get_status() {
        use std::sync::{Arc, Mutex};
        let store = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&store).expect("schema");
        let status_arc: Arc<Mutex<SyncStatus>> = Arc::new(Mutex::new(SyncStatus::Complete {
            metrics: crate::sync::SyncMetrics {
                files_total: 4,
                files_synced: 3,
                files_deleted: 1,
                chunks_created: 8,
                chunks_deleted: 0,
                duration_ms: 25,
                errors: 0,
            },
        }));
        let server = DocServer::new(store).with_sync_status(Arc::clone(&status_arc));
        let result = server.get_status(Parameters(GetStatusParams {})).unwrap();
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("complete")
                && text.contains('3')
                && text.contains('8')
                && text.contains("25"),
            "expected completion metrics in status output, got: {text}"
        );
    }

    #[test]
    fn with_sync_status_error_appears_in_get_status() {
        use std::sync::{Arc, Mutex};
        let store = DataStore::open_mem().expect("in-memory store");
        ensure_schema(&store).expect("schema");
        let status_arc: Arc<Mutex<SyncStatus>> = Arc::new(Mutex::new(SyncStatus::Error(
            "2 file(s) failed during sync (3 files synced, 8 chunks, 25 ms)".to_string(),
        )));
        let server = DocServer::new(store).with_sync_status(Arc::clone(&status_arc));
        let result = server.get_status(Parameters(GetStatusParams {})).unwrap();
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("error:")
                && text.contains('3')
                && text.contains('8')
                && text.contains('2')
                && text.contains("25"),
            "expected degraded completion metrics in status output, got: {text}"
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

    // ── Helpers for positive-path tests ──────────────────────────────────────

    fn populated_store() -> DataStore {
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

    // ── search_local_docs positive-path ───────────────────────────────────────

    #[test]
    fn search_local_docs_empty_query_returns_invalid_params() {
        let server = test_server();
        let params = SearchParams {
            query: "   ".to_string(),
            source_id: None,
            top_k: None,
        };
        let result = server.search_local_docs(Parameters(params));
        assert!(result.is_err(), "empty query should return an error");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("empty"),
            "error should mention empty query, got: {}",
            err.message
        );
    }

    #[test]
    fn search_local_docs_returns_matching_chunk() {
        let s = populated_store();
        upsert_chunk(
            &s,
            "src-a",
            &chunk(
                "auth-chunk",
                "docs/auth.md",
                "authentication token validation flow",
            ),
        )
        .expect("upsert");
        let server = DocServer::new(s);

        let params = SearchParams {
            query: "authentication token".to_string(),
            source_id: None,
            top_k: None,
        };
        let result = server
            .search_local_docs(Parameters(params))
            .expect("search should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("auth.md"),
            "expected result referencing auth.md, got: {text}"
        );
    }

    #[test]
    fn with_stores_search_aggregates_across_databases() {
        let primary = populated_store();
        let secondary = populated_store();

        upsert_chunk(
            &primary,
            "src-primary",
            &chunk(
                "primary-chunk",
                "docs/primary.md",
                "shared multi database query",
            ),
        )
        .expect("upsert primary");
        upsert_chunk(
            &secondary,
            "src-secondary",
            &chunk(
                "secondary-chunk",
                "docs/secondary.md",
                "shared multi database query",
            ),
        )
        .expect("upsert secondary");

        let server = DocServer::with_stores(primary, vec![secondary]);
        let params = SearchParams {
            query: "shared multi database".to_string(),
            source_id: None,
            top_k: Some(10),
        };
        let result = server
            .search_local_docs(Parameters(params))
            .expect("search should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("primary.md"),
            "expected primary result, got: {text}"
        );
        assert!(
            text.contains("secondary.md"),
            "expected secondary result, got: {text}"
        );
    }

    #[test]
    fn search_local_docs_uses_readonly_store_while_database_lock_is_held() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let db_root = workspace.path().join(".graphtor");
        std::fs::create_dir_all(&db_root).expect("create db root");
        let db_path = db_root.join("primary.db");

        let store = DataStore::open_sqlite(&db_path, workspace.path())
            .expect("open read-write sqlite store");
        ensure_schema(&store).expect("ensure schema");
        upsert_source(&store, &source("source-readonly")).expect("seed source");
        upsert_chunk(
            &store,
            "source-readonly",
            &chunk(
                "readonly-chunk",
                "docs/readonly.md",
                "shared read only search query",
            ),
        )
        .expect("seed chunk");

        let _lock = crate::lock::DatabaseLock::acquire(&db_root, &db_path, false)
            .expect("database lock should be acquired");
        let readonly = DataStore::open_sqlite_readonly(&db_path, workspace.path())
            .expect("open read-only sqlite store");

        let server = DocServer::new(readonly);
        let params = SearchParams {
            query: "shared read only".to_string(),
            source_id: None,
            top_k: Some(10),
        };
        let result = server
            .search_local_docs(Parameters(params))
            .expect("search should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("readonly.md"),
            "expected read-only result, got: {text}"
        );
    }

    #[test]
    fn search_local_docs_source_filter_restricts_results() {
        let s = populated_store();
        upsert_chunk(
            &s,
            "source-alpha",
            &chunk("ca-1", "docs/a.md", "blob storage container migration"),
        )
        .expect("upsert a");
        upsert_chunk(
            &s,
            "source-beta",
            &chunk("cb-1", "docs/b.md", "blob storage container migration"),
        )
        .expect("upsert b");
        let server = DocServer::new(s);

        let params = SearchParams {
            query: "blob storage".to_string(),
            source_id: Some("source-alpha".to_string()),
            top_k: None,
        };
        let result = server
            .search_local_docs(Parameters(params))
            .expect("search should succeed");
        let text = format!("{:?}", result.content);
        assert!(text.contains("a.md"), "should include source-alpha result");
        assert!(!text.contains("b.md"), "should exclude source-beta result");
    }

    #[test]
    fn search_local_docs_top_k_limits_result_count() {
        let s = populated_store();
        for i in 0..8_usize {
            upsert_chunk(
                &s,
                "src-x",
                &chunk(
                    &format!("tkc-{i}"),
                    &format!("docs/item-{i}.md"),
                    &format!("pagination rate limit throttle item {i}"),
                ),
            )
            .expect("upsert");
        }
        let server = DocServer::new(s);

        let params = SearchParams {
            query: "pagination rate limit".to_string(),
            source_id: None,
            top_k: Some(3),
        };
        let result = server
            .search_local_docs(Parameters(params))
            .expect("search should succeed");
        let text = format!("{:?}", result.content);
        // Count how many distinct item-N paths appear — must be ≤ 3.
        let hit_count = (0..8)
            .filter(|i| text.contains(&format!("item-{i}.md")))
            .count();
        assert!(
            hit_count <= 3,
            "expected at most 3 results, got {hit_count}"
        );
    }

    // ── traverse_doc_links positive-path ──────────────────────────────────────

    #[test]
    fn traverse_doc_links_via_server_finds_related_chunks() {
        let s = populated_store();
        upsert_chunk(&s, "src", &chunk("node-a", "a.md", "content a")).expect("upsert a");
        upsert_chunk(&s, "src", &chunk("node-b", "b.md", "content b")).expect("upsert b");
        upsert_edge(&s, &edge("node-a", "b.md")).expect("edge a→b");
        let server = DocServer::new(s);

        let params = TraverseParams {
            chunk_id: "node-a".to_string(),
            max_depth: Some(1),
        };
        let result = server
            .traverse_doc_links(Parameters(params))
            .expect("traverse should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("node-b") || text.contains("b.md"),
            "expected traversal to find node-b, got: {text}"
        );
    }

    #[test]
    fn traverse_doc_links_empty_chunk_id_returns_invalid_params() {
        let server = test_server();
        let params = TraverseParams {
            chunk_id: "   ".to_string(),
            max_depth: None,
        };
        let result = server.traverse_doc_links(Parameters(params));
        assert!(result.is_err(), "empty chunk_id should return an error");
    }

    // ── get_chunk_by_id positive-path ─────────────────────────────────────────

    #[test]
    fn get_chunk_by_id_returns_existing_chunk_content() {
        let s = populated_store();
        upsert_chunk(
            &s,
            "src-r",
            &chunk("known-chunk", "docs/known.md", "unique retrieval content"),
        )
        .expect("upsert");
        let server = DocServer::new(s);

        let params = GetChunkParams {
            chunk_id: "known-chunk".to_string(),
        };
        let result = server
            .get_chunk_by_id(Parameters(params))
            .expect("get should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("known.md") || text.contains("unique retrieval"),
            "expected chunk content in response, got: {text}"
        );
    }

    // ── get_document positive-path ────────────────────────────────────────────

    #[test]
    fn get_document_returns_chunks_in_reading_order() {
        let s = populated_store();
        // Insert chunks at positions 1 and 0 (out of order) for the same path.
        let mut c0 = chunk("doc-c0", "docs/guide.md", "first section content");
        c0.position = 0;
        let mut c1 = chunk("doc-c1", "docs/guide.md", "second section content");
        c1.position = 1;
        upsert_chunk(&s, "src-doc", &c1).expect("upsert c1 first");
        upsert_chunk(&s, "src-doc", &c0).expect("upsert c0 second");
        let server = DocServer::new(s);

        let params = GetDocumentParams {
            source_id: String::new(),
            path: "docs/guide.md".to_string(),
        };
        let result = server
            .get_document(Parameters(params))
            .expect("get_document should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("guide.md"),
            "expected path in response, got: {text}"
        );
        // Both chunks must appear AND "first section" (position 0) must
        // precede "second section" (position 1) — validating reading order.
        let pos0 = text
            .find("first section")
            .expect("expected first-section chunk in response");
        let pos1 = text
            .find("second section")
            .expect("expected second-section chunk in response");
        assert!(
            pos0 < pos1,
            "reading order: 'first section' (pos {pos0}) should precede 'second section' (pos {pos1})"
        );
    }

    #[test]
    fn get_document_source_filter_restricts_to_source() {
        let s = populated_store();
        upsert_chunk(
            &s,
            "src-x",
            &chunk("gdx-c", "shared/doc.md", "content from source x"),
        )
        .expect("upsert x");
        upsert_chunk(
            &s,
            "src-y",
            &chunk("gdy-c", "shared/doc.md", "content from source y"),
        )
        .expect("upsert y");
        let server = DocServer::new(s);

        let params = GetDocumentParams {
            source_id: "src-x".to_string(),
            path: "shared/doc.md".to_string(),
        };
        let result = server
            .get_document(Parameters(params))
            .expect("get_document should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("gdx-c") || text.contains("source x"),
            "expected src-x chunk in response, got: {text}"
        );
        assert!(
            !text.contains("gdy-c") && !text.contains("source y"),
            "should not include src-y chunk, got: {text}"
        );
    }

    // ── list_sources positive-path ────────────────────────────────────────────

    #[test]
    fn list_sources_returns_inserted_sources() {
        let s = populated_store();
        upsert_source(&s, &source("repo-one")).expect("upsert src 1");
        upsert_source(&s, &source("repo-two")).expect("upsert src 2");
        let server = DocServer::new(s);

        let result = server
            .list_sources(Parameters(ListSourcesParams {}))
            .expect("list_sources should succeed");
        let text = format!("{:?}", result.content);
        assert!(
            text.contains("repo-one"),
            "expected repo-one in sources list, got: {text}"
        );
        assert!(
            text.contains("repo-two"),
            "expected repo-two in sources list, got: {text}"
        );
    }

    // ── research_topic with data ──────────────────────────────────────────────

    #[test]
    fn research_topic_without_model_returns_combined_results() {
        let s = populated_store();
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
        let server = DocServer::new(s); // no model — falls back to text search

        let params = ResearchTopicParams {
            query: "graph traversal".to_string(),
            top_k: Some(5),
            max_depth: Some(1),
        };
        let result = server
            .research_topic(Parameters(params))
            .expect("research_topic should succeed");
        let text = format!("{:?}", result.content);
        // Should surface the direct match.
        assert!(
            text.contains("rt-a.md") || text.contains("graph traversal"),
            "expected direct match in results, got: {text}"
        );
    }

    // ── get_status with data ──────────────────────────────────────────────────

    #[test]
    fn get_status_reflects_inserted_data_counts() {
        let s = populated_store();
        upsert_source(&s, &source("status-src")).expect("upsert source");
        upsert_chunk(
            &s,
            "status-src",
            &chunk("st-c1", "docs/s1.md", "status test chunk one"),
        )
        .expect("upsert c1");
        upsert_chunk(
            &s,
            "status-src",
            &chunk("st-c2", "docs/s2.md", "status test chunk two"),
        )
        .expect("upsert c2");
        let server = DocServer::new(s);

        let result = server
            .get_status(Parameters(GetStatusParams {}))
            .expect("get_status should succeed");
        let text = format!("{:?}", result.content);
        // The status markdown uses the format produced by `format_db_status`:
        // "- **Chunks:** N" and "- **Sources:** N".
        assert!(
            text.contains("**Chunks:** 2"),
            "expected '**Chunks:** 2' in status, got: {text}"
        );
        assert!(
            text.contains("**Sources:** 1"),
            "expected '**Sources:** 1' in status, got: {text}"
        );
    }
}
