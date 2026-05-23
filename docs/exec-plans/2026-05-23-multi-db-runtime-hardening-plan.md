---
title: "Multi-Database Runtime Hardening — Implementation Plan"
type: impl-plan
source: docs/decisions/2026-05-23-multi-db-runtime-hardening-deliberation.md
stash_id: E6E6477A
created: 2026-05-23
---

# Multi-Database Runtime Hardening — Implementation Plan

## Problem Frame

The `DataStore::open_sqlite` function (`src/db/store.rs`) opens CozoDB with default SQLite
options — no WAL mode, no `busy_timeout`, no read-only flag. The workspace-level advisory
lock (`src/workspace/lock.rs`) protects against concurrent processes but does nothing for
concurrent in-process access to the same database file from parallel async tasks.

When the CLI `status` command aggregates data from multiple databases, or when MCP search
tools query databases in parallel, CozoDB's default SQLite backend panics on
`SQLITE_BUSY` instead of returning a recoverable error.

Affected modules: `src/db/store.rs`, `src/workspace/lock.rs`, `src/cli/mod.rs`,
`src/mcp/` (search tools).

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| No panic on concurrent read access | Configure SQLite WAL mode + busy_timeout via CozoDB engine options |
| Per-database locking for writes | Extend advisory lock to per-database granularity |
| Read-only handles for status/search | Open immutable `DataStore` handles for query-only operations |
| Graceful error on lock conflict | Return `GraphtorError::DatabaseLocked` instead of panic |
| Production readiness | Integration tests verifying concurrent read access |

## Implementation Units

### Unit 1: Spike — CozoDB SQLite Engine Options

**Posture**: spike (investigate, then implement)

Investigate whether CozoDB's SQLite backend accepts `busy_timeout` and `journal_mode`
through the engine options map (`BTreeMap<String, String>` third argument to
`DbInstance::new`). Write a minimal test that opens the same database twice with
WAL mode and verifies concurrent reads succeed.

* **Files**: `src/db/store.rs`, `tests/db_concurrency.rs` (new)
* **Changes**: Add test; document findings as code comments
* **Tests**: `test_concurrent_read_no_panic`
* **Verifiable outcome**: Test either passes (options work) or fails with documented reason

### Unit 2: DataStore Read-Only Open Mode

**Posture**: test-first

Add `DataStore::open_sqlite_readonly(path, root)` that opens with
`ScriptMutability::Immutable` and WAL-mode + busy_timeout engine options.
This handle can be used for status/search without write-lock contention.

* **Files**: `src/db/store.rs`
* **Changes**: New constructor; engine options configuration
* **Tests**: Unit test verifying immutable handle rejects write operations
* **Verifiable outcome**: `cargo test` passes with new unit tests

### Unit 3: Per-Database Advisory Lock

**Posture**: test-first

Extend `WorkspaceLock` (or create `DatabaseLock`) to support per-database lock
files at `.graphtor/{db_name}.lock`. Write operations acquire the database-specific
lock; read operations skip it.

* **Files**: `src/workspace/lock.rs` (or new `src/workspace/db_lock.rs`)
* **Changes**: New lock type parameterized by database filename
* **Tests**: Unit tests for acquire/release/stale-detection per database
* **Verifiable outcome**: `cargo test` passes; lock files created/removed correctly

### Unit 4: GraphtorError::DatabaseLocked Variant

**Posture**: test-first

Add a `DatabaseLocked { db_name, holder_pid }` variant to `GraphtorError` so that
lock conflicts are reported as structured errors rather than panics.

* **Files**: `src/error/mod.rs` (or wherever `GraphtorError` is defined)
* **Changes**: New error variant with Display impl
* **Tests**: Unit test verifying error message formatting
* **Verifiable outcome**: Compiles; error displays user-friendly message

### Unit 5: Integrate Read-Only Handles in CLI Status

**Posture**: test-first

Modify the `status` command to open databases via `open_sqlite_readonly` for
aggregation queries. Write operations (sync) continue using `open_sqlite` with
the per-database lock.

* **Files**: `src/cli/mod.rs` (status handler)
* **Changes**: Replace `open_sqlite` with `open_sqlite_readonly` for read paths
* **Tests**: Integration test running parallel `status` without panic
* **Verifiable outcome**: `cargo test`; manual verification of parallel `status`

### Unit 6: Integrate Read-Only Handles in MCP Search Tools

**Posture**: test-first

Modify MCP search tool implementations to use `open_sqlite_readonly` for all
query operations. Ensure search across multiple databases uses read-only handles.

* **Files**: `src/mcp/` (search tool implementations)
* **Changes**: Use read-only `DataStore` for search operations
* **Tests**: Integration test querying multiple databases concurrently
* **Verifiable outcome**: `cargo test`; no panic on concurrent MCP queries

