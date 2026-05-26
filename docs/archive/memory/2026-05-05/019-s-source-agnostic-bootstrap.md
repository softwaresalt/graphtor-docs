---
type: session-memory
date: 2026-05-05
shipment: 019-S
pr: 34
merge_commit: e84dbbe
agent: Ship
---

# 019-S: Source-Agnostic Rebranding & Session Bootstrap

## Outcome

Shipped PR #34. All 3 docs/script files updated; all quality gates passed.
Shipment archived: `028-F`, `028.001-T`, `028.002-T`, `019-S`.

## Changes

### `AGENTS.md`
- Project Context: MS-specific → source-agnostic acquisition language
- Storage: LanceDB/Kùzu → CozoDB (embedded, sqlite backend)
- MCP tool examples: `search_ms_docs_semantic` → `search_local_docs`, `search_semantic`, `get_chunk_by_id`
- rmcp version: 0.5 → 1.5
- MCP module path: `src/mcp/tools/` → `src/mcp/server.rs` (Copilot review fix)

### `.github/copilot-instructions.md`
- Tool name examples: `search_ms_docs_semantic`, `explore_concept_graph` → `search_local_docs`, `search_semantic`, `traverse_doc_links`
- MCP module path corrected (Copilot review fix)

### `start.ps1`
- Pre-sync block added after `$env:GITHUB_TOKEN` assignment
- `graphtor-docs sync` + `backlogit sync` before copilot launch
- `Get-Command` existence checks (graceful degradation)
- Non-fatal: `Write-Warning` on non-zero exit, continues

## Decisions

- Copilot review caught 2 valid findings (`src/mcp/tools/` path stub) — fixed in follow-up commit `f7bc92d`
- Merged with `--admin` to bypass `REVIEW_REQUIRED` branch protection after explicit operator approval

## Next

- **020-S** (Zero-Config Adoption & Composite Research Tool) is queued
- Stash `AEAA9E54` (CLI JSON-RPC `--json` + `manifest` command) awaiting deliberation
