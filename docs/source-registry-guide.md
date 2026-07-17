---
title: "Source Registry Guide"
description: "Multi-file source registry layout, database routing, duplicate-intake detection, and migration from sources.yaml"
created: 2026-05-24
---

## Overview

The source registry tells graphtor-docs which local docline output directories
to ingest, how to filter them, which database to route each local source into,
and which existing databases to expose read-only. You declare sources in YAML
files under `.graphtor/config/`.

Two source types are supported:

* `type: local` — ingests a local directory of docline-emitted Markdown
* `type: database` — serves an existing `.db` file read-only and performs no
  ingestion

Each local source directory must contain docline-emitted standardized Markdown
files with valid v1 frontmatter. Local ingestion is Markdown-only.

Two layouts are supported:

* **Single-file** (legacy) — one `sources.yaml` file.
* **Multi-file** — one or more `*.sources.yaml` files, each owned by a team or
  product domain.

---

## Single-file layout

Place a single file at `.graphtor/config/sources.yaml`:

```yaml
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs
    include: ["**/*.md"]
  - type: local
    id: internal-api
    path: ./out/api-docs
    include: ["**/*.md"]
  - type: database
    id: shared-runbooks
    path: .graphtor/shared-runbooks.db
```

Local sources route to the default database (`graph.db`) when the `database`
field is absent. Database entries do not use `database`; their `path` points to
the read-only database to serve.

---

## Multi-file layout

Split sources across files named `<prefix>.sources.yaml` under
`.graphtor/config/`:

```text
.graphtor/config/
  graph.sources.yaml
  internal.sources.yaml
  powerbi.sources.yaml
```

graphtor-docs discovers all matching files, sorts them alphabetically for
deterministic load order, and merges them into a single registry.

### Required: `database` field for local sources

In multi-file mode, **every `type: local` source must declare an explicit
`database` field**. This prevents ambiguous routing when multiple files
contribute to the merged registry. `type: database` entries are exempt because
they name an existing file to serve and have no ingestion target.

```yaml
# graph.sources.yaml
sources:
  - type: local
    id: graph-docs
    path: ./out/graph-docs
    database: graph.db
    include: ["**/*.md"]
```

```yaml
# internal.sources.yaml
sources:
  - type: local
    id: internal-api
    path: ./out/api-docs
    database: internal.db
    include: ["**/*.md"]
```

Omitting `database` in multi-file mode produces a validation error at
startup for a local source.

> [!NOTE]
> The `database` field value must be a plain filename (e.g. `graph.db`).
> It must not contain path separators or `..` components.

### Read-only database entries

Use `type: database` to expose a pre-built database explicitly:

```yaml
# shared.sources.yaml
sources:
  - type: database
    id: shared-runbooks
    path: .graphtor/shared-runbooks.db
```

The entry has exactly `type`, `id`, and `path`. Do not add `database`,
`include`, `exclude`, `formats`, or `read_only`; unknown fields are rejected.
The `path` must resolve inside `.graphtor/`. A `..` escape, symlink escape, or
Windows junction/reparse-point escape is rejected fail-closed.

Database entries are never passed to `sync`, `prewarm`, or background ingestion.
They are merged into the served database set for read-only consumption.

---

## Multi-database command behaviour

`database` on a local source routes that source to a specific CozoDB `.db` file
(SQLite backend). The value is resolved relative to the parent directory of
`--db-path`, so with the default `--db-path`, `database: product.db` targets
`.graphtor/product.db`.

Command behaviour follows the same routing model:

* `sync` splits local ingestion work by target database
* `prewarm` uses the same per-database local-source routing and ignores
  `type: database` entries
* `serve` assembles the union of configured local target databases, explicit
  `type: database` entries, and existing `.db` files directly under
  `.graphtor/`
* `status` uses the same served database discovery as `serve`

Auto-discovered databases and explicit `type: database` entries remain
read-only unless a real `type: local` source with ingestible docline Markdown
targets the same database and promotes it to generation mode.

