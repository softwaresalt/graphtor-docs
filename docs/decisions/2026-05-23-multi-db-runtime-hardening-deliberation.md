---
title: "Multi-Database Runtime Hardening"
description: "Harden concurrent multi-database access so status and search do not panic on SQLite lock contention"
topic: "multi-database runtime hardening"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-05-23-multi-db-runtime-hardening-plan.md"
tags:
  - "database"
  - "multi-db"
  - "locking"
  - "concurrency"
  - "mcp"
stash_ids:
  - "E6E6477A"
---

## Problem Frame

Graphtor-docs supports routing documentation sources to multiple database files
(e.g. `graph.db`, `powerbi.db`) via the `database` field in `sources.yaml`. The
runtime currently opens each database as an independent `DataStore` (CozoDB over
SQLite). When multiple operations attempt concurrent access — even read-only
operations like `status` — CozoDB panics with `database is locked` rather than
gracefully handling SQLite's locking semantics.

### Who cares

The operator (single developer) running graphtor-docs locally. A panic during
routine status queries or MCP tool searches is unacceptable for production use.

### Constraints

* Must work with CozoDB's SQLite backend (no engine swap)
* Must support concurrent read-only queries across databases without panic
* Must support serial write access (sync/ingest is already serial)
* Must not require external infrastructure (embedded-only architecture)
* Must remain `#![forbid(unsafe_code)]`

### Success criteria

* `graphtor-docs status` and `graphtor-docs status --json` can run in parallel
  without panic
* MCP search tools can query multiple databases concurrently (read-only)
* Write operations (sync/ingest) acquire exclusive access gracefully
* Lock conflicts produce a user-friendly error, never a panic

### Scope boundaries

* OUT: Changing the graph database engine away from CozoDB
* OUT: Multi-process write concurrency (single writer is acceptable)
* IN: Per-database advisory locking (upgrade from single workspace lock)
* IN: SQLite WAL mode or busy-timeout configuration for CozoDB
* IN: Read-only connection mode for search/status operations

## Research Findings

### Current architecture

* `DataStore::open_sqlite` opens CozoDB with `DbInstance::new("sqlite", path, Default::default())`
* No SQLite pragma configuration (no WAL mode, no busy_timeout)
* `WorkspaceLock` at `.graphtor/graphtor.lock` is workspace-wide, not per-database
* The `status` CLI command opens all databases to aggregate source counts
* MCP search tools (`search_local_docs`, `search_semantic`) query the active database

### Root cause of panic

CozoDB wraps SQLite but does not configure `busy_timeout` or WAL mode by default.
When two CozoDB instances access the same `.db` file concurrently, SQLite returns
`SQLITE_BUSY`, which CozoDB propagates as a panic rather than a recoverable error.

### Relevant patterns

* The existing `WorkspaceLock` pattern provides advisory file-based locking
* CozoDB supports read-only mode via `ScriptMutability::Immutable`
* SQLite WAL mode allows concurrent readers with one writer

## Options

### Option A: Per-Database Advisory Lock + Read-Only Handles

Open databases in read-only/immutable mode for status and search queries.
Acquire per-database advisory locks only for write operations (sync/ingest).
Configure SQLite busy_timeout through CozoDB options if supported.

* **Pros**: Minimal architectural change; leverages existing `WorkspaceLock` pattern; read-only handles avoid lock contention entirely
* **Cons**: Requires verifying CozoDB supports immutable open or busy_timeout; if not, may need a wrapper retry layer
* **Effort**: Medium
* **Fit**: High — directly addresses the panic with minimal blast radius

### Option B: Connection Pool with Serialized Write Queue

Create a connection pool abstraction that manages multiple `DataStore` handles
per database — one writer, N readers. All writes go through a serialized channel.

* **Pros**: Full concurrency model; future-proof for multi-process
* **Cons**: Significant complexity; over-engineered for single-developer use; requires async refactoring of sync pipeline
* **Effort**: High
* **Fit**: Medium — solves the problem but excessive for the use case

### Option C: Retry-with-Backoff Wrapper

Wrap all CozoDB query calls with a retry loop that catches `SQLITE_BUSY`-related
errors and retries with exponential backoff.

* **Pros**: Simple to implement; no architectural change
* **Cons**: Does not prevent the panic (CozoDB may panic before returning an error); masks latency issues; not a true fix
* **Effort**: Low
* **Fit**: Low — CozoDB panics rather than returning errors, so retry cannot catch

## Decision

**Chosen: Option A — Per-Database Advisory Lock + Read-Only Handles**

Rationale:
1. CozoDB's `ScriptMutability::Immutable` flag prevents writes and may allow
   concurrent reads without triggering SQLite's exclusive lock
2. Per-database advisory locks (extending `WorkspaceLock`) provide clear
   write exclusion without over-engineering
3. If CozoDB doesn't support concurrent immutable opens, the fallback is
   configuring SQLite WAL mode through the engine options map
4. This approach has minimal blast radius — changes are localized to `DataStore`
   and the workspace lock module

### Rejected alternatives

* **Option B**: Over-engineered for a single-developer embedded tool
* **Option C**: Cannot catch panics; does not address root cause

### Risks and mitigations

| Risk | Mitigation |
|---|---|
| CozoDB may not respect immutable flag for concurrent opens | Investigate engine options map; fall back to WAL mode configuration |
| SQLite WAL mode may not be configurable through CozoDB's API | Spike: test with `cozo::DbInstance::new("sqlite", path, options)` |
| Existing tests may not cover concurrent access | Add integration tests with parallel read operations |

### Unresolved questions

* Does CozoDB's SQLite backend support `PRAGMA journal_mode=WAL` through options?
* Does `ScriptMutability::Immutable` open the database in read-only mode at the SQLite level?
* These should be resolved via a focused spike task during implementation.
