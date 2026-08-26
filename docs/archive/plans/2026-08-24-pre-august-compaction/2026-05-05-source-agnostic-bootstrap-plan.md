---
title: "Source-Agnostic Rebranding & Session Bootstrap"
feature: 028-F
shipment: 019-S
tasks: [028.001-T, 028.002-T]
date: 2026-05-05
---

# Implementation Plan: Source-Agnostic Rebranding & Session Bootstrap

## Problem Frame

graphtor-docs is already source-agnostic in its implementation — MCP tool names
(`search_local_docs`, `search_semantic`, `traverse_doc_links`, `list_sources`,
`get_chunk_by_id`, `get_document`, `get_status`) and README.md are generic.
However, two developer-facing documentation files still reference **outdated
tool names** (`search_ms_docs_semantic`, `explore_concept_graph`) and
**MS-only project context** that misrepresents graphtor's actual capabilities.

Additionally, the `start.ps1` session bootstrap script launches Copilot without
pre-syncing the graphtor or backlogit indexes, meaning agents start with stale
data until they manually trigger sync.

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| Tool names in docs reflect actual code | Update AGENTS.md line 188, copilot-instructions.md line 253 |
| Project context is source-agnostic | Update AGENTS.md lines 30-40 to reflect multi-source support |
| Stale DB references corrected | Fix "LanceDB/Kùzu" → "CozoDB" in AGENTS.md line 39 |
| Session starts with fresh indexes | Add sync calls to start.ps1 before copilot launch |
| Sync failures don't block startup | Wrap sync calls with error handling (warn + continue) |

## Implementation Units

### Unit 1: Update documentation tool name examples (028.001-T)

**Changes:**
1. `AGENTS.md` line 188: Replace `search_ms_docs_semantic`, `explore_concept_graph`,
   `get_document_chunk` with actual tool names: `search_local_docs`,
   `search_semantic`, `get_chunk_by_id`
2. `.github/copilot-instructions.md` line 253: Replace
   `search_ms_docs_semantic`, `explore_concept_graph` with actual names
3. `AGENTS.md` lines 30-40: Update project context to reflect that graphtor
   supports git repos, local directories, AND web URLs (not just MS Docs)
4. `AGENTS.md` line 39: Fix "LanceDB (vector) and Kùzu (property graph)" →
   "CozoDB (embedded, sqlite backend)"
5. `AGENTS.md` line 188 convention note: Keep rmcp version reference at 1.5
   (already correct in copilot-instructions; AGENTS.md says 0.5 — fix)

**Files affected:**
- `AGENTS.md` (3 edits)
- `.github/copilot-instructions.md` (1 edit)

**Tests:** Documentation-only change — verify with `cargo check` (no compile
breakage) and manual review.

**Execution posture:** Direct edit — no test-first needed for documentation.

### Unit 2: Add pre-sync to start.ps1 (028.002-T)

**Changes:**
1. Insert sync block after environment setup (line 23) but before copilot
   launch (line 37):
   ```powershell
   # ── Pre-sync indexes ──────────────────────────────────────────────────────
   # Ensure graphtor and backlogit indexes are fresh before agent starts.
   # Failures are non-fatal — warn and continue.
   if (Get-Command "graphtor-docs" -ErrorAction SilentlyContinue) {
       Write-Host "[sync] graphtor-docs sync..." -ForegroundColor DarkGray
       graphtor-docs sync 2>&1 | Out-Null
       if ($LASTEXITCODE -ne 0) {
           Write-Warning "graphtor-docs sync failed (exit $LASTEXITCODE) — continuing"
       }
   }
   if (Get-Command "backlogit" -ErrorAction SilentlyContinue) {
       Write-Host "[sync] backlogit sync..." -ForegroundColor DarkGray
       backlogit sync 2>&1 | Out-Null
       if ($LASTEXITCODE -ne 0) {
           Write-Warning "backlogit sync failed (exit $LASTEXITCODE) — continuing"
       }
   }
   ```
2. Position: After `$env:GITHUB_TOKEN` assignment (line 23), before
   `$copilotExe` resolution (line 24).

**Files affected:**
- `start.ps1` (1 insertion block)

**Tests:** Script-only change — verify by running `start.ps1` manually and
confirming sync output appears. No cargo tests apply.

**Execution posture:** Direct edit.

## Dependency Graph

```text
Unit 1 (docs update) ─┐
                       ├── no dependency between units
Unit 2 (start.ps1)  ──┘
```

Units are independent and can be implemented in either order or in parallel.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Keep `search_local_docs` name (not shorten to `search_docs`) | The `local` prefix communicates that this is a local-first tool, reinforcing core principle I |
| Wrap sync with `Get-Command` check | Allows start.ps1 to work in workspaces where graphtor or backlogit isn't installed |
| Pipe sync output to `Out-Null` | Keeps startup clean; failures surface via `Write-Warning` only |
| Don't add `--quiet` flag to sync commands | Neither tool currently supports a quiet flag; redirecting stderr is sufficient |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| sync commands slow down startup | Both are incremental (< 5s typical); if slow, user can remove the block |
| backlogit or graphtor not on PATH | `Get-Command` check skips gracefully |
| AGENTS.md edits conflict with other branches | Low risk — main is clean, no active feature branches |

## Plan Hardening Signals

* public API, schema, or contract change: **NO** — MCP tool names in code are unchanged
* security, auth, permission, or compliance-sensitive behavior: **NO**
* migration, backfill, destructive data/config action, or irreversible step: **NO**
* external integration, operator checkpoint, or external dependency: **NO**
* high runtime, rollout, or rollback risk: **NO**

**Requires plan hardening: no**

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification |
|---|---|---|
| Unit 1 (docs) | None (documentation only) | Visual review of rendered markdown |
| Unit 2 (start.ps1) | CLI startup script | Run `.\start.ps1 --help` and confirm sync messages appear before copilot launches |

No operational closure artifacts needed — both changes are low-risk, instantly
reversible via git revert.

## Plan Review

**Gate Decision: PASS**

Reviewed by: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor,
Learnings Researcher, Architecture Strategist, Agent-Native Parity Reviewer.

### Findings

| # | Severity | Persona | Finding | Recommendation |
|---|----------|---------|---------|----------------|
| 1 | P3 | Learnings Researcher | Plan aligns with compound learning `keep-docs-synchronized-with-implementation.md` — docs should describe what exists, not planned names. | Supportive — no action needed. |
| 2 | P3 | Rust Reviewer | AGENTS.md line 197 says `rmcp` 0.5 but Cargo.toml shows 1.5. Plan should fix this as part of Unit 1. | Already covered by plan Unit 1 bullet 5. |
| 3 | P3 | Scope Boundary Auditor | Verify that no other files reference the old tool names (`search_ms_docs_semantic`) beyond AGENTS.md and copilot-instructions.md. | Grep confirmed — only those two files. |
| 4 | P3 | Architecture Strategist | The stale "LanceDB/Kùzu" reference is only in AGENTS.md (confirmed absent from copilot-instructions.md). Single edit point. | Covered by plan Unit 1 bullet 4. |

### Hardening Assessment

Plan explicitly states `Requires plan hardening: no` with all 5 signals absent.
Confirmed: no public API change, no security-sensitive behavior, no migrations,
no external integrations, no rollback risk. Hardening requirement satisfied.

### Gate Rationale

- Zero P0/P1 findings
- All P3 items are advisory and already addressed by the plan
- Both units respect width isolation (docs ≠ script)
- 2-hour rule satisfied (each unit < 30 minutes of effort)
- Plan is ready for harvest → execution
