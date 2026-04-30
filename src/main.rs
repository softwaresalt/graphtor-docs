//! `graphtor-docs` — `LocalDocRAG` MCP plugin server entry point.
//!
//! Starts the MCP server over STDIO JSON-RPC transport (`localhost` only).
//! The database path defaults to `.graphtor/graph.db` relative to the
//! current working directory; override with the `GRAPHTOR_DB_PATH`
//! environment variable.
//!
//! # Usage
//!
//! ```text
//! graphtor-docs
//! GRAPHTOR_DB_PATH=/path/to/graph.db graphtor-docs
//! ```

use std::path::PathBuf;

use anyhow::Context as _;
use graphtor_core::{init_logging, DataStore, DocServer, LogVerbosity};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging(LogVerbosity::Normal).context("failed to initialize logging")?;

    let db_path: PathBuf = std::env::var("GRAPHTOR_DB_PATH")
        .map_or_else(|_| PathBuf::from(".graphtor/graph.db"), PathBuf::from);

    let root = std::env::current_dir().context("failed to determine working directory")?;

    info!(db_path = %db_path.display(), "opening database");
    let store =
        DataStore::open_sqlite(&db_path, &root).context("failed to open documentation database")?;
    store
        .ensure_schema()
        .context("failed to ensure database schema")?;

    info!("starting MCP STDIO server");
    rmcp::serve_server(DocServer::new(store), rmcp::transport::stdio())
        .await
        .context("MCP server failed to start")?
        .waiting()
        .await
        .context("MCP server terminated with error")?;

    Ok(())
}
