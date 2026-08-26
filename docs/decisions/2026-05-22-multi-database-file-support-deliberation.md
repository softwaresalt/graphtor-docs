---
title: "Multi-Database File Support"
description: "Route documentation sources into separate database files for domain isolation and cross-project reuse"
topic: "multi-database file support"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-22-multi-database-file-support-plan.md"
tags:
  - "config"
  - "acquire"
  - "database"
  - "multi-db"
stash_ids:
  - "03D96C20"
  - "1F123CF3"
  - "B751FA6D"
---

## Problem Frame

Graphtor-docs currently operates with a single database file (`graph.db`) for all indexed
documentation. This creates friction when the operator wants domain-separated knowledge
bases (Rust docs, C# docs, Power BI docs) that can be independently managed, reused across
projects, and kept at different freshness cadences.

**Who cares**: The single developer operating graphtor-docs across multiple projects with
overlapping documentation needs.

**Pain**: Re-ingesting the same documentation sources for each project; inability to compose
a project-specific search surface from pre-built domain databases.

**Success criteria**:

* `sources.yaml` supports a `database` field per source or source group that routes content
  into a named database file
* The sync pipeline respects per-source database routing
* The MCP serve command can load multiple database files simultaneously
* A source ingested into `rust-docs.db` can be referenced from any project without re-sync

**Scope boundary**: This feature does NOT cover cross-database graph edges, merged search
ranking across databases, or database versioning/migration. Those are follow-up concerns.

## Research Findings

### Current Architecture

* **Single DB path**: `main.rs` resolves one `db_path` via CLI `--db-path` or default
  `.graphtor/graph.db`. Every command (`sync`, `serve`, `status`, `prewarm`) receives this
  single path.
* **DataStore**: `DataStore::open_sqlite(db_path, cwd)` opens one CozoDB instance per
  invocation.
* **sources.yaml**: Flat list of `Source` entries (Git, Local, Url) with no database routing
  field.
* **Sync pipeline**: Iterates all sources, chunks, embeds, and stores into the single
  DataStore.

### Key Integration Points

* `src/config/source.rs` — `Source` enum and per-variant structs (`GitSource`,
  `LocalSource`, `UrlSource`)
* `src/main.rs` — `db_path` resolution and command dispatch
* Sync orchestration — `run_incremental_sync` takes a single store

## Options Evaluated

### Option A: Per-Source `database` Field

Add an optional `database: "filename.db"` field to each source entry in `sources.yaml`.
Sources without the field use the default database. The sync pipeline groups sources by
target database, opens each DataStore, and syncs the relevant source subset.

* **Pros**: Fine-grained routing; backward compatible (field is optional); minimal config
  schema change
* **Cons**: Repetitive if many sources share the same database; must handle multiple
  DataStore instances in sync
* **Effort**: Medium
* **Fit**: High — directly addresses the stash requirement (B751FA6D)

### Option B: Source Groups with `database` at Group Level

Introduce a `groups` key in `sources.yaml` where each group has a `database` and a list of
source IDs or inline sources. Ungrouped sources go to the default database.

* **Pros**: Clean separation; reduces repetition; natural organizational unit
* **Cons**: Breaking schema change (new top-level key); more complex config parsing;
  migration path needed for existing configs
* **Effort**: High
* **Fit**: Medium — elegant but over-engineered for current needs

### Option C: Per-Source `database` Field (Option A) with Future Group Extension

Start with Option A (per-source field). Document that a future `groups` syntax may be added
when source counts grow. This is the incremental approach.

* **Pros**: Ships quickly; fully backward compatible; leaves design space open
* **Cons**: Slight repetition if 10+ sources target the same DB (acceptable at current scale)
* **Effort**: Medium
* **Fit**: High — pragmatic delivery path

## Trade-off Comparison

| Criterion | Option A | Option B | Option C |
|---|---|---|---|
| Backward compatibility | Full | Breaking | Full |
| Implementation complexity | Medium | High | Medium |
| Config ergonomics at scale | Moderate | High | Moderate (upgradable) |
| Time to ship | ~6h | ~10h | ~6h |
| Risk | Low | Moderate | Low |

## Decision

**Chosen: Option C** — Per-source `database` field with documented future group extension.

This delivers the core capability (multi-database routing, cross-project reuse) without
breaking existing configurations or over-investing in organizational syntax before it is
needed. The serve command will accept multiple `--db-path` arguments or discover all `.db`
files in `.graphtor/`.

## Rejected Alternatives

* **Option B (Groups)**: Over-engineering for current source count (~8 sources). Can be
  layered on later without breaking Option C's per-source field.

## Unresolved Questions

* How does `serve` combine search results from multiple databases? (Defer to follow-up
  feature — initial implementation serves from one specified or all discovered databases.)
* Should `prewarm` iterate all databases or accept a filter? (Initial: iterate all.)

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Multiple concurrent DataStore opens exhaust file handles | Open one at a time during sync; document handle limits |
| Config migration confusion | New field is optional; existing configs work unchanged |
| Serve latency with many databases | Defer optimization; initial feature targets ≤5 databases |
