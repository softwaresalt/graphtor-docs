---
title: Troubleshooting Guide
description: "Common graphtor-docs setup and runtime problems — symptoms, root causes, and resolutions"
---

This guide covers the most common problems encountered when setting up and
using graphtor-docs.

## Diagnostic Commands

Run these first when something is wrong:

```sh
# Check workspace health (config, DB, binary, MCP configs)
graphtor-docs doctor

# Show indexed sources and last-sync timestamps
graphtor-docs status

# Run sync with detailed logging
graphtor-docs --verbose sync
```

---

## Common Issues

### `sources.yaml not found`

**Symptom:**
```text
error: sources.yaml not found at .graphtor/config/sources.yaml
```

**Cause:** The `.graphtor/` workspace has not been initialised, or `sync` is
being run from a different directory than where `install` was run.

**Resolution:**
1. Run `graphtor-docs init` to create a starter `sources.yaml`
2. Edit `.graphtor/config/sources.yaml` to add your local docline output directories
3. Run `graphtor-docs sync`

Or use a custom config location:
```sh
graphtor-docs --config /path/to/my-sources.yaml sync
```

---

### Slow first sync / embedding model download

**Symptom:** First `sync` takes many minutes; progress appears to hang.

**Cause:** On first use, the `all-MiniLM-L6-v2` model (~80 MB) is downloaded
from HuggingFace Hub and cached at `~/.cache/huggingface/hub/`. The download
only happens once.

**Resolution:**
- Wait for the download to complete — it is a one-time cost
- If network access is unavailable, run `sync --no-embed` to skip embeddings
  entirely and index text-only; re-run without `--no-embed` later when network
  is available
- To check the model cache location: `echo $HF_HOME` (defaults to
  `~/.cache/huggingface`)

---

### `database unavailable` or WAL lock error

**Symptom:**
```text
error: failed to open database at .graphtor/graph.db
```
Or sync hangs indefinitely accessing the database.

**Cause:** The CozoDB SQLite database may be locked by another process (e.g.,
a previously crashed `sync` or `serve` run left a WAL lock), or the file
permissions are wrong.

**Resolution:**
1. Check for stale SQLite WAL lock files:

   **Linux / macOS:**
   ```sh
   ls .graphtor/graph.db-*    # Look for -shm or -wal files
   ```
   **Windows (PowerShell):**
   ```powershell
   Get-Item .graphtor\graph.db-* -ErrorAction SilentlyContinue
   ```
2. If a previous process crashed and left WAL files, it is safe to delete them
   **when no other process is using the database**:

   **Linux / macOS:**
   ```sh
   # Only if you are certain no graphtor-docs process is running
   rm .graphtor/graph.db-shm .graphtor/graph.db-wal
   ```
   **Windows (PowerShell):**
   ```powershell
   # Only if you are certain no graphtor-docs process is running
   Remove-Item .graphtor\graph.db-shm, .graphtor\graph.db-wal
   ```
3. Check permissions: the user running `graphtor-docs` must have read/write
   access to `.graphtor/`
4. If a stale workspace lock (`graphtor.lock`) is blocking startup:
   ```sh
   rm .graphtor/graphtor.lock   # Linux / macOS
   ```
   ```powershell
   Remove-Item .graphtor\graphtor.lock   # Windows
   ```
   The `--force-unlock` flag on `install`, `upgrade`, and `uninstall`
   auto-removes the lock before those operations run.

---

### Git clone failures

> **Note:** Git source ingestion was removed in the docline pivot. If you are
> seeing git-related errors, you may be using an outdated configuration with
> `type: git` sources. Replace them with `type: local` pointing at docline
> output directories. See the [Configuration Guide](configuration.md).

---

### Sync shows 0 chunks or missing results

**Symptom:** `graphtor-docs status` shows sources but 0 chunks, or search
returns no results for content you know is in the source.

**Cause 1 — Glob or format filter:** The `include`/`exclude` glob patterns or
`formats` are filtering out all files.

**Cause 2 — Invalid frontmatter:** Files have missing or malformed docline v1
frontmatter and were rejected during validation.

