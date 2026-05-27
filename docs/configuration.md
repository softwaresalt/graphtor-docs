---
title: Configuration Guide
description: "Complete sources.yaml reference for Git, local, and URL source types — all fields, defaults, and annotated examples"
---

graphtor-docs is configured via a single YAML file, `sources.yaml`, which
defines the documentation sources to acquire, index, and serve.

## Config File Location

graphtor-docs resolves the config path in this order:

1. `--config <FILE>` CLI flag (or `-c <FILE>`)
2. `GRAPHTOR_SOURCES` environment variable
3. `.graphtor/config/sources.yaml` relative to the **current working directory**

Run `graphtor-docs init` to generate a starter file at the default location.

## Multi-file layout

You can split your source registry across multiple files by using the
`*.sources.yaml` naming convention. Place each file under
`.graphtor/config/`:

```text
.graphtor/config/
  azure.sources.yaml
  internal-runbooks.sources.yaml
  web.sources.yaml
```

graphtor-docs discovers all `*.sources.yaml` files in that directory,
sorts them alphabetically, and merges them into a single source registry
before running any pipeline stage.

When any `*.sources.yaml` file is present, the legacy `sources.yaml`
fallback is ignored. In multi-file mode every source entry must include
an explicit `database` field so routing is unambiguous.

> [!TIP]
> Use multi-file layout when separate teams or products own different
> source groups. Each file is independently editable and reviewable.

## Auto-generated stub

When any command loads config via `load_source_config` and finds an existing
database file but no source configuration, graphtor-docs writes a minimal stub
at `.graphtor/config/sources.yaml` containing only `sources: []`.

This prevents background sync from triggering on a database that was
imported without a configuration. The stub is written only when:

* The database file exists, **and**
* Neither `sources.yaml` nor any `*.sources.yaml` file is present in
  `.graphtor/config/`

The stub is never overwritten once any source configuration exists.

## File Format

```yaml
sources:
  - type: git         # or "local" or "url"
    id: my-source-id  # unique identifier for this source
    # … type-specific fields …
```

The top-level key is `sources`, containing an ordered list of source entries.
Each entry must have a `type` field (`git`, `local`, or `url`) and a unique
`id` string.

## Source Types

### Git source (`type: git`)

Shallow-clones a remote Git repository and indexes its documentation files.

```yaml
sources:
  - type: git
    id: azure-docs                          # required; unique ID
    url: https://github.com/MicrosoftDocs/azure-docs.git  # required
    branch: main                            # optional; default: "main"
    database: primary.db                    # optional; default: primary --db-path
    include:                                # optional; glob allow-list
      - "**/*.md"
    exclude:                                # optional; glob deny-list
      - "**/drafts/**"
      - "**/CONTRIBUTING.md"
    formats:                                # optional; extension allow-list
      - md
      - pdf
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `id` | string | **yes** | — | Unique source identifier (used as directory name and DB key) |
| `url` | string | **yes** | — | Git clone URL (HTTPS or SSH) |
| `branch` | string | no | `"main"` | Branch to clone |
| `database` | string | no | primary `--db-path` | Route this source into a specific database file |
| `include` | list\<string\> | no | (all files) | Glob patterns — only matching files are indexed |
| `exclude` | list\<string\> | no | (none) | Glob patterns — matching files are skipped |
| `formats` | list\<string\> | no | `["md","pdf","docx"]` | Extension allow-list (see [Formats](#formats)) |

### Local source (`type: local`)

Indexes files from a local filesystem directory.

```yaml
sources:
  - type: local
    id: internal-api-docs                   # required
    path: ./docs/api                        # required; path to directory
    database: runbooks.db                   # optional; default: primary --db-path
    include:                                # optional
      - "**/*.md"
    exclude:                                # optional
      - "**/private/**"
    formats:                                # optional
      - md
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `id` | string | **yes** | — | Unique source identifier |
| `path` | string | **yes** | — | Path to the local directory (relative to cwd or absolute) |
| `database` | string | no | primary `--db-path` | Route this source into a specific database file |
| `include` | list\<string\> | no | (all files) | Glob patterns — only matching files are indexed |
| `exclude` | list\<string\> | no | (none) | Glob patterns — matching files are skipped |
| `formats` | list\<string\> | no | `["md","pdf","docx"]` | Extension allow-list |

### URL source (`type: url`)

Crawls a web URL via BFS and indexes each page (converted to Markdown via
`htmd`).

