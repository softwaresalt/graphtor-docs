---
doc_type: memory
source: stage-correction-session
title: Shipment 049-S Stage consensus-review remediation cycle 3 (final)
date: 2026-08-21
backlog_refs:
  - 049-S
  - 056-F
  - 7BF1961D
---

## Outcome

The third and **final** fresh bounded Stage correction session (hard review-fix
cap) applied a report-only remediation queue against the Consensus-cycle-2
artifacts on branch `chore/stage-049-S` (reviewed input HEAD `b6133ed`,
unpushed). Only Stage-owned planning/backlog/docs-memory artifacts were edited —
no source/tests, no `.mcp.json`, no `.backlogit/runtime/`, no
branches/commits/pushes, no merge, no GitHub thread interaction. The user-owned
modified `.mcp.json` and the untracked `.backlogit/runtime/` were preserved
untouched. No fresh multi-persona PASS is claimed; a fresh current-HEAD
report-only review must gate the corrected artifacts before Ship.

All corrections were grounded in actual source via `ENGRAM_DIRECT=1` engram CLI
(workspace-status / symbols / map-code) plus exact file reads.

## Finding dispositions

1. P1-1 (BLOCKING) — H3 expanded to two-mode client/transport compatibility
   (APPLIED). The prior text routed a client that ignores/rejects the managed
   `cwd` to H3, but H3's discriminator assumed the child stays alive, whereas an
   ignored `cwd` makes the managed-launch child start in a foreign cwd and
   early-exit — a contradiction. `056.011-T` (T-H3) now owns client/transport
   compatibility with two modes: mode A (framing/version — child alive,
   `initialize` never negotiates → rmcp bump / minimal framing fix, observed-red
   handshake test) and mode B (client ignores/rejects the pinned `cwd` →
   managed-launch early exit → an evidence-backed client-compatibility
   adjustment: a supported CLI version or a client-honored working-directory
   mechanism, verified by a manual compatibility check — no server-side
   external-path fallback, containment unchanged F1/F2/F3/N1). H3 is
   distinguished from H0a by generated-contract / client-capability evidence.
   Updated: plan Likely Surfaces (T-H3 + Tests rows), T-H3 section, T2d routing,
   T4 baseline, observation-window baseline + rollback trigger, Risky actions
   (T-H3), Rollback/Compatibility, Constitution II/VI, Test-First Expectations;
   deliberation H3 row / Decision / residual; `056.011-T`, `056.008-T`,
   `056.001-T`, `056.004-T`.
2. P1-2 — branch-sensitive T2c sink verification (APPLIED). When the T2c sink
   (`056.006-T`) is activated because the CLI discards child stderr, T4
   reads/validates the configured env-gated sink file (e.g. under
   `.graphtor/logs/`) instead of `logs/serve-stderr.log` (impossible on that
   branch); the normal branches keep the stderr redirect. Updated: plan T4 +
   T2c + observation window + Verification Commands; `056.004-T`, `056.006-T`.
3. P1-3 — `cmd_upgrade` canonical-project-root derivation (APPLIED). Engram +
   exact reads confirmed `find_workspace_dir` (`src/workspace/paths.rs:37-63`)
   returns the `.graphtor` directory itself, `project_root` is its `.parent()`,
   and `generate_mcp_config(project_root)` does `project_root.join(".mcp.json")`
   + validates the binary within `project_root`; `cmd_install`/`cmd_install_full`
   pass their `cwd`. `056.009-T`'s `cmd_upgrade` refresh must pass the canonical
   project root = the located `.graphtor` parent (`workspace_dir.parent()` /
   `workspace::paths::project_root`) — never the nested invocation `cwd`, never
   `.graphtor` itself — with a nested-subdirectory invocation red test and
   marker-safe user-entry preservation. Updated: plan Likely Surfaces (T2e row),
   T2e section, Risky actions (T2e); `056.009-T`.
