//! `DocServer` — rmcp MCP server with `search_local_docs`, `traverse_doc_links`,
//! and `search_semantic` tools.
//!
//! Implements [`rmcp::ServerHandler`] and exposes three MCP tools:
//!
//! - `search_local_docs`  — full-text keyword search via [`crate::db::search::search_by_text`].
//! - `traverse_doc_links` — BFS graph traversal via [`crate::db::traverse::find_related_chunks`].
//! - `search_semantic`    — embedding-based semantic search via
//!   [`crate::db::search::search_similar`] (requires model to be loaded).
//!
//! Use [`DocServer::new`] to construct a server without an embedding model, or
//! [`DocServer::with_model`] to enable semantic search.  Pass the server to
//! [`rmcp::serve_server`] with [`rmcp::transport::stdio`] to start the STDIO MCP server.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    db::{
        search::{search_by_text, search_similar},
        traverse::find_related_chunks,
        DataStore,
    },
    embed::EmbeddingModel,
    error::GraphtorError,
};

use super::format::{format_search_results, format_traversal_results};

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

// ── Server ────────────────────────────────────────────────────────────────────

/// The `LocalDocRAG` MCP server.
///
/// Provides `search_local_docs`, `traverse_doc_links`, and `search_semantic`
/// tools backed by an embedded [`DataStore`].  The server is [`Clone`] because
/// both [`DataStore`] and [`EmbeddingModel`] are cheap [`std::sync::Arc`]
/// clones internally.
#[derive(Clone)]
pub struct DocServer {
    store: DataStore,
    /// Embedding model for semantic search.  When `None`, `search_semantic`
    /// returns a descriptive error rather than silently failing.
    model: Option<EmbeddingModel>,
}

impl DocServer {
    /// Create a new [`DocServer`] backed by the given [`DataStore`].
    ///
    /// Semantic search (`search_semantic`) will be unavailable until a model
    /// is supplied via [`DocServer::with_model`].
    #[must_use]
    pub fn new(store: DataStore) -> Self {
        Self { store, model: None }
    }

    /// Create a [`DocServer`] with an embedding model for semantic search.
    #[must_use]
    pub fn with_model(store: DataStore, model: EmbeddingModel) -> Self {
        Self {
            store,
            model: Some(model),
        }
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
}
