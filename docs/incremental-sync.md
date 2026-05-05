---
title: Incremental Sync Design
description: "How graphtor-docs detects changed files and re-ingests only what has changed, including sync state format and change-detection strategies"
---

graphtor-docs avoids re-indexing unchanged documentation on every sync. This
document describes how the incremental sync engine detects changed files and
re-ingests only what has changed.

## Sync State File

Per-source tracking data is persisted in a JSON file:

```text
.graphtor/sync_state.json
```

This path is resolved relative to the **current working directory** when
`graphtor-docs sync` is run (specifically, in the same directory as
`--db-path`, which defaults to `.graphtor/graph.db`).

The file is created on the first successful sync and updated after each
subsequent sync. If the file is missing, the engine treats every file as new
(full ingest).

### File Structure

```json
{
  "sources": {
    "azure-docs": {
      "last_commit": "a3f7b2c9d4e1f0a8b5c2d6e3f7a0b1c4d5e6f7a8",
      "file_mtimes": {},
      "last_sync": "1714857600"
    },
    "team-runbooks": {
      "last_commit": null,
      "file_mtimes": {
        "guides/deploy.md": 1714857600,
        "guides/rollback.md": 1714857601
      },
      "last_sync": "1714857602"
    }
  }
}
```

### `SourceSyncState` Fields

| Field | Type | Used by | Description |
|---|---|---|---|
| `last_commit` | `string \| null` | Git sources | SHA-1 of the last fully processed commit |
| `file_mtimes` | `object` | Local sources | Map of `relative/path` → Unix mtime (seconds since epoch) |
| `last_sync` | `string \| null` | All sources | Unix epoch seconds of the last sync (informational) |

> **Path key format:** Keys in `file_mtimes` always use **forward-slash
> separators** regardless of the host OS. On Windows, backslash paths are
> normalized to forward slashes before storage. This ensures chunk IDs
> (derived from the same path strings) are consistent across platforms.

---

## Change Detection Strategies

### Git Sources

**Strategy:** compare the current HEAD commit SHA-1 against `last_commit`.

On each incremental sync:

1. Open the cloned repository at `.graphtor/data/{source_id}/`
2. Read the current HEAD commit SHA-1
3. If `last_commit` is `null` (first sync), treat all files as new (full ingest)
4. If `last_commit` equals HEAD, no changes — skip this source
5. Otherwise, run `git diff {last_commit}..HEAD --name-status` to enumerate
   changed files
6. Re-ingest `A` (added) and `M` (modified) files through the full
   parse → embed → load pipeline
7. Delete `doc_chunks`, `doc_edges`, `doc_code`, and `doc_vectors` entries for
   `D` (deleted) files
8. Update `last_commit` to the current HEAD SHA-1

This strategy is exact: only files that changed in git history are processed.

> **Note:** Acquisition skips repositories that are already cloned (FR-003
> idempotency). The `sync` command does **not** fetch or pull the remote.
> To pick up new upstream commits, pull the repository manually before running
> `sync`:
> ```sh
> git -C .graphtor/data/{source_id} pull
> graphtor-docs sync
> ```

### Local Sources

**Strategy:** compare current file `mtime` values against the stored mtime map.

On each incremental sync:

1. Walk the source directory and collect current `mtime` for each file
2. Compare against the `file_mtimes` map in the sync state
3. A file is considered **changed** if:
   - Its path is not in `file_mtimes` (new file), or
   - Its current mtime differs from the stored mtime (modified file)
4. A file is considered **deleted** if its path is in `file_mtimes` but no
   longer exists on disk
5. Re-ingest changed files; delete chunks for deleted files
6. Update the `file_mtimes` map with current values

### URL Sources

**Strategy:** always re-crawl (no stable diff signal).

URL sources do not have a reliable change-detection mechanism (HTTP ETags and
`Last-Modified` headers are not universally supported). On each sync, the full
BFS crawl runs within the `max_pages` limit. Previously indexed chunks for
pages that no longer appear in the crawl are **not** automatically deleted —
to remove stale chunks, delete the database and run a fresh sync.

---

## Re-Ingestion

When files are identified as changed, the `reingest` stage:

1. **Deletes** all existing `doc_chunks`, `doc_edges`, `doc_code`, and
   `doc_vectors` rows for the changed file path within the source
2. **Re-runs** the full parse → embed → load pipeline on the new file content
3. Because chunk IDs are deterministic (SHA-256 of content + path), unchanged
   chunks within a changed file are re-inserted with the same ID — CozoDB
   upserts are idempotent

---

## Forced Full Sync

To bypass incremental detection and re-ingest everything from scratch:

```sh
graphtor-docs sync --full
```

Full sync:
1. Runs the complete acquire → parse → embed → load pipeline unconditionally
   for every file in every source
2. Does **not** clear the database first — existing entries are overwritten via
   upsert
3. Does **not** update the sync state file — incremental sync state is
   preserved for the next run

When to use `--full`:
- After a schema change (run `sync --full` to rebuild all entries)
- When the sync state file is corrupted or lost
- When you suspect stale or missing entries from a previous failed sync

---

## Edge Cases

### First-Time Sync

When `sync_state.json` does not exist (first run or manually deleted), the
engine treats every source as having never been synced:

- Git sources: `last_commit` is `null` → full ingest of all files in the branch
- Local sources: `file_mtimes` is empty → full ingest of all files
- URL sources: full crawl (always)

### Deleted Files

Files that existed in a previous sync but no longer exist are detected by:
- Git: `git diff` output with `D` status
- Local: path present in `file_mtimes` but absent from the current directory scan

Deleted files trigger removal of their `doc_chunks`, `doc_edges`, `doc_code`,
and `doc_vectors` rows.

### Source ID Rename

If a source's `id` field is changed in `sources.yaml`, the engine treats the
new ID as a brand-new source (full ingest). The old source's data remains in
the database under the old ID — it is not automatically pruned. To clean up,
run `sync --full` after removing the old source from `sources.yaml`.

### Concurrent Sync Runs

The `sync` command does not currently acquire a workspace lock. Running two
`sync` processes simultaneously against the same workspace is not recommended:
concurrent writes to CozoDB may produce partial or inconsistent state.

If you need to guard against concurrent runs in CI or scheduled scripts, use an
external lock (e.g., `flock` on Linux, a named mutex, or a CI job concurrency
group) rather than relying on `graphtor-docs` itself.

> **Note:** The workspace lock (`.graphtor/graphtor.lock`) is only acquired by
> `install`, `upgrade`, and `uninstall`. It is **not** acquired by `sync`.
