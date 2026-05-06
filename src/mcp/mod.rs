//! MCP plugin server for `LocalDocRAG`.
//!
//! Exposes [`DocServer`] over the Model Context Protocol via STDIO
//! JSON-RPC transport.  Use [`crate::mcp::list_mcp_tools`] to retrieve
//! the full list of tools advertised by the server at runtime.
//!
//! Start the server from a binary entry point by calling
//! [`DocServer::new`] and passing the result to [`rmcp::serve_server`]
//! with [`rmcp::transport::stdio`] as the transport.

pub mod format;
pub mod server;

pub use server::{DocServer, SyncStatus};

/// Return all MCP tool definitions for the CLI `manifest` command.
///
/// The returned [`rmcp::model::Tool`] values are identical to the tool
/// attributes registered with the server's tool router, ensuring the
/// manifest output exactly mirrors the `tools/list` response served over
/// the MCP STDIO transport.
#[must_use]
pub fn list_mcp_tools() -> Vec<rmcp::model::Tool> {
    DocServer::list_tools()
}
