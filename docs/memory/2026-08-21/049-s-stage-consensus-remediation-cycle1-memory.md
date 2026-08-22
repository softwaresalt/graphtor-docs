---
doc_type: memory
source: stage-correction-session
title: Shipment 049-S Stage consensus-review remediation cycle 1
date: 2026-08-21
backlog_refs:
  - 049-S
  - 056-F
  - 7BF1961D
---

## Outcome

A fresh bounded Stage correction session (remediation cycle 1) applied the
deduplicated queue from a **3-model adversarial consensus review** against PR
#106 / HEAD `22d18f1`. Only Stage-owned planning/backlog/docs-memory artifacts
were edited — **no source/tests, no `.mcp.json`, no `.backlogit/runtime/`, no
branches/commits/pushes, no GitHub comment interaction**. The user-owned
`.mcp.json` edit and the untracked `.backlogit/runtime/` were preserved. **No
fresh multi-persona PASS is claimed**; a fresh current-HEAD report-only review
must gate the corrected artifacts before Ship.

The headline change **reverses** the prior Stage correction session's
split-root / target-derived authorization (P1-5): no target self-authorizes.

## Consensus finding dispositions

1. **F1/F2/F3/N1 (BLOCKING containment)** — removed all split-root /
   target-derived authorization and every prescription to call
   `project_root`/`find_workspace_dir` on an explicit target. Runtime
   `cmd_serve` keeps validating explicit `--db-path`/`--config` against the
   authorized project-root cwd via the shared `discover_served_databases` /
   `validate_path` / `is_reparse_point` primitives (`candidate_root = cwd`);
   H0a connectivity is owned solely by `056.008-T` (pins child cwd to the
   canonical project root) + `056.009-T` delivery. `056.003-T` retitled and
   re-scoped to non-conditional diagnostics (loud exit-2 errors + serve-ready
   log) with its own red→green test; it no longer claims to green a no-target
   wrong-cwd managed launch.
2. **F4 (status parity)** — chosen scope: no discovery signature change, so
   `discover_served_databases` / `classify_serve_postures` /
   `discover_status_db_paths` / `cmd_status` parity is untouched, no new test.
3. **F6 (H3 branch)** — created queued conditional task `056.011-T` (rmcp /
   client transport compatibility, H3-only, low-confidence but live); wired to
   `056.002-T`, `049-S`, and `056.004-T`; branch taxonomy/T4/baseline and the
   deliberation updated.
4. **F7 (config schema)** — extended T0 `056.001-T` to record the exact Copilot
   CLI MCP config schema (`type` vs `transport`; `cwd`/`env`). `056.008-T` emits
   the evidenced field, preserving marker/legacy recognition. Evidence: local
   `.mcp.json` siblings use `type: "stdio"` + `env`.
5. **S1 (migration)** — `056.009-T` makes the marker-safe `cmd_upgrade` refresh
   the primary code acceptance with an observed-red migration test; reinstall is
   a manual fallback/rollback only.
6. **S7 (hardening)** — added ProposedAction/ActionRisk/rollback entries for
   T2e (`056.009-T`) and T2f (`056.010-T`, backups + operator approval + no
   fail-closed weakening) in the plan and task Safety bullets.
7. **F5 (stale wording)** — removed remaining "cwd inside `.graphtor`" /
   "cannot escape foreign launch cwd" requirements; authoritative rule: generated
   cwd = canonical project root; file targets project-root-derived, validated
   against the project root; no external-path capability.
8. **Per-surface test-first** — observed-red tests added/confirmed for
   `056.003` (serve-ready-log/loud-error), `056.006` (sink), `056.007`
   (pid-reuse/live/legacy), `056.008` (generated contract), `056.009` (upgrade
   migration), `056.011` (H3). T1 sole pass assertion = successful `initialize`
   preserved; diagnostics explain red only.
9. **Template/tooling** — added BEGIN/END description markers to `056.008-T`;
   updated plan current-status to HEAD `22d18f1` (re-review pending); added the
   Consensus cycle 1 review appendix section.
10. **`056.003` title/body** narrowed; no sink promise.
11. **Out of scope (untouched):** the pre-existing `013.008-T` orphan, unrelated
    stale lock files, and pre-existing symlink-write backlog items.

**Preserved false-positive dispositions:** stash journal left as-is
(missing-journal claim refuted); `056.008-T` keeps its `056.002-T` dependency
(unnecessary-dependency claim refuted).

## Files modified

* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (Likely Surfaces, T0 F7, T1 coupling, T2 rewrite, T2d/T2e, new T-H3, T4,
  Rollback, Constitution II/III/IV/VI, Plan Hardening Signals + intro +
  invariants (1)(5), Risky actions T2/T2d + new T2e/T2f/T-H3, Test-First
  Expectations, Plan Review current-status + Consensus cycle 1 section + DAG).
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
  (Decision step 3 split-root removal, H3 owner `056.011-T`, F7 open question).
* `.backlogit/queue/056.001-T.md` (F7 schema + recipes), `056.002-T.md`
  (curative-branch coupling), `056.003-T.md` (retitle + diagnostics scope),
  `056.004-T.md` (deps + branch taxonomy), `056.006-T.md`, `056.007-T.md`
  (observed-red tests), `056.008-T.md` (description markers + F7 + split-root
  removal), `056.009-T.md` (S1), `056.010-T.md` (S7), `056-F.md` (DoD).
* Created `.backlogit/queue/056.011-T.md`; `049-S.md` membership +1.

## Final DAG

`056.001 → 056.002 → {056.003, 056.005, 056.006, 056.007, 056.008, 056.010,
056.011}`; `056.008 → 056.009`; `056.004` depends on `{056.003, 056.005,
056.006, 056.007, 056.008, 056.009, 056.010, 056.011}`. `056.003` is
non-conditional diagnostics (always lands). Curative branches: H0a → 056.008 +
056.009; H0b → 056.007; H0c → 056.010; H1 → 056.005; H3 → 056.011; non-selected
close *not-needed*. Shipment `049-S` members: `056-F` + `056.001-T`..`056.011-T`.

## Engram evidence (ENGRAM_DIRECT=1)

`engram workspace-status` (bound, 1307 files scanned, not stale);
`engram symbols --prefix discover_` / `--prefix classify_`;
`engram map-code discover_served_databases` (confirmed `candidate_root` =
project-root validation of explicit `--db-path`; shared `validate_path` /
`is_reparse_point` guards) and `classify_serve_postures`;
`engram map-code discover_status_db_paths`. Corroborated by exact reads of
`src/main.rs` (`cmd_serve` ~2446-2520, `discover_status_db_paths` ~2664-2720),
`src/workspace/serve_discovery.rs`, `src/workspace/mcp_config.rs`
(`managed_server_value` ~526-544, `is_exact_legacy_shape`), `Cargo.toml`
(`rmcp = "1.5"`), and read-only `.mcp.json` (F7 `type`/`env` siblings).

## Validation

* `backlogit_sync_index`, `doctor` (clean except the pre-existing, out-of-scope
  `013.008-T` orphan), shipment/dependency/queue checks, `docs_lint`, and
  Markdown/frontmatter/cross-reference checks — see the session summary.

## Next steps

1. Run a fresh current-HEAD report-only review to gate the corrected artifacts.
2. On `P0=0, P1=0`, update PR #106 readiness, obtain merge approval, merge to
   `main`, then route `049-S` to Ship for T0 evidence capture + test-first work.
3. The open bot threads were addressed in-artifact but not replied to or
   resolved (per session constraints).