---

## Cross-database duplicate-intake detection

When two sources from different databases point at the same local directory
path, graphtor-docs detects the conflict before sync begins.

**Default behaviour (no `--force`):** sync exits with code 2 and prints a
report to stderr:

```text
error: cross-database duplicate intakes detected:
1 cross-database duplicate intake(s) detected:
  intake: ./out/shared-docs
    - source 'shared-a' -> database 'alpha.db'
    - source 'shared-b' -> database 'beta.db'
use --force to proceed anyway
```

**With `--force`:** sync emits a warning and continues:

```text
warning: cross-database duplicate intakes detected (proceeding due to --force):
1 cross-database duplicate intake(s) detected:
  intake: ./out/shared-docs
    ...
```

### Local path normalization

Local paths are compared after lexical normalization so that equivalent paths
written in different forms are treated as the same source:

| Written as | Normalized to |
|---|---|
| `./docs` | `docs` |
| `docs` | `docs` |
| `/abs/path/../docs` | `/abs/docs` |

A source at `./docs` and a source at `docs` pointing to different databases
are flagged as a cross-database duplicate.

### Local source overlap detection

For local sources that share the same normalized root path, graphtor-docs
checks whether the **filtered file intakes overlap** rather than merely
comparing directory roots. Two local sources with the same root directory but
non-overlapping `include`/`exclude` glob filters produce disjoint file sets
and are **not** flagged as conflicting.

For example, if source A includes only `docs/**/*.md` and source B includes
only `api/**/*.md`, their file intakes are disjoint even though they share a
root. No conflict is reported.

Overlap detection requires an accessible workspace root. When the workspace
root is not provided (for example, in non-interactive validation), the system
falls back to conservative path-key comparison.

> [!IMPORTANT]
> **Conservative fallback for unreadable roots:** when any local source root
> does not exist or cannot be enumerated at preflight time, graphtor-docs falls
> back conservatively and flags the pair as a conflict. This avoids a silent
> false negative that would allow duplicate intakes to proceed undetected.

> [!IMPORTANT]
> The `--force` flag is non-interactive. It does not prompt for confirmation.
> Use it in CI pipelines or scripted workflows where you have intentionally
> accepted the duplication risk.

---

## Migrating from `sources.yaml` to `*.sources.yaml`

1. Rename `.graphtor/config/sources.yaml` to a domain-scoped name, for
   example `.graphtor/config/core.sources.yaml`.

2. Add a `database` field to every local source entry.

3. Verify config and duplicate-intake behaviour with a text-only sync:

   ```bash
   graphtor-docs sync --no-embed
   ```

   If duplicate intakes are detected, review the report and either consolidate
   sources or use `--force` to proceed.

4. Remove the old `sources.yaml` file once you have confirmed the new layout
   works.

> [!CAUTION]
> If you rename the file without adding `database` fields, validation fails
> immediately with a descriptive error for local sources. Existing single-file
> users are not affected — `sources.yaml` (without a prefix) is still accepted
> as a legacy fallback.

---

## Exit codes for `sync`

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Partial failures (some files failed; others succeeded) |
| `2` | Fatal error — includes duplicate-intake block without `--force` |

---

## Reference

| Config field | Applies to | Requirement | Description |
|---|---|---|---|
| `type` | all entries | required | Must be `local` or `database` |
| `id` | all entries | required | Unique source identifier; no path separators or `..` |
| `path` | `local` | required | Path to the local docline output directory |
| `path` | `database` | required | Existing database path that resolves inside `.graphtor/` |
| `database` | `local` | required in multi-file mode, optional in `sources.yaml` | Target database filename (e.g. `graph.db`) |
| `include` | `local` | optional | Glob patterns to include |
| `exclude` | `local` | optional | Glob patterns to exclude |
| `formats` | `local` | optional | File extension allow-list (only `md` and `markdown` accepted; default: `["md"]`) |
