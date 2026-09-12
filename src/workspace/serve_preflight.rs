//! Typed pre-transport exit/error observability seam for `cmd_serve`
//! (`056.003-T`).
//!
//! `cmd_serve` runs a sequence of fail-closed preflight checks (source
//! registry resolution, database discovery/classification, duplicate-intake
//! detection, and per-database open/schema gating) before it ever calls
//! `rmcp::serve_server`. Before this module existed, each check printed its
//! own ad hoc `eprintln!` message and returned (or propagated) an exit code
//! inline, with nothing durably recorded via [`tracing`] — an operator who
//! redirected stderr elsewhere, or ran with `RUST_LOG=off`, had no
//! structured record of *why* the server refused to start, and there was no
//! single enumerable seam an implementer could consult to confirm every
//! pre-transport exit was accounted for.
//!
//! This module is **non-conditional, parity-safe observability only**
//! (Constitution V — runtime-owned diagnostics). It changes nothing about
//! `cmd_serve`'s actual fail-closed gating behaviour:
//!
//! - every existing `eprintln!`/errfmt fatal message is preserved
//!   byte-for-byte and unconditionally — this module never converts a loud
//!   failure into a `tracing`-only one, and `RUST_LOG=off` (which silences
//!   `tracing` output, not raw `eprintln!`/the top-level fatal renderer)
//!   continues to leave every user-facing failure message intact;
//! - stdout is never touched by any function in this module — the MCP
//!   stdio transport's protocol-clean-stdout invariant is unaffected.
//!
//! Three pieces are provided:
//!
//! - [`ServePreflightExit`]: one exhaustive enum listing every pre-transport
//!   NORMAL exit `cmd_serve` (and its `open_serve_databases`/
//!   duplicate-intake-preflight call sites) can take — an intentional
//!   `Ok(code)` return before any transport begins. Each variant's
//!   [`ServePreflightExit::trace`] emits one stable, additional
//!   `tracing::error!` event alongside the caller's existing `eprintln!`.
//! - [`ServePreflightErrorStage`] plus [`trace_preflight_error`] /
//!   [`trace_stage`]: a single stable structured event
//!   (`mcp_serve_preflight_error`) emitted immediately before a
//!   registry/discovery/open/schema/model-resolution error is propagated
//!   (via `?`) out of `cmd_serve`. The top-level fatal renderer
//!   (`cli::errfmt::eprint_fatal` in `main()`, or its `--json` `wrap_error`
//!   counterpart) remains the SOLE renderer of the actual fatal message for
//!   these errors — this only adds structured, `tracing`-observable
//!   evidence; it never constructs, converts, or duplicates the error.
//! - [`trace_serve_ready`]: emits the `mcp_serve_ready` structured info
//!   event immediately before `rmcp::serve_server` is called. This records
//!   "preflight complete, about to call `serve_server`" ONLY — never
//!   transport readiness, loop entry, or a completed `initialize` handshake.

use std::fmt;
use std::path::{Path, PathBuf};

/// Every pre-transport NORMAL exit `cmd_serve` (including its
/// `open_serve_databases` and duplicate-intake-preflight call sites) can
/// take before `rmcp::serve_server` is ever called. Each variant owns
/// exactly the context its existing message needs; this enum is exhaustive
/// by construction — no wildcard arm is used in [`ServePreflightExit::tag`]
/// or [`ServePreflightExit::exit_code`], so adding a new pre-transport exit
/// site to `cmd_serve` without adding a corresponding variant here fails to
/// compile at the match site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServePreflightExit {
    /// An explicit `--config`/config-override file does not exist on disk.
    ConfigOverrideNotFound {
        /// The path the operator supplied.
        path: PathBuf,
    },
    /// No database file was found to serve, either because the
    /// configured+auto-discovered candidate union was empty, or because
    /// every remaining candidate was filtered out (a `ReadOnly` candidate
    /// that does not exist on disk is never attempted).
    NoDatabasesToServe {
        /// The `.graphtor/` directory that was scanned.
        graphtor_dir: PathBuf,
    },
    /// The duplicate-intake preflight (shared with `sync`/`prewarm` via
    /// `run_duplicate_intake_preflight`) rejected the resolved generation
    /// sources. That shared helper already renders its own detailed
    /// `eprintln!` report before returning; this variant exists purely to
    /// give that shared exit a `serve`-specific structured trace event at
    /// its `cmd_serve` call site.
    DuplicateIntakeConflict,
    /// A database resolved for serving still carries a pre-v4 schema and
    /// must be rebuilt with `sync` before `serve` can start.
    PreV4Schema {
        /// The database path that failed the pre-v4 gate.
        db_path: PathBuf,
    },
    /// Defence-in-depth: the read-only store iterator was unexpectedly
    /// empty immediately after `open_serve_databases` returned success.
    /// Structurally unreachable given the earlier
    /// [`ServePreflightExit::NoDatabasesToServe`] gates, but handled
    /// explicitly rather than via `unreachable!()`.
    NoPrimaryStore,
}

