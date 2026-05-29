//! Shared embedding-model resolver used by `sync`, `serve`, and `prewarm`.
//!
//! Previously, each command site had its own copy of the
//! `EmbeddingModel::load("sentence-transformers/all-MiniLM-L6-v2")` block with
//! subtly different log messages and degraded-mode behaviour. That divergence
//! caused operator confusion because the same model lookup produced different
//! diagnostics depending on which command was invoked.
//!
//! This module centralizes the resolution policy:
//!
//! 1. Honour `--no-embed` (or equivalent flag) by skipping resolution.
//! 2. Attempt to load the canonical embedding model via the Hugging Face Hub
//!    cache (`EmbeddingModel::load`).
//! 3. On failure, emit a structured, actionable diagnostic to stderr (single
//!    source of truth) and return `Ok(None)` so callers can decide whether to
//!    degrade gracefully or treat the missing model as fatal.
//!
//! The diagnostic includes the model id, the underlying error, the canonical
//! cache location, and remediation hints so the operator can determine
//! whether the failure is network, disk, or configuration.

#![allow(clippy::module_name_repetitions)]

use tracing::{info, warn};

use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;

/// The canonical embedding model identifier used across all commands.
pub const DEFAULT_MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// Where in this resolver the model lookup is being performed.
///
/// The variant is used only to vary the log prefix so operators can tell
/// whether the warning originated in `sync`, `serve`, or `prewarm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolverCaller {
    /// Invoked from `sync` (incremental or full).
    Sync,
    /// Invoked from `serve` (MCP server startup).
    Serve,
    /// Invoked from `prewarm`.
    Prewarm,
}

impl ResolverCaller {
    fn label(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Serve => "serve",
            Self::Prewarm => "prewarm",
        }
    }
}

/// Resolve the canonical embedding model for any caller.
///
/// * If `no_embed` is `true`, returns `Ok(None)` immediately with an info
///   log noting embeddings are disabled by request.
/// * Otherwise attempts [`EmbeddingModel::load`] with the canonical model id.
///   On success, returns `Ok(Some(model))`.
///   On failure, emits a structured stderr diagnostic and returns `Ok(None)`
///   so the caller can continue in degraded mode if appropriate.
///
/// Callers that require embeddings (for example, `serve` when semantic search
/// is mandatory) may treat `Ok(None)` as fatal at their own discretion.
///
/// # Errors
///
/// This function never returns `Err` today — model-load failures are reported
/// via stderr and `Ok(None)`. The `Result` return type is reserved for future
/// fatal cases such as unreachable model configuration.
pub fn resolve_embedding_model(
    caller: ResolverCaller,
    no_embed: bool,
) -> Result<Option<EmbeddingModel>, GraphtorError> {
    if no_embed {
        info!(
            caller = caller.label(),
            "embeddings disabled by --no-embed; continuing without semantic vectors"
        );
        return Ok(None);
    }

    match EmbeddingModel::load(DEFAULT_MODEL_ID) {
        Ok(model) => {
            info!(
                caller = caller.label(),
                model = DEFAULT_MODEL_ID,
                "embedding model loaded"
            );
            Ok(Some(model))
        }
        Err(err) => {
            emit_resolution_diagnostic(caller, &err);
            warn!(
                caller = caller.label(),
                model = DEFAULT_MODEL_ID,
                error = %err,
                "embedding model unavailable; continuing in degraded mode"
            );
            Ok(None)
        }
    }
}

/// Emit a single multi-line stderr diagnostic so operators see actionable
/// remediation guidance without parsing structured log output.
fn emit_resolution_diagnostic(caller: ResolverCaller, err: &GraphtorError) {
    let cache_hint = canonical_cache_hint();
    eprintln!(
        "[embed] embedding model unavailable in `{}`:",
        caller.label()
    );
    eprintln!("  model     : {DEFAULT_MODEL_ID}");
    eprintln!("  cause     : {err}");
    eprintln!("  cache dir : {cache_hint}");
    eprintln!("  remediation:");
    eprintln!(
        "    - verify network access to https://huggingface.co (first run downloads ~90 MiB)"
    );
    eprintln!("    - confirm the cache directory above is writable and not pruned by disk cleanup");
    eprintln!(
        "    - re-run with `--no-embed` to proceed without embeddings (semantic search disabled)"
    );
}

/// Best-effort description of the Hugging Face cache directory.
///
/// The hf-hub crate resolves the cache via `$HF_HOME`, `$XDG_CACHE_HOME`, or
/// `~/.cache/huggingface`. We replicate the resolution order here for the
/// diagnostic only — the actual cache is owned by hf-hub.
fn canonical_cache_hint() -> String {
    if let Ok(home) = std::env::var("HF_HOME") {
        return home;
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return format!("{xdg}/huggingface");
    }
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return format!("{profile}\\.cache\\huggingface");
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/.cache/huggingface");
        }
    }
    "~/.cache/huggingface".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_embed_returns_none_without_loading() {
        let result =
            resolve_embedding_model(ResolverCaller::Sync, true).expect("no-embed must not error");
        assert!(result.is_none(), "no-embed mode must skip model load");
    }

    #[test]
    fn resolver_caller_label_distinguishes_sites() {
        assert_eq!(ResolverCaller::Sync.label(), "sync");
        assert_eq!(ResolverCaller::Serve.label(), "serve");
        assert_eq!(ResolverCaller::Prewarm.label(), "prewarm");
    }

    #[test]
    fn cache_hint_returns_non_empty_path() {
        let hint = canonical_cache_hint();
        assert!(
            !hint.is_empty(),
            "cache hint must always produce something for operator guidance"
        );
    }
}
