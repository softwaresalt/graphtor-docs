---
title: graphtor-docs
description: "Local-first documentation RAG — indexes docline Markdown into an embedded CozoDB graph+vector store and serves AI agents via MCP"
---

**graphtor-docs** gives developers and AI-agent users a private documentation
RAG for local docs. It indexes docline-emitted Markdown into an embedded CozoDB
(SQLite backend) graph+vector store, then serves semantic search and graph
cross-references to local agents over STDIO
[Model Context Protocol (MCP)][mcp].

Your docs stay on your machine. There is no hosted RAG service, no external
database, and no separate model server: install one binary, point it at local
docline Markdown, sync, and query.

[mcp]: https://modelcontextprotocol.io

## Features

* **Docline Markdown ingestion** — scans local directories with glob filters and
  indexes only [docline](https://github.com/softwaresalt/docline)-emitted
  `.md` / `.markdown` files; Git, URL/web, PDF, DOCX, and HTML ingestion are
  not supported
* **Docline v1 frontmatter contract** — every file is validated against the v1
  contract (required fields: `title`, `source`, `ingested_at`, `doc_type`,
  `source_path`); malformed or missing frontmatter fails deterministically
* **Embedded graph + vector store** — CozoDB with the SQLite backend, schema v4,
  deterministic SHA-256 chunk IDs, Markdown links/code metadata, and 384-dim
  embeddings stored inline on `doc_chunks`
* **Exact semantic search** — `all-MiniLM-L6-v2` embeddings via Candle
  (in-process Rust ML inference) plus exact brute-force cosine k-NN over stored
  chunk embeddings; no HNSW index is built or maintained. Ranking is exact
  within each database; results from multiple databases are merged round-robin
* **Graph cross-referencing** — follows extracted document links with bounded
  BFS traversal for related context
* **Incremental sync** — re-ingests changed docline Markdown files using
  mtime-based state
* **Multi-database routing** — routes sources to per-source `.db` files and
  supports multi-database `serve`, `status`, and `prewarm`
* **Read-only database sources** — `type: database` entries expose existing
  workspace-contained `.db` files without ingestion
* **Consumption-first serve** — `serve` auto-discovers dropped `.db` files under
  `.graphtor/` and keeps read-only databases out of the write/sync path
* **STDIO MCP server** — exposes `search_local_docs`, `search_semantic`,
  `research_topic`, `traverse_doc_links`, `list_sources`, `get_chunk_by_id`,
  `get_document`, and `get_status`
* **Agent-friendly CLI output** — `--json` wraps CLI output in JSON-RPC 2.0
  envelopes, and `manifest` prints the same tool definitions as the MCP server
* **Workspace containment** — filesystem operations fail closed when paths
  escape the authorized root through `..`, symlinks, or Windows reparse points

## Quick Start

### 1. Install the binary

**macOS / Linux — one-liner:**

```sh
curl -sSf https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.sh | sh
```

**Windows — one-liner (PowerShell 5.1+):**

```powershell
irm https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.ps1 | iex
```

Both scripts download the latest release from [GitHub Releases][releases],
verify the SHA-256 checksum, and install the binary to `~/.local/bin/`
(macOS/Linux) or `%LOCALAPPDATA%\graphtor-docs\bin\` (Windows).

**Direct download:**

Pre-built binaries for macOS (Apple Silicon & Intel), Linux (x86_64), and
Windows (x86_64) are available on the [Releases page][releases].

**Install with cargo:**

```sh
cargo install --git https://github.com/softwaresalt/graphtor-docs --bin graphtor-docs --locked
```

**Build from source (Rust 1.75+ required):**

```sh
git clone https://github.com/softwaresalt/graphtor-docs.git
cd graphtor-docs
cargo build --release
# Binary is at target/release/graphtor-docs
```

[releases]: https://github.com/softwaresalt/graphtor-docs/releases

### 2. Initialize an ingestion workspace

From the workspace where your AI agent will run:

```sh
graphtor-docs install --with-ingestion
```

This creates the ingestion-capable `.graphtor/` scaffold, a template
`.graphtor/config/sources.yaml`, and an MCP config entry.

> [!TIP]
> If you only want to consume an already-generated database, run
> `graphtor-docs install` without flags, drop a `.db` file directly into
> `.graphtor/`, and run `graphtor-docs serve`. No `sources.yaml` or `sync` is
> required for that read-only path.

### 3. Configure a local docline Markdown source

Edit `.graphtor/config/sources.yaml` so it points at a local directory of
docline-emitted Markdown files:

```yaml
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs
    include:
      - "**/*.md"
    formats:
      - md
    database: product.db
```

Every Markdown file in the source directory must contain a valid docline v1
frontmatter block with `title`, `source`, `ingested_at`, `doc_type`, and
`source_path`. Files with missing or malformed frontmatter, unsupported
`schema_version` major versions, or mismatched `content_sha256` values are
rejected deterministically.

See the [Configuration Guide](docs/configuration.md) for all options.

When you omit `database`, the source uses the primary `--db-path` target
(`.graphtor/graph.db` by default). When you set `database`, graphtor-docs
creates or reuses `.graphtor/<database>` and keeps that source's incremental
state in the matching `*.sync_state.json` file.

### 4. Sync documentation

```sh
graphtor-docs sync
```

`sync` scans the configured local directories, validates docline frontmatter,
chunks Markdown by heading, embeds chunks when the model is available, and
writes the graph+vector database. Subsequent runs are incremental
(mtime-based).

By default, graphtor-docs loads `all-MiniLM-L6-v2` from the Hugging Face cache
on first use. Set `GRAPHTOR_EMBED_MODEL_DIR` to a local model directory for
offline or air-gapped environments.

### 5. Run your first semantic search

Use a natural-language query that matches the kind of content in your docs:

```sh
graphtor-docs search-semantic "deployment checklist" --top-k 5
```

A successful query returns the nearest indexed chunks. Within a single database
the ranking is exact; when multiple databases are configured, graphtor-docs
merges each database's exact top matches round-robin rather than computing one
global ranking. If the embedding model is unavailable, fix the model location
and re-run `graphtor-docs sync --full` so chunks receive embeddings.

### 6. Serve to your AI agent

```sh
graphtor-docs serve
```

Add to your MCP client config (e.g., `.mcp.json`):

```json
{
  "mcpServers": {
    "graphtor-docs": {
      "type": "stdio",
      "command": "graphtor-docs",
      "args": ["serve"]
    }
  }
}
```

See the [MCP Tool Reference](docs/mcp-tools.md) for available tools.

## Documentation

| Guide | Description |
|---|---|
| [Architecture Overview](docs/architecture.md) | System design, components, data flow |
| [Developer Guide](docs/developer-guide.md) | Build, test, extend |
| [Configuration Guide](docs/configuration.md) | `sources.yaml` full reference |
| [CLI Reference](docs/cli-reference/graphtor-docs.md) | All subcommands and flags |
| [Pipeline Reference](docs/pipeline.md) | Acquire → parse → embed → load |
| [Incremental Sync](docs/incremental-sync.md) | Change detection design |
| [MCP Tool Reference](docs/mcp-tools.md) | All 8 MCP tools |
| [Troubleshooting](docs/troubleshooting.md) | Common issues and fixes |

## License

MIT
