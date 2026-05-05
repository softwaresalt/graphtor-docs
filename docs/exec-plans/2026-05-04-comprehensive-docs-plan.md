---
title: "Implementation Plan: Comprehensive graphtor-docs Documentation"
description: "Plan for authoring 8 documentation units covering README, architecture, developer guide, CLI reference, pipeline, sync, MCP tools, and troubleshooting"
date: 2026-05-04
---

# Implementation Plan: Comprehensive graphtor-docs Documentation

**Date**: 2026-05-04  
**Source**: Stash `493CA939`  
**Output**: `docs/` directory with well-structured user-facing and developer-facing markdown

---

## Problem Frame

The `graphtor-docs` binary is a fully functional local-first documentation RAG system with
8 CLI subcommands, 7 MCP tools, 3 source types, a CozoDB schema with 6 relations, and an
incremental sync engine. Yet the project README is two lines and no user-facing documentation
exists. Developers integrating the MCP server or configuring `sources.yaml` have no reference
material. Contributors extending the pipeline have no architecture guide.

Affected modules and paths:
- CLI entry: `src/main.rs` (8 subcommands), `src/cli/mod.rs`
- MCP tools: `src/mcp/server.rs` (7 tools: `search_local_docs`, `traverse_doc_links`,
  `search_semantic`, `list_sources`, `get_chunk_by_id`, `get_document`, `get_status`)
- Config: `src/config/source.rs` (`GitSource`, `LocalSource`, `UrlSource`, `formats` field)
- Pipeline: `src/pipeline/mod.rs` (acquire → parse → embed → load)
- Schema: `src/db/schema.rs` (6 relations: `doc_sources`, `doc_chunks`, `doc_edges`,
  `doc_code`, `doc_vectors`, `doc_schema_ver`)
- Embeddings: `src/embed/model.rs` (`all-MiniLM-L6-v2` via Candle, 384-dim)
- Sync: `src/sync/` (git_diff, mtime_diff, reingest, state)
- Workspace: `src/workspace/` (`.graphtor/` subdirs: bin, data, cache, config, logs)

---

## Requirements Trace

| Requirement | Implementation Unit |
|---|---|
| Architecture overview | Unit 1 |
| Developer setup guide | Unit 2 |
| Configuration guide (sources.yaml schema, all source types) | Unit 3 |
| CLI command reference | Unit 4 |
| Data pipeline stages (acquire/parse/embed/load) | Unit 5 |
| CozoDB schema and query patterns | Unit 5 |
| Embedding model details (all-MiniLM-L6-v2 via Candle) | Unit 5 |
| Incremental sync design | Unit 6 |
| MCP tool reference | Unit 7 |
| Troubleshooting guide | Unit 8 |
| Update README.md project overview | Unit 1 |

---

## Implementation Units

### Unit 1 — README + Architecture Overview
**Scope**: docs-only  
**Files**: `README.md`, `docs/architecture.md`  
**Execution posture**: Write-first (no tests needed for documentation)

**What changes**:
- Expand `README.md`: project description, quick-start (install → configure → sync → serve),
  feature highlights, links to detailed docs
- Create `docs/architecture.md`: local-first architecture, component map (CLI → pipeline →
  DB → MCP), data flow diagram (text), tech-stack table with justifications, design principles

**Verifiable outcome**: README renders correctly on GitHub; architecture doc explains the
end-to-end flow without requiring source access.

---

### Unit 2 — Developer Setup Guide
**Scope**: docs-only  
**Files**: `docs/developer-guide.md`  
**Execution posture**: Write-first

**What changes**:
- Prerequisites: Rust stable (1.85+), git2 system deps, optional PDFium DLL
- Building from source: `cargo build --release`
- Running quality gates: `cargo check` → `cargo clippy` → `cargo fmt` → `cargo test`
- Workspace layout: `src/` module map, `tests/integration/`, `.graphtor/` workspace dirs
- Contribution workflow: feature branch → PR → quality gates → review
- Extending the pipeline: adding a new parser, adding an MCP tool, adding a source type

**Verifiable outcome**: A new contributor can clone, build, and run tests following only
this guide.

---

### Unit 3 — Configuration Guide (sources.yaml)
**Scope**: docs-only  
**Files**: `docs/configuration.md`  
**Execution posture**: Write-first