**Resolution:**
1. Check `sources.yaml` — verify `include` globs match your file extensions:
   ```yaml
   include:
     - "**/*.md"   # not "*.md" (would only match root-level files)
   ```
2. Run with `--verbose` to see per-file processing decisions:
   ```sh
   graphtor-docs --verbose sync
   ```
3. Check that the source directory actually contains `.md` files:
   ```sh
   ls ./out/your-source-id/
   ```
4. Verify that each `.md` file has a valid docline v1 frontmatter block with
   the required fields: `title`, `source`, `ingested_at`, `doc_type`, and
   `source_path`. Files that fail frontmatter validation are skipped and
   reported in sync output.

---

### `search_semantic` returns no results or "model not loaded" error

**Symptom:** `search_semantic` returns an error like "semantic search is
disabled: the embedding model is not loaded".

**Cause 1 — Model unavailable:** The embedding model could not be loaded at
server startup (network unavailable, HuggingFace Hub unreachable, or disk
space insufficient for the model cache).

**Resolution:** Run `graphtor-docs serve` with network access so the model can
be downloaded and cached. After the first successful startup with the model,
subsequent starts use the local cache.

**Cause 2 — Sync ran with `--no-embed`:** If `graphtor-docs sync --no-embed`
was used, no embeddings were stored in `doc_chunks.embedding`. Even with the
model loaded, `search_semantic` will return empty results.

**Resolution:** Re-run sync without the flag to populate embeddings:
```sh
graphtor-docs sync --full
```

**Cause 3 — Empty embeddings:** First sync only partially completed before
being interrupted.

**Resolution:** Run `graphtor-docs sync --full` to force a complete rebuild.

**Fallback:** Use `search_local_docs` for keyword-based search, which does not
require the embedding model.

---

### Windows: search returns no results for local sources

**Symptom:** `search_local_docs` or `get_document` returns no results for
local source files, even though the sync reported success.

**Cause:** On Windows, file paths produced by `Path::to_string_lossy()` use
backslash separators. If the source was indexed with a build older than the
Windows path normalization fix (PR #13), chunk IDs and stored paths may use
backslashes while the MCP tools generate forward-slash paths — causing mismatches.

**Resolution:**
1. Check your binary version: `graphtor-docs --version`
2. If using a build from before the fix, rebuild from source (version ≥ the
   commit in PR #13)
3. After upgrading, force a full re-index to regenerate all chunk IDs with
   normalized paths:
   ```sh
   graphtor-docs sync --full
   ```

---

### `serve` exits immediately or fails to start

**Symptom:** `graphtor-docs serve` starts and exits within seconds with no
output.

**Cause:** The database cannot be opened (missing `.graphtor/` workspace, bad
`--db-path`, or permission error), or `sources.yaml` is invalid.

**Resolution:**
1. Run `graphtor-docs doctor` to identify the specific failure
2. Ensure `graphtor-docs sync` has been run at least once before `serve`
3. Check that `--db-path` (if specified) points to a valid writable location
4. Check `sources.yaml` syntax: `graphtor-docs doctor` validates it

---

### `doctor` reports schema mismatch

**Symptom:**
```text
[✗] database: schema version mismatch (expected 4, found 1)
```

**Cause:** The database was created by an older version of graphtor-docs.
Upgrading from v1–v3 triggers an automatic data migration; upgrading from
a pre-pivot build to the current version (v4) prunes pre-pivot ingested data
so the docline pipeline can re-ingest from scratch.

**Resolution:** If the automatic migration did not run (or you are starting
fresh), delete the database and re-run sync to rebuild it:
```sh
del .graphtor\graph.db        # Windows
rm .graphtor/graph.db         # Linux/macOS
graphtor-docs sync --full
```

This deletes all indexed content. Re-indexing will re-run parse and embed
against your configured local source directories.

---

## Getting More Information

- Run `graphtor-docs --verbose <subcommand>` for debug-level logging
- Check `graphtor-docs doctor` for workspace health
- See the [Pipeline Reference](pipeline.md) for how data flows through the system
- See the [Incremental Sync Design](incremental-sync.md) for sync state details
- See the [CLI Reference](cli-reference/graphtor-docs.md) for all flags
