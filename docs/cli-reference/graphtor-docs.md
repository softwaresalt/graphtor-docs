---
title: graphtor-docs CLI Reference
description: "Complete reference for all graphtor-docs subcommands, global flags, and exit codes"
---

```text
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
| `--verbose` | `-v` | — | off | Enable debug-level logging |
| `--json` | — | — | off | Wrap all output in JSON-RPC 2.0 envelopes (`{"jsonrpc":"2.0","id":null,"result":{...}}`) |
| `--config <FILE>` | `-c` | `GRAPHTOR_SOURCES` | `.graphtor/config/sources.yaml` | Path to `sources.yaml` |
| `--db-path <FILE>` | `-d` | `GRAPHTOR_DB_PATH` | `.graphtor/graph.db` | Path to the primary CozoDB database |

> **Note:** `--data-dir` is a deprecated alias for `--db-path`. It still works
> but prints a warning. Use `--db-path` in new invocations.
>
> Sources can override the primary database target with the `database` field in
> `sources.yaml`. Those sources sync into sibling `.db` files under the same
> parent directory as `--db-path`.

---

## Subcommands

### `sync`

Run the ingestion pipeline.

```text
graphtor-docs sync [FLAGS]
```

By default, detects changes since the last sync (via file mtime) and
re-processes only added, modified, and deleted files. Use `--full` to
force a complete acquire → validate → parse → embed → load cycle.

| Flag | Default | Description |
|---|---|---|
| `--full` | off | Force full re-ingestion of all files; ignores sync state |
| `--no-embed` | off | Skip the embedding step (faster; no vectors stored; `search_semantic` returns empty results) |
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

```text
graphtor-docs serve [--read-only]
```

Binds to STDIO (localhost only) and serves the 8 graphtor-docs MCP tools.
Blocks until stdin closes (i.e., until the MCP client disconnects).

`serve` auto-discovers the databases to open as the union of: any
`sources.yaml`-resolved target (including an explicit `--db-path`), any
explicit `type: database` entry (see
[Configuration Guide](../configuration.md#source-type-database-type-database)),
and any `.db` file dropped directly inside `.graphtor/` — no configuration is
required to serve a dropped database. See
[Consumption-first serve: auto-discovery, posture, and the operator trust boundary](../design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md)
for the full discovery, posture, and trust-boundary design.

Each discovered database is classified **independently**, purely from
content:

* A database with a real, resolvable `local` source targeting it (an
  existing, non-empty directory matching that source's filters) is opened
  read-write and kept in sync by the normal background sync task.
* Every other database — no `sources.yaml`, an empty/stale registry, or a
  dropped file with no source targeting it — is opened through the
  engine-enforced **read-only** primitive and is never background-synced.
  This is the fail-safe default on any ambiguity.

On startup:

1. Resolves the served database set and each database's posture (above)
2. Opens each database according to its resolved posture
3. Attempts to load the `all-MiniLM-L6-v2` embedding model
   - If the model loads successfully, `search_semantic` is enabled
   - If the model is unavailable, `search_semantic` returns a descriptive
     error; all other tools continue to work normally
4. Starts the JSON-RPC STDIO server

**Flags:**

| Flag | Description |
|---|---|
| `--read-only` | Force every discovered database to read-only posture, even one with a real, resolvable source. There is no corresponding "force read-write" flag — read-only is already the default posture on any ambiguity. |

Configure the database path and sources file via the global `--db-path` and
`--config` flags if needed.

**Example:**

```sh
# Start the MCP server (auto-discovers .graphtor/*.db with zero config)
graphtor-docs serve

# Start with a custom database location
graphtor-docs --db-path /data/myproject.db serve

# Force read-only even in a workspace with real sources
graphtor-docs serve --read-only
```

---

### `status`

Print database statistics.

```text
graphtor-docs status [FLAGS]
```

Reports registered sources, their kind (`local`), path, and last-sync
timestamp. If the database does not exist, prints a helpful message and exits
with code `0`.

`status` shares the same database auto-discovery `serve` uses (see the
`serve` section above): a `.db` file dropped directly into `.graphtor/` with
no `sources.yaml` at all is reported here too, with source metadata
synthesized from that database's own stored records.

| Flag | Default | Description |
|---|---|---|
| `--json` | off | Output as JSON instead of human-readable text |

**Example output (text):**

```text
database: .graphtor/graph.db
sources:  2
  [local] product-docs — ./out/product-docs (last sync: never)
  [local] team-runbooks — ./out/runbooks (last sync: never)
