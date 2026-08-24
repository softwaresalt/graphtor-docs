---
doc_type: memory
source: stage-correction-session
title: Shipment 049-S Stage consensus-review remediation cycle 2
date: 2026-08-21
backlog_refs:
  - 049-S
  - 056-F
  - 7BF1961D
---

## Outcome

A second fresh bounded Stage correction session (remediation cycle 2) applied a
report-only remediation queue against the Consensus-cycle-1 artifacts on branch
`chore/stage-049-S` (HEAD `19c6a0d`, unpushed). Only Stage-owned
planning/backlog/docs-memory artifacts were edited — **no source/tests, no
`.mcp.json`, no `.backlogit/runtime/`, no branches/commits/pushes, no merge, no
GitHub thread interaction**. The user-owned modified `.mcp.json` and the
untracked `.backlogit/runtime/` were preserved untouched. **No fresh
multi-persona PASS is claimed**; a fresh current-HEAD report-only review must
gate the corrected artifacts before Ship.

All corrections were grounded in actual source via `ENGRAM_DIRECT=1` engram CLI
(unified search / symbols / map-code) plus exact file reads and a literal read
of the stash journal.

## Finding dispositions

1. **P1 H0c pre-v4 remediation → `sync`, not `upgrade` (APPLIED).**
   `workspace::upgrade::upgrade` (`src/workspace/upgrade.rs`) only replaces the
   `.graphtor/bin/` binary ("Preserves config and data directories"); it never
   rebuilds an index or touches schema. The serve gate (`open_serve_databases`)
   and status gate (`load_status_databases`) both emit *"has pre-v4 schema; run
   `graphtor-docs sync` to rebuild the index"*. The pre-v4→v4 rebuild lives in
   `src/sync/mod.rs::validate_and_apply_v4_migration` → `apply_v4_prune` →
   `prune_v4_data_for_rebuild` → `migrate_to_v4`. Fixed in plan T2f bullet,
   Likely-Surfaces T2f row, Risky-actions T2f (incl. "schema upgrade" →
   "pre-v4→v4 schema rebuild"), and `056.010-T`. Binary/config `cmd_upgrade`
   refresh (T2e/`056.009-T`) kept distinct; fail-closed gate left intact.
2. **P1 exit-site completeness → four sites (APPLIED).** Exact reads of
   `cmd_serve` enumerate four distinct pre-transport exit-2 sites: (1) missing
   explicit `--config`; (2) `served_paths.is_empty()`; (3)
   `classified.postures.is_empty()` after the phantom-default `retain` filter (a
   second, distinct "no databases found to serve" the old text conflated with
   site 2); (4) `primary` None (`stores.next()==None`). T2 and `056.003-T` now
   own all four with a red diagnostic test per site + serve-ready-log test;
   no discovery-signature / status-parity change (F4 preserved).
3. **P2 false H0a env/arg fallback removed (APPLIED).** Removed the claim that
   `GRAPHTOR_DB_PATH`/`GRAPHTOR_SOURCES`/explicit args substitute when the CLI
   ignores managed `cwd`. Runtime stays cwd-anchored by containment
   (`candidate_root = cwd`); a CLI ignoring `cwd` routes to H3 (`056.011-T`) or
   an explicit operational unsupported-client path. Fixed in plan T2d and
   `056.008-T`. Pinned-`cwd` lever + within-root complement retained.
4. **P2 verification consistency (APPLIED).** Verification Commands now annotate
   that `cargo test --test mcp_serve_handshake_test` is the reusable T1 harness
   (`056.002-T`) success scenario; H0a proof is `056.008-T`'s generated-contract
   test; `056.003-T` owns its diagnostic exit-site tests and does not green the
   raw no-target wrong-cwd `initialize`. Updated Verification Commands, T2,
   `056.003-T`.
5. **Conditional dependency claim = FALSE POSITIVE (DAG preserved).** Curatives
   depend on `056.002-T` → `056.001-T` (T0), so the T0 gate is transitively
   enforced. DAG unchanged; transitive gate documented for clarity.
6. **H3 pre-fix baseline added to Plan Hardening observation window (APPLIED)**
   to match T4/`056.004-T`.
