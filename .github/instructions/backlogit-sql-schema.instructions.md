---
description: "SQLite schema reference and example queries for agents using the backlogit MCP backlogit_query_sql tool"
applyTo: '**'
---

# Backlogit SQL Schema Reference

The SQLite index at `.backlogit/backlogit.db` is an ephemeral, read-only query cache. The Markdown files under `.backlogit/` are the source of truth. Use `backlogit_query_sql` to run `SELECT` statements against this cache. Rebuild the cache at any time with `backlogit_sync_index`.

## Tables

### `items`: Work artifacts

The primary table for all backlogit artifacts (features, tasks, subtasks, deliberations, shipments).

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | Hierarchical ID: `001-F`, `001.001-T`, `001.001.001-ST` |
| `title` | TEXT | Artifact title |
| `status` | TEXT | Persisted union of all work item type status vocabularies; accepted values are constrained per artifact type — see [Status values by artifact type](#status-values-by-artifact-type) |
| `artifact_type` | TEXT | `feature`, `task`, `subtask`, `deliberation`, `shipment` |
| `parent_id` | TEXT | ID of the parent artifact (`NULL` for level-1) |
| `sprint` | TEXT | Sprint ID (optional) |
| `priority` | TEXT | `low`, `medium`, `high`, `critical` |
| `description` | TEXT | Artifact body text |
| `custom_fields` | TEXT | JSON blob of extra fields (`source_stash_id`, `harness_status`, etc.) |
| `created_at` | DATETIME | ISO-8601 creation timestamp |
| `updated_at` | DATETIME | ISO-8601 last-update timestamp |
| `assigned_to` | TEXT | Assignee identifier |
| `owner` | TEXT | Owner identifier |
| `labels` | TEXT | Comma-separated label string |
| `dependencies` | TEXT | Denormalized comma-separated dependency IDs |
| `references` | TEXT | Comma-separated reference paths |
| `commit` | TEXT | Last associated commit SHA |
| `level` | INTEGER | Hierarchy depth: `1`=feature, `2`=task, `3`=subtask |
| `hierarchy_path` | TEXT | Path without type suffix: `001`, `001.001`, `001.001.001` |

**FTS5 virtual table**: `items_fts` (columns: `id`, `title`, `description`, `labels`). Linked to `items` via `rowid`.

#### Status values by artifact type

`items.status` persists the union of every work item type (WIT) status vocabulary, so a value that appears in this column is not accepted by every artifact type. The WIT definitions in `.backlogit/header-def.yaml` are authoritative: each artifact type accepts only the values listed for its own type below. Resolve a status against the artifact's `artifact_type` before treating it as terminal — this matters for shipment safe-close validation, where the only terminal statuses are `shipped` and `abandoned`, recorded before lifecycle archival.

| Artifact type | Accepted `status` values (WIT) | Terminal statuses |
|---|---|---|
| `shipment` | `queued`, `active`, `shipped`, `abandoned` | `shipped` or `abandoned` |
| `feature` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |
| `task` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |
| `subtask` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |
| `bug` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |
| `review` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |
| `deliberation` | `queued`, `active`, `blocked`, `review`, `done`, `accepted`, `rejected`, `archived` | `done`, `accepted`, or `rejected` as configured |

`archived` is applied by lifecycle archival (`backlogit_archive_item`, `backlogit_move_item`), not by an authoring transition, and no shipment WIT transition targets it. Archive routing in `.backlogit/registry.yaml` admits `done`, `accepted`, `rejected`, and `archived`. It deliberately does **not** admit `shipped` or `abandoned`: safe-close requires the shipment record to stay live after `move --status shipped` so it can be re-read and verified before the explicit single-artifact `backlogit archive` call, and routing those statuses would relocate the record immediately and make that verification step halt every time. Those two statuses remain in the shipment status vocabulary; they are simply not archive-routing statuses. Artifacts therefore reach `archive/` either by routing or by explicit archival, so files under `archive/` may persist a type-specific terminal status (for example `done` or `shipped`) or `archived`. Queue routing admits `queued`, `active`, `blocked`, and `review`.

### `item_deps`: Dependencies

| Column | Type | Notes |
|---|---|---|
| `item_id` | TEXT | Source artifact ID |
| `depends_on` | TEXT | Target artifact ID |
| `dep_type` | TEXT | `blocks`, `relates_to`, `parent_of` |

### `item_links`: Relationships

Explicit typed links between artifacts.

| Column | Type | Notes |
|---|---|---|
| `source_id` | TEXT | Source artifact ID |
| `target_id` | TEXT | Target artifact ID |
| `link_type` | TEXT | `related_to`, `duplicate_of`, `informs`, `supersedes`, `spike_ref` |
| `created_at` | DATETIME | Link creation timestamp |

### `stash_entries`: Stash items

| Column | Type | Notes |
|---|---|---|
| `stash_id` | TEXT PK | Hex ID (e.g. `A1B2C3D4`) |
| `priority` | TEXT | `low`, `medium`, `high`, `critical` |
| `kind` | TEXT | `feature`, `task`, `bug`, `epic`, `unknown` |
| `text` | TEXT | Stash entry text |
| `deliberation_id` | TEXT | Linked deliberation artifact ID (optional) |
| `state` | TEXT | `active`, `harvested`, `removed` |
| `source_path` | TEXT | Relative path to the stash backing store (format may vary) |
| `updated_at` | DATETIME | Last state update |

### `stash_links`: Stash-to-artifact harvest links

| Column | Type | Notes |
|---|---|---|
| `stash_id` | TEXT PK | Stash entry ID |
| `item_id` | TEXT | Harvested artifact ID |
| `linked_at` | DATETIME | Harvest timestamp |

### `commit_links`: Commit traceability

| Column | Type | Notes |
|---|---|---|
| `item_id` | TEXT | Artifact ID |
| `commit_sha` | TEXT | Full commit SHA |
| `message` | TEXT | Commit message |
| `author` | TEXT | Commit author |

### `item_logs`: Log registry

| Column | Type | Notes |
|---|---|---|
| `item_id` | TEXT PK | Artifact ID |
| `log_path` | TEXT | Relative path to the backing log file |
| `updated_at` | DATETIME | Log file last-updated timestamp |

### `item_log_entries`: Individual log events

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Auto-increment row ID |
| `item_id` | TEXT | Artifact ID |
| `log_path` | TEXT | Relative log file path |
| `timestamp` | DATETIME | Event timestamp |
| `actor` | TEXT | Event actor (agent name or `backlogit`) |
| `event_type` | TEXT | Event type string |
| `content` | TEXT | Human-readable event text |
| `delta_json` | TEXT | JSON payload for the event |

**FTS5 virtual table**: `item_log_entries_fts` (columns: `item_id`, `actor`, `event_type`, `content`). Linked to `item_log_entries` via `rowid`.

### `telemetry_sessions`: Agent telemetry sessions

| Column | Type | Notes |
|---|---|---|
| `session_id` | TEXT PK | Unique session identifier |
| (additional columns) | | See telemetry schema |

### `telemetry_tool_usage`: Per-session tool call stats

Tracks tool invocation counts and token usage per session.

## Example Queries

### List all active features

```sql
SELECT id, title, status, priority
FROM items
WHERE artifact_type = 'feature' AND status = 'active'
ORDER BY priority DESC, created_at ASC
```

### List tasks under a specific feature

```sql
SELECT id, title, status, priority
FROM items
WHERE artifact_type = 'task' AND parent_id = '022-F'
ORDER BY id ASC
```

### Get full work queue (queued + active) for agent pickup

```sql
SELECT id, title, artifact_type, status, priority, parent_id, level
FROM items
WHERE status IN ('queued', 'active')
  AND artifact_type IN ('feature', 'task', 'subtask')
ORDER BY level ASC, priority DESC, created_at ASC
```

### Find items without a parent (orphaned tasks, should be investigated)

```sql
SELECT id, title, artifact_type, status
FROM items
WHERE artifact_type IN ('task', 'subtask')
  AND (parent_id IS NULL OR parent_id = '')
ORDER BY artifact_type, id
```

### Check hierarchy integrity (tasks with wrong ID prefix)

```sql
SELECT i.id, i.parent_id, p.id AS expected_parent
FROM items i
JOIN items p ON i.parent_id = p.id
WHERE i.artifact_type = 'task'
  AND i.id NOT LIKE p.id || '.%'
```

### Count items by status

```sql
SELECT status, COUNT(*) AS count
FROM items
WHERE artifact_type IN ('feature', 'task', 'subtask')
GROUP BY status
ORDER BY status
```

### Find all items in a shipment

```sql
SELECT i.id, i.title, i.artifact_type, i.status
FROM items s
JOIN items i ON instr(',' || json_extract(s.custom_fields, '$.items') || ',',
                      ',' || i.id || ',') > 0
WHERE s.id = '005-S'
ORDER BY i.level, i.id
```

### Get dependencies for an item

```sql
SELECT d.depends_on AS blocked_by, i.title, i.status
FROM item_deps d
JOIN items i ON d.depends_on = i.id
WHERE d.item_id = '022.004-T'
```

### Find items that block a given item

```sql
SELECT d.item_id AS blocker, i.title, i.status
FROM item_deps d
JOIN items i ON d.item_id = i.id
WHERE d.depends_on = '022.004-T'
```

### Look up stash entries awaiting harvest

```sql
SELECT stash_id, priority, kind, text, deliberation_id
FROM stash_entries
WHERE state = 'active'
ORDER BY CASE priority
  WHEN 'critical' THEN 1
  WHEN 'high'     THEN 2
  WHEN 'medium'   THEN 3
  WHEN 'low'      THEN 4
  ELSE 5
END
```

### Get stash entries with their harvested artifact IDs

```sql
SELECT se.stash_id, se.kind, se.text, sl.item_id, i.title
FROM stash_entries se
JOIN stash_links sl ON se.stash_id = sl.stash_id
JOIN items i ON sl.item_id = i.id
ORDER BY sl.linked_at DESC
```

### Full-text search across items

```sql
SELECT i.id, i.title, i.artifact_type, i.status
FROM items_fts f
JOIN items i ON f.rowid = i.rowid
WHERE items_fts MATCH 'stash'
ORDER BY rank
LIMIT 20
```

### Get recent log events for an item

```sql
SELECT timestamp, actor, event_type, content, delta_json
FROM item_log_entries
WHERE item_id = '022.004-T'
ORDER BY timestamp DESC
LIMIT 20
```

### Full-text search across log events

```sql
SELECT le.item_id, le.timestamp, le.event_type, le.content
FROM item_log_entries_fts f
JOIN item_log_entries le ON f.rowid = le.rowid
WHERE item_log_entries_fts MATCH 'blocked reason'
ORDER BY le.timestamp DESC
LIMIT 10
```

### Find commits associated with an item

```sql
SELECT commit_sha, message, author
FROM commit_links
WHERE item_id = '022-F'
ORDER BY rowid DESC
```

### List all shipments with item counts

```sql
SELECT s.id, s.title, s.status,
       json_array_length(json_extract(s.custom_fields, '$.items')) AS item_count
FROM items s
WHERE s.artifact_type = 'shipment'
ORDER BY s.created_at DESC
```

Shipment `status` is `queued`, `active`, `shipped`, or `abandoned`; treat only `shipped` and `abandoned` as terminal when validating safe-close.

### List semantic links for an item

```sql
SELECT source_id, target_id, link_type, created_at
FROM item_links
WHERE source_id = '022-F' OR target_id = '022-F'
ORDER BY created_at DESC
```

### Find duplicate-of links across the backlog

```sql
SELECT source_id, target_id, created_at
FROM item_links
WHERE link_type = 'duplicate_of'
ORDER BY created_at DESC
```

### List items with lifecycle-archival status

```sql
SELECT id, title, artifact_type, status, updated_at
FROM items
WHERE status = 'archived'
ORDER BY updated_at DESC
LIMIT 20
```

This matches only `status = 'archived'`. Artifacts routed under `archive/` while still holding a type-specific terminal status (`done`, `accepted`, `rejected`, `shipped`, `abandoned`) are not returned; widen the filter to those values when auditing archive routing.

## Indexes

For query planning awareness:

| Index | Table | Columns |
|---|---|---|
| `idx_items_status` | `items` | `status` |
| `idx_items_type` | `items` | `artifact_type` |
| `idx_items_parent` | `items` | `parent_id` |
| `idx_items_sprint` | `items` | `sprint` |
| `idx_items_hierarchy` | `items` | `hierarchy_path` |
| `idx_item_deps_item` | `item_deps` | `item_id` |
| `idx_item_deps_dep` | `item_deps` | `depends_on` |
| `idx_stash_entries_state` | `stash_entries` | `state` |
| `idx_stash_links_item` | `stash_links` | `item_id` |
| `idx_item_log_entries_item` | `item_log_entries` | `item_id` |
| `idx_item_log_entries_timestamp` | `item_log_entries` | `timestamp` |

## Gate rules

`backlogit_query_sql` only accepts `SELECT` statements. `INSERT`, `UPDATE`, `DELETE`, `DROP`, `CREATE`, `ALTER`, and `ATTACH` are rejected. All results are capped at 500 rows. Use `LIMIT` to constrain large result sets.

## Compatibility

This schema reference covers the backlogit v1.2.0+ index surface. The `telemetry_sessions` and `telemetry_tool_usage` tables are materialized by the `backlogit_telemetry_harvest` operation and may not exist until the first harvest run. All other tables are created on `backlogit_sync_index`.

Generated by autoharness | Template: backlogit-sql-schema.instructions.md.tmpl
