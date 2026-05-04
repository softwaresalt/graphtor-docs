---
title: graphtor-docs CLI Reference
description: "Complete reference for all graphtor-docs subcommands, global flags, and exit codes"
---

```
graphtor-docs [GLOBAL FLAGS] <SUBCOMMAND> [SUBCOMMAND FLAGS]
```

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success — all operations completed without error |
| `1` | Partial failure — some files failed; others succeeded |
| `2` | Fatal error — pipeline stage failed, database unavailable, config invalid, etc. |

## Global Flags

These flags are accepted by every subcommand.

| Flag | Short | Env var | Default | Description |
|---|---|---|---|---|
| `--verbose` | `-v` | — | off | Enable verbose logging (sets `RUST_LOG=debug`) |
| `--config <FILE>` | `-c` | `GRAPHTOR_SOURCES` | `.graphtor/config/sources.yaml` | Path to `sources.yaml` |
| `--db-path <FILE>` | `-d` | `GRAPHTOR_DB_PATH` | `.graphtor/graph.db` | Path to CozoDB database |

> **Note:** `--data-dir` is a deprecated alias for `--db-path`. It still works
> but prints a warning. Use `--db-path` in new invocations.

---

## Subcommands

### `sync`

Run the ingestion pipeline.

```
graphtor-docs sync [FLAGS]
```

By default, detects changes since the last sync (via git diff or file mtime)
and re-processes only added, modified, and deleted files. Use `--full` to
force a complete acquire → parse → embed → load cycle.

| Flag | Default | Description |
|---|---|---|
| `--full` | off | Force full re-ingestion of all files; ignores sync state |
| `--no-embed` | off | Skip the embedding step (faster; no vectors stored; `search_semantic` unavailable) |
| `--batch-size <N>` | `20` | Number of files to process per parse/embed/load cycle |
| `--data-root <DIR>` | `.graphtor/data` | Root directory for acquired source files |

**Exit codes:**
- `0` — all sources synced without error
- `1` — one or more files failed to process; successfully processed files are stored
- `2` — fatal error (config not found, database unavailable)

**Example:**

```sh
# Incremental sync (default)
graphtor-docs sync

# Full re-ingest without embeddings (e.g., after schema change)
graphtor-docs sync --full --no-embed

# Verbose incremental sync
graphtor-docs --verbose sync
```

---

### `serve`

Start the MCP STDIO server.

```
graphtor-docs serve
```

Binds to STDIO (localhost only) and serves the 7 graphtor-docs MCP tools.
Blocks until stdin closes (i.e., until the MCP client disconnects).

On startup:
1. Opens the CozoDB database
2. Attempts to load the `all-MiniLM-L6-v2` embedding model
   - If the model loads successfully, `search_semantic` is enabled
   - If the model is unavailable, `search_semantic` returns a descriptive
     error; all other tools continue to work normally
3. Starts the JSON-RPC STDIO server

No additional flags. Configure the database path and sources file via the
global `--db-path` and `--config` flags if needed.

**Example:**

```sh
# Start the MCP server (normal use)
graphtor-docs serve

# Start with a custom database location
graphtor-docs --db-path /data/myproject.db serve
```

---

### `status`

Print database statistics.

```
graphtor-docs status [FLAGS]
```

Reports registered sources, their kind (git/local), URL, and last-sync
timestamp. If the database does not exist, prints a helpful message and exits
with code `0`.

| Flag | Default | Description |
|---|---|---|
| `--json` | off | Output as JSON instead of human-readable text |

**Example output (text):**

```
database: .graphtor/graph.db
sources:  2
  [git]   azure-docs — https://github.com/MicrosoftDocs/azure-docs.git (last sync: 2026-05-01T10:30:00Z)
  [local] team-runbooks — ./runbooks (last sync: 2026-05-02T08:00:00Z)
```

**Example output (`--json`):**

```json
{
  "database": ".graphtor/graph.db",
  "sources": [
    {
      "id": "azure-docs",
      "name": "azure-docs",
      "kind": "git",
      "url": "https://github.com/MicrosoftDocs/azure-docs.git",
      "synced_at": "2026-05-01T10:30:00Z"
    }
  ]
}
```

---

### `init`

Generate a template `sources.yaml`.

```
graphtor-docs init [FLAGS]
```

Creates `.graphtor/config/sources.yaml` with commented examples for Git and
local sources. Does **not** overwrite an existing file unless `--force` is
passed.

| Flag | Default | Description |
|---|---|---|
| `--force` | off | Overwrite an existing `sources.yaml` |

After running `init`, edit the generated file to add your documentation
sources, then run `graphtor-docs sync`.

---

### `install`

Install graphtor-docs into the current workspace.

```
graphtor-docs install [FLAGS]
```

Creates the `.graphtor/` workspace directory scaffold:

```
.graphtor/
  bin/        ← installed binary copy
  data/       ← acquired source files (git clones and url crawl cache)
  cache/      ← model cache (HuggingFace)
  config/     ← sources.yaml
  logs/       ← transient log files
  graph.db    ← CozoDB SQLite database
  sync_state.json ← incremental sync tracking (git SHA-1 + file mtimes)
```

Also:
- Copies the currently running binary to `.graphtor/bin/graphtor-docs`
- Runs `init` to generate a starter `sources.yaml` (non-destructive)
- Adds `.graphtor/` to `.gitignore` (unless `--no-gitignore`)
- Generates MCP client config files for configured editors

| Flag | Default | Description |
|---|---|---|
| `--no-gitignore` | off | Skip updating `.gitignore` |
| `--editor <EDITOR>` | (all) | Comma-separated editor names for MCP config generation (`vscode`, `cursor`, `copilot`) |
| `--force-unlock` | off | Force-release the workspace lock before installing (use when a stale lock exists) |

---

### `doctor`

Diagnose workspace health.

```
graphtor-docs doctor
```

Runs a series of health checks and prints a pass/warn/fail report:

| Check | What it validates |
|---|---|
| Binary version | Current binary is runnable |
| Database | `.graphtor/graph.db` exists and is accessible |
| `sources.yaml` | Config file exists and parses without error |
| MCP client configs | Config files exist and are valid JSON |
| Disk usage | `.graphtor/` total disk usage |

Output format:
```
[✓] binary: graphtor-docs v0.1.0
[✓] database: .graphtor/graph.db (accessible)
[!] sources.yaml: duplicate source ID "azure-docs"
[✗] embedding model: not found
```

Exit codes: `0` if all checks pass or warn; `2` if any check fails.

---

### `upgrade`

Upgrade the installed graphtor-docs binary.

```
graphtor-docs upgrade [FLAGS]
```

Replaces `.graphtor/bin/graphtor-docs` with the currently running binary.
Preserves all configuration and data.

| Flag | Default | Description |
|---|---|---|
| `--force` | off | Replace even if the installed binary appears up-to-date |
| `--force-unlock` | off | Force-release the workspace lock before upgrading |

---

### `uninstall`

Remove graphtor-docs from the current workspace.

```
graphtor-docs uninstall --confirm [FLAGS]
```

Removes `.graphtor/` and generated MCP client config files. Also cleans
`.gitignore` entries added by `install`.

`--confirm` is **required** to prevent accidental data deletion.

| Flag | Default | Description |
|---|---|---|
| `--confirm` | (required) | Required confirmation flag |
| `--keep-config` | off | Keep `sources.yaml` and workspace config; only remove runtime data |
| `--force-unlock` | off | Force-release the workspace lock before uninstalling |
