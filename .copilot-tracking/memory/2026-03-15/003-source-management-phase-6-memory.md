# Phase 6 Memory: Idempotent Acquisition (US4)

**Spec**: 003-source-management  
**Phase**: 6 — User Story 4 — Re-run Acquisition Safely  
**Tasks**: T030–T035  
**Status**: Complete  

## What Was Built

- `tests/acquire_plan_test.rs` — 2 integration tests (S048 idempotent git, S049 local re-scan)
- `src/acquire/plan.rs` — full `plan()` implementation + `validate_sources()` stub for Phase 7
- `src/acquire/mod.rs` — full `execute()` with `dispatch_planned_source`, `execute_clone_git`, `execute_scan_local`, `scan_and_filter` helpers
- `src/acquire/result.rs` — added `allowed_root: PathBuf` field to `AcquisitionPlan`

## Key Decisions

1. **`AcquisitionPlan.allowed_root`** — added to pass through to `execute()` which needs it for `scan_local_source()`. This avoids changing the `execute(plan)` API signature.
2. **plan() uses `validate_path(data_root, allowed_root)` after `create_dir_all`** — canonical data_root is obtained this way (no direct access to `canonicalize_clean` which is `pub(super)`).
3. **Git targets validated in plan() even before clone** — `validate_path(&target_dir, allowed_root)` handles non-existent paths via resolve_path walk-up logic.
4. **execute() is `#[must_use]`** — it has no side effects on its output; clippy pedantic requires this.
5. **validate_sources() returns all-valid stub** — placeholder for Phase 7 implementation; returns `ValidationReport { valid_count: all, total_count: all, errors: [] }`.
6. **scan_and_filter helper** — shared by both CloneGit (post-clone scan) and ScanLocal paths to reduce duplication.

## Test Outcomes

- S048: first run CloneGit → success, second run SkipGit → skipped ✅
- S049: local scan first run 2 files, add file, second run 3 files ✅
- Full suite: 112 tests pass (79 unit + 4 git + 4 local + 4 filter + 2 plan + 6 config + 4 error + 2 logging + 5 path + 2 doc)

## Gates Passed

- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` ✅  
- `cargo fmt --all -- --check` ✅  
- `cargo test` ✅ (112 tests)