```yaml
sources:
  - type: url
    id: ms-learn-dotnet                     # required
    url: https://learn.microsoft.com/en-us/dotnet/  # required; start URL
    max_depth: 3                            # optional; default: 3
    max_pages: 100                          # optional; default: 100
    domain_lock: true                       # optional; default: true
    rate_limit_ms: 500                      # optional; default: 500
    database: web.db                        # optional; default: primary --db-path
    include: []                             # optional
    exclude: []                             # optional
    formats:                                # optional
      - md
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `id` | string | **yes** | — | Unique source identifier |
| `url` | string | **yes** | — | Start URL for BFS crawl (https:// or http://) |
| `max_depth` | integer | no | `3` | Maximum BFS depth relative to `url` |
| `max_pages` | integer | no | `100` | Maximum number of pages to crawl |
| `domain_lock` | boolean | no | `true` | When true, crawler stays within `url`'s registered domain |
| `rate_limit_ms` | integer | no | `500` | Minimum milliseconds between consecutive HTTP requests |
| `database` | string | no | primary `--db-path` | Route this source into a specific database file |
| `include` | list\<string\> | no | (all pages) | Glob patterns applied to the crawled page path |
| `exclude` | list\<string\> | no | (none) | Glob patterns applied to the crawled page path |
| `formats` | list\<string\> | no | `["md","pdf","docx"]` | Extension allow-list |

URL sources always re-crawl on each `sync` run (no stable diff signal).
Use `max_pages` to cap the crawl scope.

## Database Routing

Set `database` on a source when you want that source to sync into a dedicated
SQLite file instead of the primary `--db-path` target.

```yaml
sources:
  - type: git
    id: product-docs
    url: https://github.com/example/product-docs.git
    database: product.db

  - type: local
    id: team-notes
    path: ./notes
    database: notes.db
```

`database` is resolved relative to the parent directory of `--db-path`. With
the default `--db-path`, `database: notes.db` writes to `.graphtor/notes.db`.

graphtor-docs also creates one incremental state file per database:

* `graph.db` → `graph.sync_state.json`
* `notes.db` → `notes.sync_state.json`

Use simple file names for `database`. Values must not be empty, must not
contain `/` or `\`, and must not contain parent-directory components such as
`..`.

## Formats

The `formats` field is an **extension allow-list** applied after glob
filtering. Values are file extensions without the leading dot, compared
case-insensitively.

| Value | Parser used |
|---|---|
| `md` or `markdown` | `pulldown-cmark` (Markdown AST) |
| `pdf` | `pdf-extract` (with optional PDFium backend for files ≥ 20 MiB) |
| `docx` | ZIP/XML docx parser |

**Empty list** means no restriction — all extensions supported by the pipeline
are processed.

**Non-empty list** is a strict allow-list — only files whose extension
matches one of the listed strings are passed to the parse stage.

```yaml
# Index only Markdown files (ignore PDFs and DOCX)
formats:
  - md

# Index Markdown and PDF (default behaviour)
formats:
  - md
  - pdf
  - docx
```

## Glob Patterns

`include` and `exclude` use standard glob syntax:

| Pattern | Matches |
|---|---|
| `**/*.md` | All `.md` files at any depth |
| `articles/**` | Everything under the `articles/` subtree |
| `**/drafts/**` | Any path containing a `drafts/` component |
| `README.md` | A file named `README.md` at the root |

When both `include` and `exclude` match a file, `exclude` wins.

## Validation

graphtor-docs validates `sources.yaml` on every `sync` or `serve` invocation.
Common validation errors:

| Error | Cause |
|---|---|
| `duplicate source ID` | Two sources share the same `id` value |
| `missing required field` | `id` or `url`/`path` is absent |
| `invalid glob pattern` | A pattern in `include` or `exclude` is malformed |
| `invalid database name` | `database` is empty or contains path traversal characters |

Run `graphtor-docs doctor` to validate the config file without running a sync.

## Annotated Full Example

```yaml
sources:
  # ── Git: shallow-clone a public GitHub docs repo ─────────────────────────
  - type: git
    id: azure-docs
    url: https://github.com/MicrosoftDocs/azure-docs.git
    branch: main
    database: azure.db
    include:
      - "articles/**/*.md"
    exclude:
      - "**/includes/**"
      - "**/media/**"
    formats:
      - md

  # ── Local: index files from a local directory ─────────────────────────────
  - type: local
    id: team-runbooks
    path: /home/user/runbooks
    database: runbooks.db
    include:
      - "**/*.md"
      - "**/*.pdf"
    formats:
      - md
      - pdf

  # ── URL: crawl a documentation website ───────────────────────────────────
  - type: url
    id: ms-learn-azure-functions
    url: https://learn.microsoft.com/en-us/azure/azure-functions/
    max_depth: 2
    max_pages: 50
    domain_lock: true
    rate_limit_ms: 1000
    database: web.db
    formats:
      - md
```
