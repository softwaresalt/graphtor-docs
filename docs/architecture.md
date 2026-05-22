---
title: Architecture Overview
description: "Component map, data flow, technology stack, storage layout, and chunk identity for graphtor-docs"
---

graphtor-docs is a local-first documentation RAG system. All computation
runs in-process. No cloud services, no networked databases, no external
model servers. The entire system compiles to a single Rust binary.

## Design Principles

| Principle | Implementation |
|---|---|
| **Local-first** | Embedded CozoDB (SQLite backend); Candle ML inference in-process |
| **Lightweight footprint** | ~80 MB embedding model; single binary; no runtime dependencies |
| **Data pipeline integrity** | SHA-256 chunk IDs; deterministic AST parsing; idempotent upserts |
| **MCP-native interface** | All capabilities exposed via 8 MCP tools; STDIO transport |
| **Automation & reproducibility** | Incremental sync; single `sync` command; re-runnable without intervention |

## Component Map

```text
┌─────────────────────────────────────────────────────────────────┐
│  CLI (graphtor-docs binary)                                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────────┐   │
│  │  sync    │ │  serve   │ │  status  │ │ init/install/…  │   │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └─────────────────┘   │
│       │            │            │                               │
│  ┌────▼─────┐  ┌───▼──────┐  ┌─▼──────────────────────────┐   │
│  │ Pipeline │  │   MCP    │  │       DataStore             │   │
│  │ Stage    │  │  Server  │  │  (CozoDB, SQLite backend)   │   │
│  │          │  │ (8 tools)│  │  .graphtor/*.db             │   │
│  │ acquire  │  └──────────┘  └─────────────────────────────┘   │
│  │ parse    │                                                   │
│  │ embed ◄──┼── EmbeddingModel (all-MiniLM-L6-v2, Candle)     │
│  │ load     │                                                   │
│  └──────────┘                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Data Flow

```text
sources.yaml
    │
    ├── optional per-source database file name
    │
    ▼
┌──────────────────────────────────────────────────────┐
│ Acquire                                              │
│  Git  → git2 shallow clone → .graphtor/data/{id}/   │
│  Local → directory scan   → (in-place; no copy)     │
│  URL  → ureq BFS crawl    → .graphtor/data/{id}/    │
└──────────────────────┬───────────────────────────────┘
                       │ files on disk
                       ▼
┌──────────────────────────────────────────────────────┐
│ Parse                                                │
│  .md   → pulldown-cmark AST → chunks + edges        │
│  .pdf  → pdf-extract / PDFium → chunks               │
│  .docx → docx parser → chunks                        │
│  Output: ParsedDocument { chunks, edges }            │
└──────────────────────┬───────────────────────────────┘
                       │ ParsedDocument
                       ▼
┌──────────────────────────────────────────────────────┐
│ Embed (skipped with --no-embed)                      │
│  all-MiniLM-L6-v2 via Candle                         │
│  384-dim float32 vectors, mean pooling               │
│  Downloaded from HuggingFace Hub, cached locally     │
│  Output: Vec<f32> per chunk                          │
└──────────────────────┬───────────────────────────────┘
                       │ chunks + vectors
                       ▼
┌──────────────────────────────────────────────────────┐
│ Load (CozoDB upserts)                                │
│  doc_sources  — source registry                      │
│  doc_chunks   — chunk content + metadata             │
│  doc_edges    — document link graph                  │
│  doc_code     — extracted code snippets              │
│  doc_vectors  — embedding vectors                    │
└──────────────────────────────────────────────────────┘
                       │
                       ▼
               MCP Server (serve)
          search_local_docs  ─── text search via doc_chunks
          search_semantic    ─── vector cosine similarity
          traverse_doc_links ─── BFS over doc_edges
          research_topic     ─── combined search + graph traversal
          list_sources       ─── doc_sources registry
          get_chunk_by_id    ─── single chunk lookup
          get_document       ─── all chunks for a path
         get_status         ─── health + counts
```

## Technology Stack

| Concern | Technology | Notes |
|---|---|---|
| Language | Rust (stable, 1.75+, edition 2021) | `#![forbid(unsafe_code)]` enforced |
| Unified store | CozoDB (`cozo` crate, SQLite backend) | Embedded; Datalog queries; property-graph traversal |
| Embeddings | `all-MiniLM-L6-v2` via Candle | ~80 MB model; 384-dim; pure Rust inference |
| Graph extraction | `pulldown-cmark` | AST-based; deterministic; 100% precision |
| MCP interface | `rmcp` crate | Async STDIO JSON-RPC via tokio |
| Git operations | `git2` crate | Native bindings; shallow clones; no shell-out |
| URL crawling | `ureq` (sync) | Avoids tokio nested-runtime issues |
| Configuration | `serde_yaml` + `serde_json` | Type-safe YAML/JSON deserialization |
| CLI | `clap` (derive) | Subcommands; global flags |
| Async runtime | `tokio` | MCP server; async acquire |
| Error handling | `thiserror` + `anyhow` | Typed domain errors; binary entry-point propagation |
| Logging | `tracing` + `tracing-subscriber` | Structured async-safe; JSON or human format |

## Storage Layout

```text
.graphtor/                  ← workspace root (relative to cwd)
  bin/                      ← installed binary
  config/
    sources.yaml            ← documentation source registry
  data/
    {source_id}/            ← acquired files (git clones and url crawl cache)
  cache/                    ← HuggingFace model cache (see ~/.cache/huggingface/hub/)
  logs/                     ← transient output files
  graph.db                  ← primary CozoDB SQLite database
  *.db                      ← optional per-source CozoDB SQLite databases
  graph.sync_state.json     ← incremental sync state for graph.db
  *.sync_state.json         ← incremental sync state for routed databases
```

## Chunk Identity

Every chunk has a stable **chunk ID** — the SHA-256 hash of its content,
a NUL byte separator (`\0`), and its source-relative path (using
forward-slash separators on all platforms). This ID is the correlation key
across `doc_chunks`,
`doc_vectors`, and `doc_edges`. Upserts by chunk ID are safe to re-run: the
same input always produces the same ID and the same stored record.

## Incremental Sync

The sync engine tracks change at the source level:

- **Git sources**: compares HEAD commit SHA-1 to the `last_commit` stored in
  the database-specific `*.sync_state.json` file; re-ingests only files that
  appear in the diff
- **Local sources**: compares current file `mtime` to the stored mtime map;
  re-ingests only modified or new files
- **URL sources**: always re-crawls (no stable diff signal); `max_pages`
  caps the crawl scope

When a source sets `database`, sync, serve, status, and prewarm route that
source through the matching `.db` file and aggregate results across all loaded
databases.

See the [Incremental Sync Design](incremental-sync.md) for full details.
