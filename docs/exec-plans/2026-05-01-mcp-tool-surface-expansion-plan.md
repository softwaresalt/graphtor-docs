# Implementation Plan: MCP Tool Surface Expansion (017-F)

**Source:** backlogit feature `017-F` — MCP Tool Surface Expansion
**Date:** 2026-05-01
**Shipment:** 009-S

## Problem Frame

The MCP server (`src/mcp/server.rs`) currently exposes three tools:
`search_local_docs`, `traverse_doc_links`, and `search_semantic`. AI agents
using the LocalDocRAG system lack the ability to enumerate indexed sources,
retrieve individual chunks by ID, view all chunks for a document, or check
system health — operations that are fundamental for effective tool selection
and diagnostic workflows.

The underlying DB layer (`src/db/`) already implements most of the required
data access functions:

| Needed Tool | DB Function | Status |
|---|---|---|
| `list_sources` | `db::nodes::list_sources` | ✅ Exists |
| `get_chunk_by_id` | `db::chunks::get_chunk` | ✅ Exists |
| `get_document` | (list chunks filtered by path) | ⚠️ Needs new query |
| `get_status` | (aggregate counts across relations) | ⚠️ Needs new function |
| `search_semantic` | `db::search::search_similar` | ✅ Already shipped |

The work is primarily **MCP wiring** — adding parameter types, tool
implementations, response formatters, and tests for four new tools.

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| Enumerate indexed sources | Add `list_sources` MCP tool wrapping `db::nodes::list_sources` |
| Retrieve chunk by SHA-256 ID | Add `get_chunk_by_id` MCP tool wrapping `db::chunks::get_chunk` |
| Get all chunks for a document path | Add `list_chunks_by_path` DB function + `get_document` MCP tool |
| Get DB stats and health | Add `get_status` DB function + `get_status` MCP tool |
| `search_semantic` (vector search) | ✅ Already implemented — no work needed |

## Implementation Units

### Unit 1: DB Layer — `list_chunks_by_path` and `get_status`

**What:** Add two new DB-layer functions:
1. `list_chunks_by_path(store, path) -> Vec<ChunkRecord>` in `src/db/chunks.rs`
   — returns all chunks matching a given relative document path, ordered by position.
2. `get_status(store) -> DbStatus` in `src/db/store.rs` — returns counts for
   each relation (`doc_sources`, `doc_chunks`, `doc_edges`, `doc_code`,
   `doc_vectors`) and the schema version.

**Files affected:**
- `src/db/chunks.rs` — add `list_chunks_by_path`
- `src/db/store.rs` — add `DbStatus` struct and `get_status` method
- `src/db/mod.rs` — re-export new items

**Tests:**
- Unit test: `list_chunks_by_path` returns ordered chunks for a known path
- Unit test: `list_chunks_by_path` returns empty vec for unknown path
- Unit test: `get_status` returns correct counts after upserts
- Unit test: `get_status` returns zeros for empty store

**Execution posture:** Test-first. Write tests against the existing `DataStore::open_mem()` pattern.

### Unit 2: MCP Format Helpers

**What:** Add format functions for the new tool responses:
1. `format_sources_list(&[SourceRecord]) -> String` — markdown table of sources
2. `format_chunk_detail(&ChunkRecord) -> String` — full chunk with all metadata
3. `format_document_chunks(&[ChunkRecord]) -> String` — ordered chunk listing for a document
4. `format_status(&DbStatus) -> String` — health/stats markdown

**Files affected:**
- `src/mcp/format.rs` — add four new formatting functions

**Tests:**
- Unit test: `format_sources_list` empty → "No sources indexed."
- Unit test: `format_sources_list` with data → contains source_id, name, kind
- Unit test: `format_chunk_detail` includes chunk_id, path, headings, content
- Unit test: `format_document_chunks` preserves chunk ordering
- Unit test: `format_status` shows relation counts

**Execution posture:** Test-first. Follows existing `format_search_results` pattern.

### Unit 3: MCP Tool Implementations

**What:** Add four new tools to `DocServer` in `src/mcp/server.rs`:
1. `list_sources` — no required params, returns formatted source list
2. `get_chunk_by_id` — param: `chunk_id: String`, returns chunk detail or not-found error
3. `get_document` — params: `path: String`, `source_id: Option<String>`, returns document chunks
4. `get_status` — no required params, returns DB stats

Each tool follows the established pattern: param struct → `#[tool]` macro →
validation → DB call → format → `CallToolResult::success`.

**Files affected:**
- `src/mcp/server.rs` — add param structs + tool methods to `#[tool_router] impl DocServer`

**Tests:**
- Unit test: `list_sources` on empty store returns success with "No sources" message
- Unit test: `get_chunk_by_id` with invalid ID returns descriptive error
- Unit test: `get_chunk_by_id` with empty string returns `invalid_params`
- Unit test: `get_document` with empty path returns `invalid_params`
- Unit test: `get_status` returns success with counts

**Execution posture:** Test-first. Follows existing `search_local_docs`/`traverse_doc_links` test pattern using `test_server()` helper.

### Unit 4: Integration Test — Full Tool Surface

**What:** Add an integration test that verifies the complete tool surface works
end-to-end: upsert source → upsert chunks → call each new tool → verify output
contains expected data.

**Files affected:**
- `tests/integration/` — add `mcp_tools_expansion.rs` (or extend existing MCP test file)

**Tests:**
- Integration test: full lifecycle — register source, upsert chunks, query via each tool
- Integration test: `get_document` filters correctly by source_id prefix

**Execution posture:** Test-first. Runs against in-memory store with known test data.

## Dependency Graph

