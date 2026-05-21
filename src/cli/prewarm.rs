//! Arguments for the `prewarm` subcommand.

use std::path::PathBuf;

/// Arguments for the `prewarm` subcommand.
///
/// `prewarm` syncs all configured documentation sources in sequence, writing
/// per-file progress lines to stderr and emitting a single JSONL telemetry
/// record to stdout on completion.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, clap::Args)]
pub struct PrewarmArgs {
    /// Disable the embedding step (faster; no vectors stored).
    #[arg(long)]
    pub no_embed: bool,

    /// Root directory for derived data stores (graph.db, cache).
    ///
    /// Defaults to `.graphtor/data` relative to the current working directory
    /// when not specified.
    #[arg(long, value_name = "DIR")]
    pub data_root: Option<PathBuf>,

    /// Suppress per-file progress output to stderr.
    ///
    /// When set, only the JSONL telemetry line is written to stdout.
    /// Useful for scripted or CI environments where stderr noise should be
    /// minimised.
    #[arg(long)]
    pub quiet: bool,
}