```

> **Note:** `last sync` shows `never` until a future release populates `synced_at`
> in the pipeline. Use the matching `*.sync_state.json` file to inspect
> `last_sync` (Unix epoch) for each source in the meantime.

**Example output (`--json`):**

```json
{
  "jsonrpc": "2.0",
  "id": null,
  "result": {
    "database": ".graphtor/graph.db",
    "sources": [
      {
        "id": "product-docs",
        "name": "product-docs",
        "kind": "local",
        "url": "./out/product-docs",
        "synced_at": null
      }
    ]
  }
}
```

If configured sources use the `database` field, human-readable output prints
one section per database file and JSON output returns a `databases` array
instead of a single `database` field.

---

### `init`

Generate a template `sources.yaml`.

```text
graphtor-docs init [FLAGS]
```

Creates `.graphtor/config/sources.yaml` with commented examples for local
docline sources. Does **not** overwrite an existing file unless `--force` is
passed.

| Flag | Default | Description |
|---|---|---|
| `--force` | off | Overwrite an existing `sources.yaml` |

After running `init`, edit the generated file to add your local docline output
directories, then run `graphtor-docs sync`.

---

### `install`

Install graphtor-docs into the current workspace.

```text
graphtor-docs install [FLAGS]
```

By **default**, `install` creates a consumption-first minimal footprint:

```text
.graphtor/    ← workspace root; drop an already-generated `.db` file here
              for `serve`/`status` to auto-discover with zero configuration
```

That is the ENTIRE default footprint: no `bin/`, `data/`, `cache/`,
`config/`, or `logs/` subdirectories, no copied binary, and no
`sources.yaml`. `install` also:

- Generates a workspace-root `.mcp.json` MCP client config registering a
  bare `graphtor-docs` PATH command (non-destructive)
- Does **not** touch `.gitignore` (there is nothing generated to ignore)

Pass `--with-ingestion` to instead create the full, ingestion-capable
scaffold — see [Ingestion setup](#ingestion-setup) below.

| Flag | Default | Description |
|---|---|---|
| `--with-ingestion` | off | Create the full scaffold (see [Ingestion setup](#ingestion-setup)) instead of the consumption-first minimal default |
| `--no-gitignore` | off | With `--with-ingestion`, skip updating `.gitignore`; has no effect on the default minimal install |
| `--force-unlock` | off | Force-release the workspace lock before installing (use when a stale lock exists) |

---

### Ingestion setup

Use `install --with-ingestion` when you want THIS workspace to ingest and
generate its own documentation index, rather than only serving an
already-generated `.db` file dropped in from elsewhere.

```text
graphtor-docs install --with-ingestion
```

This creates the full workspace directory scaffold:

```text
.graphtor/
  bin/        ← installed binary copy
  cache/      ← directory scaffold only; HuggingFace model cache is at ~/.cache/huggingface/hub/
  config/     ← sources.yaml (local docline source registrations)
  logs/       ← transient log files
  graph.db    ← primary CozoDB SQLite database
  *.db        ← optional routed database files from `sources.yaml`
  graph.sync_state.json ← incremental sync tracking for graph.db
  *.sync_state.json     ← incremental sync tracking for routed databases
```

When a source sets `database`, graphtor-docs stores that source in the named
`.db` file instead of the primary `graph.db` target.

`install --with-ingestion` also:

- Copies the currently running binary to `.graphtor/bin/graphtor-docs`
- Runs `init` to generate a starter `sources.yaml` (non-destructive)
- Adds `.graphtor/` to `.gitignore` (unless `--no-gitignore`)
- Generates a workspace-root `.mcp.json` MCP client config pinned to the
  copied binary's absolute path (non-destructive)

**Full ingestion setup workflow:**

1. `graphtor-docs install --with-ingestion` — create the scaffold above
2. Edit `.graphtor/config/sources.yaml` to add your local docline output
   directories — see the [Configuration Guide](../configuration.md) for the
   full `sources.yaml` reference
3. `graphtor-docs sync` — ingest the configured sources and generate the
   `.db` file(s)
4. `graphtor-docs serve` or configure your MCP client — the generated
   database(s) resolve to `Generation` posture and stay incrementally synced
   in the background (see
   [Consumption-first serve: auto-discovery, posture, and the operator trust boundary](../design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md#the-devauthoring-workspace-generation-exception))

**Downstream read-only consumption:** once a `.db` file has been generated by
the workflow above, you can copy it into any OTHER workspace's `.graphtor/`
directory — that downstream workspace needs only the consumption-first
minimal install (the default, no `--with-ingestion`) for `serve` to
auto-discover and serve it read-only, with no ingestion setup required there
at all.

---

### `doctor`

Diagnose workspace health.

```text
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

