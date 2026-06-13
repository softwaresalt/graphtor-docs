---
title: "Source Registry Guide"
description: "Multi-file source registry layout, database routing, duplicate-intake detection, and migration from sources.yaml"
created: 2026-05-24
---

## Overview

The source registry tells graphtor-docs which local docline output directories
to index, how to filter them, and which database to route each source into. You
declare sources in YAML files under `.graphtor/config/`.

Only `type: local` sources are supported. Each source directory must contain
docline-emitted standardized Markdown files with valid v1 frontmatter.

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
```

All sources route to the default database (`graph.db`) when the `database`
field is absent.

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

### Required: `database` field

In multi-file mode, **every source must declare an explicit `database`
field**. This prevents ambiguous routing when multiple files contribute to
the merged registry.

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
startup.

> [!NOTE]
> The `database` field value must be a plain filename (e.g. `graph.db`).
> It must not contain path separators or `..` components.

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
falls back to the conservative path-key comparison used for Git and URL
sources.

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

2. Add a `database` field to every source entry.

3. Verify config with a dry-run:

   ```bash
   graphtor-docs sync --no-embed
   ```

   If duplicate intakes are detected, review the report and either consolidate
   sources or use `--force` to proceed.

4. Remove the old `sources.yaml` file once you have confirmed the new layout
   works.

> [!CAUTION]
> If you rename the file without adding `database` fields, validation fails
> immediately with a descriptive error. Existing single-file users are not
> affected — `sources.yaml` (without a prefix) is still accepted as a
> legacy fallback.

---

## Exit codes for `sync`

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Partial failures (some files failed; others succeeded) |
| `2` | Fatal error — includes duplicate-intake block without `--force` |

---

## Reference

| Config field | Required in multi-file mode | Description |
|---|---|---|
| `id` | yes | Unique source identifier |
| `type` | yes | Must be `local` |
| `path` | yes | Path to the local docline output directory |
| `database` | **yes** | Target database filename (e.g. `graph.db`) |
| `include` | no | Glob patterns to include |
| `exclude` | no | Glob patterns to exclude |
| `formats` | no | File extension allow-list (only `md` and `markdown` accepted; default: `["md"]`) |
