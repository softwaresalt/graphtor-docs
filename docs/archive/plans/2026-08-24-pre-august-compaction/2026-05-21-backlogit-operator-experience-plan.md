---
title: "Backlogit Operator Experience — Telemetry & Progress Reporting"
source: "docs/decisions/2026-05-21-backlogit-operator-experience.md"
feature_id: "036-F"
shipment_id: "027-S"
date: 2026-05-21
---

## Objective

Add structured telemetry metrics and operator-visible progress reporting to the
graphtor-docs sync pipeline, exposed via the MCP `get_status` tool and an
optional CLI `--metrics` JSON output.

## Implementation Units

### Unit 1: SyncMetrics return type and tracing instrumentation

**Scope**: `src/sync/mod.rs`, `src/sync/reingest.rs`

1. Define `SyncMetrics` struct: `files_total`, `files_synced`, `files_deleted`,
   `chunks_created`, `chunks_deleted`, `duration_ms`, `errors`.
2. Modify `sync_source` to accumulate metrics during the sync loop and return
   `Result<SyncMetrics, GraphtorError>` (additive — callers that ignore the
   return value are unaffected due to existing `Result<(), _>` → can wrap).
3. Add `tracing::info!` events at milestones: sync start, per-file reingest,
   sync complete with summary fields.
4. Unit test: `sync_source` returns correct metrics for a known fixture.

**Files touched**: `src/sync/mod.rs`, `src/sync/reingest.rs`
**Risk**: Low — additive return type change.

### Unit 2: SyncStatus progress enhancement and MCP integration

**Scope**: `src/mcp/server.rs`, `src/cli/` (sync subcommand)

1. Extend `SyncStatus` enum:
   ```rust
   InProgress { source: String, current: usize, total: usize }
   Complete { metrics: SyncMetrics }
   ```
2. Update the background sync task (in `src/bin/` or `src/cli/`) to write
   `SyncStatus::InProgress` atomically via the existing `Arc<Mutex<SyncStatus>>`.
3. Update `get_status` MCP tool handler to format progress as
   `"Syncing: {source} ({current}/{total} files)"`.
4. Add `--metrics` flag to the CLI `sync` subcommand. When set, print
   `SyncMetrics` as JSON to stdout after sync completes.
5. Integration test: start server, trigger sync, poll `get_status`, verify
   progress reporting. CLI test: run with `--metrics`, parse JSON output.

**Files touched**: `src/mcp/server.rs`, `src/cli/mod.rs` or `src/cli/init.rs`,
`src/bin/graphtor.rs` (if sync task lives there)
**Risk**: Moderate — must ensure `Arc<Mutex<>>` update does not deadlock with
MCP tool handler reads. Existing pattern already handles this safely.

## Dependency Order

Unit 1 → Unit 2 (Unit 2 depends on `SyncMetrics` type from Unit 1).

## Acceptance Criteria (Plan-Level)

- `cargo test` passes with new tests for both units
- `cargo clippy --all-targets -- -D warnings` clean
- `get_status` MCP tool reports sync progress when sync is active
- CLI `sync --metrics` emits valid JSON with expected fields
- No breaking changes to existing public API surface
