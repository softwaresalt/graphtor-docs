---
title: "Backlogit Operator Experience — Telemetry & Progress Reporting"
description: "Deliberation on adding structured sync telemetry and operator-visible progress reporting through MCP status and CLI metrics output."
topic: "Sync telemetry and progress reporting"
depth: "lightweight"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-21-backlogit-operator-experience-plan.md"
tags:
  - "sync"
  - "observability"
  - "mcp"
  - "cli"
  - "telemetry"
---

## Context

graphtor-docs is a local documentation RAG system that syncs sources, indexes
content into CozoDB, and serves search/traversal via an MCP STDIO server. Two
related operator-experience gaps exist:

1. **No telemetry/metrics output**: Sync and indexing operations produce no
   structured metrics (file count, chunk count, timing, error count) that can be
   consumed by backlogit or other tooling for operational visibility.

2. **No progress reporting during sync**: The `sync_source` pipeline operates
   silently. When invoked at server startup (pre-warm) or on-demand, the
   operator sees no progress indication — only completion or error.

Both issues share the same code surface: `src/sync/` (orchestration) and
`src/mcp/server.rs` (status reporting). Fixing them together avoids double-
touching the sync pipeline.

## Options Considered

### Option A: Structured tracing events + SyncStatus enhancement

- Emit `tracing` events with structured fields (`files_total`, `files_synced`,
  `chunks_created`, `elapsed_ms`) at key sync milestones.
- Enhance `SyncStatus` enum to carry progress state (InProgress { current, total }).
- `get_status` MCP tool already exists — expose progress through it.
- Telemetry consumers (backlogit hooks, log aggregation) subscribe via tracing subscriber.

**Pros**: Idiomatic Rust, zero new dependencies, MCP-native progress visibility.
**Cons**: Requires tracing subscriber configuration for metrics export.

### Option B: Dedicated metrics crate (prometheus/opentelemetry)

- Integrate `metrics` crate with counters/histograms for sync operations.
- Run a metrics endpoint or file-based export.

**Pros**: Industry-standard metrics format.
**Cons**: Heavy dependency for a single-binary local tool; over-engineered for
the operator-local use case.

### Option C: Progress callback trait

- Define a `SyncProgress` trait that sync functions accept.
- Implement for MCP (SyncStatus), CLI (stderr progress bar), and silent (noop).

**Pros**: Clean abstraction, testable.
**Cons**: More boilerplate; trait object overhead for a simple progress report.

## Decision

**Option A** with a lightweight progress callback for testability:

- Use structured `tracing::info_span!` and `tracing::info!` events with
  counters at sync milestones.
- Enhance `SyncStatus` to `SyncStatus::InProgress { source, current, total }`.
- The existing `get_status` MCP tool reports progress to the operator.
- Add a `sync_metrics` summary struct returned from `sync_source` for
  programmatic consumption (file count, chunk count, duration).
- Backlogit integration: emit a JSON summary to stdout (when `--metrics` flag
  is passed to CLI) that backlogit hooks can consume.

## Scope Boundary

**In scope**:
- `src/sync/mod.rs`: Return `SyncMetrics` from `sync_source`, emit tracing events
- `src/mcp/server.rs`: Enhance `SyncStatus` to carry progress, update atomically during sync
- `src/cli/`: Add `--metrics` flag to `sync` subcommand for JSON metrics output
- Tests: unit test for `SyncMetrics` aggregation, integration test for progress status

**Out of scope**:
- External metrics endpoints (prometheus, OTLP)
- UI/TUI progress bars (deferred to future session)
- Multi-database support (separate stash entry 03D96C20)

## Risk Assessment

- **Blast radius**: Low. Changes touch sync return types and MCP status enum.
  Existing API contracts preserved (additive changes only).
- **Complexity**: Moderate. Thread-safe progress update from sync task to MCP
  status requires `Arc<Mutex<>>` (already in place for `SyncStatus`).
- **Estimated effort**: 2 tasks × 2 hours = 4 hours total.
