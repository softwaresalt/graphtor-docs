---
doc_type: memory
source: stage-correction-session
title: Shipment 049-S Stage correction — six P1 plan-contract defects + P3
date: 2026-08-21
backlog_refs:
  - 049-S
  - 056-F
  - 7BF1961D
---

## Outcome

A new bounded Stage correction session (operator-directed continuation after the
prior three-cycle review-fix cap; fresh budget, not a hidden reset) corrected the
**six P1 plan-contract defects + one P3 advisory** that a report-only review
recorded against PR #106 / HEAD `4525cd0`. Only Stage-owned planning/backlog
artifacts were edited — **no source/tests, no `.mcp.json`, no `.backlogit/runtime/`,
no branches/commits/pushes, no GitHub comment interaction**. The user-owned
`.mcp.json` edit was preserved. A fresh current-HEAD report-only review must now
gate the corrected artifacts; **no fresh multi-persona PASS was claimed.**

## P1 / P3 dispositions

1. **P1-1 red-test polarity** — T1's sole pass assertion is a successful
   `initialize`; the reproduced pipe/exit/timeout is diagnostic evidence only.
2. **P1-2 H0a proof coupling** — T1 common transport harness greens `056.003-T`;
   a managed-launch integration test greens `056.008-T`; a migration test greens
   the new `056.009-T`. No single test proves an unrelated surface.
3. **P1-3 existing-install migration** — added `056.009-T` (idempotent,
   marker-safe `generate_mcp_config` refresh on `cmd_upgrade` and/or required
   reinstall recipe) so the reporter's already-installed workspace is repairable.
4. **P1-4 H0c closure** — added `056.010-T` (operational remediation of the
   evidenced fail-closed cause reaching a healthy handshake without weakening any
   gate); wired into `056.004-T`; deliberation gained an explicit H0c branch.
5. **P1-5 authorized-root / launch-cwd** — split trust roots: `cwd` authorized by
   equality to project root (not inside `.graphtor`); `--db-path`/`--config`
   validated as project-root-derived; explicit inputs validate against a
   target-derived root, not a foreign launch cwd. Copilot CLI stdio MCP supports
   `cwd`+`env` (verified), so the cwd-pin lever is viable.
6. **P1-6 live-lock age** — a matching pid + process-start-time identity stays
   live regardless of lock age; `STALE_SECS` age evicts only as a fallback when
   strong identity is unavailable (legacy pid-only), with concurrent-release
   NotFound preserved.
7. **P3 title** — `056.003-T` retitled to single-scope
   "Harden cmd_serve pre-serve workspace-root resolution (H0a-only, green)".

## Files modified

* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (Likely Surfaces table, T1/T2/T2d, new T2e/T2f, T4 deps, Test-First
  Expectations, Constitution Check III/IV + II + VI, Plan Hardening invariants
  1/3/5/6, Plan Review appendix + Stage correction session, scope → `056.010-T`).
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
  (Decision step 3: split-root wording, existing-install delivery, H0c branch).
* `.backlogit/queue/056.002-T.md`, `056.003-T.md` (+title), `056.004-T.md`
  (+deps `056.009-T`/`056.010-T`), `056.007-T.md`, `056.008-T.md`, `056-F.md` (DoD).
* Created `.backlogit/queue/056.009-T.md`, `056.010-T.md`.
* `docs/memory/2026-08-21/049-s-stage-review-cap-block-memory.md` (4→6 P1,
  removed duplicate H1 per Markdown rule).

## Final DAG

`056.001 → 056.002 → {056.003, 056.005, 056.006, 056.007, 056.008, 056.010}`;
`056.008 → 056.009`; `056.004` depends on `{056.003, 056.005, 056.006, 056.007,
056.008, 056.009, 056.010}`. Branch activation: H0a → 056.003 + 056.008 + 056.009;
H0b → 056.007; H0c → 056.010; H1 → 056.005; non-selected close *not-needed*.
Shipment `049-S` members: `056-F` + `056.001-T`..`056.010-T`.

## Engram evidence (ENGRAM_DIRECT=1)

`engram symbols --prefix cmd_` (cmd_install/cmd_install_full/cmd_upgrade),
`engram map-code generate_mcp_config` / `managed_server_value`,
`engram symbols --file src/lock.rs` + `--file src/workspace/upgrade.rs`,
`engram search` for the managed launch contract — each corroborated by exact
reads of `src/main.rs` (cmd_upgrade ~3480-3538, install call-sites ~3258/~3360,
discovery-vs-cwd ~2476-2499), `src/lock.rs` (is_stale_with_system ~472-481,
STALE_SECS=3600), `src/workspace/mcp_config.rs` (managed_server_value ~528-544).
External: GitHub Copilot CLI/SDK MCP docs confirm stdio `cwd`+`env` support.

## Validation

* `backlogit_sync_index` (474), shipment get, dependency/queue, `doctor` clean
  except a **pre-existing** orphan `013.008-T` (unrelated, out of scope).
* Markdown: no H1 in title-bearing docs (the 5 `# ` hits are shell comments in a
  fenced block); cross-refs resolve to exactly `056.001-T`..`056.010-T`.
* `docs_lint`: memory files valid. Plan + deliberation flag missing
  `doc_type`/`source` — a **systemic pre-existing** gap (0/31 exec-plans and
  0/25 decisions carry `doc_type`); left unchanged to avoid out-of-scope,
  sibling-inconsistent frontmatter churn.

## Next steps

1. Run a fresh current-HEAD report-only review to gate the corrected artifacts.
2. On `P0=0, P1=0`, update PR #106 readiness, obtain merge approval, merge to
   `main`, then route `049-S` to Ship for T0 evidence capture + test-first work.
3. The six open bot threads were addressed in-artifact but **not** replied to or
   resolved (per session constraints).
