# Phase 4 Memory: Local Scanning (US2)

**Spec**: 003-source-management  
**Phase**: 4 — User Story 2 — Index Local Documentation Directories  
**Tasks**: T019–T025  
**Status**: Complete  

## What Was Built

- `tests/acquire_local_test.rs` — 4 integration tests (S017, S019, S020, S021)
- `src/acquire/local.rs` — full `scan_local_source()` implementation with walkdir, path validation, sort, tracing
- `src/acquire/mod.rs` — added `pub use` re-exports for `filter_files`, `clone_git_source`, `scan_local_source`

## Key Decisions

1. **Path validation before walkdir** — call `crate::path::validate_path()` first; if path is outside allowed root, return PathViolation before attempting any filesystem walk.
2. **`is_dir()` check after validate_path** — catches non-existent dirs (which validate_path allows) and returns Pipeline error with descriptive message including source ID.
3. **`follow_links(false)`** — do not follow symlinks (FR-005 compliance); prevents loops and security escapes.
4. **Sort after collection** — `files.sort()` gives lexicographic determinism regardless of OS directory iteration order.
5. **Re-exports in mod.rs** — `pub use filter_files`, `clone_git_source`, `scan_local_source` added so they're accessible as `graphtor_core::acquire::scan_local_source`.

## Test Outcomes

- All 4 local integration tests pass: S017 (happy path), S019 (deterministic sort), S020 (missing dir), S021 (path violation)
- Full suite: 106 tests pass (79 unit + 4 git + 4 local + 6 config + 4 error + 2 logging + 5 path + 2 doc)

## Gates Passed

- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` ✅  
- `cargo fmt --all -- --check` ✅  
- `cargo test` ✅ (106 tests)