**What changes**:
- Full `sources.yaml` schema reference:
  - `GitSource`: `id`, `url`, `branch` (default: `main`), `include`, `exclude`,
    `formats` (default: `["md","pdf","docx"]`)
  - `LocalSource`: `id`, `path`, `include`, `exclude`, `formats`
  - `UrlSource`: `id`, `url`, `max_depth` (default: 3), `max_pages` (default: 100),
    `domain_lock` (default: true), `rate_limit_ms` (default: 500), `include`, `exclude`, `formats`
- Format allow-list semantics: empty = all supported; non-empty = strict allow-list
- Validation rules: duplicate IDs, empty required fields, unknown format values
- Annotated `sources.yaml` example with all three source types
- Sources resolution: config path precedence (`--config` flag → env var → `.graphtor/config/sources.yaml`)

**Verifiable outcome**: A user can configure all three source types without reading source code.

---

### Unit 4 — CLI Command Reference
**Scope**: docs-only  
**Files**: `docs/cli-reference/graphtor-docs.md`  
**Execution posture**: Write-first

**What changes**:
- Global flags: `--db-path`, `--config`, `--verbose`
- `sync`: `--full`, `--batch-size`, `--no-embed`, `--data-root`; incremental vs full; exit codes
- `serve`: starts STDIO MCP server; model loading; when semantic search is unavailable
- `status`: database statistics output format
- `init`: generates template `sources.yaml` at `.graphtor/config/sources.yaml`
- `install`: copies binary to `.graphtor/bin/`; workspace setup
- `doctor`: health checks (config exists, DB accessible, model available)
- `upgrade`: self-update from latest release
- `uninstall`: removes `.graphtor/` workspace
- Exit code table: `0` success, `1` partial failures, `2` fatal errors

**Verifiable outcome**: Every CLI flag and subcommand is documented; matches `--help` output.

---

### Unit 5 — Pipeline, Schema, and Embeddings Reference
**Scope**: docs-only  
**Files**: `docs/pipeline.md`  
**Execution posture**: Write-first

**What changes**:
- Pipeline stages with data contracts:
  - **Acquire**: git shallow clone (git2, `--depth 1`), local directory scan, URL BFS crawl
    (ureq, htmd HTML→Markdown). Output: files in `.graphtor/data/{source_id}/`
  - **Parse**: pulldown-cmark AST for Markdown (heading-based chunking, link extraction, code
    blocks); pdf-extract + HeadingAwareOutput OutputDev for PDF (font-size histogram, two-pass);
    optional PDFium backend for files ≥20 MiB. Output: `ParsedDocument` with chunks+edges
  - **Embed**: `all-MiniLM-L6-v2` via Candle (in-process, 384-dim, `MAX_LENGTH=512` tokens,
    mean pooling, downloaded from HuggingFace Hub, cached at `~/.cache/huggingface/`).
    `--no-embed` skips this stage. Output: `Vec<f32>` per chunk
  - **Load**: upsert to CozoDB (sqlite backend at `.graphtor/graph.db`). Continue-on-failure
    semantics; partial-success exit code 1
- CozoDB schema (v2):
  - `doc_sources`: `source_id → url, kind, name, synced_at`
  - `doc_chunks`: `chunk_id → source_id, path, title, position, char_offset, headings, content`
  - `doc_edges`: `src_chunk_id, target_path → link_text, anchor`
  - `doc_code`: `snippet_id → chunk_id, language, content`
  - `doc_vectors`: `chunk_id → embedding` (JSON-serialized `Vec<f32>`)
  - `doc_schema_ver`: `ver` (current: 2)
- Chunk ID derivation: SHA-256 of `content + source_path` for stable, deterministic keys
- Sample Datalog queries: search by path prefix, find outgoing edges, count chunks per source

**Verifiable outcome**: A developer can understand the full data contract between pipeline stages
and know what lives in each CozoDB relation.

---

### Unit 6 — Incremental Sync Design
**Scope**: docs-only  
**Files**: `docs/incremental-sync.md`  
**Execution posture**: Write-first

**What changes**:
- Sync state storage: `.graphtor/cache/.sync_state.json` keyed by source ID
- Git source diffing: compare HEAD commit hash → re-ingest only changed/added files
- Local source diffing: compare file `mtime` → re-ingest only modified files
- URL source: always full re-crawl (no diff signal available)
- Forced full sync: `sync --full` bypasses all state; use when schema changes
- Re-ingestion: `reingest.rs` deletes old chunks for changed files, then re-runs pipeline
- Idempotency guarantees: SHA-256 chunk IDs ensure upserts are safe to re-run

