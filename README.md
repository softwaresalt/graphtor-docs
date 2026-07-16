---
title: graphtor-docs
description: "Local-first documentation RAG — indexes your docs into an embedded graph+vector store and serves them to AI agents via MCP"
---

**graphtor-docs** is a local-first documentation RAG system that indexes
your documentation sources into an embedded graph+vector store and serves
them to AI agents via the [Model Context Protocol (MCP)][mcp].

No cloud services. No external databases. One binary.

[mcp]: https://modelcontextprotocol.io

## Features

- **Standardized-Markdown only** — indexes local directories of
  [docline](https://github.com/softwaresalt/docline)-emitted Markdown files;
  no git clone, web crawl, PDF, or DOCX ingestion
- **Docline v1 frontmatter contract** — every file is validated against the
  v1 contract (required fields: `title`, `source`, `ingested_at`, `doc_type`,
  `source_path`); malformed or missing frontmatter fails deterministically
- **Unified graph + vector store** — CozoDB (SQLite backend) with
  deterministic SHA-256 chunk IDs and inline HNSW-indexed embeddings
- **Semantic search** — `all-MiniLM-L6-v2` embeddings via Candle (384-dim,
  in-process Rust ML inference; no external model server)
- **Graph traversal** — follow document link graphs with BFS traversal
- **Incremental sync** — re-ingest only changed files (mtime-based)
- **Per-source database routing** — send selected sources to separate `.db`
  files with automatic multi-database `serve`, `status`, and `prewarm` support
- **8 MCP tools** — search, traverse, research, retrieve, status — for AI agents
- **JSON-RPC 2.0 output** — `--json` global flag wraps all CLI output in
  JSON-RPC 2.0 envelopes for agent and script consumption
- **`manifest` subcommand** — prints tool name/description table or a
  `tools/list`-compatible JSON-RPC 2.0 envelope with the same tool definitions
  as the live MCP server (note: ordering may differ from the live server)
- **Single binary** — zero runtime dependencies

## Quick Start

### 1. Install

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

> **Just want to consume an already-generated database?** Skip straight to
> `graphtor-docs install` (no flags) in an empty workspace, drop a `.db`
> file into the resulting `.graphtor/` directory, and run `graphtor-docs
> serve` — no `sources.yaml` or `sync` required. The steps below are for
> ingesting and generating your own documentation index; see
> [Ingestion setup](docs/cli-reference/graphtor-docs.md#ingestion-setup) for
> the full CLI-driven workflow.

### 2. Configure sources

Create `.graphtor/config/sources.yaml` pointing at your local docline output
directories:

```yaml
sources:
  - type: local
    id: product-docs
    path: ./out/product-docs     # directory of docline-emitted .md files
    include: ["**/*.md"]
    database: product.db

  - type: local
    id: team-runbooks
    path: ./out/runbooks
    include: ["**/*.md"]
```

Every `.md` file in these directories must contain a valid docline v1
frontmatter block. Files that are missing required frontmatter fields or whose
`content_sha256` digest mismatches are rejected and reported — they do not
silently propagate partial data.

See the [Configuration Guide](docs/configuration.md) for all options.

When you omit `database`, the source uses the primary `--db-path` target
(`.graphtor/graph.db` by default). When you set `database`, graphtor-docs
creates or reuses `.graphtor/<database>` and keeps that source's incremental
state in the matching `*.sync_state.json` file.

### 3. Sync documentation

```sh
graphtor-docs sync
```

First sync downloads the embedding model (~80 MB) from HuggingFace Hub and
ingests all configured sources. Subsequent syncs are incremental (mtime-based).

### 4. Serve to your AI agent

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
