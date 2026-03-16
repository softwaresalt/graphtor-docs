# Phase 7 Memory: Source Validation (US5)

**Spec**: 003-source-management  
**Phase**: 7 — User Story 5 — Validate Source Registry Before Processing  
**Tasks**: T036–T042  
**Status**: Complete  

## What Was Built

- `tests/acquire_plan_test.rs` — 4 new validation tests (S035, S036, S038, S040) added to existing file
- `src/acquire/plan.rs` — full `validate_sources()` implementation + private helpers: `is_valid_git_url`, `is_valid_https_url`, `is_valid_ssh_url`, `validate_globs`

## Key Decisions

1. **`validate_sources` is infallible** — returns `ValidationReport` with zero or more errors; never returns `Result`. This enables single-pass error collection without early exit.
2. **URL validation rules**:
   - HTTPS: must start with `https://` with non-empty host after scheme
   - SSH: must match `git@host:path` — both host and path non-empty after the colon
   - Rejected: `http://`, `ftp://`, bare strings like `not-a-url`
3. **Path existence before path security** — clippy `if_not_else` required inverting the condition. Now: if exists → check security; else → report missing.
4. **`valid_count` computation**: counts distinct source IDs with errors, subtracts from total. Uses `HashSet<&str>` to deduplicate (a source with multiple errors still counts once as invalid).
5. **`validate_globs` uses `globset::Glob::new(pattern).is_err()`** — reuses the existing globset dependency; no new deps added.

## Test Outcomes

- S035: valid HTTPS + valid local dir → empty error list ✅
- S036: `not-a-valid-url` → ValidationError on `url` field ✅  
- S038: non-existent local path → ValidationError on `path` field ✅
- S040: 2 invalid sources → 2 errors collected in single pass ✅
- Full suite: 116 tests pass (79 unit + 4 git + 4 local + 4 filter + 6 plan + 6 config + 4 error + 2 logging + 5 path + 2 doc)

## Gates Passed

- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` ✅  
- `cargo fmt --all -- --check` ✅  
- `cargo test` ✅ (116 tests)
