//! Integration tests for `graphtor_core::logging` module.
//!
//! Verifies that `init_logging` configures the tracing subscriber and
//! handles double-initialization gracefully (returns error, does not panic).
//!
//! Note: `tracing` uses a single global subscriber per process. Tests in
//! this file must not assume which test runs first.

use graphtor_core::logging::{init_logging, LogVerbosity};
use graphtor_core::GraphtorError;

/// Verifies `init_logging` semantics end-to-end.
///
/// - Any first call returns either `Ok(())` or `Err(GraphtorError::Config)`
///   (depending on whether another test already set the subscriber).
/// - A second call within the same test **always** returns `Err(Config)`.
/// - The function never panics.
#[test]
fn init_logging_returns_ok_or_config_error_and_double_init_always_errors() {
    // First attempt — may or may not succeed depending on test ordering
    let first = init_logging(LogVerbosity::Normal);
    if let Err(ref e) = first {
        assert!(
            matches!(e, GraphtorError::Config { .. }),
            "if init_logging errors it must be a Config variant: {e:?}"
        );
        let msg = e.to_string();
        assert!(
            msg.contains("already"),
            "Config error message should indicate subscriber is already initialized: {msg}"
        );
    }

    // Second call within the same test must always fail (subscriber is now set)
    let second = init_logging(LogVerbosity::Verbose);
    assert!(
        second.is_err(),
        "second init_logging call in same test must return an error"
    );
    assert!(
        matches!(second.unwrap_err(), GraphtorError::Config { .. }),
        "double-init error must be the Config variant"
    );
}

/// `init_logging` with any verbosity returns `Result`, never panics.
#[test]
fn init_logging_never_panics_for_any_verbosity() {
    for verbosity in [
        LogVerbosity::Quiet,
        LogVerbosity::Normal,
        LogVerbosity::Verbose,
    ] {
        let result = init_logging(verbosity);
        // Accept Ok or Err(Config) — anything else is a bug
        if let Err(e) = result {
            assert!(
                matches!(e, GraphtorError::Config { .. }),
                "init_logging({verbosity:?}) must only error with Config variant: {e:?}"
            );
        }
    }
}