`doctor` is layout-aware: on a consumption-first minimal install (no
`--with-ingestion`), the missing `bin/`/subdirs, `sources.yaml`, and
`graph.db` checks are informational (not warn/fail), since none of those are
expected to exist there. A full (`--with-ingestion`) install is checked
exactly as before.

Output format:
```text
[✓] binary: graphtor-docs v0.1.0
[✓] database: .graphtor/graph.db (accessible)
[!] sources.yaml: duplicate source ID "azure-docs"
[✗] embedding model: not found
```

Exit codes: `0` if all checks pass or warn; `2` if any check fails.

---

### `upgrade`

Upgrade the installed graphtor-docs binary.

```text
graphtor-docs upgrade [FLAGS]
```

Replaces `.graphtor/bin/graphtor-docs` with the currently running binary.
Preserves all configuration and data. On a consumption-first minimal install
(no `bin/` scaffold), `upgrade` is a safe no-op — there is no managed binary
to replace, and `upgrade` never creates one as a side effect.

| Flag | Default | Description |
|---|---|---|
| `--force` | off | Replace even if the installed binary appears up-to-date |
| `--force-unlock` | off | Force-release the workspace lock before upgrading |

---

### `uninstall`

Remove graphtor-docs from the current workspace.

```text
graphtor-docs uninstall --confirm [FLAGS]
```

Removes only the graphtor-created subdirectories (`bin/`, `data/`, `cache/`,
`config/` unless `--keep-config`, `logs/`) — never an arbitrary top-level
`.graphtor/` entry. A user-dropped `.db` file living directly in `.graphtor/`
(the read-only serve auto-discovery drop location) always survives, and
`.graphtor/` itself is removed only if it ends up completely empty
afterward. Also removes generated MCP client config entries, and cleans the
managed `.gitignore` block ONLY for a full (`--with-ingestion`) install — a
minimal install never wrote one. Before deleting anything, `uninstall`
prints the exact set of directories it is about to remove.

`--confirm` is **required** to prevent accidental data deletion.

| Flag | Default | Description |
|---|---|---|
| `--confirm` | (required) | Required confirmation flag |
| `--keep-config` | off | Keep `sources.yaml` and workspace config; only remove runtime data |
| `--force-unlock` | off | Force-release the workspace lock before uninstalling |

---

### `manifest`

Print a manifest of available MCP tools.

```text
graphtor-docs manifest [--json]
```

Without `--json`, prints a human-readable table of tool names and descriptions.
With `--json` (global flag), emits a `tools/list`-compatible JSON-RPC 2.0
response envelope with the same tool definitions as the MCP server. Note that
the tool list is sorted alphabetically for deterministic output; ordering may
differ from the live server's `tools/list` response.

Tool definitions are derived from the same source as the MCP server,
guaranteeing parity of tool names, descriptions, and parameter schemas.

No additional subcommand flags. Use the global `--json` flag for machine-readable output.

**Example (human-readable):**

```sh
graphtor-docs manifest
```

```text
Tool                  Description
----                  -----------
search_local_docs     Full-text keyword search over indexed documentation chunks.
search_semantic       Semantic similarity search using embeddings.
research_topic        In-depth topic research combining keyword search and graph traversal.
traverse_doc_links    BFS graph traversal following document link relationships.
list_sources          List all registered documentation sources.
get_chunk_by_id       Retrieve a single documentation chunk by its SHA-256 chunk ID.
get_document          Retrieve all chunks for a document path, in reading order.
get_status            Return current database status and sync state.
```

**Example (JSON-RPC 2.0 envelope):**

```sh
graphtor-docs --json manifest
```

```json
{
  "jsonrpc": "2.0",
  "id": null,
  "result": {
    "tools": [
      { "name": "search_local_docs", "description": "...", "inputSchema": { ... } }
    ]
  }
}
```
