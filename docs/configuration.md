---
title: Configuration Guide
description: "Complete source registry reference for local docline sources, read-only database entries, routing, and serve posture"
---

graphtor-docs is configured via one or more YAML source registry files, with
`.graphtor/config/sources.yaml` as the legacy single-file layout. The registry
defines local documentation sources to validate, index, and serve, plus optional
read-only database entries to serve without ingestion. Every ingestible source
must be a local directory of docline-emitted standardized Markdown files. Git,
URL, and web-crawl source types are not supported.

## Config File Location

When you provide an explicit config path, graphtor-docs resolves it in this
order:

1. `--config <FILE>` CLI flag (or `-c <FILE>`)
2. `GRAPHTOR_SOURCES` environment variable

Without an explicit path, graphtor-docs discovers source registry files under
`.graphtor/config/` relative to the **current working directory**:

1. All `*.sources.yaml` files, sorted alphabetically
2. `.graphtor/config/sources.yaml` only when no `*.sources.yaml` files exist

Missing registry handling is command-specific:

* `sync` and `prewarm` fail closed because they need a registry before ingestion
* `serve` and `status` share database auto-discovery: both find `.db` files
  dropped under `.graphtor/` even without a registry
* `status` reports an empty list only when no databases are discovered and no
  `--db-path` is given

No stub file is created automatically.

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
fallback is ignored. In multi-file mode every `type: local` entry must include
an explicit `database` field so routing is unambiguous. A `type: database`
entry has no `database` field because it names a pre-built file to serve, not a
source to ingest.

> [!TIP]
> Use multi-file layout when separate teams or products own different
> source groups. Each file is independently editable and reviewable.

## File Format

```yaml
sources:
  - type: local         # ingests and generates a database
    id: my-source-id   # unique identifier for this source
    # … fields …

  - type: database      # serves an existing database read-only; no ingestion
    id: my-db-alias
    path: .graphtor/some-existing.db
```

The top-level key is `sources`, containing an ordered list of source entries.
Every entry must have a `type` (`local` or `database`) and a unique `id`
string.

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
| `path` | string | **yes** | — | Path to the local docline output directory |
| `database` | string | no | primary `--db-path` | Route this source into a specific database file |
| `include` | list\<string\> | no | (all files) | Glob patterns — only matching files are indexed |
| `exclude` | list\<string\> | no | (none) | Glob patterns — matching files are skipped |
| `formats` | list\<string\> | no | `["md"]` | Extension allow-list; only `md` and `markdown` are accepted |

Every `.md` file in a local source directory must contain a valid docline v1
frontmatter block. Files that fail contract validation (missing required
fields, bad `content_sha256`, unsupported `schema_version` major) are rejected
with a deterministic error. See [Pipeline Reference](pipeline.md) for details.

## Source Type: Database (`type: database`)

Names an existing database file that `serve` should expose read-only,
independent of auto-discovery. Unlike `type: local`, this entry never
ingests anything and is never handed to `sync` or the background sync task —
it exists purely to make an explicit, named alias for a database `serve`
should open.

```yaml
sources:
  - type: database
    id: shared-runbooks          # required; unique alias/name for this entry
    path: .graphtor/shared.db    # required; must stay within .graphtor/
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `id` | string | **yes** | — | Unique alias/name for this served database |
| `path` | string | **yes** | — | Path to the database file; must resolve within `.graphtor/` |

There is no `database`, `include`, `exclude`, `formats`, or `read_only` field
for this type — it names a database to serve, it does not describe content to
ingest. Unknown fields are rejected at parse time. The `path` is canonicalized
and validated to stay within the same authorized root as auto-discovery —
`.graphtor/` itself, not the broader project root: an out-of-root path (`..`, a
symlink, or a Windows junction/reparse point escape, or any path outside
`.graphtor/`) is rejected rather than served. External (outside `.graphtor/`)
database paths are not supported.

If the same underlying file is also reachable through auto-discovery
(the entry's `path` necessarily resolves into `.graphtor/`, the same
directory auto-discovery scans), both resolve to the same served database
rather than being opened twice.

See
[Consumption-first serve: auto-discovery, posture, and the operator trust boundary](design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md)
for how this entry participates in serve's discovery and posture rules.

## Database Routing

Set `database` on a local source when you want that source to sync into a
dedicated CozoDB `.db` file (SQLite backend) instead of the primary `--db-path`
target.

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

## Install Footprints and Read-only Serve

The default install is consumption-first:

* `graphtor-docs install` creates `.graphtor/` and a minimal `.mcp.json` entry
* it does **not** create `sources.yaml`, `bin/`, `data/`, `cache/`, `config/`,
  or `logs/`
* it does **not** copy the binary or manage `.gitignore`

This minimal footprint is enough to drop an existing `.db` file directly under
`.graphtor/` and run `graphtor-docs serve`. Serve auto-discovers `.db` files in
that flat directory and opens them read-only unless a real `type: local` source
with ingestible docline Markdown promotes its target database to generation
mode.

Use `graphtor-docs install --with-ingestion` when you need the full
ingestion-capable scaffold. That mode creates `bin/`, `data/`, `cache/`,
`config/`, and `logs/`, copies the binary, writes a template
`.graphtor/config/sources.yaml`, and manages `.gitignore` unless
`--no-gitignore` is set.

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

graphtor-docs validates the source registry on every `sync`, `serve`, and
`status` invocation.
Common validation errors:

| Error | Cause |
|---|---|
| `duplicate source ID` | Two sources share the same `id` value |
| `missing required field` | `id` or `path` is absent |
| `invalid glob pattern` | A pattern in `include` or `exclude` is malformed |
| `invalid database name` | `database` is empty or contains path traversal characters |
| `invalid database path` | A `type: database` path escapes `.graphtor/` |
| `invalid format` | A `formats` value is not `md` or `markdown` |

Run `graphtor-docs sync --no-embed` when you need full registry validation and
text-only ingestion without embedding work. `graphtor-docs doctor` performs
workspace health checks and YAML parse checks; it is not a full source-registry
schema validator.

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
failures. The workspace convention is
`.graphtor/models/all-MiniLM-L6-v2`; if the variable is unset, the resolver
falls back to the Hugging Face Hub cache.

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

> [!NOTE]
> Automatic `.env.local` loading is specific to the PowerShell harness
> (`start.ps1`). The Bash harness (`start.sh`) does not load `.env.local`; under
> Bash or any other launch method, export `GRAPHTOR_EMBED_MODEL_DIR` yourself
> (for example `export GRAPHTOR_EMBED_MODEL_DIR=/abs/path` or via your own
> dotenv loader) before invoking graphtor-docs.

## Annotated Full Example

```yaml
# Local: index a docline output directory
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs
    database: product.db
    include:
      - "**/*.md"
    exclude:
      - "**/drafts/**"

  # Local: separate source routed to its own database
  - type: local
    id: team-runbooks
    path: /home/user/runbooks/out
    database: runbooks.db
    include:
      - "**/*.md"

  # Database: serve an existing database read-only; no ingestion
  - type: database
    id: shared-runbooks
    path: .graphtor/shared-runbooks.db
```