**Verifiable outcome**: A user can predict which files will be re-indexed on each sync run.

---

### Unit 7 — MCP Tool Reference
**Scope**: docs-only  
**Files**: `docs/mcp-tools.md`  
**Execution posture**: Write-first

**What changes**:
- Server overview: STDIO transport, localhost-only, `graphtor-docs serve` entry point,
  `.mcp.json` installation snippet
- One section per tool with: description, when to use, parameters (name, type, required/optional,
  default), response format, example invocation
  - `search_local_docs`: full-text keyword search; `query`, `source_id?`, `top_k?`
  - `traverse_doc_links`: BFS graph traversal; `chunk_id`, `max_depth?` (default 2, max 5)
  - `search_semantic`: embedding similarity; `query`, `top_k?`; requires model loaded
  - `list_sources`: no params; returns all indexed sources
  - `get_chunk_by_id`: `chunk_id`; returns full chunk content
  - `get_document`: `source_id`, `path`; returns all chunks for a document
  - `get_status`: no params; returns DB statistics (source count, chunk count, vector count)
- AI-agent selection guide: which tool to reach for first in common scenarios
- Tool chain patterns: `search_local_docs` → extract chunk_id → `traverse_doc_links`

**Verifiable outcome**: An AI agent (or human) can select and invoke the correct tool from
the reference alone.

---

### Unit 8 — Troubleshooting Guide
**Scope**: docs-only  
**Files**: `docs/troubleshooting.md`  
**Execution posture**: Write-first

**What changes**:
- Common issues with diagnosis steps and fixes:
  - `sources.yaml not found` → run `graphtor-docs init`
  - Embedding model unavailable at first run → network needed to download from HuggingFace Hub
  - Large PDF timeout (>20 min) → install PDFium DLL; set `GRAPHTOR_PDFIUM_PATH`
  - `database unavailable` → check `.graphtor/graph.db` permissions; WAL lock after crash
  - `git2` clone failures → SSH key setup; firewall / proxy for HTTPS clones
  - Sync shows 0 chunks → check `formats` field; `include`/`exclude` glob patterns
  - MCP server not responding → confirm STDIO transport; check `.mcp.json` config
  - `--no-embed` sync leaves `search_semantic` non-functional → re-run without flag
- Diagnostic commands: `graphtor-docs doctor`, `graphtor-docs status`, `--verbose` flag
- WAL lock recovery (surfaced in compound learnings)

**Verifiable outcome**: The most common first-hour problems are covered with actionable fixes.

---

## Dependency Graph

```
Unit 1 (README + arch) ──────────────────────────────────────────┐
Unit 2 (developer guide) ─────────────────────────────────────────┤
Unit 3 (config guide) ────────────────────────────────────────────┤ → independent; can
Unit 4 (CLI reference) ───────────────────────────────────────────┤    ship in any order
Unit 5 (pipeline + schema) ──────────────────────────────────────-┤
Unit 6 (incremental sync) ─── depends on Unit 5 (sync uses pipeline concepts)
Unit 7 (MCP tools) ───────────────────────────────────────────────┤
Unit 8 (troubleshooting) ─── depends on Units 3, 4, 5, 7 (references those surfaces)
```

Units 1–5 and 7 are fully independent and can be executed in parallel.  
Unit 6 depends on Unit 5 (shares pipeline vocabulary).  
Unit 8 depends on Units 3, 4, 5, 7 (cross-references their content).

---

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Flat `docs/` layout (no nesting) | Matches existing `docs/` structure; avoids deep paths that complicate cross-links |
| Separate file per concern | Respects 2-hour rule and width-isolation; each file is independently shippable |
| No auto-generated docs (no `cargo doc` output) | `///` docs are already comprehensive in source; user-facing docs need narrative structure, not API dumps |
| Include Datalog query examples in pipeline doc | CozoDB is unfamiliar to most Rust developers; concrete examples reduce onboarding friction |
| Troubleshooting covers PDFium DLL issue explicitly | This is the #1 operational surprise for large-PDF users based on compound learnings |

---

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Docs drift from implementation | Follow compound learning `keep-docs-synchronized-with-implementation.md`: add a docs-review step to the PR template |
| MCP tool descriptions in docs may differ from `#[tool(description = "...")]` strings | Verify each description against `src/mcp/server.rs` before committing |
| `sources.yaml` default values may change in future features | Reference `src/config/source.rs` const functions for canonical defaults |
| Troubleshooting section incomplete for new platforms (Linux, macOS) | Document what's verified; use "may vary" where untested |

