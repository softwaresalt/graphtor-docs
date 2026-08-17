---
date: 2026-08-17
slug: 047-s-release-observability-evidence
shipment: 047-S
mode: release-observability
status: READY
owner: copilot
---

# Release-Observability Evidence — 047-S Read-Only Serve Guarantee Honesty

Produced by subtask `054.001.003-ST` for the Unit A1 runtime-surface change:
the `open_engine_readonly` startup-log wording correction
(`src/db/store.rs`, `open_engine_readonly`). This is releasability evidence
for shipment `047-S`, carried forward verbatim into the
`operational-closure` artifact rather than duplicated there.

## Scope of the Change Being Observed

* **Surface**: a single `tracing::info!` log line emitted once per
  `DataStore::open_engine_readonly` call, at `serve` startup when a database
  resolves to `ReadOnly` posture (engine/filesystem-enforced read-only path).
* **Not in scope**: no `EngineReadonlyGuard::lock`/`Drop` change, no CLI flag
  change, no schema change, no guard runtime-behavior change. The revert is
  text-only (see Rollback below).

## 1) Owner

* **Owner**: repository maintainer (`@softwaresalt`), as the sole operator of
  this single-developer local-only MCP server. There is no on-call rotation;
  the owner is whoever runs `graphtor-docs serve` and reads its logs.

## 2) Baseline

* **Current (pre-change) log line**: `opened engine-enforced read-only SQLite
  DataStore (filesystem lock active)`.
* **New (post-change) log line**: `opened engine-enforced read-only SQLite
  DataStore (filesystem lock active: robust while this DataStore is the sole
  guard on the file; best-effort, not a cross-process guarantee, whenever the
  same file is independently guarded more than once - same- or
  cross-process - see F6)` — see `ENGINE_READONLY_OPEN_LOG_MESSAGE` in
  `src/db/store.rs`.
* **Emission site/frequency**: emitted exactly once per
  `DataStore::open_engine_readonly` call, i.e. once per served database that
  resolves to `ServeMode::ReadOnly` at `serve` startup (see
  `open_serve_databases` in `src/main.rs`). It is not emitted on a hot path,
  not repeated per-query, and not emitted for `Generation`-posture databases
  (those log `"opening database"` with `posture = "generation"` instead).
* **Level/target**: `INFO`, target `graphtor_core::db::store`.
* **Downstream log-scraping/alerting**: none configured. This is a
  single-developer, local-only tool (Constitution: "All processing is
  local-only — no data leaves the developer's machine") with no centralized
  log aggregation, dashboard, or alert rule keyed on this or any other log
  substring. The observation window below is therefore a manual one, not an
  automated dashboard.

## 3) Bounded Post-Deploy Observation Window

* **Duration**: the first 10 `graphtor-docs serve` invocations against a
  `ReadOnly`-posture database after this change reaches the operator's
  installed binary, or 14 calendar days after merge, whichever comes first.
* **Owner**: `@softwaresalt` (same as above) — manually inspects `serve`
  startup output (stderr, `RUST_LOG=info` or default `Normal` verbosity)
  during that window.
* **Activity during the window**: confirm the qualified log line renders as a
  single coherent line (no truncation, no encoding issue with the added
  punctuation), and confirm no operator confusion is reported (there being no
  other consumer of this log line today, "no report" is the expected/passing
  signal).

## 4) Failure Threshold

The change is considered to have broken operator reasoning or log
consumption if, within the observation window:

* the log line fails to render (panics, truncates, or corrupts output) when
  `open_engine_readonly` succeeds; or
* an operator (including the maintainer) misreads the qualified wording as
  reintroducing an unconditional guarantee (i.e., the wording accidentally
  drops the "best-effort" / "F6" qualification when read in context); or
* any future log-scraping/alerting that keys on the substring `"filesystem
  lock active"` (none exists today, per Baseline) is added and observed to
  silently stop matching because of the appended qualification text.

No dashboard or automated alert exists to detect these conditions
mechanically; detection is by manual read during the observation window,
consistent with the bounded-manual-observation posture this low-risk,
text-only change warrants (per
`.github/instructions/release-observability.instructions.md`: "If the
workspace does not have a monitoring system, record the monitoring plan as a
structured checklist ... and flag it as a manual observation requirement").

## 5) Rollback Trigger and Rollback Procedure

* **Rollback trigger**: any Failure Threshold condition above is observed
  during the bounded observation window.
* **Rollback procedure**: revert the single wording-change commit
  (`fix(cli): qualify read-only guard contract wording across all surfaces`)
  on the `post-merge/054-f-readonly-serve-guarantee-honesty` history, or apply
  an equivalent follow-up commit restoring the prior
  `"opened engine-enforced read-only SQLite DataStore (filesystem lock
  active)"` literal in `ENGINE_READONLY_OPEN_LOG_MESSAGE` (or the `info!`
  call site directly). This is a **text-only revert**:
  * `EngineReadonlyGuard::lock`/`Drop` are untouched by the original change,
    so rollback has zero effect on guard acquisition, permission
    capture/restore, or sidecar cleanup behavior.
  * `is_engine_enforced_readonly()` continues to return
    `self.engine_readonly_guard.is_some()` before, during, and after
    rollback — no caller-visible predicate semantics change at any point.
  * No schema, CLI flag, or data-migration state is touched, so rollback
    requires no data backfill or compatibility shim.
* **Rollback risk**: none beyond the cosmetic log-text change reverting;
  rated `ActionRisk: low` (text-only, no runtime behavior).

## Releasability Evidence Summary

| Evidence | Status |
| --- | --- |
| Monitoring plan | Manual observation (no dashboard exists or is warranted for this scope) |
| Pre-deploy audit | N/A — no feature flag, no migration, no cross-service dependency; single in-process log line |
| Post-deploy observation window | 10 `ReadOnly` `serve` starts or 14 days, owner `@softwaresalt` |
| Rollback trigger | Any Failure Threshold condition above |
| Rollback procedure | Single-commit text revert; zero guard/runtime impact |

**Releasability status**: `READY`. This is a low-risk, text-only,
behavior-preserving change to a startup log line and accompanying rustdoc;
the bounded manual observation window and single-commit rollback are
proportionate to that risk.

## Cross-References

* `docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md` —
  Unit A1 implementation plan (this evidence satisfies its "Runtime
  Verification and Closure" section's observability requirement).
* `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
  — corrected "Read-only serve hardening" section (054.002-T).
* `.github/instructions/release-observability.instructions.md` — monitoring
  plan, pre-deploy audit, observation window, and rollback-trigger
  requirements this evidence satisfies.
