---
title: Documentation Alignment Plan
source: stash/0E1FC522
status: draft
created: 2026-05-06
---

# Documentation Alignment Plan

## Problem Frame

Project documentation (README.md, AGENTS.md, CLI reference, MCP tool reference)
lags behind shipped features. Specifically:

1. **MCP tool count**: documentation says "7 tools" but the server registers 8
   (`research_topic` is undocumented)
2. **CLI reference**: missing `--json` global flag and `manifest` subcommand
   (shipped in 021-S / PR #36)
3. **README.md**: features section references outdated tool count; no mention of
   `--json` or `manifest`
4. **AGENTS.md**: still says "LocalDocRAG" in the Project Context heading; Domain
   Errors section references "LanceDB or Kùzu" (should be CozoDB); Database Access
   Rules reference `src/db/vector.rs` and `src/db/graph.rs` (stale paths — the
   current module structure is `src/db/{store,schema,chunks,nodes,edges,traverse,search}.rs`)

## Requirements Trace

| Requirement | Implementation action |
|---|---|
| All CLI commands documented | Add `--json` to global flags table; add `manifest` subcommand section |
| MCP tools reference is complete | Add `research_topic` tool entry; update count from 7 → 8 |
| README reflects current features | Update features list, tool count, add `--json`/`manifest` mention |
| AGENTS.md reflects current architecture | Replace LocalDocRAG → graphtor-docs; LanceDB/Kùzu → CozoDB; update db module paths |

## Implementation Units

### Unit 1: CLI Reference — `--json` flag and `manifest` subcommand

**Files:** `docs/cli-reference/graphtor-docs.md`

**Changes:**
- Add `--json` row to the Global Flags table (after `--db-path`)
- Add `### manifest` subcommand section (between `uninstall` and the end)
- Update `status` section to note that `--json` is now handled by the global flag
  (remove the per-command `--json` flag from the status flags table since it's global now)

**Verification:** Markdown renders without broken tables; all info matches `src/cli/mod.rs`

**Execution posture:** Direct documentation edit

---

### Unit 2: MCP Tool Reference — add `research_topic`, fix count

**Files:** `docs/mcp-tools.md`

**Changes:**
- Update frontmatter description and opening line: "7" → "8"
- Add `research_topic` to the Quick Selection Guide table
- Add full `### research_topic` section (between `traverse_doc_links` and `list_sources`)
  with parameters table, description, and example
- Verify all 8 tools are listed

**Verification:** Document lists exactly 8 tools matching `src/mcp/server.rs`

**Execution posture:** Direct documentation edit

---

### Unit 3: README.md — features, `--json`, `manifest`

**Files:** `README.md`

**Changes:**
- Update "7 MCP tools" → "8 MCP tools" in the Features section
- Add bullet: "**JSON-RPC 2.0 output** — `--json` global flag wraps all CLI
  output in JSON-RPC 2.0 envelopes for agent consumption"
- Add bullet or mention: "`manifest` subcommand mirrors MCP `tools/list`"
- Update MCP Tool Reference link text in docs table: "All 7 MCP tools" → "All 8 MCP tools"

**Verification:** Feature list is accurate and complete

**Execution posture:** Direct documentation edit

---

### Unit 4: AGENTS.md — rebranding and architecture alignment

**Files:** `AGENTS.md`

**Changes:**
- Replace "LocalDocRAG" with "graphtor-docs" in the Project Context section header
  and body text (line 30)
- Update Domain Errors section (line 115): "LanceDB or Kùzu operation failures"
  → "CozoDB operation failures"
- Update Database Access Rules section (lines 128-131): remove LanceDB/Kùzu
  references; replace with unified CozoDB/`src/db/` module structure matching
  the current layout (`store.rs`, `schema.rs`, `chunks.rs`, `nodes.rs`,
  `edges.rs`, `traverse.rs`, `search.rs`)
- Update "Last updated" date to 2026-05-06

**Verification:** No stale product names or architecture references remain

**Execution posture:** Direct documentation edit

---

## Dependency Graph

```text
Unit 1 (CLI ref)  ─┐
Unit 2 (MCP ref)  ─┼─→  No dependencies between units (all independent)
Unit 3 (README)   ─┤
Unit 4 (AGENTS)   ─┘
```

All 4 units are independent and can be executed in any order or in parallel.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Separate units per doc file | Width isolation — each unit touches exactly 1 file |
| Don't restructure docs layout | Scope control — this is an alignment chore, not a docs rewrite |
| Keep `research_topic` between `traverse_doc_links` and `list_sources` | Logical grouping: search tools first (search_local, semantic, research), then graph (traverse), then data access (list_sources, get_chunk, get_document, get_status) |
| Don't update `.github/copilot-instructions.md` | That file's Architecture Reference table already says CozoDB correctly; the stale references are only in AGENTS.md |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| CLI flags could change before merge | Verify against current `src/cli/mod.rs` at implementation time |
| MCP tool parameters could change | Verify against current `src/mcp/server.rs` at implementation time |
| Missing other stale references | Grep for "LocalDocRAG", "LanceDB", "Kùzu" across all docs before closing |

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | **No** | Documentation only; no code changes |
| Security, auth, permission, or compliance-sensitive | **No** | N/A |
| Migration, backfill, destructive data/config action | **No** | N/A |
| External integration, operator checkpoint | **No** | N/A |
| High runtime, rollout, or rollback risk | **No** | Markdown changes; easily reverted |

**Requires plan hardening: no**

## Runtime Verification and Closure

No runtime surfaces are changed by this documentation chore. The CLI and MCP
server behavior remain identical. Verification is limited to:

- Markdown renders correctly (no broken tables or links)
- All referenced files exist (cross-reference integrity)
- No `{{...}}` placeholders remain
- `grep -r "LocalDocRAG\|LanceDB\|Kùzu" docs/ README.md AGENTS.md` returns zero matches after completion

---

## Plan Review

**Gate Decision: PASS**

**Plan hardening required:** No (all 5 signals absent — docs-only chore, no runtime changes)

### Reviewer Personas Consulted

| Persona | Findings |
|---|---|
| Constitution Reviewer | 0 — no principle violations; docs-only chore |
| Rust Reviewer | 0 — no code changes proposed |
| Scope Boundary Auditor | 0 — width isolation satisfied (1 file/unit); 2-hour rule easily met |
| Learnings Researcher | 0 — plan is consistent with `keep-docs-synchronized-with-implementation.md` |
| Architecture Strategist | 0 — correctly identifies stale architecture references for remediation |
| Agent-Native Parity Reviewer | 0 — tool documentation will match `src/mcp/server.rs` source of truth |

### Findings

#### P3 — Advisory

**[P3-1] Placement rationale slightly inconsistent with stated grouping**

The Decisions table says research_topic goes "between traverse_doc_links and
list_sources" with rationale "search tools first (search_local, semantic,
research), then graph (traverse)…". This implies research_topic should precede
traverse_doc_links. However, the actual placement (after traverse_doc_links) is
functionally sound since research_topic is a composite tool bridging search and
traversal. The rationale text could be clearer but doesn't affect correctness.

**Recommendation:** At implementation time, consider placing research_topic after
`search_semantic` and before `traverse_doc_links` for stricter consistency with
the stated grouping. Either placement is acceptable.

### Compound Learnings Verified

- `keep-docs-synchronized-with-implementation.md` — plan follows "describe what
  exists" and remediates the doc-lag pattern the learning warns against.

### Summary

Plan is well-structured, narrowly scoped, and ready for harvest. All units are
independent with no blocking dependencies. Proceed to the harvest skill.
