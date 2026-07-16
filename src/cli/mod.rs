//! Command-line interface definitions for `graphtor-docs`.
//!
//! Defines the top-level [`Cli`] struct and [`Command`] enum parsed by
//! [`clap`]. Each variant maps to a distinct binary subcommand.
//!
//! When the `--json` global flag is set, command output is wrapped in
//! JSON-RPC 2.0 response envelopes via [`jsonrpc`].

pub mod errfmt;
pub mod jsonrpc;
pub mod prewarm;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Output format for command results.
///
/// Controls whether commands print human-readable text or JSON-RPC 2.0
/// response envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output (default).
    Human,
    /// JSON-RPC 2.0 envelope output (`{"jsonrpc":"2.0","id":null,"result":{...}}`).
    Json,
}

/// `GraphRAG` documentation index — MCP plugin server and ingestion pipeline.
#[derive(Debug, Parser)]
#[command(
    name = "graphtor-docs",
    version,
    about = "GraphRAG documentation index — MCP plugin and ingestion pipeline",
    long_about = None
)]
pub struct Cli {
    /// Enable verbose logging (sets `RUST_LOG=debug`).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output results as JSON-RPC 2.0 response envelopes.
    ///
    /// When set, all command output is wrapped in
    /// `{"jsonrpc":"2.0","id":null,"result":{...}}` or
    /// `{"jsonrpc":"2.0","id":null,"error":{...}}` envelopes, matching the
    /// format the MCP server returns over STDIO.  Suitable for consumption
    /// by agents and scripts without a running MCP server.
    #[arg(long, global = true)]
    pub json: bool,

    /// Path to the `sources.yaml` documentation registry.
    ///
    /// Defaults to `.graphtor/config/sources.yaml` relative to the current
    /// working directory.
    #[arg(
        short,
        long,
        global = true,
        env = "GRAPHTOR_SOURCES",
        value_name = "FILE"
    )]
    pub config: Option<PathBuf>,

    /// Path to the `CozoDB` database file.
    ///
    /// Defaults to `.graphtor/graph.db` relative to the current working
    /// directory.
    ///
    /// `--data-dir` is a deprecated alias for this flag; use `--db-path` instead.
    #[arg(
        short,
        long = "db-path",
        alias = "data-dir",
        global = true,
        env = "GRAPHTOR_DB_PATH",
        value_name = "FILE"
    )]
    pub db_path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the ingestion pipeline (incremental by default).
    ///
    /// By default, detects changes since the last sync (via git diff or file
    /// mtime) and surgically re-processes only added, modified, and deleted
    /// files. Pass `--full` to force a complete acquire → parse → embed → load
    /// cycle over all files regardless of change state. Pass `--metrics` to
    /// emit raw sync metrics JSON for scripting.
    Sync(SyncArgs),

    /// Start the MCP STDIO server.
    ///
    /// Binds to STDIO (localhost only) and serves the `GraphRAG` MCP tools to
    /// any connected MCP client. Blocks until stdin closes.
    Serve(ServeArgs),

    /// Print database statistics.
    ///
    /// Reports total chunks, documents, nodes, edges, and last sync time per
    /// source. Use `--json` for machine-readable output.
    Status(StatusArgs),

    /// Generate a template `sources.yaml` in the current directory.
    ///
    /// Creates a `.graphtor/config/sources.yaml` with commented examples for
    /// Git and local sources. Does not overwrite an existing file.
    Init(InitArgs),

    /// Install graphtor-docs into the current workspace.
    ///
    /// Creates the `.graphtor/` directory scaffold: `bin/`, `data/`,
    /// `cache/`, `config/`, `logs/`. Copies the binary and configures
    /// `.gitignore` and MCP client config files. Idempotent.
    Install(InstallArgs),

    /// Diagnose workspace health.
    ///
    /// Validates binary version, database accessibility, `sources.yaml`
    /// syntax, MCP client configs, and disk usage. Prints a pass/warn/fail
    /// report.
    Doctor,

    /// Upgrade the installed graphtor-docs binary.
    ///
    /// Replaces the binary in `.graphtor/bin/` with the currently running
    /// binary. Preserves config and data. Use `--force` to replace even when
    /// the installed binary appears up-to-date.
    Upgrade(UpgradeArgs),

    /// Uninstall graphtor-docs from the current workspace.
    ///
    /// Removes `.graphtor/` and MCP client config files. Requires
    /// `--confirm`. Cleans `.gitignore` entries.
    Uninstall(UninstallArgs),

    /// Print a JSON-RPC 2.0 manifest of graphtor-docs MCP tools.
    ///
    /// Without `--json`, prints a human-readable table of tool names and
    /// descriptions.  With `--json`, emits a `tools/list`-compatible response
    /// envelope that mirrors what the MCP server returns on initialisation.
    /// Tool definitions are derived from the same source as the MCP server,
    /// guaranteeing parity.
    Manifest,

    /// Pre-warm all configured documentation sources with progress reporting.
    ///
    /// Syncs all sources in sequence, writing per-file progress lines to stderr
    /// in the format `[syncing] source_id: file_name (idx/total) [pct%]`, and
    /// emits a single JSONL telemetry record to stdout on completion.  Pass
    /// `--quiet` to suppress stderr progress while preserving stdout telemetry.
    Prewarm(prewarm::PrewarmArgs),

    /// Full-text keyword search across all indexed documentation sources.
    ///
    /// CLI equivalent of the `search_local_docs` MCP tool. Opens the configured
    /// databases read-only and prints matching chunks. Use `--json` for a
    /// structured JSON-RPC envelope suitable for agent consumption.
    Search(SearchArgs),

    /// Semantic (vector-similarity) search across all indexed documentation.
    ///
    /// CLI equivalent of the `search_semantic` MCP tool. Requires the embedding
    /// model; the command fails when the model is unavailable. Use `--json` for
    /// a structured JSON-RPC envelope.
    SearchSemantic(SearchSemanticArgs),

    /// In-depth topic research: search plus document-graph traversal.
    ///
    /// CLI equivalent of the `research_topic` MCP tool. Combines keyword or
    /// semantic search with BFS traversal from the top results to surface
    /// related context. Use `--json` for a structured JSON-RPC envelope.
    Research(ResearchArgs),

    /// Traverse the document link graph starting from a chunk ID.
    ///
    /// CLI equivalent of the `traverse_doc_links` MCP tool. Follows outgoing
    /// document links via BFS. Use `--json` for a structured JSON-RPC envelope.
    Traverse(TraverseArgs),

    /// List all registered documentation sources.
    ///
    /// CLI equivalent of the `list_sources` MCP tool. Use `--json` for a
    /// structured JSON-RPC envelope.
    ListSources,

    /// Retrieve a single documentation chunk by its stable chunk ID.
    ///
    /// CLI equivalent of the `get_chunk_by_id` MCP tool. Use `--json` for a
    /// structured JSON-RPC envelope.
    GetChunk(GetChunkArgs),

    /// Retrieve all chunks for a document path, in reading order.
    ///
    /// CLI equivalent of the `get_document` MCP tool. Provide `--source-id` to
    /// scope to one source, or omit it to retrieve across all sources. Use
    /// `--json` for a structured JSON-RPC envelope.
    GetDocument(GetDocumentArgs),
}

