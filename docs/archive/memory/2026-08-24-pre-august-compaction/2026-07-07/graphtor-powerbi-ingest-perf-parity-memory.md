---
type: session-memory
date: 2026-07-07
topic: Power BI docline ingestion, load-stage perf, offline embeddings, CLI/MCP parity
status: in-progress
---

# Power BI ingestion + graphtor performance/feature session

## Task summary

Operator added the docline documentation output (`C:\Source\Docs\docline\powerbi`,
8,618 markdown files, 5 doc-sets) as a read-only source and asked to ingest it into a
single `powerbi.db`, then iteratively fix issues and improve performance.

## Key findings & decisions

### Ingestion / docline contract

- **Workspace containment**: every local source in `sources.yaml` must resolve under the
  process cwd (`src/config/validation.rs::prepare_local_source` → `validate_path`;
  `workspace_root == std::env::current_dir()`). The docline output is outside the repo, so
  graphtor is run with `cwd = C:\Source` while directing ALL writes into the repo
  (`--db-path <repo>\.graphtor\graph.db --data-root <repo>\.graphtor\data`); the source's
  `database: powerbi.db` lands next to `--db-path` via `resolve_source_db_path`.
- **content_sha256 contract (docline bug, FIXED by operator)**: docline computed
  `content_sha256` BEFORE injecting chunk anchors (`app.py:338` vs `assemble.py::_inject_chunk_anchors`),
  but graphtor re-hashes the emitted (post-anchor) body. Fixed docline hashes the final body →
  0 mismatches after regeneration.
- **duplicate source_path**: one graphtor source spanning 5 doc-sets collides on identical
  relative paths (README.md, .github/pull_request_template.md). Fixed by splitting into
  **5 per-doc-set sources** all targeting `powerbi.db`. Safe because `chunk_id` is
  source-scoped: `generate_chunk_id` hashes `content \0 source_id \0 source_path`
  (`src/chunk/id.rs`). Result: dupes 13 → 0, errors 19 → 6, docs 8,599 → 8,612.

### Load-stage performance (PR #79, merged 09617fc)

- Root cause: per-row CozoDB `:put` (each `store.mutate`/`store.query` recompiles the Datalog
  script). Fix: batched multi-row `:put` per relation per `process_batch` + single batched
  vector read (`upsert_chunks_batch`/`upsert_edges_batch`/`upsert_code_snippets_batch`/
  `upsert_url_index_batch`, `get_vectors_for` in src/db/; `process_batch` load rewrite in
  src/pipeline/mod.rs). Edge/url keys deduped in Rust (last-writer-wins) vs Cozo sorted-tuple
  resolution. **33 min (unfinished) → 4.2 min** for 64k chunks.

### Offline embeddings (PR #80, merged a5cb2d8)

- **hf-hub 0.3.2 is too old** to follow HF's current redirects → `RelativeUrlWithoutBase`;
  every embedded run silently degraded to no-embed. Network is fine (Python huggingface_hub
  downloads OK).
- Fix: `GRAPHTOR_EMBED_MODEL_DIR` env var → resolver loads via existing
  `EmbeddingModel::from_path` (no network). `select_model_source` pure routing in
  `src/embed/resolver.rs`. Model files downloaded to `.graphtor/models/all-MiniLM-L6-v2/`.
- **Config-in-both-modes**: the env var is set in workspace-root `.env.local`;
  `start.ps1` (post-PR#78) loads it into the session, and BOTH the CLI and the MCP `serve`
  child inherit it (process env inheritance). Validated: `serve` semantic query loaded the
  local model and returned relevant results. **No graphtor config file needed** for this.

### Embedded ingest result

- Full embedded ingest: **8,612 docs / 64,291 chunks / 1.18 GB `powerbi.db`**, 0 sha
  mismatches, semantic search verified (`search_semantic` returns on-topic chunks).
- **HNSW bottleneck**: embedded pass took ~4.6 h (~3.9 chunks/s). Throughput steady (not
  superlinear); Cozo's HNSW vector-index insertion dominates, not embedding inference.
  → backlog 047-F (defer/batch HNSW index build; e.g., drop index → bulk load → build once).

### CLI/MCP parity gap (backlog 046-F)

- 8 MCP tools in `src/mcp/server.rs`: search_local_docs, traverse_doc_links, search_semantic,
  research_topic, list_sources, get_chunk_by_id, get_document, get_status.
- **7 have NO CLI equivalent** (only `status` partially covers get_status). CLI Command enum
  (`src/cli/mod.rs`): sync, serve, status, init, install, doctor, upgrade, uninstall, manifest,
  prewarm.
- Handlers call shared fns (DocServer `*_all` → db::search/traverse/chunks/nodes) and format via
  `src/mcp/format.rs`. Parity plan: extract a reusable query/service layer from DocServer,
  add CLI subcommands reusing the same fns + formatters (+ `--json` via src/cli/jsonrpc.rs).
- Motivation: harness must fall back to CLI when MCP transport fails mid-session.

## PRs this session

- #72 root `.mcp.json` migration; #74 cross-source link resolution; #75/#77 CI skip-gate +
  PowerShell port; #78 harness/config baseline; **#79 load-stage batching**; **#80 local embed model dir**.

## Backlog

- 045-F → reduced to DOCS: document `GRAPHTOR_EMBED_MODEL_DIR` + `.env.local` convention (both modes).
- 046-F → CLI/MCP query parity (large; refactor + subcommands).
- 047-F → HNSW vector-index build optimization for bulk ingest.

## Next steps
1. 047-F HNSW fix; 046-F CLI parity; 045-F docs — ship each through the pipeline.
2. `powerbi.db` (1.18 GB) + model dir live under `.graphtor/` (gitignored, local).
3. To use embedded db: ensure `.env.local` has `GRAPHTOR_EMBED_MODEL_DIR` (added this session).
