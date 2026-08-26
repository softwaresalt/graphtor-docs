---
title: "Pre-warm Sync Mode with Progress Reporting and Backlogit Telemetry"
description: "Deliberation on adding a dedicated pre-warm CLI subcommand with file-level progress output and structured telemetry consumable by backlogit."
topic: "Pre-warm sync mode and telemetry integration"
depth: "lightweight"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-21-prewarm-sync-progress-plan.md"
source_stash_ids:
  - "3FE2DDFB"
  - "0D214027"
tags:
  - "sync"
  - "cli"
  - "progress"
  - "telemetry"
  - "backlogit"
---

## Context

graphtor-docs shipped structured `SyncMetrics` and `SyncStatus::InProgress` reporting in 036-F.
The `get_status` MCP tool now reports source-level progress and the CLI supports `--metrics` JSON
output. Two operator-experience gaps remain (explicitly deferred from 036-F scope):

1. **No dedicated pre-warm mode**: The operator must start the full MCP server to trigger sync.
   There is no CLI subcommand that indexes all sources and exits — the graph output during startup
   is confusing and progress is only visible via MCP polling.

2. **No backlogit-consumable telemetry**: The `--metrics` flag outputs JSON but does not emit it
   in a format that backlogit's telemetry harvest pipeline can directly consume (expects JSONL with
   `event_type`, `timestamp`, and typed payload fields).

Both share the `src/sync/` and `src/cli/` code surfaces and build directly on the 036-F
infrastructure.

## Options Considered

### Option A: `graphtor prewarm` subcommand with inline progress + JSONL telemetry

- Add a `prewarm` CLI subcommand that loads config, runs `sync_source` for all configured sources,
  and exits.
- During sync: emit per-file progress lines to stderr (`[2/47] ingesting docs/api/auth.md (4%)`)
  showing individual file and total completion percentage.
- On completion: emit a `SyncMetrics` JSONL record to stdout in backlogit-consumable format.
- Optionally support `--progress-bar` flag for indicatif-style bar (future enhancement, not MVP).

**Pros**: Clean separation of concerns (prewarm is distinct from serve), builds on existing
`SyncMetrics`, stderr progress doesn't pollute JSON output on stdout.
**Cons**: New subcommand surface; requires clap subcommand addition.

### Option B: `graphtor serve --prewarm --progress` flags

- Add `--prewarm` flag to the existing serve command that syncs before opening STDIO transport.
- Add `--progress` flag that emits file-level progress to stderr during startup.

**Pros**: No new subcommand; single entry point.
**Cons**: Conflates server lifecycle with sync-only mode; progress output on stderr may confuse
MCP clients reading STDIO.

### Option C: Progress callback trait with pluggable reporters

- Define a `SyncProgressReporter` trait with `on_file_start`, `on_file_complete`, `on_complete`.
- Implement `StderrProgressReporter` (file-by-file lines) and `JsonlTelemetryReporter`.
- Wire into `sync_source` as an optional callback parameter.

**Pros**: Maximum flexibility, testable, future-proof for GUI.
**Cons**: More abstraction than needed for two concrete reporters; over-design risk.

## Decision

**Option A** with elements of C for testability:

- Add a `prewarm` CLI subcommand via clap.
- Add a lightweight progress callback (`FnMut` closure, not full trait) to `sync_source` that
  reports `(file_path, file_index, total_files)` at each file boundary.
- The `prewarm` subcommand wires this callback to emit formatted progress lines to stderr.
- On sync completion, emit a JSONL telemetry record to stdout with backlogit-compatible structure.
- The trait approach (Option C) is deferred — the closure callback is sufficient and simpler.

## Scope Boundary

**In scope**:

- `src/cli/mod.rs`: Add `Prewarm` subcommand with clap derive
- `src/sync/mod.rs`: Add optional progress callback parameter to `sync_source`
- New `src/cli/prewarm.rs`: Prewarm command handler with progress and telemetry output
- Tests: unit test for progress callback invocation, integration test for JSONL output format

**Out of scope**:

- indicatif progress bars (future enhancement — plain text lines are MVP)
- MCP-based progress notifications (already handled by `SyncStatus::InProgress`)
- Multi-database support (separate stash entry 03D96C20)
- Modifying the existing `serve` subcommand startup behavior

## Risk Assessment

- **Blast radius**: Low. Additive subcommand; progress callback is opt-in parameter addition.
- **Complexity**: Low–Moderate. The callback threading through `sync_source` is the only change
  to existing code paths.
- **Dependencies on 036-F**: Direct — uses `SyncMetrics` for telemetry output format.
- **Estimated effort**: 3 tasks × 2 hours = 6 hours total.

## Requires plan hardening

no
