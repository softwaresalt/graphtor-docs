---
title: "End-to-end readiness assessment for Cosmos DB documentation ingestion"
type: spike
date: 2026-04-30
time_box: "4h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["queue", "learnings"]
tags:
  - "e2e-testing"
  - "cosmos-db"
  - "gap-analysis"
---

## Goal

Can graphtor-docs ingest, embed, and serve queries against a real
documentation corpus (Azure Cosmos DB docs) today? What gaps exist between
the current implementation and a fully functional service?

## Success Criteria

- Inventory all pipeline stages and assess completeness
- Identify which acquisition path works for Cosmos DB docs
- Catalog every gap that would prevent or degrade real-world usage
- Produce stashed requirements for each gap

## Scope Constraints

- Read-only investigation — no code changes
- Assessment only; implementation deferred to feature planning

## Investigation Approach

1. Audit every `src/` module for implementation completeness
2. Trace the full `sync` command path from CLI through pipeline
3. Identify the Cosmos DB docs acquisition strategy
4. Catalog gaps between current state and production readiness
5. Classify gaps by severity and effort

## Findings

### What Works Today (end-to-end path exists)

The system has a **complete acquire → parse → embed → load pipeline** that
should work for Git-hosted documentation:

| Stage | Module | Status | Notes |
|-------|--------|--------|-------|
| CLI entry | `src/main.rs` | ✅ Complete | `sync`, `serve`, `status`, `init`, `install`, `doctor`, `upgrade`, `uninstall` |
| Config | `src/config/` | ✅ Complete | `sources.yaml` with `type: git` and `type: local` sources, glob include/exclude |
| Acquire (Git) | `src/acquire/git.rs` | ✅ Complete | Shallow clone via `git2`, skip-if-exists |
| Acquire (Local) | `src/acquire/local.rs` | ✅ Complete | `walkdir` scan with glob filtering |
| Parse | `src/parse/` | ✅ Complete | pulldown-cmark AST: frontmatter strip, heading chunking, link extraction, code blocks |
| Chunk IDs | `src/chunk/` | ✅ Complete | Deterministic SHA-256 (content + path) |
| Embed | `src/embed/` | ✅ Complete | all-MiniLM-L6-v2 via Candle, 384-dim vectors, in-process |
| Load | `src/db/` | ✅ Complete | CozoDB upsert: chunks, edges, code snippets, source nodes |
| Search | `src/db/search.rs` | ✅ Keyword only | Case-insensitive substring matching in CozoDB |
| Graph traverse | `src/db/traverse.rs` | ✅ Complete | BFS over doc_edges with configurable depth |
| MCP server | `src/mcp/` | ✅ Basic | 2 tools: `search_local_docs`, `traverse_doc_links` |
| Incremental sync | `src/sync/` | ✅ Complete | Git diff + mtime diff, surgical re-ingestion |
| Workspace mgmt | `src/workspace/` | ✅ Complete | install, upgrade, lock, doctor, uninstall |
| Tests | `tests/` | ✅ 26 files | Unit + integration coverage for all modules |

### Cosmos DB Docs Acquisition Strategy

The Azure Cosmos DB documentation lives at:
**`https://github.com/MicrosoftDocs/azure-cosmos-db-docs`**

This is a standard Git repository — **the exact source type graphtor-docs
was designed for**. A `sources.yaml` entry would be:

```yaml
sources:
  - type: git
    id: azure-cosmos-db
    url: https://github.com/MicrosoftDocs/azure-cosmos-db-docs.git
    branch: main
    include:
      - "**/*.md"
    exclude:
      - "**/includes/**"
      - "**/breadcrumb/**"
```

### Gap Analysis

#### GAP-1: Embeddings Not Persisted (HIGH impact)

Vectors are computed during `sync` but **discarded** — not stored in CozoDB.
The `search_similar()` function returns `"not yet implemented"`. This means:

- Only keyword/substring search works, not semantic search
- The embedding computation is wasted CPU cycles
- The primary value proposition (vector similarity) is unavailable

**Effort**: Medium — requires CozoDB HNSW index creation + vector upsert +
search query. Planned as 009-F in the codebase comments.

#### GAP-2: Only 2 MCP Tools (MEDIUM impact)

The MCP server exposes only:
- `search_local_docs` — keyword search
- `traverse_doc_links` — BFS graph traversal