```text
Unit 1 (DB layer) ─┐
                    ├─→ Unit 3 (MCP tools)
Unit 2 (formatters) ┘         │
                               ↓
                        Unit 4 (integration)
```

Units 1 and 2 are independent and can be implemented in parallel.
Unit 3 depends on both. Unit 4 depends on Unit 3.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Keep tools in single `server.rs` rather than per-tool modules | The file is currently 410 lines with 3 tools. Adding 4 more keeps it under ~700 lines — manageable. Per-tool modules add complexity without benefit at this scale. Refactor when tool count exceeds 10. |
| `get_document` uses `path` not `source_id` as primary key | Users discover paths from search results. Path is the natural lookup key. `source_id` is an optional filter for disambiguation. |
| `get_status` returns relation counts, not row-level diagnostics | Health check should be fast and lightweight. Detailed diagnostics (orphaned vectors, broken edges) belong in a future `diagnose` tool. |
| `DbStatus` struct lives on `store.rs` not a separate file | It's a small diagnostic type tightly coupled to `DataStore`. |
| `list_sources` has no parameters | All sources are returned. At the expected scale (~50 sources max), pagination is unnecessary. |

## Risks and Caveats

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `list_chunks_by_path` returns many chunks for large documents | Medium | Large response may exceed LLM context | Add optional `limit` param to `get_document` (default 50) |
| `format_document_chunks` produces verbose output | Low | Degrades LLM response quality | Truncate content to first 200 chars in listing mode |
| `get_status` count queries are slow on large DBs | Low | Slow health check | CozoDB `::relations` already provides metadata; leverage if available |
| Tool descriptions may confuse agents on tool selection | Medium | Wrong tool chosen | Write descriptions that clearly distinguish when to use each tool |

## Plan Hardening Signals

* **Public API, schema, or contract change:** YES — adds 4 new MCP tools visible to all connected agents. Tool names and parameter schemas become a public contract.
* **Security, auth, permission, or compliance-sensitive behavior:** No — all tools are read-only against the local store. No new network exposure (STDIO-only transport unchanged).
* **Migration, backfill, destructive data/config action, or irreversible step:** No — purely additive. No schema changes. No data migration.
* **External integration, operator checkpoint, or external dependency:** No — no new crates or external services.
* **High runtime, rollout, or rollback risk:** No — new tools are independent. Old tools remain unchanged. Rollback = revert the commit.

**Requires plan hardening: no**

The public API addition is the only signal present, but it's purely additive (no breaking changes to existing tools), read-only, and follows established patterns exactly. Standard review is sufficient.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification |
|---|---|---|
| Unit 3 | MCP STDIO server | After build, start `graphtor-docs serve` and send JSON-RPC tool calls for each new tool. Verify well-formed responses. |
| All | Tool discovery | Verify `tools/list` JSON-RPC response includes all 7 tools with correct schemas. |

**Operational closure:** No monitoring, alerting, or rollback automation needed — this is a local-only developer tool. Verification is: build passes, tests pass, manual JSON-RPC smoke test confirms tools respond.

## Plan Review

**Reviewed:** 2026-05-01
**Gate Decision:** PASS
**Hardening required:** No (confirmed — purely additive read-only tools, no schema change)

### Reviewer Personas Executed

| Persona | Finding Count |
|---|---|
| Constitution Reviewer | 0 |
| Rust Reviewer | 1 (P3) |
| Scope Boundary Auditor | 0 |
| Learnings Researcher | 1 (P2) |
| Architecture Strategist | 0 |
| Agent-Native Parity Reviewer | 1 (P3) |

### Findings

#### P2 — Learnings Researcher

**CozoScript column-struct alignment (from compound library)**

The compound learning `cozo-query-columns-must-match-struct` documents a past bug where
query column order diverged from the row decoder's positional indexing. Unit 1's
`list_chunks_by_path` must ensure its CozoScript `?[...]` projection matches the existing
`row_to_chunk` column order exactly. The plan references reusing `row_to_chunk` but does not
explicitly call out the alignment constraint.

**Recommendation:** The implementing agent should reuse the identical column projection from
`list_chunks_for_source` (which already aligns with `row_to_chunk`) as the template for
`list_chunks_by_path`. Note this as a known-pattern guardrail in the task description.

#### P3 — Rust Reviewer

**`DbStatus` should derive standard traits**

The plan specifies `DbStatus` in `store.rs` but does not specify which derives it needs.
For consistency with other DB types (`ChunkRecord`, `SourceRecord`, `SearchResult`) and
future serialization needs, it should derive `Debug, Clone, PartialEq` at minimum, and
`serde::Serialize` for potential JSON-RPC use.

**Recommendation:** Advisory — include `#[derive(Debug, Clone, PartialEq, Serialize)]` in
the task description for Unit 1.

#### P3 — Agent-Native Parity Reviewer

**Tool naming: `get_chunk_by_id` vs `get_chunk`**

The `_by_id` suffix is redundant when the sole parameter is an ID. Existing tools use
descriptive names without redundant suffixes (`search_local_docs` not `search_local_docs_by_query`).
However, `get_chunk` might collide with a future `get_chunk_by_position` tool, so the current
naming is a defensible choice.

**Recommendation:** Advisory — retain `get_chunk_by_id` as proposed. The explicitness helps
agents disambiguate when multiple retrieval strategies exist.

### Gate Rationale

- No P0 or P1 findings
- Single P2 is a guardrail reminder, not a structural flaw — the code pattern is already established in the repo
- P3 items are advisory improvements, not blocking
- Plan hardening signals correctly assessed: one additive API signal, no breaking changes
- Runtime verification section is adequate for a local-only tool
- All five core principles satisfied (local-first, lightweight, pipeline integrity, MCP-native, automation)

**Decision: PASS — proceed to harvest.**