/// Arguments for the `sync` subcommand.
#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct SyncArgs {
    /// Batch size — number of files to process per parse/embed/load cycle.
    #[arg(long, default_value = "20", value_name = "N")]
    pub batch_size: usize,

    /// Disable the embedding step (faster; no vectors stored).
    #[arg(long)]
    pub no_embed: bool,

    /// Root directory for derived data stores (graph.db, cache).
    ///
    /// Defaults to `.graphtor/data` relative to the current working directory
    /// when not specified.
    #[arg(long, value_name = "DIR")]
    pub data_root: Option<PathBuf>,

    /// Run a full re-ingestion of all files instead of incremental sync.
    ///
    /// By default, `sync` detects changes since the last run (via git diff
    /// or file mtime) and only re-processes modified, added, or deleted files.
    /// Use `--full` to force a complete acquire → parse → embed → load cycle
    /// over all files regardless of change state.
    #[arg(long)]
    pub full: bool,

    /// Emit raw sync metrics JSON instead of human-readable sync output.
    ///
    /// This flag is specific to the `sync` subcommand and takes precedence over
    /// the global `--json` JSON-RPC envelope mode.
    #[arg(long)]
    pub metrics: bool,

    /// Proceed even when cross-database duplicate intakes are detected.
    ///
    /// By default, `sync` blocks when the same source URL or path is indexed
    /// into more than one database.  Use `--force` to emit a warning and
    /// continue regardless.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for the `serve` subcommand.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Force every database to `ReadOnly` posture, regardless of resolved
    /// sources.
    ///
    /// An escape hatch on top of the content-derived default (P1-T2): even a
    /// database with a real, resolvable `local` source pointing at it is
    /// served read-only and never background-synced when this flag is set.
    /// There is no corresponding "force read-write" flag in this phase —
    /// the content-derived default is authoritative unless explicitly
    /// overridden toward the safer (read-only) posture.
    #[arg(long)]
    pub read_only: bool,
}

