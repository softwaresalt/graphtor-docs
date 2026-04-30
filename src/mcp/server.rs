//! `DocServer` — rmcp MCP server with `search_local_docs` and `traverse_doc_links` tools.
//!
//! Implements [`rmcp::ServerHandler`] and exposes two MCP tools:
//!
//! - `search_local_docs` — full-text keyword search using [`crate::db::search::search_by_text`].
//! - `traverse_doc_links` — BFS graph traversal using [`crate::db::traverse::find_related_chunks`].
//!
//! Use [`DocServer::new`] to construct a server from a [`DataStore`], then pass it to
//! [`rmcp::serve_server`] with [`rmcp::transport::stdio`] to start the STDIO MCP server.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    db::{search::search_by_text, traverse::find_related_chunks, DataStore},
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

// ── Server ────────────────────────────────────────────────────────────────────

/// The `LocalDocRAG` MCP server.
///
/// Provides `search_local_docs` and `traverse_doc_links` tools backed
/// by an embedded [`DataStore`].  The server is [`Clone`] because
/// [`DataStore`] wraps an [`std::sync::Arc`] internally.
#[derive(Clone)]
pub struct DocServer {
    store: DataStore,
}

impl DocServer {
    /// Create a new [`DocServer`] backed by the given [`DataStore`].
    #[must_use]
    pub fn new(store: DataStore) -> Self {
        Self { store }
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
        let filtered: Vec<_> = if let Some(sid) = &params.source_id {
            results
                .into_iter()
                .filter(|r| r.path.starts_with(sid.as_str()))
                .collect()
        } else {
            results
        };
        let limit = usize::try_from(params.top_k.unwrap_or(10).min(50)).unwrap_or(10);
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
        let depth = usize::try_from(params.max_depth.unwrap_or(2).min(5)).unwrap_or(2);
        let results = find_related_chunks(&self.store, &params.chunk_id, depth)
            .map_err(|e| into_tool_err(&e))?;
        let md = format_traversal_results(&params.chunk_id, &results);
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

/// Convert a [`GraphtorError`] to an MCP [`ErrorData`] internal error response.
fn into_tool_err(e: &GraphtorError) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use super::*;
    use crate::db::{schema::ensure_schema, DataStore};

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
}