### Unit 7: Integration Test — Concurrent Multi-DB Access

**Posture**: test-first

End-to-end test that opens multiple databases, runs parallel read queries
(simulating concurrent `status` and `search`), and verifies no panics.
Optionally test write + read concurrency with lock acquisition.

* **Files**: `tests/multi_db_concurrency.rs` (new)
* **Changes**: New integration test file
* **Tests**: `test_parallel_status_no_panic`, `test_parallel_search_no_panic`, `test_write_lock_blocks_correctly`
* **Verifiable outcome**: `cargo test` all green

## Dependency Graph

```text
Unit 1 (spike)
  ↓
Unit 2 (read-only open) ← Unit 4 (error variant)
  ↓                          ↓
Unit 3 (per-db lock) ←──────┘
  ↓
Unit 5 (CLI integration)
Unit 6 (MCP integration)
  ↓
Unit 7 (integration tests)
```

Units 2, 3, and 4 can proceed in parallel after Unit 1 resolves the spike.
Units 5 and 6 depend on Units 2+3+4. Unit 7 depends on all prior units.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| WAL mode via engine options | SQLite WAL allows concurrent readers; CozoDB's options map is the configuration surface |
| Separate read-only constructor | Explicit API boundary; prevents accidental writes from read paths |
| Per-database lock (not global) | Allows reading DB-A while writing DB-B |
| Advisory lock, not OS-level | Consistent with existing `WorkspaceLock`; works cross-platform |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| CozoDB may not pass engine options to SQLite | Unit 1 spike resolves this; if blocked, escalate to CozoDB issue/PR |
| WAL mode may behave differently on Windows | Test on Windows (primary dev platform per evidence) |
| Stale per-database locks on crash | Same stale-detection logic as `WorkspaceLock` (1-hour threshold) |

## Plan Hardening Signals

* Public API, schema, or contract change: **YES** — new `DataStore` constructor is public API
* Security, auth, permission, or compliance: **No**
* Migration, destructive data/config action: **No**
* External integration or operator checkpoint: **No**
* High runtime, rollout, or rollback risk: **YES** — changes database open semantics; could affect all data paths

**Requires plan hardening: yes**

## Plan Hardening

### Risk Triggers

| Signal | Present | Detail |
|---|---|---|
| Public API change | YES | New `DataStore::open_sqlite_readonly` constructor; new `GraphtorError::DatabaseLocked` variant |
| Security/auth/compliance | No | — |
| Migration/destructive action | No | — |
| External integration/checkpoint | No | — |
| High runtime/rollback risk | YES | Changes database open semantics for every read path (status, search) |

### Protected Invariants

1. **No data loss**: Read-only handles cannot corrupt databases; write path unchanged
2. **No behavioral regression**: Existing single-database workflows must work identically
3. **Panic elimination**: Zero panics from concurrent database access in all test scenarios
4. **Lock hygiene**: Per-database locks must never outlive their owning process (stale detection)

### Risky Actions (strict-safety vocabulary)

| ProposedAction | ActionRisk | Approval | Rollback |
|---|---|---|---|
| Change `DataStore::open_sqlite` engine options (WAL mode) | moderate | Agent (test-verified) | Revert to default options; no data migration needed |
| Add per-database lock files to `.graphtor/` | low | Agent | Remove lock files; revert to workspace-level lock |
| Replace `open_sqlite` with `open_sqlite_readonly` in CLI/MCP read paths | moderate | Agent (test-verified) | Revert call sites to `open_sqlite` |

### Reinforced Verification Plan

* Unit 1 spike MUST resolve before any production code changes proceed
* If CozoDB does not support WAL-mode options, escalate to operator before implementing workarounds
* Integration tests MUST verify: (a) parallel reads succeed, (b) write+read coexistence works, (c) write+write is blocked gracefully
* Windows-specific testing required (primary platform per evidence)

### Rollback Strategy

* All changes are additive — `open_sqlite` remains available; revert is call-site changes only
* No database schema migration; no data format changes
* Lock files are advisory; removing them has no data impact

## Runtime Verification and Closure

### Changed runtime surfaces

* CLI `status` command — concurrent access behavior changes
* MCP search tools — concurrent query behavior changes
* Database open lifecycle — new lock files appear in `.graphtor/`

### Verification expectations

* Run parallel `status` commands — no panic, correct output
* Run parallel MCP search queries — no panic, correct results
* Run `sync` while `status` is active — sync acquires lock, status uses read-only handle
* Verify `.graphtor/{db}.lock` files are created during write and removed after

### Closure expectations

* Document the per-database locking model in `docs/design-docs/`
* Note the WAL mode dependency in operational docs
* Rollback trigger: if concurrent reads still panic after deployment, revert to serial access
