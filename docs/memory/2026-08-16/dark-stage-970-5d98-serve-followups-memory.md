---
title: "Dark-mode Stage session — 970AE45A / 5D98DBCC + serve auto-discovery follow-ups"
type: session-memory
date: 2026-08-16
agent: Stage
mode: P-017 dark-factory (DARK_MODE_ACTIVE)
tags:
  - stage
  - dark-mode
  - read-only-serve
  - workspace-containment
  - serve-discovery
---

## Scope

Dark-mode Stage run over four scoped stash entries. Operator AFK; merge
pre-authorized for this bounded scope; Stage must not create/push/merge PRs;
admin fallback not authorized; intercom unavailable (degraded local visibility
logged).

* `970AE45A` (spike, medium) — read-only-serve cross-process coordination design (F2/F6). Group A leader.
* `5D98DBCC` (feature, medium) — shared external read-only DBs via `read-sources.yaml`. Group A. Touches NON-NEGOTIABLE Principle III/IV.
* `B88E37BF` (task, low) — short-circuit `source_has_ingestible_content`. Group B.
* `5868A7C5` (task, low) — served-alias canonicalization evaluation. Group B.

## Environment

* Capability packs: agent-intercom (UNAVAILABLE — no MCP surface), agent-engram (OK, workspace bound + indexed), backlogit (OK v1.9.0), adversarial-review (alt provider google/gemini configured), graphtor-docs (server MCP surface unavailable — file-based doc fallback).
* Model routing: tier1 haiku, tier2 sonnet-5, tier3 opus-4.8; adversarial alt = google/gemini-2.5-pro.

## Decisions and rationale

* **Incidental index repair (logged, autonomous):** Step 0.1 `backlogit_sync_index` failed on a pre-existing malformed archived artifact `.backlogit/archive/013-S.md` (`shipped_at`/`pr`/`merge_commit` mis-indented under scalar `status: done`). A broken canonical index risks harvest ID collisions across the 439-item archive and could hard-block `create_item`. Repaired minimally by relocating the three keys into `custom_fields` (canonical location, matches 045-S/046-S). Re-sync OK (443 indexed). Judged operational unblocking of a mandatory gate, within Stage backlog authority — not product-scope expansion.
* **Grouping (operator-directed, evidence-consistent):** Group A = 970AE45A → 5D98DBCC (security/reliability chain; spike gates feature). Group B = B88E37BF + 5868A7C5 (both `src/workspace/serve_discovery.rs`, PR90 deferrals, low-risk).
* **Group A direction:** external-path containment relaxation conflicts with NON-NEGOTIABLE Principle III/IV; drive spike + deliberation toward a constitution-compliant interoperable alternative; do not create an unsafe implementation shipment.

## Code grounding (read-only)

* `src/path/security.rs:143` `validate_path` — canonicalize + `starts_with(root)`; the Principle III/IV enforcement point the feature would bypass.
* `src/workspace/serve_discovery.rs:91` `discover_served_databases` — layered containment; doc: "External-path support is explicitly out of Phase-1 scope."
* `src/workspace/serve_discovery.rs:333` `source_has_ingestible_content` — B88E37BF target (WalkDir + batch `filter_files`).
* `src/db/store.rs:197` `open_engine_readonly`, `:472` `EngineReadonlyGuard` (+ Drop `:582`) — per-process, non-refcounted FS-attribute lock; app-level `AccessMode` primary; `open_sqlite` self-heals stale locks.
* Trust-boundary doc claims "every connection — including later pool refills — is genuinely denied write access at the OS/filesystem level" — the exact guarantee the F6 cross-process gap invalidates.

## Prior art consulted

* `docs/compound/git-pull-blocked-by-sqlite-wal-lock.md` — `.db-wal`/`.db-shm` are ephemeral cache artifacts; cross-process file/lock semantics fragile.
* `docs/decisions/2026-05-22-multi-database-file-support-deliberation.md` (R4 cross-project reuse), `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md` (external-path deferral).

## Next steps

* Write spike (970), deliberation A (5D98 — reject external-path, reshape), deliberation B (Group B), impl-plans, run plan-review + adversarial (3+ cross-model), harvest, shipments, archive stash, commit Stage artifacts.
