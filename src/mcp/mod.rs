//! MCP plugin server for `LocalDocRAG`.
//!
//! Exposes [`DocServer`] over the Model Context Protocol via STDIO
//! JSON-RPC transport.  The server provides two tools:
//!
//! - `search_local_docs` — keyword search over indexed documentation chunks.
//! - `traverse_doc_links` — BFS traversal of the document link graph.
//!
//! Start the server from a binary entry point by calling
//! [`DocServer::new`] and passing the result to [`rmcp::serve_server`]
//! with [`rmcp::transport::stdio`] as the transport.

pub mod format;
pub mod server;

pub use server::DocServer;