4. P2 — raw-harness H0a success-list correction (APPLIED). The Verification
   Commands no longer claim the raw `mcp_serve_handshake_test` success scenario
   is greened by `H0a 056.008-T`; the raw harness greens H0b/H0c/H1/H3 mode A,
   H0a is proven by `056.008-T`'s generated-contract integration test under
   `cargo test --all-targets`, and H3 mode B by that generated-contract launch +
   a manual check. Updated Verification Commands and Constitution II.
5. P2 — H0c destructive/approval-gated acknowledged (APPLIED). Constitution
   Check VII and the Plan Hardening Signals migration/destructive signal no
   longer read flat "none/absent"; they acknowledge the conditional H0c
   remediation (`056.010-T`) can require a pre-v4→v4 schema rebuild via
   `graphtor-docs sync` or a source-registry replacement — high-risk,
   approval-gated, backup-first, never a fail-closed-gate weakening. (`056.010-T`
   already carried ActionRisk high + operator approval at task level; left
   as-is.)
6. P2 — `056.003-T` table-driven diagnostic matrix (APPLIED, supersedes F9
   per-site granularity). One table-driven red diagnostic matrix (four exit-2
   rows + one serve-ready-log row), every semantic site preserved, and the
   existing `tests/explicit_db_target_no_registry_test.rs` negative "config
   file" assertion (verified present) preserved while wording new messages.
   Updated plan T2 + Test-First Expectations; `056.003-T`.
7. P2 — `056.007-T` forward-compat lock test (APPLIED). Added an observed-red
   test that a lock file with an unknown extra field parses without error
   (forward-compatible, no `deny_unknown_fields`), alongside the existing
   start-time / legacy / pid-reuse / live-long-running tests; matching
   pid + start-time identity stays live regardless of age. Updated plan T2b;
   `056.007-T`.
8. P2 — `Cargo.toml` rmcp anchored by dependency name (APPLIED). The T-H3
   surface and Risky action reference `Cargo.toml` `[dependencies]` `rmcp` pin,
   not the brittle line ~44.
9. Plan Review status/identity bound to `b6133ed` (APPLIED). Current-status and
   "Reviewed artifact identity" now name the last committed HEAD `b6133ed` as
   the reviewed input carrying findings, with the corrected artifacts'
   report-only gate PENDING against the next committed HEAD — explicitly not a
   PASS. Frontmatter `status` kept `reviewed` (sibling exec-plans use only
   `draft` / `reviewed` / `shipped`; no `pending` status exists, so none was
   invented — the body carries the authoritative not-a-PASS state).

## Preserved dispositions / false positives (unchanged)

- Stash journal (VERIFIED — PRESERVED). `7BF1961D` present exactly once
  (`.backlogit/archive/stash.jsonl:51`); not duplicated (F16 stands).
- `049-S` frontmatter-only. The advisory body/items suggestion is
  generated-manifest formatting; backlogit standard shipment format is
  frontmatter-only (items in `custom_fields.items`), so no body was hand-woven.
- `056.008-T` parsed `serde_json::Value` equality + `is_exact_legacy_shape`
  preservation retained; its `056.002-T` dependency retained (refuted claim).
- Cycle-3 "056.003 conditional H0a-only" superseded marker (F14) retained.
- Staging is planning-only: absent implementation / evidence-pending task work
  is NOT treated as a review defect.
- Out of 049-S scope, untouched: the pre-existing `013.008-T` orphan, unrelated
  stale `.lock` files in the queue, and pre-existing symlink-write backlog items.

## Grounded accuracy facts (this cycle)

- `find_workspace_dir(start_dir)` (`src/workspace/paths.rs:37-63`) returns the
  `.graphtor` directory itself, not the project root; `project_root(cwd)` =
  `find_workspace_dir(cwd).parent()`.
- `generate_mcp_config(project_root)` (`src/workspace/mcp_config.rs`) joins
  `project_root/.mcp.json` and validates the binary within `project_root`;
  `cmd_install` (~3258) / `cmd_install_full` (~3360) pass `cwd`; `cmd_upgrade`
  (~3480-3538) resolves `workspace_dir = find_workspace_dir(cwd)` and never
  rewrites `.mcp.json` today.

