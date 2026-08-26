---
type: session-memory
date: 2026-07-16
agent: ship
shipment: 046-S
pr: 94
merge_commit: fcd8358d69771cba416c9c5a57ede91f170a64ac
status: shipped-and-closed
---

# Session Memory — Shipment 046-S (docs refresh + brochure quick-start)

## Outcome

Shipped and closed the documentation-refresh shipment `046-S` for graphtor-docs
v0.3.1. PR #94 merged as merge commit `fcd8358` (P-009 merge-commit strategy),
branch `docs/refresh-brochure-quickstart` deleted (local + remote).

## Tasks Completed

Feature `053-F` + tasks `053.001-T` … `053.006-T` (all done/archived):

- 053.002-T — audit current docs vs v0.3.1 code (done earlier in session)
- 053.001/003/004/006-T — refreshed the 9 doc files across 4 parallel subagents
- 053.005-T — T6 consistency/accuracy/markdownlint/link gate

## Files Modified (committed on branch, merged to main)

- `README.md` — value-first brochure lead + quick-start (install → configure →
  sync → `search-semantic` → serve)
- `docs/architecture.md`, `docs/pipeline.md`, `docs/incremental-sync.md`,
  `docs/configuration.md`, `docs/source-registry-guide.md`,
  `docs/mcp-tools.md`, `docs/troubleshooting.md`, `docs/developer-guide.md`
- Commits: `4f9ffab` (docs), `5a1671f` (backlog task archival), `45c178a`
  (5 shadow-review accuracy fixes)

## Shadow Review

Copilot shadow review raised 5 inline accuracy comments — all verified TRUE
against source and fixed in `45c178a`. Each comment replied to and its thread
resolved via GraphQL `resolveReviewThread`. Re-review on new HEAD: "no new
comments." 0 unresolved threads.

## Key Accuracy Corrections (verified against code)

1. Incremental sync skips unchanged-mtime files → chunks indexed without an
   embedding model stay unembedded; must run `sync --full` to backfill
   (src/sync/mod.rs L120-121).
2. `status` shares `serve`'s DB auto-discovery of dropped `.db` files under
   `.graphtor/` (src/main.rs discover_served_databases), not `--db-path` only.
3. `type: database` read-only guarantee is conditional: co-targeting by a
   `type: local` source promotes it to read-write generation
   (src/workspace/serve_discovery.rs L247-252).
4/5. "Exact k-NN" is per-database; multi-database results merge round-robin
   (src/query/mod.rs merge_search_results L92, search_semantic L228), not a
   single global exact ranking.

## Closure

- GI/GR reconcile (shipment-reconcile safe-close): all 7 manifest items
  `pre-archived`; protected set empty (full-feature shipment); shipment record
  `046-S` archived as its own artifact (NOT cascade `ship_shipment`).
  Report: `.backlogit/reconcile/046-S-safe-close-20260716-182055.md`.
- CI: `detect code changes`=success, `build`=skipped (docs-only, expected).

## Remaining Backlog

- Stash: `970AE45A` (spike), `B88E37BF` (task), `5868A7C5` (task)
- Blocked: `013.008-T` — upstream RUSTSEC-2026-0041 (lz4_flex advisory,
  allowlisted in audit.toml)