impl ServePreflightExit {
    /// The process exit code this normal exit returns. Every current
    /// variant exits `2` (the crate's documented fatal-error exit code),
    /// but this stays a method rather than a shared constant so a future
    /// variant can diverge without disturbing existing callers.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ConfigOverrideNotFound { .. }
            | Self::NoDatabasesToServe { .. }
            | Self::DuplicateIntakeConflict
            | Self::PreV4Schema { .. }
            | Self::NoPrimaryStore => 2,
        }
    }

    /// A short, stable, machine-greppable variant tag used for the
    /// `tracing` event and for tests. Never the interpolated human message
    /// (that stays owned by the caller's existing `eprintln!`).
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::ConfigOverrideNotFound { .. } => "config_override_not_found",
            Self::NoDatabasesToServe { .. } => "no_databases_to_serve",
            Self::DuplicateIntakeConflict => "duplicate_intake_conflict",
            Self::PreV4Schema { .. } => "pre_v4_schema",
            Self::NoPrimaryStore => "no_primary_store",
        }
    }

    /// Emit the stable structured `mcp_serve_preflight_exit` `tracing::error!`
    /// event for this exit. Prints nothing itself — callers keep their
    /// existing unconditional `eprintln!` call exactly as before; this only
    /// adds the additional `tracing`-observable record, which `RUST_LOG=off`
    /// may legitimately silence without affecting the loud stderr message.
    pub fn trace(&self) {
        tracing::error!(
            event = "mcp_serve_preflight_exit",
            exit = self.tag(),
            code = self.exit_code(),
            "serve preflight exit"
        );
    }
}

impl fmt::Display for ServePreflightExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.tag())
    }
}

/// Stage tag for a registry/discovery/open/schema/model-resolution error
/// that is about to be propagated (via `?` / `return Err(..)`) out of
/// `cmd_serve` or one of its preflight helpers, rather than handled as a
/// [`ServePreflightExit`] normal exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServePreflightErrorStage {
    /// Loading/parsing the source registry (`sources.yaml` /
    /// `.graphtor/config/*`).
    SourceConfig,
    /// Discovering which databases to serve
    /// (`serve_discovery::discover_served_databases`).
    Discovery,
    /// Acquiring the advisory database lock for a `Generation` target.
    DatabaseLock,
    /// Opening the read-write or read-only `DataStore`.
    DatabaseOpen,
    /// Ensuring/checking the on-disk schema (`ensure_schema` /
    /// `needs_v4_migration`).
    Schema,
    /// Resolving the embedding model used for semantic search.
    EmbeddingModel,
}

impl fmt::Display for ServePreflightErrorStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::SourceConfig => "source_config",
            Self::Discovery => "discovery",
            Self::DatabaseLock => "database_lock",
            Self::DatabaseOpen => "database_open",
            Self::Schema => "schema",
            Self::EmbeddingModel => "embedding_model",
        };
        write!(f, "{s}")
    }
}

/// Emit the single stable `mcp_serve_preflight_error` structured event for a
/// registry/discovery/open/schema/model-resolution error immediately before
/// it is propagated out of `cmd_serve`.
///
/// This is **additional** structured observability only: the existing
/// top-level fatal renderer (`cli::errfmt::eprint_fatal` in `main()`, or its
/// `--json` `wrap_error` counterpart) remains the SOLE renderer of the
/// actual fatal message for propagated errors. This function never prints
/// to stdout/stderr itself and never constructs, wraps, or returns an
/// error — it only observes the one already in hand.
pub fn trace_preflight_error(stage: ServePreflightErrorStage, err: &anyhow::Error) {
    tracing::error!(
        event = "mcp_serve_preflight_error",
        stage = %stage,
        error = %err,
        "serve preflight error"
    );
}

/// Convenience wrapper: on `Err`, calls [`trace_preflight_error`] with
/// `stage` and re-returns the *same* error unchanged (no wrapping, no new
/// context, no altered message); on `Ok`, returns the value unchanged.
///
/// Lets call sites write `trace_stage(Stage::X, fallible_call())?` instead
/// of repeating the trace-then-return-error pattern inline at every
/// propagation point.
pub fn trace_stage<T>(
    stage: ServePreflightErrorStage,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    result.map_err(|e| {
        trace_preflight_error(stage, &e);
        e
    })
}

