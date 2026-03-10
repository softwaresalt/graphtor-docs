//! Structured logging initialization via `tracing`.
//!
//! Provides [`LogVerbosity`] and [`init_logging`] for configuring the
//! global `tracing` subscriber at application startup. Call [`init_logging`]
//! once at program entry; subsequent calls return an error rather than
//! panicking.

use crate::error::GraphtorError;

/// Verbosity level for structured log output.
///
/// Maps to `tracing` filter levels:
/// - [`Quiet`](LogVerbosity::Quiet) → `ERROR` only
/// - [`Normal`](LogVerbosity::Normal) → `INFO`, `WARN`, `ERROR`
/// - [`Verbose`](LogVerbosity::Verbose) → `DEBUG`, `INFO`, `WARN`, `ERROR`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogVerbosity {
    /// Show only errors. Suitable for production batch pipelines.
    Quiet,
    /// Show info, warnings, and errors. Suitable for interactive use.
    Normal,
    /// Show all messages including per-item debug details.
    Verbose,
}

impl LogVerbosity {
    /// Convert this verbosity level to the corresponding [`tracing::Level`].
    pub(crate) fn as_tracing_level(self) -> tracing::Level {
        match self {
            Self::Quiet => tracing::Level::ERROR,
            Self::Normal => tracing::Level::INFO,
            Self::Verbose => tracing::Level::DEBUG,
        }
    }
}

/// Initialize the global `tracing` subscriber with the given verbosity.
///
/// Configures `tracing-subscriber` with a level filter and stderr output.
/// This function is safe to call from application entry points.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if the global subscriber has already
/// been initialized (e.g., a second call in the same process).
pub fn init_logging(verbosity: LogVerbosity) -> Result<(), GraphtorError> {
    let level = verbosity.as_tracing_level();
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|e| GraphtorError::Config {
            message: format!("logging already initialized: {e}"),
            field: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T030: LogVerbosity → tracing level mapping ────────────────────────

    #[test]
    fn quiet_maps_to_error_level() {
        assert_eq!(
            LogVerbosity::Quiet.as_tracing_level(),
            tracing::Level::ERROR
        );
    }

    #[test]
    fn normal_maps_to_info_level() {
        assert_eq!(
            LogVerbosity::Normal.as_tracing_level(),
            tracing::Level::INFO
        );
    }

    #[test]
    fn verbose_maps_to_debug_level() {
        assert_eq!(
            LogVerbosity::Verbose.as_tracing_level(),
            tracing::Level::DEBUG
        );
    }

    #[test]
    fn all_variants_produce_distinct_levels() {
        let quiet = LogVerbosity::Quiet.as_tracing_level();
        let normal = LogVerbosity::Normal.as_tracing_level();
        let verbose = LogVerbosity::Verbose.as_tracing_level();
        assert_ne!(
            quiet, normal,
            "Quiet and Normal must map to different levels"
        );
        assert_ne!(
            normal, verbose,
            "Normal and Verbose must map to different levels"
        );
        assert_ne!(
            quiet, verbose,
            "Quiet and Verbose must map to different levels"
        );
    }

    #[test]
    fn log_verbosity_implements_debug_and_clone() {
        let v = LogVerbosity::Normal;
        let cloned = v;
        assert_eq!(v, cloned);
        let _ = format!("{v:?}");
    }
}
