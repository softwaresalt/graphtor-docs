//! Command-line interface definitions for `graphtor-docs`.
//!
//! Defines the top-level [`Cli`] struct and [`Command`] enum parsed by
//! [`clap`]. Each variant maps to a distinct binary subcommand.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    #[arg(
        short,
        long,
        global = true,
        env = "GRAPHTOR_DB_PATH",
        value_name = "FILE"
    )]
    pub data_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the full ingestion pipeline: acquire → parse → embed → load.
    ///
    /// Reads sources from the configured `sources.yaml`, acquires each source,
    /// parses and chunks documents, computes embeddings, and loads everything
    /// into the `CozoDB` database. Idempotent: re-running on the same sources
    /// is safe.
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
    /// Replaces the binary in `.graphtor/bin/`, checks schema migrations, and
    /// optionally triggers a re-index. Preserves config and data.
    Upgrade(UpgradeArgs),

    /// Uninstall graphtor-docs from the current workspace.
    ///
    /// Removes `.graphtor/` and MCP client config files. Requires
    /// `--confirm`. Cleans `.gitignore` entries.
    Uninstall(UninstallArgs),
}

/// Arguments for the `sync` subcommand.
#[derive(Debug, clap::Args)]
pub struct SyncArgs {
    /// Batch size — number of files to process per parse/embed/load cycle.
    #[arg(long, default_value = "20", value_name = "N")]
    pub batch_size: usize,

    /// Disable the embedding step (faster; no vectors stored).
    #[arg(long)]
    pub no_embed: bool,
}

/// Arguments for the `serve` subcommand.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    // Currently no extra arguments; kept as a struct for future flags.
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

    /// Target editor(s) for MCP client config generation.
    ///
    /// Comma-separated values. Supported: `vscode`, `cursor`, `copilot`.
    /// Defaults to all supported editors.
    #[arg(long, value_delimiter = ',', value_name = "EDITOR")]
    pub editor: Vec<String>,
}

/// Arguments for the `upgrade` subcommand.
#[derive(Debug, clap::Args)]
pub struct UpgradeArgs {
    /// Force upgrade even if the installed version matches.
    #[arg(long)]
    pub force: bool,
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
}
