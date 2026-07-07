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
//! 2. If `GRAPHTOR_EMBED_MODEL_DIR` is set, load the model from that local
//!    directory (no network) via `EmbeddingModel::from_path`.
//! 3. Otherwise attempt to load the canonical embedding model via the Hugging
//!    Face Hub cache (`EmbeddingModel::load`).
//! 4. On failure, emit a structured, actionable diagnostic to stderr (single
//!    source of truth) and return `Ok(None)` so callers can decide whether to
//!    degrade gracefully or treat the missing model as fatal.
//!
//! The diagnostic includes the model id, the underlying error, the canonical
//! cache location, and remediation hints so the operator can determine
//! whether the failure is network, disk, or configuration.

#![allow(clippy::module_name_repetitions)]

use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::embed::EmbeddingModel;
use crate::error::GraphtorError;

/// The canonical embedding model identifier used across all commands.
pub const DEFAULT_MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// Environment variable that overrides model resolution to load the embedding
/// model from a local directory instead of the Hugging Face Hub.
///
/// When set to a non-empty path, the resolver loads `config.json`,
/// `tokenizer.json`, and `model.safetensors` from that directory via
/// [`EmbeddingModel::from_path`] and performs **no network access**. This
/// supports air-gapped or offline operation and sidesteps Hub-download
/// failures (for example, an outdated `hf-hub` client that cannot follow the
/// Hub's current redirect responses).
pub const MODEL_DIR_ENV: &str = "GRAPHTOR_EMBED_MODEL_DIR";

/// Where the resolver should obtain the embedding model.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelSource {
    /// Embeddings are disabled; skip resolution.
    Disabled,
    /// Load the model from a local directory (no network).
    LocalDir(PathBuf),
    /// Load the canonical model from the Hugging Face Hub cache.
    Hub,
}

/// Decide where to obtain the embedding model from the request flag and the
/// optional local-directory override.
///
/// Pure and side-effect free so the routing policy can be unit-tested without
/// touching the process environment or the network. `--no-embed` takes
/// precedence over the local-directory override; a blank or whitespace-only
/// override is ignored and falls back to the Hub.
fn select_model_source(no_embed: bool, model_dir_override: Option<&str>) -> ModelSource {
    if no_embed {
        return ModelSource::Disabled;
    }
    if let Some(dir) = model_dir_override {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return ModelSource::LocalDir(PathBuf::from(trimmed));
        }
    }
    ModelSource::Hub
}

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
    let source = select_model_source(no_embed, std::env::var(MODEL_DIR_ENV).ok().as_deref());
    match source {
        ModelSource::Disabled => {
            info!(
                caller = caller.label(),
                "embeddings disabled by --no-embed; continuing without semantic vectors"
            );
            Ok(None)
        }
        ModelSource::LocalDir(dir) => Ok(load_from_local_dir(caller, &dir)),
        ModelSource::Hub => Ok(load_from_hub(caller)),
    }
}

/// Load the canonical model from the Hugging Face Hub cache, degrading to
/// `None` (with a structured stderr diagnostic) on failure.
fn load_from_hub(caller: ResolverCaller) -> Option<EmbeddingModel> {
    match EmbeddingModel::load(DEFAULT_MODEL_ID) {
        Ok(model) => {
            info!(
                caller = caller.label(),
                model = DEFAULT_MODEL_ID,
                "embedding model loaded"
            );
            Some(model)
        }
        Err(err) => {
            emit_resolution_diagnostic(caller, &err);
            warn!(
                caller = caller.label(),
                model = DEFAULT_MODEL_ID,
                error = %err,
                "embedding model unavailable; continuing in degraded mode"
            );
            None
        }
    }
}

/// Load the model from a local directory (no network), degrading to `None`
/// (with a structured stderr diagnostic) on failure.
fn load_from_local_dir(caller: ResolverCaller, dir: &Path) -> Option<EmbeddingModel> {
    match EmbeddingModel::from_path(dir) {
        Ok(model) => {
            info!(
                caller = caller.label(),
                dir = %dir.display(),
                "embedding model loaded from local directory"
            );
            Some(model)
        }
        Err(err) => {
            emit_local_dir_diagnostic(caller, dir, &err);
            warn!(
                caller = caller.label(),
                dir = %dir.display(),
                error = %err,
                "local embedding model unavailable; continuing in degraded mode"
            );
            None
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

/// Emit a structured stderr diagnostic when the local-directory model load
/// fails, mirroring [`emit_resolution_diagnostic`] but pointing the operator at
/// the configured directory rather than the Hub cache.
fn emit_local_dir_diagnostic(caller: ResolverCaller, dir: &Path, err: &GraphtorError) {
    eprintln!(
        "[embed] local embedding model unavailable in `{}`:",
        caller.label()
    );
    eprintln!("  model dir : {}", dir.display());
    eprintln!("  source    : {MODEL_DIR_ENV} (environment override)");
    eprintln!("  cause     : {err}");
    eprintln!("  remediation:");
    eprintln!(
        "    - ensure the directory contains config.json, tokenizer.json, and model.safetensors"
    );
    eprintln!("    - unset {MODEL_DIR_ENV} to fall back to the Hugging Face Hub");
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

    #[test]
    fn select_source_disabled_when_no_embed_even_with_override() {
        assert_eq!(
            select_model_source(true, Some("/models/minilm")),
            ModelSource::Disabled,
            "--no-embed takes precedence over the local-dir override"
        );
    }

    #[test]
    fn select_source_local_dir_when_override_set() {
        assert_eq!(
            select_model_source(false, Some("/models/minilm")),
            ModelSource::LocalDir(PathBuf::from("/models/minilm"))
        );
    }

    #[test]
    fn select_source_trims_override_and_ignores_blank() {
        assert_eq!(
            select_model_source(false, Some("  /m  ")),
            ModelSource::LocalDir(PathBuf::from("/m")),
            "surrounding whitespace is trimmed"
        );
        assert_eq!(
            select_model_source(false, Some("   ")),
            ModelSource::Hub,
            "blank override falls back to the Hub"
        );
    }

    #[test]
    fn select_source_hub_when_no_override() {
        assert_eq!(select_model_source(false, None), ModelSource::Hub);
    }
}
