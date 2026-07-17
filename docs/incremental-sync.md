---
title: Incremental Sync Design
description: "Docline-Markdown mtime sync, re-ingestion, deletion handling, and per-database sync state"
---

graphtor-docs avoids re-indexing unchanged documentation on every sync. The
incremental engine is local and docline-Markdown only: it scans configured
`type: local` directories, tracks Markdown files by modification time, and
re-ingests only files whose tracked state changed. It does not contact remote
sources, update repositories, or ingest non-Markdown document formats.

## Scope

Incremental sync applies only to ingestible local sources:

* `type: local` sources are scanned in-place and filtered by `include`,
  `exclude`, and `formats`
* `type: database` sources are served read-only and never passed to sync
* Tracked files must be Markdown and must pass the docline v1 frontmatter
  contract before they are loaded

The parser accepts Markdown through the normalized `md` format path. Other
extensions are ignored by the incremental tracker or rejected by configuration
validation before sync starts.

## Sync State File

Sync state is stored next to the database it describes. With the default
database, the state file is:

```text
.graphtor/graph.sync_state.json
```

When a source routes to a different database, the database name determines the
state file:

```text
.graphtor/notes.db          → .graphtor/notes.sync_state.json
.graphtor/reference.db      → .graphtor/reference.sync_state.json
```

For backward compatibility, an existing legacy `.graphtor/sync_state.json` file
is reused. Otherwise graphtor-docs uses the per-database `*.sync_state.json`
path. If the state file is missing, every tracked Markdown file is treated as
new.

### File Structure

```json
{
  "sources": {
    "team-runbooks": {
      "file_mtimes": {
        "guides/deploy.md": 1714857600,
        "guides/rollback.md": 1714857601
      },
      "file_contract_paths": {
        "guides/deploy.md": "runbooks/deploy.md",
        "guides/rollback.md": "runbooks/rollback.md"
      },
      "last_sync": "1714857602",
      "contract_epoch": "docline-v1"
    }
  }
}
```

### `SourceSyncState` Fields

| Field | Type | Description |
|---|---|---|
| `file_mtimes` | `object` | Map of source-root-relative filesystem path to Unix mtime seconds |
| `file_contract_paths` | `object` | Map of filesystem path to the validated docline `source_path` from the last successful ingest |
| `last_sync` | `string \| null` | Unix epoch seconds when the source state was last written |
| `contract_epoch` | `string \| null` | Ingestion contract epoch; current value is `docline-v1` |

Path keys use forward slashes on every platform, including Windows. The same
normalization is used for chunk identity and deletion tracking.

## Change Detection

For each local source, sync builds the current tracked file set:

1. Recursively scan the source directory without following symlinks
2. Apply the source's `include` and `exclude` glob filters
3. Keep only Markdown paths allowed by the source `formats` list
4. Record each remaining file's mtime in Unix epoch seconds

The current map is compared with `file_mtimes` from sync state:

| Classification | Rule |
|---|---|
| Added | Path exists now but is absent from stored state |
| Modified | Path exists in both maps and the current mtime is newer |
| Deleted | Path exists in stored state but no longer exists in the current tracked set |
| Unchanged | Path exists in both maps and the mtime did not advance |

When `contract_epoch` is missing or differs from the current epoch, the source
forces a full re-ingest through the incremental path. Existing stored keys are
kept for deletion detection, but their mtimes are treated as stale so every live
tracked Markdown file is reprocessed. A pending v4 database migration uses the
same forced-reingest behavior.

## Re-Ingestion

Added and modified files are processed one file at a time:

1. Validate the file path against the workspace boundary
2. Parse the file with the docline v1 contract-enforced Markdown parser
3. Use the validated frontmatter `source_path` as the document's canonical
   identity
4. Delete stale records for the old contract path, scoped to the source
5. Load the new chunks, link edges, code snippets, and derived lookup entries
6. Compute and store embeddings when the embedding model is available
7. Persist the new mtime and contract-path mapping after successful processing

If a file's frontmatter `source_path` changed, the old
`file_contract_paths` entry lets sync delete rows under the previous identity
before inserting the new document. This prevents orphaned rows when a file moves
or when docline changes the canonical `source_path`.

When embeddings are disabled for an ordinary incremental run, unchanged chunk
IDs keep any previously stored embedding. New or content-changed chunks remain
without embeddings until a later sync runs with the model available. Because
incremental sync skips files whose mtime has not advanced, run `sync --full`
(or modify the affected files) to backfill embeddings for chunks that were
indexed while the model was unavailable.

Per-file failures are non-fatal. Failed modified files keep their previous mtime
in state, and failed new files are omitted from state, so the next sync retries
them.

## Deletion Handling

Deleted files are cleaned up by source-scoped document identity:

1. Read the old contract path from `file_contract_paths`
2. Fall back to the filesystem-relative path for legacy state without contract
   mappings
3. Delete matching `doc_chunks` rows for that source and path
4. Remove dependent edge, code, and derived lookup records for the deleted chunks
5. Remove the mtime and contract-path state entries after successful cleanup

Embeddings are stored inline on `doc_chunks`, so deleting chunk rows removes the
corresponding vectors as part of the same cleanup. If deletion cleanup fails,
the old state entry is preserved so the next sync retries the delete.

## Full Sync

`graphtor-docs sync --full` bypasses mtime change detection and runs the full
Acquire → Validate → Parse → Embed → Load pipeline for every configured local
source. It still uses the same docline-Markdown parser and does not process
read-only database sources or unsupported formats.

After a successful full sync, graphtor-docs seeds the per-database sync state
from a pre-pipeline snapshot of the live source tree. That snapshot prevents a
file changed during the full-sync window from being recorded as already synced.
Prior-state entries for files that disappeared, or for documents whose
`source_path` changed, are carried forward so the next incremental sync can
clean up stale rows left behind by upsert-only full loading.

## Edge Cases

### First-Time Sync

When no sync state exists for a source, every tracked Markdown file is treated
as added. Files are validated, parsed, embedded when possible, and loaded into
the target database.

### Contract Validation Failure

A Markdown file with missing or malformed docline v1 frontmatter fails during
parse. The file is reported as an error, excluded from the database update, and
left pending for the next sync.

### Duplicate `source_path`

Before re-ingesting a batch of changed files, sync checks for duplicate
validated `source_path` values within the source and against unchanged stored
documents. Conflicting files fail closed and are retried only after the
frontmatter conflict is corrected.

### Source ID Rename

Changing a source's `id` creates a new source namespace. Existing rows under the
old source ID are not automatically removed because they no longer belong to a
tracked source state entry. Remove or rebuild the old database when retiring a
source ID.
