# Session Memory: 003-source-management Phase 2

**Date**: 2026-03-15
**Spec**: `specs/003-source-management/`
**Phase**: 2 — Foundational (Blocking Prerequisites)
**Status**: COMPLETE

## Task Overview

Phase 2 implements shared foundation types and the glob filter that all user stories
depend on. T006/T007 were satisfied by the Phase 1 T003 implementation. The primary
new work was T008/T009: TDD implementation of `filter_files()`.

**Tasks**: T006–T010 (5 tasks, all complete)

## Current State

### Tasks Completed

| Task | Description | Status |
|------|-------------|--------|
| T006 | Unit tests for result types — satisfied by T003 in Phase 1 (16 tests in result.rs) | ✅ Done |
| T007 | Implement all result types — satisfied by T003 in Phase 1 (all 9 types in result.rs) | ✅ Done |
| T008 | Write unit tests for `filter_files()` covering S026–S034 + error cases (12 tests) | ✅ Done (TDD red first) |
| T009 | Implement `filter_files()` using globset with include-then-exclude logic | ✅ Done (TDD green) |
| T010 | Verify `cargo test acquire` passes | ✅ Done — 28 acquire tests pass |

### Files Modified

| File | Action | Notes |
|------|--------|-------|
| `src/acquire/filter.rs` | Replaced stub | Full implementation + 12 TDD tests |
| `specs/003-source-management/tasks.md` | Updated | T006–T010 marked `[x]` |

### Test Results

- **98 tests** total: **all pass** (79 unit + 6 config + 4 error + 2 logging + 5 path + 2 doc)
- 12 new filter tests covering all S026–S034 scenarios + 2 error cases + 1 edge case
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`: **PASS**
- `cargo fmt --all -- --check`: **PASS** (applied auto-fix once for assert_eq! formatting)

## Important Discoveries

### `filter_files()` Implementation Notes

- **Path normalization**: Explicit `replace('\\', "/")` before GlobSet matching ensures correct behavior on Windows where walkdir returns backslash paths (globset doesn't always normalize internally in 0.4.x).
- **API**: `filter_files(files, include, exclude) -> Result<Vec<PathBuf>, GraphtorError>` — returns raw `Vec<PathBuf>`, not `FilteredFileSet`. The `FilteredFileSet` wrapper is assembled by the caller (Phase 5 `execute()`).
- **Warn on empty**: `tracing::warn!` logged when filtering produced empty set from non-empty input (S032).
- **GlobSet helper**: Private `build_glob_set(patterns, kind)` extracts compilation logic; returns `None` for empty input (caller interprets as "include all" or "exclude none").

### TDD Validation

Red → Green sequence confirmed:
- Red: 9 of 12 tests failed with stub returning `Ok(vec![])`
- Green: All 12 pass after implementation

## Next Steps (Phase 3)

Phase 3 implements US1 — Git acquisition (T011–T018):

### Tests to write first (T011–T014, all [P])
- T011: `clone_git_source()` happy path — use `git2::Repository::init_bare()` to create local bare repo, clone to target dir, verify `.git` exists (S008)
- T012: Skip-if-exists — pre-create target dir with `.git`, verify clone skipped (S010)
- T013: Non-existent branch — clone with invalid branch, verify Pipeline error (S012)
- T014: Unreachable URL — invalid URL, verify Pipeline error with source ID (S011)

### Implementation (T015–T017)
- T015: `clone_git_source()` — `git2::build::RepoBuilder` with `FetchOptions` for depth=1, single-branch
- T016: `git_error_to_pipeline()` helper — maps git2 errors to `GraphtorError::Pipeline { stage: "acquire" }`
- T017: Tracing instrumentation — INFO on start/complete, WARN on skip, ERROR on failure

### Key Implementation Notes
- Tests use `git2::Repository::init_bare()` to create local repos — NO network needed
- `clone_git_source(source: &GitSource, target_dir: &Path) -> Result<PathBuf, GraphtorError>`
- Skip logic: if `target_dir.join(".git").exists()` → return Ok(target_dir) without cloning (FR-003)
- Depth=1 via `FetchOptions::set_depth(1)` in `git2`
- Single branch via `FetchOptions` with specific refspec

## Context to Preserve

- `filter_files` is in `src/acquire/filter.rs` and is `pub` — accessed via `crate::acquire::filter::filter_files`
- It is NOT yet re-exported from `mod.rs` (only result types are re-exported)
- `GraphtorError::Pipeline { message, stage: "acquire".to_string() }` for all acquire errors
- `git2` crate v0.19 is in dependencies — `git2::build::RepoBuilder`, `git2::FetchOptions`
- `walkdir` v2 is in dependencies — not used until Phase 4
- Test isolation: all Phase 3 tests use `tempfile::tempdir()` — NO network access
