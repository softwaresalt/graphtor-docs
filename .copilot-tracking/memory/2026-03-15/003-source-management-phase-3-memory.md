# Phase 3 Memory: Git Acquisition (US1)

**Spec**: 003-source-management  
**Phase**: 3 — User Story 1 — Acquire Git Documentation Repositories  
**Tasks**: T011–T018  
**Status**: Complete  

## What Was Built

- `tests/acquire_git_test.rs` — 4 integration tests (S008, S010, S011, S012) using local bare repos via `git2`
- `src/acquire/git.rs` — full `clone_git_source()` implementation with shallow clone, skip-if-exists, cleanup on failure, and tracing instrumentation
- `build.rs` — Windows MSVC linker fix emitting `cargo:rustc-link-lib=advapi32` for integration tests

## Key Decisions

1. **`git_error_to_pipeline` takes `&git2::Error`** — clippy `needless_pass_by_value` requires reference; callers pass `&e`.
2. **Shallow fallback logic** — when `file://` local transport raises `GIT_ERROR_NET` with "shallow" in message, retry without `depth(1)`. This is test-infra-specific but correct behavior.
3. **advapi32 workaround via build.rs** — `libgit2-sys` emits `advapi32` link pragma but it does NOT propagate to integration test binaries on Windows MSVC. `build.rs` re-emits it unconditionally on Windows.
4. **File mode literal** — clippy `unreadable_literal` requires `0o100_644` not `0o100644`.

## Test Outcomes

- All 4 git integration tests pass: S008 (happy path), S010 (skip), S011 (bad URL), S012 (bad branch)  
- Full suite: 102 tests pass (79 unit + 4 git + 6 config + 4 error + 2 logging + 5 path + 2 doc)

## Gates Passed

- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` ✅  
- `cargo fmt --all -- --check` ✅  
- `cargo test` ✅ (102 tests)
