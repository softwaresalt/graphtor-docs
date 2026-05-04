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
```
error: sources.yaml not found at .graphtor/config/sources.yaml;
       run `graphtor-docs init` first
```

**Cause:** The `.graphtor/` workspace has not been initialised, or `sync` is
being run from a different directory than where `install` was run.

**Resolution:**
1. Run `graphtor-docs init` to create a starter `sources.yaml`
2. Edit `.graphtor/config/sources.yaml` to add your documentation sources
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

### PDF extraction failures or poor quality

**Symptom:** PDF files are skipped, produce garbled text, or timeout on large
files.

**Cause:** The pure-Rust `pdf-extract` backend has known limitations with
complex PDFs. For files ≥ 20 MiB, graphtor-docs routes through PDFium if
available.

**Resolution — install PDFium:**
1. Download the pre-built PDFium shared library from
   [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
   for your platform
2. Place it in the same directory as the `graphtor-docs` binary, **or** set
   the environment variable:
   ```sh
   export GRAPHTOR_PDFIUM_PATH=/path/to/libpdfium.so  # Linux
   set GRAPHTOR_PDFIUM_PATH=C:\libs\pdfium.dll         # Windows
   ```
3. Re-run sync: `graphtor-docs sync --full`

Without PDFium, large PDFs fall back to `pdf-extract` — slower and lower
quality but functional.

---

### `database unavailable` or WAL lock error

**Symptom:**
```
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

**Symptom:**
```
acquisition had failures; affected sources may be skipped
```
Or authentication errors during clone.

**Cause:** Network access issues, SSH key not configured, HTTPS credentials
not available, or firewall/proxy blocking git traffic.

**Resolution:**
- **HTTPS sources:** ensure git credential helper is configured, or use a
  personal access token embedded in the URL:
  ```sh
  https://token@github.com/org/repo.git
  ```
- **SSH sources:** ensure `ssh-agent` is running and has your key loaded:
  ```sh
  eval $(ssh-agent)
  ssh-add ~/.ssh/id_ed25519
  ```
- **Proxy:** set `https_proxy` / `http_proxy` environment variables before
  running `sync`
- **Firewall:** ensure outbound port 443 (HTTPS) or 22 (SSH) is allowed

---

### Sync shows 0 chunks or missing results

**Symptom:** `graphtor-docs status` shows sources but 0 chunks, or search
returns no results for content you know is in the source.

**Cause:** The `formats` or `include`/`exclude` glob patterns are filtering out
all files, or the source directory is empty.

**Resolution:**
1. Check `sources.yaml` — verify `include` globs match your file extensions:
   ```yaml
   include:
     - "**/*.md"   # not "*.md" (would only match root-level files)
   ```
2. Check `formats` — ensure the file extensions you want are listed (or omit
   `formats` entirely to allow all supported types)
3. Run with `--verbose` to see per-file processing decisions:
   ```sh
   graphtor-docs --verbose sync
   ```
4. Check that the source directory actually contains files:
   ```sh
   ls .graphtor/data/{source-id}/
   ```

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
was used, no vectors were stored in `doc_vectors`. Even with the model loaded,
`search_semantic` will return empty results.

**Resolution:** Re-run sync without the flag to populate vectors:
```sh
graphtor-docs sync --full
```

**Cause 3 — Empty `doc_vectors`:** First sync only partially completed before
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
```
[✗] database: schema version mismatch (expected 2, found 1)
```

**Cause:** The database was created by an older version of graphtor-docs with
a different schema. Currently there is no automatic migration path.

**Resolution:** Delete the database and re-run sync to rebuild it:
```sh
del .graphtor\graph.db        # Windows
rm .graphtor/graph.db         # Linux/macOS
graphtor-docs sync --full
```

This deletes all indexed content. Re-indexing will re-download nothing (source
files are already in `.graphtor/data/`) but will re-run parse and embed.

---

## Getting More Information

- Run `graphtor-docs --verbose <subcommand>` for debug-level logging
- Check `graphtor-docs doctor` for workspace health
- See the [Pipeline Reference](pipeline.md) for how data flows through the system
- See the [Incremental Sync Design](incremental-sync.md) for sync state details
- See the [CLI Reference](cli-reference/graphtor-docs.md) for all flags
