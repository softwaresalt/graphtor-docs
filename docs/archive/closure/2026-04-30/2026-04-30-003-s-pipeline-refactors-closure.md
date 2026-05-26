---
date: 2026-04-30
slug: 003-s-pipeline-refactors
pr: 9
merge_commit: 16d8aba
shipment: 003-S
mode: post-merge
status: READY
owner: copilot
---

## Operational Closure — 003-S Pipeline Refactors

### Change Summary

PR #9 completes the follow-up refactors for shipment **003-S** (Dependency Hygiene & Pipeline
Foundation). All changes are confined to `src/pipeline/mod.rs` and `src/lib.rs`; no runtime
surfaces, database schema, CLI commands, or MCP tools were modified.

| Task | Change | Scope |
|---|---|---|
| 007.007-T | `process_batch` returns `BatchResult` struct (was tuple) | Private |
| 007.008-T | `FileError::path` changed from `String` to `PathBuf` | Public API |
| 007.009-T | `build_source_record` clones `id` once instead of twice | Private |
| Post-review | `FileError::path` doc updated; idiomatic `PathBuf::from`; crate re-exports | Public API |

### CI and Review Status

| Signal | Result |
|---|---|
| `cargo check` | ✅ Pass |
| `cargo clippy --pedantic` | ✅ Pass (zero warnings) |
| `cargo fmt` | ✅ Pass |
| Unit tests (81) | ✅ Pass |
| Pipeline integration tests (4) | ✅ Pass |
| Copilot Review | ✅ No comments |
| GitHub Actions CI | ✅ Pass (1m 30s) |

### Invariants to Preserve

1. `FileError` accumulates per-file failures without aborting sibling files — continue-on-failure
   semantics must not regress.
2. `PipelineResult::errors_encountered` receives errors from all three stages: parse, load, and
   acquisition failure.
3. Chunk IDs remain deterministic (SHA-256 of content + source-relative path) — unaffected by
   this change.
4. `process_batch` is idempotent — running twice on the same input produces the same output.

### Pre-Deploy Audits

Not applicable. This is a library-only change. There is no deployment step; the binary is
rebuilt from source on demand.

**Callers that pattern-match or directly access `FileError::path` as `String` must be updated.**
In this codebase, only `tests/pipeline_resilience_test.rs` accessed the field — it was updated
in the same PR. No external consumers exist at v0.1.0.

### Deployment / Rollout Path

Merge-only. The change is live on `main` at commit `16d8aba`. No deployment, migration, or
feature-flag step is required.

### Post-Deploy Checks

1. Run `cargo check` on a fresh clone to confirm the crate root re-exports compile cleanly.
2. Run `cargo test --test pipeline_resilience_test` to confirm `FileError::path` PathBuf
   assertions pass.
3. Confirm `cargo doc` generates documentation for `FileError`, `PipelineResult`, and
   `PipelineConfig` at the crate root (previously they were only reachable via
   `graphtor_core::pipeline`).

### Risky Action Record

No risky actions required approval. All changes were safe refactors with full test coverage.
The only semver-visible change (`FileError::path: String → PathBuf`) is acceptable at v0.1.0
pre-release and is documented in the `FileError::path` doc comment.

### Healthy Signals

- `cargo test` passes in full
- `graphtor_core::FileError`, `graphtor_core::PipelineResult`, `graphtor_core::PipelineConfig`
  are accessible at the crate root
- Pipeline error records carry `PathBuf` values — log consumers that call `.to_string_lossy()`
  produce the same textual output as before

### Failure Signals

- Any `FileError::path` construction that still uses `String` (would be caught by `cargo check`)
- Integration test `pipeline_skips_invalid_utf8_and_processes_valid_files` failing —
  indicates the `PathBuf` assertion was regressed

### Monitoring Plan

No production monitoring required. This is an embedded local library with no network exposure.

Log output from the pipeline (`tracing::warn!`) continues to display `path = %display_path`
using `file.to_string_lossy()` — no change to log format or content.

### Rollback Trigger

Not applicable. The change is a pure type refactor. If a regression is found, revert PR #9
via `git revert 16d8aba` and re-run quality gates.

### Rollback Procedure

```powershell
git revert 16d8aba --no-edit
git push origin main
```

Re-run `cargo test` to confirm revert is clean.

### Validation Window

24 hours of nominal development use. Given zero external consumers and full test coverage,
risk of regression is low.

### Outstanding Items

| ID | Severity | Item | Action |
|---|---|---|---|
| ARCH-004 | P3 | Synthetic `source:{id}` paths stored as `PathBuf` — consider typed enum | Backlog |
| RUST-001 | P3 | Document `FileError::path` type change in CHANGELOG | Backlog |
| 007.006-T | Blocked | Upgrade cozo/git2 deps (RUSTSEC-2026-0041, RUSTSEC-2026-0008) | Awaits upstream |

### Readiness Status

**READY** — merge complete, all checks passing, no runtime surfaces affected, no monitoring
action required. Backlog items recorded above for future follow-up.
