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

- **Three source types** — Git repositories (shallow-cloned), local
  directories, and web URLs
- **Unified graph + vector store** — CozoDB (SQLite backend) with
  deterministic SHA-256 chunk IDs
- **Semantic search** — `all-MiniLM-L6-v2` embeddings via Candle (384-dim,
  in-process Rust ML inference; no external model server)
- **Graph traversal** — follow document link graphs with BFS traversal
- **Incremental sync** — re-ingest only changed files (git diff or mtime)
- **8 MCP tools** — search, traverse, research, retrieve, status — for AI agents
- **JSON-RPC 2.0 output** — `--json` global flag wraps all CLI output in
  JSON-RPC 2.0 envelopes for agent and script consumption
- **`manifest` subcommand** — prints tool name/description table or a
  `tools/list`-compatible JSON-RPC 2.0 envelope with the same tool definitions
  as the live MCP server (note: ordering may differ from the live server)
- **Single binary** — zero runtime dependencies

## Quick Start

### 1. Install

```sh
# Build from source (Rust 1.75+ required)
cargo build --release

# Install into your workspace (.graphtor/bin/)
./target/release/graphtor-docs install
```

### 2. Configure sources

```sh
# Generate a starter sources.yaml
graphtor-docs init
```

Edit `.graphtor/config/sources.yaml`:

```yaml
sources:
  - type: git
    id: my-docs
    url: https://github.com/example/my-docs.git
    include: ["**/*.md"]

  - type: local
    id: local-notes
    path: ./notes
```

See the [Configuration Guide](docs/configuration.md) for all options.

### 3. Sync documentation

```sh
graphtor-docs sync
```

First sync downloads the embedding model (~80 MB) from HuggingFace Hub and
ingests all configured sources. Subsequent syncs are incremental.

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