7. **Cycle-3 "056.003 conditional H0a-only" marked superseded in place
   (APPLIED)** — inline marker points to Consensus cycle 1 (056.003 is now
   NON-conditional); history retained.
8. **Plan Review current status de-HEAD-anchored (APPLIED)** — names "next
   committed HEAD" instead of a stale hard-coded HEAD.
9. **Stash journal (VERIFIED — PRESERVED).** Literal read confirms `7BF1961D`
   present exactly once at `.backlogit/archive/stash.jsonl:51`. Adversarial
   presence claim verified; the "absent" claim is the false positive. Left
   as-is; no duplication, no backlogit repair.
10. **Rust guidance (APPLIED to task notes).** `056.008-T`: compare parsed
    `serde_json::Value`, not raw bytes/order; preserve `is_exact_legacy_shape`.
    `056.007-T`: debug diagnostic on legacy start-time-less lock parse fallback.
    `056.003-T`: DRY loud-message formatting, no scope creep.
11. **False positives preserved unchanged.** `056.008-T` dep on `056.002-T`; no
    target self-authorization; no split-root helper/signature change; pre-existing
    `013.008-T` orphan, unrelated stale `.lock` files, and symlink-write backlog
    items (out of 049-S scope) left untouched.

## Grounded accuracy fix (beyond the queue)

`acquire_database_lock` IS a real symbol (`src/main.rs` ~2803-2824) wrapping
`workspace::lock::DatabaseLock::acquire` — the prior "there is no
`acquire_database_lock` symbol" claim was false. Corrected in `056.001-T`,
`056.003-T`, `056.007-T`; the H0b liveness change still lands in the
`src/lock.rs` primitives, not the wrapper.

## Files modified

- `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (T2 four sites + per-site tests; T2d false-fallback removal; T2f + Risky
  actions pre-v4 `sync`; Verification Commands annotation; H3 observation
  baseline; current-status de-HEAD; line-919 four sites; Cycle-3 superseded
  marker; new "Consensus review remediation cycle 2" section)
- `.backlogit/queue/056.010-T.md` (pre-v4 `sync`; safety wording)
- `.backlogit/queue/056.003-T.md` (four exit-2 sites + per-site tests; DRY;
  `acquire_database_lock` fix)
- `.backlogit/queue/056.008-T.md` (env-fallback removal; serde_json::Value note)
- `.backlogit/queue/056.007-T.md` (`acquire_database_lock` fix; debug-diagnostic
  note)
- `.backlogit/queue/056.001-T.md` (`acquire_database_lock` fix)
- `updated_at` bumped to `2026-08-22T01:20:00Z` on the five edited task files

## Engram evidence (ENGRAM_DIRECT=1)

- `engram symbols --prefix cmd_` → `cmd_serve` (main.rs:2446-2654), `cmd_upgrade`
  (3480-3538), `cmd_sync` (441-601)
- `engram map-code cmd_upgrade` → `workspace::upgrade::upgrade`
- `engram map-code needs_v4_migration` / `validate_and_apply_v4_migration` → v4
  rebuild owned by `src/sync/mod.rs` + `src/db/schema.rs::apply_v4_prune`
- `engram symbols --prefix acquire_` + `map-code acquire_database_lock` →
  `src/main.rs` ~2803-2824 wrapping `DatabaseLock::acquire`
- `engram search` for the pre-v4 gate message → serve/status gates instruct
  `graphtor-docs sync`
- Exact reads: `src/main.rs` (`cmd_serve` 2446-2660, `open_serve_databases`
  2370-2443, `load_status_databases` 2760-2801), `src/workspace/upgrade.rs`;
  literal read `.backlogit/archive/stash.jsonl:51`

## Validation

- backlogit `sync_index`, `doctor`, `get_queue`, `get_shipment 049-S`,
  dependency graph — see session output
- Markdown/frontmatter/cross-refs re-checked; plan stays CRLF, task files LF

## Next steps

- Fresh current-HEAD report-only multi-persona review must gate the corrected
  artifacts before Ship (PR #106 readiness remains BLOCKED).
- No commit/push performed this session (Stage role boundary).
