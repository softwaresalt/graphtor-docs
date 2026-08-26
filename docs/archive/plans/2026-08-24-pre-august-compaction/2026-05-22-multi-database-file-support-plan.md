---
title: "Multi-Database File Support — Implementation Plan"
description: "Route documentation sources into per-source database files via sources.yaml config"
source: "docs/decisions/2026-05-22-multi-database-file-support-deliberation.md"
stash_ids:
  - "03D96C20"
  - "1F123CF3"
  - "B751FA6D"
---

## Problem Frame

Graphtor-docs operates on a single `DataStore` instance backed by one SQLite/CozoDB file
(`.graphtor/graph.db`). The sync pipeline opens this single store and writes all chunked,
embedded documentation into it regardless of source. The serve command loads this one
database for MCP queries.

The goal is to allow `sources.yaml` to declare a `database` field per source entry so that
content is routed into named database files (e.g., `rust-docs.db`, `powerbi.db`). Sources
without the field continue using the default database. The serve command loads all discovered
databases (or a specified subset) to answer queries across domains.

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| R1: Per-source `database` field in config | Add optional `database: String` to source structs |
| R2: Sync routes content to target DB | Group sources by database, open per-group DataStore |
| R3: Serve loads multiple databases | Discover `.graphtor/*.db` or accept multiple `--db-path` |
| R4: Cross-project reuse without re-sync | Database files are self-contained; symlink or path reference |
| R5: Backward compatibility | Field is optional; absent means default `graph.db` |

## Implementation Units

### Unit 1: Config Schema — Add `database` Field

**Domain**: config (code)
**Files**: `src/config/source.rs`, `.graphtor/config/sources.yaml` (example)
**Changes**:

* Add `#[serde(default)] pub database: Option<String>` to `GitSource`, `LocalSource`,
  `UrlSource` structs
* Add a helper `Source::database(&self) -> Option<&str>` method
* Update validation to reject empty strings and path-traversal attempts in database names

**Tests**: Unit tests in `src/config/source.rs` — parse a source with `database: "rust.db"`,
parse without (defaults to None), reject `database: "../escape.db"`

**Execution posture**: Test-first
**Verifiable outcome**: `cargo test` passes with new validation cases

### Unit 2: Sync Pipeline — Multi-Database Routing

**Domain**: sync orchestration (code)
**Files**: `src/main.rs` (cmd_sync function and helpers)
**Changes**:

* Group parsed sources by `source.database().unwrap_or("graph.db")`
* For each group, resolve the database path relative to `.graphtor/`
* Open a `DataStore` per group and run `run_incremental_sync` with the subset
* Report per-database sync metrics

**Tests**: Integration test in `tests/` — sync two sources targeting different databases,
verify both `.graphtor/rust.db` and `.graphtor/graph.db` exist and contain expected chunks

**Execution posture**: Test-first
**Verifiable outcome**: Integration test passes; two DB files created
**Dependencies**: Unit 1 (config field must exist)

### Unit 3: Serve Command — Multi-Database Loading

**Domain**: serve/MCP (code)
**Files**: `src/main.rs` (cmd_serve function)
**Changes**:

* If `--db-path` is not specified, discover all `*.db` files in `.graphtor/`
* If `--db-path` is specified (one or more), load those specific databases
* Search tools query across all loaded databases and merge results
* Status tool reports per-database statistics

**Tests**: Integration test — serve with two databases, invoke `search_local_docs`, verify
results from both databases appear

**Execution posture**: Test-first
**Verifiable outcome**: MCP search returns results from multiple databases
**Dependencies**: Unit 2 (databases must be syncable to test serve)

### Unit 4: Prewarm and Status — Multi-Database Awareness

**Domain**: CLI commands (code)
**Files**: `src/main.rs` (cmd_prewarm, cmd_status)
**Changes**:

* Prewarm iterates all discovered databases (or filtered by `--db-path`)
* Status reports per-database chunk counts and index health

**Tests**: Unit/integration test — status with two databases shows both
**Execution posture**: Test-first
**Verifiable outcome**: Status output lists both databases
**Dependencies**: Unit 2

### Unit 5: Documentation and Config Examples

**Domain**: docs
**Files**: `README.md`, `.graphtor/config/sources.yaml`, `docs/design-docs/`
**Changes**:

* Document the `database` field in README config section
* Add example sources with different database targets
* Graduate the design rationale from the deliberation artifact

**Tests**: N/A (docs only)
**Execution posture**: After Units 1–4 pass
**Dependencies**: Units 1–4

## Dependency Graph

```text
Unit 1 (config schema)
  ├── Unit 2 (sync routing) ──┐
  │       │                    │
  │       ├── Unit 3 (serve)   │
  │       └── Unit 4 (prewarm/status)
  │                            │
  └────────────────────────────┘
                               │
                        Unit 5 (docs)
```

Execution order: 1 → 2 → [3, 4] (parallel) → 5

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Optional field, not required | Backward compatibility; existing configs must work unchanged |
| Database path relative to `.graphtor/` | Keeps all DB files in one discoverable location |
| Reject path traversal in database names | Security boundary — prevent writing outside workspace |
| Discover all `*.db` in serve mode | Zero-config multi-database experience; explicit `--db-path` for filtering |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| File handle exhaustion with many databases | Open one at a time during sync; document limit |
| Merged search ranking across databases | Defer to follow-up; initial impl concatenates results |
| Database name collisions | Validation rejects names that resolve to the same canonical path |

## Plan Hardening Signals

* Public API, schema, or contract change: **Present** — `sources.yaml` schema gains a new
  field; MCP serve behavior changes (loads multiple databases)
* Security, auth, permission, or compliance-sensitive: **Absent**
* Migration, backfill, destructive data/config action: **Absent** — field is additive
* External integration, operator checkpoint: **Absent**
* High runtime, rollout, or rollback risk: **Absent**

**Requires plan hardening: no**

The schema change is additive (optional field), and the serve behavior change is
backward-compatible (single-DB behavior preserved when only `graph.db` exists). No
hardening needed.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification | Closure |
|---|---|---|---|
| Unit 2 | CLI `sync` command | Verify two DB files created with correct content | N/A — local-only |
| Unit 3 | MCP `serve` + search tools | Query via MCP client, confirm cross-DB results | N/A — local-only |
| Unit 4 | CLI `status`, `prewarm` | Verify output lists multiple databases | N/A — local-only |

No external deployment, monitoring, or rollback concerns — this is a local-only tool.

## Constitution Check

| Principle | Status |
|---|---|
| I. Safety-First Rust | Compliant — no unsafe, proper error propagation |
| II. Test-First | Compliant — each unit specifies test-first posture |
| III. Workspace Isolation | Compliant — database path validation rejects traversal |
| IV. CLI Containment | Compliant — all writes within `.graphtor/` |
| XI. Merge Commit | Will use merge commit strategy |