---

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | No | Documentation only; no code changes |
| Security, auth, permission, or compliance-sensitive behavior | No | No code written |
| Migration, backfill, destructive data/config action | No | Additive docs only |
| External integration, operator checkpoint | No | No external dependencies |
| High runtime, rollout, or rollback risk | No | Markdown files only |

**Requires plan hardening: no**

---

## Runtime Verification and Closure

| Unit | Changes Runtime Surface? | Runtime Verification | Closure Artifact |
|---|---|---|---|
| 1 — README + arch | No (docs only) | Render check on GitHub | n/a |
| 2 — Developer guide | No | Follow guide on clean checkout; confirm build succeeds | n/a |
| 3 — Config guide | No | Validate example sources.yaml parses without error | n/a |
| 4 — CLI reference | No | Cross-check each flag against `--help` output | n/a |
| 5 — Pipeline + schema | No | Spot-check schema table against `src/db/schema.rs` | n/a |
| 6 — Incremental sync | No | n/a | n/a |
| 7 — MCP tools | No | Spot-check tool params against `src/mcp/server.rs` | n/a |
| 8 — Troubleshooting | No | n/a | n/a |

No runtime surfaces are changed. The only verification needed is ensuring documentation
accuracy against the implementation before merge.

---

## Plan Review

**Reviewed**: 2026-05-04  
**Gate decision**: ADVISORY  
**Hardening required**: No — confirmed satisfied  
**Finding summary**: 0 P0, 0 P1, 3 P2, 2 P3

### P2 — Moderate Gaps (address before merge)

**P2-1 [Unit 7 — MCP Tools]: `.mcp.json` example must be user-facing, not derived from repo dev config**

The repo's `.mcp.json` is a developer workspace config containing engram, backlogit, context7, and
other dev tools. Unit 7's "`.mcp.json` installation snippet" must be an independent user-facing
example showing how a downstream user adds graphtor-docs to their workspace:

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

If the user has the binary on PATH after `graphtor-docs install`, this is the complete snippet.
Without this clarity, users may copy the wrong config.

**P2-2 [Unit 8 — Troubleshooting]: Add Windows path normalization troubleshooting entry**

Compound learning `windows-path-normalization-for-chunk-ids-2026-05-01.md` documents a real user
impact: on Windows, `Path::to_string_lossy()` produces backslash paths, causing:
- Chunk ID mismatches (SHA-256 of `docs\\guide\\intro.md` ≠ `docs/guide/intro.md`)
- MCP `path_matches_source` failures (search and retrieval tools silently return no results)

Unit 8 must include: "If search returns no results for local sources on Windows, verify you are
running version ≥ [the fix in PR #13]. If using an older build, rebuild from source."

**P2-3 [Unit 7 — MCP Tools]: Verify and state `search_semantic` implementation status before documenting**

The compound learning `keep-docs-synchronized-with-implementation.md` (PR #6) found that
`search_similar` was documented as "HNSW vector search" when it returned `not-implemented`. The
current codebase imports `search_similar` in `src/mcp/server.rs` and embeds to `doc_vectors`.
Before documenting `search_semantic` as functional, Ship must verify `src/db/search.rs` confirms
the implementation is complete. If it is still unimplemented, document it with `(planned)` marker.

### P3 — Advisory (log for awareness)

**P3-1 [Unit 5 — Pipeline]: Reference compound learnings as source material**

Ship should read these before authoring Unit 5 to ensure accuracy:
- `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md`
- `docs/compound/pdf-chunk-id-uniqueness-pattern-2026-05-01.md`
- `docs/compound/best-practices/` (PDF histogram and bounded scan patterns)

**P3-2 [Unit 6 — Incremental Sync]: State `.sync_state.json` default path precisely**

The plan says `.graphtor/cache/.sync_state.json`. This should be stated as the default relative
to `cwd` (the current working directory when `graphtor-docs sync` is run), not an absolute path.
The `SyncState::load/save` functions accept an arbitrary path; the binary passes `cwd.join(".graphtor/cache/.sync_state.json")`.

### Recommendations

- Units 1–5, 6 can proceed to harvest as-is; Ship addresses P2-1 and P2-2 during Unit 7 and Unit 8 authoring
- Ship must verify `search_similar` implementation status (P2-3) before writing Unit 7; takes < 5 minutes
- P3 items are noted for awareness; no plan revision required

### Gate: ADVISORY — proceed to harvest; address P2 items during authoring
