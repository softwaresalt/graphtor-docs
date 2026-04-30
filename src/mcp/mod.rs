//! MCP plugin server for `LocalDocRAG`.
//!
//! Exposes [`DocServer`] over the Model Context Protocol via STDIO
//! JSON-RPC transport.  The server provides two tools:
//!
//! - `search_local_docs` — keyword search over indexed documentation chunks.
//! - `traverse_doc_links` — BFS traversal of the document link graph.
//!
//! Start the server from a binary entry point by calling
//! [`DocServer::new`] and then calling `.serve(rmcp::transport::stdio())`
//! on the resulting value (requires importing [`rmcp::ServiceExt`]).

pub mod format;
pub mod server;

pub use server::DocServer;