## Files modified

- `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (Likely Surfaces T-H3/T2e/Tests rows; T2b, T2c, T2d, T-H3, T4 sections;
  Verification Commands; Rollback/Compatibility; Constitution II/VI/VII; Plan
  Hardening Signals + Risky actions (T2e/T-H3) + observation window; Test-First
  Expectations; Plan Review current-status + identity + new "Consensus review
  remediation cycle 3 (final)" section).
- `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
  (H3 row two modes; Decision step 3 H3 mention; final paragraph; residual risk).
- `.backlogit/queue/056.001-T.md`, `056.003-T.md`, `056.004-T.md`,
  `056.006-T.md`, `056.007-T.md`, `056.008-T.md`, `056.009-T.md`, `056.011-T.md`
  (see dispositions above). `updated_at` bumped to `2026-08-22T02:05:00Z` on the
  eight edited task files.
- Created this memory file.
- `056.010-T.md`, `056-F.md`, `049-S.md` unchanged (already correct).

## Final DAG (unchanged this cycle)

`056.001 → 056.002 → {056.003, 056.005, 056.006, 056.007, 056.008, 056.010,
056.011}`; `056.008 → 056.009`; `056.004` depends on `{056.003, 056.005,
056.006, 056.007, 056.008, 056.009, 056.010, 056.011}`. `056.003` is
non-conditional diagnostics (always lands). Curative branches: H0a → 056.008 +
056.009; H0b → 056.007; H0c → 056.010; H1 → 056.005; H3 → 056.011 (mode A
framing / mode B client ignores the pinned `cwd`); non-selected close
not-needed. Shipment `049-S` = `056-F` + `056.001-T`..`056.011-T`.

## Engram evidence (ENGRAM_DIRECT=1)

- `engram workspace-status` → bound; 1307 files scanned; not stale.
- `engram symbols --prefix find_workspace` → `find_workspace_dir`
  `src/workspace/paths.rs:37-63`.
- `engram map-code find_workspace_dir` → `project_root`; `engram map-code
  generate_mcp_config` → callers `cmd_install` / `cmd_install_full`.
- Exact reads: `src/workspace/paths.rs`, `src/workspace/mcp_config.rs`,
  `src/main.rs` (`cmd_install` ~3258 / `cmd_install_full` ~3360 / `cmd_upgrade`
  ~3480-3538), `Cargo.toml` (`rmcp` in `[dependencies]`),
  `tests/explicit_db_target_no_registry_test.rs`.

## Validation

- `backlogit_sync_index` → 475 indexed.
- `backlogit_doctor` → clean except the pre-existing, out-of-scope `013.008-T`
  orphan (no duplicate IDs, no new findings).
- `backlogit_get_shipment 049-S` → covering feature `056-F`, 11 task members
  intact; shipment frontmatter-only (no hand-woven body).
- Dependency graph (`item_deps`) verified: DAG matches the authoritative
  Consensus-cycle-1 topology; no membership or edge change.
- `backlogit_docs_lint` (plan + deliberation) → flags missing `doc_type` /
  `source` only — the systemic pre-existing gap (0/31 exec-plans, 0/25 decisions
  carry `doc_type`); left unchanged to avoid out-of-scope, sibling-inconsistent
  frontmatter churn. Memory files carry `doc_type` / `source` and lint clean.
- Markers/cross-refs re-checked; edited task files keep BEGIN/END section
  markers.

## Next steps

1. Run a fresh current-HEAD report-only multi-persona review to gate the
   corrected artifacts (PR #106 readiness remains BLOCKED).
2. On `P0=0, P1=0`, update PR #106 readiness, obtain merge approval, merge to
   `main`, then route `049-S` to Ship for T0 evidence capture + test-first work.
3. No commit/push performed this session (Stage role boundary).
