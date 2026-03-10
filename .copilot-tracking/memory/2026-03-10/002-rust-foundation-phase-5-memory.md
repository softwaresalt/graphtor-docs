# Phase 5 Memory — Structured Logging (US4)

**Date**: 2026-03-10
**Phase**: 5 of 7
**Tasks**: T030–T035
**Status**: Complete ✅

## What Was Built

### Files Created
- `src/logging/init.rs` — `LogVerbosity` enum + `init_logging()` function
- `tests/logging_test.rs` — integration tests for init and double-init

### Files Modified
- `src/logging/mod.rs` — added `pub mod init; pub use init::{…};`
- `src/lib.rs` — added `pub use logging::{init_logging, LogVerbosity};`

## Implementation Decisions

### LogVerbosity → tracing::Level Mapping
- `Quiet` → `ERROR` (production batch, minimal noise)
- `Normal` → `INFO` (interactive, default)
- `Verbose` → `DEBUG` (all messages)

### Double-Init Handling
- `tracing_subscriber::fmt().try_init()` returns `Result<(), Box<dyn Error>>`
- Mapped to `GraphtorError::Config { message: "logging already initialized: …", field: None }`
- No panics — safe for library use

### Integration Test Design Challenge
- `tracing` global subscriber is set once per process; test ordering is non-deterministic
- Two tests initially hit a race: `double_init_returns_config_variant` won the race, setting the subscriber before `init_logging_succeeds_on_first_call_and_errors_on_second` could call it
- **Fix**: Merged assertions — tests now handle either `Ok(())` or `Err(Config)` for the first call, and assert `Err(Config)` for any subsequent call within the same test function

## Test Counts
- Unit tests: 43 (was 38) — added 5 from `logging::init::tests`
- Integration tests: 13 (was 10) — added 3 from `logging_test.rs` (2 + 1 doc)
- Total: **56 tests, all passing**

## Quality Gates
- ✅ `cargo fmt --all -- --check`
- ✅ `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`
- ✅ `cargo test` (56 tests, 0 failures)
