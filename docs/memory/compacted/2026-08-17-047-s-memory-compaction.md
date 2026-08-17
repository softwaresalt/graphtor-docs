---
type: compaction-report
date: 2026-08-17
target: memory
context: "047-S post-merge closure compaction — shipment complete, build checkpoint superseded by closure artifacts"
---

# Compaction Report — 047-S (2026-08-17)

## Trigger

Ship Step 6.8 mandatory `compact-context` invocation at post-merge closure
for shipment `047-S`. `docs/memory/` was at 26 files / 127.5 KB (well under
the 40-file / 500 KB numeric thresholds), but compaction after a completed
release unit is a required workflow step regardless of threshold state.

## Consolidated Summary

Shipment `047-S` ("Read-only serve guarantee honesty F2/F6") completed in
full: implementation (`054.001.001-ST` RED→`054.001.002-ST` GREEN),
release-observability evidence (`054.001.003-ST`), design-doc correction
(`054.002-T`), 6-reviewer adversarial review (0 P0/P1) plus 4 rounds of
Copilot shadow review (3 substantive review-fix cycles applied; the 4th
round's findings were dispositioned as follow-ups rather than a 4th fix
cycle, per the 3-cycle cap), CI green, merged as PR #97
(`704b95a6c1e2930079d6f3a602ab66e9682d4916`), and safely archived (single
-artifact safe-close, protected set empty — full-feature shipment).

Key decisions and learnings (full detail retained in
`docs/closure/2026-08-17-047-s-post-merge-closure.md`,
`docs/closure/2026-08-17-047-s-release-observability-evidence.md`, and the
two `docs/compound/` entries this shipment produced/updated):

* `is_engine_enforced_readonly()` kept meaning `guard.is_some()` — NOT
  overloaded to `access_mode == ReadOnly` (rejected in the exec-plan's own
  round-1 review as *more* deceptive).
* `EngineReadonlyGuard::lock`/`Drop` bodies never modified.
* F6 (cross-process/multi-guard restore-ordering) documented as an honest,
  best-effort residual across the log constant, 3 rustdocs, the guard
  struct doc, and the design doc — not closed, not formally perfected to
  every edge case (an explicit, reasoned stopping point after 4 Copilot
  review rounds showed non-convergent finding counts: 9→4→2→10).
* A `tracing` callsite-interest-cache race under parallel `cargo test`
  required a bounded-retry + `EnvFilter` + `rebuild_interest_cache()`
  combination to reliably capture log output in tests — captured as a new
  compound learning.
* A pre-existing, unrelated `cargo audit` gap (`RUSTSEC-2026-0249`,
  `smartstring`) was discovered and fixed as necessary CI-unblocking
  maintenance, disclosed explicitly in the PR — appended to the existing
  compound learning on cargo-audit allowlist maintenance.
* 5 follow-up items stashed for Stage triage (2 pre-existing/unrelated
  security-adjacent hygiene items, 2 stowaway config-drift instances, 1
  optional further F6 wording refinement) — see stash IDs `9CEC208C`,
  `C365AB98`, `3FFE51B4`, `B883681D`, `B8C0851E`.

## Action

* Archived the build-phase checkpoint
  `docs/memory/2026-08-17/047-s-build-checkpoint-pre-pr.md` (superseded by
  the post-merge closure artifact above, which is the durable, more complete
  record) to `docs/archive/memory/2026-08-17/`.
* Left `docs/memory/2026-08-16/dark-stage-session-complete-memory.md` in
  place — it covers staging context shared with the still-queued `048-S`
  shipment and is not yet safe to archive.

## Result

* One superseded build checkpoint archived to `docs/archive/memory/`, and
  this compaction report added to `docs/memory/compacted/` — a like-for-like
  move plus one durable summary, not a net reduction in `docs/memory/`
  content.
* Durable record for `047-S` now lives in `docs/closure/` (post-merge
  closure + release-observability evidence) and `docs/compound/` (2
  learnings), per the Durable Knowledge Layout convention.
