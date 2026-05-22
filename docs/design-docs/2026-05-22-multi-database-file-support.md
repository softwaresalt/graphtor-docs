---
title: Multi-database file support
description: "Per-source database routing, multi-store MCP loading, and database-scoped incremental state"
---

## Context

graphtor-docs originally assumed a single database file for every configured
source. That model kept the runtime simple, but it forced unrelated source
sets into one storage file and one incremental sync state file.

## Decision

We allow every source in `sources.yaml` to declare an optional `database`
field. When the field is absent, the source uses the primary `--db-path`
target. When the field is present, the source syncs into a sibling database
file under the same parent directory as `--db-path`.

## Routing rules

* `database` is a file name, not a path
* `database` values must not contain path separators or parent-directory
  traversal
* `sync` groups planned sources by database file before opening stores
* `serve` opens every configured database and exposes one MCP surface across
  all loaded stores
* `status` reports all discovered databases from the active configuration
* `prewarm` follows the same routing rules as `sync`

## Incremental sync state

Incremental state is scoped per database file instead of per workspace. The
state file name is derived from the database file name:

* `graph.db` → `graph.sync_state.json`
* `notes.db` → `notes.sync_state.json`

This keeps git commit tracking, local file mtimes, and deletion detection
aligned with the database that owns the indexed chunks.

## MCP server behavior

The MCP server now loads one or more `DataStore` handles instead of a single
store. Text search, document lookup, source listing, topic research, and
status reporting aggregate across all loaded stores. Chunk-specific traversal
resolves against the store that owns the requested chunk ID.

## Consequences

The feature keeps the default single-database workflow unchanged while adding
isolation for teams that want separate storage files per source group. It also
keeps the CLI and MCP entry points stable: the routing decision lives in
configuration rather than in new commands or new tool names.
