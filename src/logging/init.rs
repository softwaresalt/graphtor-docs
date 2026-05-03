//! Structured logging initialization via `tracing`.
//!
//! Provides [`LogVerbosity`] and [`init_logging`] for configuring the
//! global `tracing` subscriber at application startup. Call [`init_logging`]
//! once at program entry; subsequent calls return an error rather than
//! panicking.
//!
//! The subscriber uses [`tracing_subscriber::EnvFilter`] so the `RUST_LOG`
//! environment variable can override the compiled-in verbosity.  The default
//! filter also silences noisy `pdf_extract` glyph-level messages that are not
//! actionable for users.

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
    fn as_tracing_level(self) -> tracing::Level {
        match self {
            Self::Quiet => tracing::Level::ERROR,
            Self::Normal => tracing::Level::INFO,
            Self::Verbose => tracing::Level::DEBUG,
        }
    }

    /// Build the `EnvFilter` directive string for this verbosity.
    ///
    /// The string suppresses noisy `pdf_extract` glyph messages by clamping
    /// that crate to `WARN` regardless of the global verbosity level.
    fn filter_string(self) -> String {
        let level = self.as_tracing_level();
        format!("{level},pdf_extract=warn")
    }
}

/// Initialize the global `tracing` subscriber with the given verbosity.
///
/// Configures `tracing-subscriber` with an [`tracing_subscriber::EnvFilter`]
/// and stderr output.  The `RUST_LOG` environment variable overrides the
/// compiled-in verbosity when set.  `pdf_extract` messages below `WARN` are
/// suppressed by default to reduce glyph-level noise.
///
/// This function is safe to call from application entry points.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if the global subscriber has already
/// been initialized (e.g., a second call in the same process), or if the
/// `RUST_LOG` value contains an invalid directive.
pub fn init_logging(verbosity: LogVerbosity) -> Result<(), GraphtorError> {
    let default_filter = verbosity.filter_string();
    let filter_str = std::env::var("RUST_LOG").unwrap_or(default_filter);
    let filter =
        tracing_subscriber::EnvFilter::try_new(&filter_str).map_err(|e| GraphtorError::Config {
            message: format!("invalid log filter directive: {e}"),
            field: None,
        })?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
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

    // ── T031: EnvFilter migration ─────────────────────────────────────────

    #[test]
    fn filter_string_contains_pdf_extract_suppression() {
        for verbosity in [
            LogVerbosity::Quiet,
            LogVerbosity::Normal,
            LogVerbosity::Verbose,
        ] {
            let s = verbosity.filter_string();
            assert!(
                s.contains("pdf_extract=warn"),
                "filter_string for {verbosity:?} must suppress pdf_extract below WARN: got {s:?}"
            );
        }
    }

    #[test]
    fn filter_string_starts_with_level() {
        let s = LogVerbosity::Normal.filter_string();
        assert!(
            s.starts_with("INFO"),
            "Normal filter string must start with INFO: got {s:?}"
        );
        let s = LogVerbosity::Quiet.filter_string();
        assert!(
            s.starts_with("ERROR"),
            "Quiet filter string must start with ERROR: got {s:?}"
        );
        let s = LogVerbosity::Verbose.filter_string();
        assert!(
            s.starts_with("DEBUG"),
            "Verbose filter string must start with DEBUG: got {s:?}"
        );
    }

    #[test]
    fn filter_string_parses_as_valid_env_filter() {
        for verbosity in [
            LogVerbosity::Quiet,
            LogVerbosity::Normal,
            LogVerbosity::Verbose,
        ] {
            let s = verbosity.filter_string();
            let result = tracing_subscriber::EnvFilter::try_new(&s);
            assert!(
                result.is_ok(),
                "filter_string for {verbosity:?} must be a valid EnvFilter directive: got {s:?}"
            );
        }
    }
}
