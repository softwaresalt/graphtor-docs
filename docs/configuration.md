---
title: Configuration Guide
description: "Complete sources.yaml reference for Git, local, and URL source types — all fields, defaults, and annotated examples"
---

graphtor-docs is configured via a single YAML file, `sources.yaml`, which
defines the local documentation sources to validate, index, and serve.
Every source must be a local directory of docline-emitted standardized
Markdown files. Git, URL, and web-crawl source types are not supported.

## Config File Location

graphtor-docs resolves the config path in this order:

1. `--config <FILE>` CLI flag (or `-c <FILE>`)
2. `GRAPHTOR_SOURCES` environment variable
3. `.graphtor/config/sources.yaml` relative to the **current working directory**

If no config file is found at the resolved path, graphtor-docs exits with a
fatal error. No stub file is created automatically.

## Multi-file layout

You can split your source registry across multiple files by using the
`*.sources.yaml` naming convention. Place each file under
`.graphtor/config/`:

```text
.graphtor/config/
  graph.sources.yaml
  powerbi.sources.yaml
  runbooks.sources.yaml
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

## File Format

```yaml
sources:
  - type: local         # only supported type
    id: my-source-id   # unique identifier for this source
    # … fields …
```

The top-level key is `sources`, containing an ordered list of source entries.
Each entry must have `type: local` and a unique `id` string.

## Source Type: Local (`type: local`)

Indexes docline-emitted Markdown files from a local filesystem directory.

```yaml
sources:
  - type: local
    id: internal-api-docs                   # required
    path: ./out/api-docs                    # required; path to directory
    database: api.db                        # optional; default: primary --db-path
    include:                                # optional
      - "**/*.md"
    exclude:                                # optional
      - "**/drafts/**"
    formats:                                # optional; only "md" and "markdown" accepted
      - md
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `id` | string | **yes** | — | Unique source identifier |
| `path` | string | **yes** | — | Path to the local directory (relative to cwd or absolute) |
| `database` | string | no | primary `--db-path` | Route this source into a specific database file |
| `include` | list\<string\> | no | (all files) | Glob patterns — only matching files are indexed |
| `exclude` | list\<string\> | no | (none) | Glob patterns — matching files are skipped |
| `formats` | list\<string\> | no | `["md"]` | Extension allow-list; only `md` and `markdown` are accepted |

Every `.md` file in the directory must contain a valid docline v1 frontmatter
block. Files that fail contract validation (missing required fields, bad
`content_sha256`, unsupported `schema_version` major) are rejected with a
deterministic error. See [Pipeline Reference](pipeline.md) for details.

## Database Routing

Set `database` on a source when you want that source to sync into a dedicated
SQLite file instead of the primary `--db-path` target.

```yaml
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs
    database: product.db

  - type: local
    id: team-runbooks
    path: ./out/runbooks
    database: runbooks.db
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
filtering. Only Markdown extensions are accepted.

| Value | Parser used |
|---|---|
| `md` or `markdown` | `pulldown-cmark` (Markdown AST) |

**Defaults to `["md"]`** when the field is absent from YAML.

Specifying `pdf`, `docx`, `html`, or any other extension is a validation
error — graphtor-docs will refuse to start with an unsupported format.

```yaml
# Index Markdown files (all valid variants)
formats:
  - md
  - markdown   # alias for md; both accepted
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
| `missing required field` | `id` or `path` is absent |
| `invalid glob pattern` | A pattern in `include` or `exclude` is malformed |
| `invalid database name` | `database` is empty or contains path traversal characters |
| `invalid format` | A `formats` value is not `md` or `markdown` |

Run `graphtor-docs doctor` to validate the config file without running a sync.

## Embedding Model (Semantic Search)

Semantic search (`search_semantic`, `research_topic`, and their MCP `serve`
equivalents) requires the `all-MiniLM-L6-v2` sentence-embedding model. By
default graphtor-docs downloads it from the Hugging Face Hub into
`~/.cache/huggingface` on first use. Pass `sync --no-embed` to skip embeddings
entirely (semantic search is then disabled).

### Offline / air-gapped: `GRAPHTOR_EMBED_MODEL_DIR`

Set the `GRAPHTOR_EMBED_MODEL_DIR` environment variable to a local directory
containing `config.json`, `tokenizer.json`, and `model.safetensors`.
graphtor-docs then loads the model from that directory with **no network
access** — useful for air-gapped environments or to sidestep Hub-download
failures.

Download the three files once (for example with Python `huggingface_hub`):

```python
from huggingface_hub import snapshot_download
snapshot_download(
    "sentence-transformers/all-MiniLM-L6-v2",
    allow_patterns=["config.json", "tokenizer.json", "model.safetensors"],
    local_dir="./.graphtor/models/all-MiniLM-L6-v2",
)
```

> [!IMPORTANT]
> Use an **absolute** path. The directory is resolved against the process
> working directory, and the MCP `serve` working directory is set by the MCP
> client — an absolute path is unambiguous in every mode.

### Workspace convention: `.env.local` (CLI and MCP parity)

Because the setting is a plain environment variable, it is inherited by every
child process. Put it in a workspace-root `.env.local` file so it applies
uniformly whether graphtor-docs runs as a CLI subcommand (`sync`) or as the
MCP `serve` server:

```text
# .env.local (git-ignored)
GRAPHTOR_EMBED_MODEL_DIR=C:\workspace\.graphtor\models\all-MiniLM-L6-v2
```

The `start.ps1` harness loader reads `.env.local` into the session environment
at startup, so both the CLI you invoke and the MCP server the client launches
inherit the value. This is the recommended way to make configuration behave
identically in CLI and MCP mode.

## Annotated Full Example

```yaml
# ── Local: index a docline output directory ──────────────────────────────────
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs
    database: product.db
    include:
      - "**/*.md"
    exclude:
      - "**/drafts/**"

  # ── Local: separate source routed to its own database ───────────────────────
  - type: local
    id: team-runbooks
    path: /home/user/runbooks/out
    database: runbooks.db
    include:
      - "**/*.md"
```