Missing tools that an AI agent would need:
- `list_sources` — enumerate indexed documentation sources
- `get_chunk_by_id` — retrieve a specific chunk by its SHA-256 ID
- `get_document` — retrieve all chunks for a document path
- `search_semantic` — vector similarity search (depends on GAP-1)
- `get_status` — database statistics and health

**Effort**: Low per tool — each is a thin wrapper over existing `db/` functions.

#### GAP-3: No Web Crawling or URL Source Type (LOW impact for v1)

The user mentioned crawling live documentation pages or downloading PDFs.
Neither is supported:

- No `type: url` or `type: web` source in `sources.yaml`
- No HTTP client for downloading pages
- No HTML-to-Markdown converter
- No link-following crawler
- No PDF parser

**Mitigation**: For Microsoft Docs, all content is available in Git repos.
Web crawling is an alternative path, not a blocker.

**Effort**: High — would require new crates (reqwest, scraper or similar),
new pipeline stage, new source type.

#### GAP-4: No PDF Ingestion (LOW impact for v1)

Microsoft Learn provides PDF downloads but graphtor-docs only parses Markdown.

**Mitigation**: Same as GAP-3 — Git repos contain the Markdown source that
PDFs are generated from. PDF is redundant when the source is available.

**Effort**: Medium — would require a PDF extraction crate.

#### GAP-5: Sync Command Uses Full Pipeline, Not Incremental (MEDIUM impact)

The `cmd_sync` function in `main.rs` uses `pipeline::run()` (full pipeline)
rather than the incremental `sync::sync_source()`. The incremental sync
module exists and is tested but is not wired into the CLI.

**Effort**: Low — wire `sync::sync_source()` into `cmd_sync` as the default
path with a `--full` flag for the existing full-pipeline behavior.

#### GAP-6: Stale Doc Comment in `src/chunk/mod.rs` (TRIVIAL)

Line 5 still references "LanceDB vectors to Kùzu graph nodes" instead of
CozoDB. Already stashed as `FF99F3D3`.

### What Was Tried and Failed

N/A — this was an assessment spike, not a prototype.

### Remaining Unknowns

1. **Real-world parse quality**: How well does the chunker handle
   MicrosoftDocs-specific markdown conventions (includes, metadata,
   zone pivots, tabs)? Needs live testing.
2. **Scale**: The azure-cosmos-db-docs repo has ~800+ markdown files.
   Performance characteristics at this scale are untested.
3. **CozoDB HNSW readiness**: The `cozo` 0.7 crate claims HNSW support
   but no graphtor-docs code exercises it. Feasibility is assumed but
   unverified.

## Recommendation

**Conclusion**: Proceed
**Confidence**: High

The system has a complete end-to-end pipeline for Git-sourced Markdown
documentation. The Cosmos DB docs repo is the ideal first test corpus because
it exercises the exact acquisition path the system was designed for.

### Recommended execution order:

1. **Smoke test (no new code)**: Configure `sources.yaml` with the Cosmos DB
   repo and run `graphtor-docs sync` + `graphtor-docs serve` to verify the
   existing pipeline works end-to-end.
2. **Wire incremental sync** (GAP-5): Connect the existing `sync_source()`
   to the CLI so subsequent runs are efficient.
3. **Expand MCP tools** (GAP-2): Add `list_sources`, `get_chunk_by_id`,
   `get_document`, `get_status` for a complete agent experience.
4. **Persist embeddings + vector search** (GAP-1): Implement CozoDB HNSW
   indexing so semantic search works.
5. **Defer web crawling and PDF** (GAP-3, GAP-4): These are alternative
   acquisition paths, not blockers. All Microsoft Docs content is available
   via Git repos.

## Next Steps

Stash each gap as a requirement for Stage to triage and route through the
planning pipeline.

## References

- `src/main.rs` — CLI entry point, `cmd_sync` function (lines 106-193)
- `src/pipeline/mod.rs` — Pipeline orchestrator (lines 149-268)
- `src/acquire/mod.rs` — Acquisition module (clone + scan)
- `src/db/schema.rs` — CozoDB schema (4 relations + schema version)
- `src/db/search.rs` — Text search + `search_similar` placeholder
- `src/mcp/server.rs` — MCP server with 2 tools
- `src/sync/mod.rs` — Incremental sync (not wired to CLI)
- `src/embed/mod.rs` — Candle embedding engine
- `https://github.com/MicrosoftDocs/azure-cosmos-db-docs` — Test corpus