/// Emit the `mcp_serve_ready` structured info event immediately before
/// calling `rmcp::serve_server`. Means "preflight complete, about to call
/// `serve_server`" ONLY — never transport readiness, loop entry, or a
/// completed `initialize` handshake. `launch_cwd` is the directory the
/// server treats as its authorized project root for this session.
pub fn trace_serve_ready(launch_cwd: &Path) {
    tracing::info!(
        event = "mcp_serve_ready",
        transport = "stdio",
        preflight_complete = true,
        launch_cwd = %launch_cwd.display(),
        "serve preflight complete; starting stdio transport"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ServePreflightExit: exhaustive tag/exit_code/Display mapping ────

    fn all_variants() -> Vec<ServePreflightExit> {
        vec![
            ServePreflightExit::ConfigOverrideNotFound {
                path: PathBuf::from("/tmp/does-not-exist.yaml"),
            },
            ServePreflightExit::NoDatabasesToServe {
                graphtor_dir: PathBuf::from("/tmp/.graphtor"),
            },
            ServePreflightExit::DuplicateIntakeConflict,
            ServePreflightExit::PreV4Schema {
                db_path: PathBuf::from("/tmp/.graphtor/graph.db"),
            },
            ServePreflightExit::NoPrimaryStore,
        ]
    }

    #[test]
    fn every_variant_exits_with_fatal_code_2() {
        for variant in all_variants() {
            assert_eq!(
                variant.exit_code(),
                2,
                "every current ServePreflightExit variant must exit 2: {variant:?}"
            );
        }
    }

    #[test]
    fn every_variant_has_a_distinct_stable_tag() {
        let tags: Vec<&'static str> = all_variants().iter().map(ServePreflightExit::tag).collect();
        let unique: std::collections::BTreeSet<_> = tags.iter().copied().collect();
        assert_eq!(
            tags.len(),
            unique.len(),
            "every ServePreflightExit variant must have a distinct tag: {tags:?}"
        );
        assert_eq!(
            unique,
            [
                "config_override_not_found",
                "no_databases_to_serve",
                "duplicate_intake_conflict",
                "pre_v4_schema",
                "no_primary_store",
            ]
            .into_iter()
            .collect(),
            "exact expected tag set (exhaustive mapping proof)"
        );
    }

    #[test]
    fn display_matches_tag_for_every_variant() {
        for variant in all_variants() {
            assert_eq!(variant.to_string(), variant.tag());
        }
    }

    #[test]
    fn trace_does_not_panic_for_any_variant() {
        // `trace()` only emits a `tracing` event; with no subscriber
        // installed in this unit-test process this is a documented no-op,
        // but it must never panic regardless of subscriber presence.
        for variant in all_variants() {
            variant.trace();
        }
    }

    // ── ServePreflightErrorStage: exhaustive Display mapping ────────────

    fn all_stages() -> Vec<ServePreflightErrorStage> {
        vec![
            ServePreflightErrorStage::SourceConfig,
            ServePreflightErrorStage::Discovery,
            ServePreflightErrorStage::DatabaseLock,
            ServePreflightErrorStage::DatabaseOpen,
            ServePreflightErrorStage::Schema,
            ServePreflightErrorStage::EmbeddingModel,
        ]
    }

    #[test]
    fn every_stage_has_a_distinct_stable_display() {
        let names: Vec<String> = all_stages().iter().map(ToString::to_string).collect();
        let unique: std::collections::BTreeSet<_> = names.iter().cloned().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "every ServePreflightErrorStage must have a distinct Display: {names:?}"
        );
        assert_eq!(
            unique,
            [
                "source_config",
                "discovery",
                "database_lock",
                "database_open",
                "schema",
                "embedding_model",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            "exact expected stage set (exhaustive mapping proof)"
        );
    }

    // ── trace_stage / trace_preflight_error: transparent pass-through ──

    #[test]
    fn trace_stage_passes_ok_through_unchanged() {
        let result: anyhow::Result<u32> = trace_stage(ServePreflightErrorStage::Schema, Ok(42));
        assert_eq!(result.expect("Ok must pass through"), 42);
    }

    #[test]
    fn trace_stage_passes_err_through_unchanged() {
        let original = anyhow::anyhow!("boom");
        let original_msg = original.to_string();
        let result: anyhow::Result<u32> =
            trace_stage(ServePreflightErrorStage::DatabaseOpen, Err(original));
        let err = result.expect_err("Err must pass through as Err");
        assert_eq!(
            err.to_string(),
            original_msg,
            "trace_stage must not alter the error message"
        );
    }

    #[test]
    fn trace_preflight_error_does_not_panic() {
        let err = anyhow::anyhow!("boom");
        for stage in all_stages() {
            trace_preflight_error(stage, &err);
        }
    }

    #[test]
    fn trace_serve_ready_does_not_panic() {
        trace_serve_ready(Path::new("/tmp/example-project-root"));
    }
}