/// Arguments for the `status` subcommand.
#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// Print output as JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for the `init` subcommand.
#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Overwrite an existing `sources.yaml` if one already exists.
    #[arg(long)]
    pub force: bool,
}

/// Arguments for the `install` subcommand.
#[derive(Debug, clap::Args)]
pub struct InstallArgs {
    /// Skip updating `.gitignore`.
    #[arg(long)]
    pub no_gitignore: bool,

    /// Force-release the workspace lock before installing.
    ///
    /// Use when a previous invocation left a stale lock file.
    #[arg(long)]
    pub force_unlock: bool,
}

/// Arguments for the `upgrade` subcommand.
#[derive(Debug, clap::Args)]
pub struct UpgradeArgs {
    /// Force upgrade even if the installed version matches.
    #[arg(long)]
    pub force: bool,

    /// Force-release the workspace lock before upgrading.
    ///
    /// Use when a previous invocation left a stale lock file.
    #[arg(long)]
    pub force_unlock: bool,
}

/// Arguments for the `uninstall` subcommand.
#[derive(Debug, clap::Args)]
pub struct UninstallArgs {
    /// Required confirmation flag — prevents accidental uninstall.
    #[arg(long)]
    pub confirm: bool,

    /// Keep `sources.yaml` and workspace config; only remove runtime data.
    #[arg(long)]
    pub keep_config: bool,

    /// Force-release the workspace lock before uninstalling.
    ///
    /// Use when a previous invocation left a stale lock file.
    #[arg(long)]
    pub force_unlock: bool,
}

/// Arguments for the `search` subcommand.
#[derive(Debug, clap::Args)]
pub struct SearchArgs {
    /// Keyword or phrase to search documentation chunks for.
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Restrict results to a specific documentation source ID.
    #[arg(long, value_name = "ID")]
    pub source_id: Option<String>,

    /// Maximum number of results to return.
    #[arg(long, default_value = "10", value_name = "N")]
    pub top_k: usize,
}

/// Arguments for the `search-semantic` subcommand.
#[derive(Debug, clap::Args)]
pub struct SearchSemanticArgs {
    /// Natural-language query to embed and search semantically.
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Maximum number of results to return.
    #[arg(long, default_value = "10", value_name = "N")]
    pub top_k: usize,
}

/// Arguments for the `research` subcommand.
#[derive(Debug, clap::Args)]
pub struct ResearchArgs {
    /// Natural-language or keyword query for the research topic.
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Maximum number of initial search results to retrieve.
    ///
    /// At most `min(top_k, 3)` of the top results seed the graph traversal.
    #[arg(long, default_value = "5", value_name = "N")]
    pub top_k: usize,

    /// BFS traversal depth from each seed chunk.
    #[arg(long, default_value = "1", value_name = "N")]
    pub max_depth: usize,
}

/// Arguments for the `traverse` subcommand.
#[derive(Debug, clap::Args)]
pub struct TraverseArgs {
    /// Stable SHA-256 chunk identifier to start BFS traversal from.
    #[arg(value_name = "CHUNK_ID")]
    pub chunk_id: String,

    /// Maximum BFS traversal depth.
    #[arg(long, default_value = "2", value_name = "N")]
    pub max_depth: usize,
}

/// Arguments for the `get-chunk` subcommand.
#[derive(Debug, clap::Args)]
pub struct GetChunkArgs {
    /// Stable SHA-256 chunk identifier to retrieve.
    #[arg(value_name = "CHUNK_ID")]
    pub chunk_id: String,
}

/// Arguments for the `get-document` subcommand.
#[derive(Debug, clap::Args)]
pub struct GetDocumentArgs {
    /// Relative document path within the source (e.g. `articles/intro.md`).
    #[arg(value_name = "PATH")]
    pub path: String,

    /// Restrict the lookup to a specific documentation source ID.
    #[arg(long, value_name = "ID")]
    pub source_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn install_help_does_not_offer_editor_flag() {
        let mut command = Cli::command();
        let install = command
            .find_subcommand_mut("install")
            .expect("install subcommand");
        let help = install.render_long_help().to_string();

        assert!(!help.contains("--editor"));
        assert!(!help.contains("copilot"));
    }
}
