---
title: "Serve Sources Stub Auto-Generation"
description: "Avoid implicit workspace ingestion by generating an empty sources stub when serving imported databases without explicit source config"
topic: "serve sources stub"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-05-26-serve-sources-stub-plan.md"
tags:
  - "config"
  - "serve"
  - "safety"
  - "imported-db"
stash_ids:
  - "25F91517"
---

## Problem Frame

When `graphtor-docs serve` is invoked in a workspace containing an existing
`.graphtor/data/*.db` (an imported database) but no `.graphtor/config/sources.yaml`,
the current fallback in `load_source_config` calls `build_workspace_source_config`,
which creates an implicit workspace source indexing all local `.md` files. This
triggers `spawn_background_sync` inside `cmd_serve`, mutating the imported database
with freshly-ingested workspace content - violating the user's intent of serving
the imported data read-only.

## Options Considered

### Option A: Guard in `cmd_serve` - skip background sync when no explicit config exists

* Pro: Minimal change; no filesystem write.
* Con: The auto-discovery still loads as the `SourceConfig`; other code paths
  may reference its sources. Partial fix - does not prevent future consumers
  from using the synthetic config.

### Option B: Auto-generate a `sources: []` stub when DB exists without config

* Pro: Makes the "no sources configured" state explicit and durable. All
  downstream code reads a real file and gets an empty list. Background sync
  is naturally skipped because `source_config.sources.is_empty()` is already
  checked. CLI `status --db-path` still works as before.
* Con: Creates a new file on disk; user must delete it if they later want
  workspace auto-discovery back.

### Option C: Introduce a `--read-only` flag to `serve`

* Pro: Explicit intent signalling.
* Con: Larger API surface; does not help existing workflows that already pass
  only `--db-path`.

## Decision

**Option B** - auto-generate a local `sources: []` stub file at
`.graphtor/config/sources.yaml` when `load_source_config` discovers no config
AND at least one `.db` file already exists in `.graphtor/data/`. The stub is
generated once and preserved; subsequent runs find the file and skip
workspace-based auto-discovery naturally.

The generation happens in `load_source_config` (or a new helper called from it),
NOT in `cmd_serve` specifically, so all commands benefit from the safety net.

## Acceptance Criteria

1. `serve` on a workspace with an imported DB and no sources config does NOT
   ingest local markdown files.
2. A `sources.yaml` with `sources: []` is written to `.graphtor/config/`.
3. `status --db-path` continues to work without changes.
4. If a user later runs `graphtor-docs init`, the generated `sources.yaml`
   remains in place unless `--force` is passed.
5. Unit test covers the stub generation path.
6. Integration test verifies `serve` does not mutate the DB when the stub
   is auto-generated.

## Covering Feature

**Title**: Serve safety - auto-generate empty sources stub for imported databases
**Kind**: feature (bug-driven safety improvement)
**Scope**: `src/config/mod.rs` (stub generation), `src/main.rs` (call site awareness), tests.

## References

- `src/config/mod.rs` - `load_source_config`, `discover_source_files`, `build_workspace_source_config`
- `src/main.rs` - `cmd_serve`, `spawn_background_sync`
