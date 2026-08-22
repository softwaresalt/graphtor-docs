---
title: "049-S Stage review-fix round 3"
doc_type: memory
source: stage-review-fix-session
date: 2026-08-22
shipment: 049-S
feature: 056-F
---

## Outcome

The exact-HEAD standard review of round 2 commit
`41adf77f1767aaec1b7b588b03fb6ea41d2a67fc` remained `BLOCKED`.
This final user-authorized correction round reconciles the valid findings into
the executable plan, deliberation, feature, task boundaries, and dependency
graph. No review PASS is claimed until the next committed HEAD receives a fresh
standard review.

This is the last of the three additional review-fix rounds authorized by the
operator. If the fresh exact-HEAD review remains blocked, report explicit
non-convergence rather than starting another correction round.

## Merged Review Findings

The report-only review was deduplicated as `P0=1, P1=5, P2=15, P3=7`.
The convergent blocking findings were:

* T1 and the H0b characterization task could hand a failing suite to a later
  task instead of producing an atomic green milestone
* T0 forced mutually exclusive cause selection and could discard a proven H0a
  prerequisite when cwd correction exposed a later blocker
* H1 left retryable `Failed` behavior ambiguous for Generation background sync
* recovery ownership spanned unrelated install, uninstall, doctor, config, and
  upgrade surfaces
* `056.003-T` and `056.015-T` could edit `cmd_serve` concurrently
* legacy lock guidance allowed age-based eviction despite live pid-only
  identity requiring conservative treatment

Coupled P2/P3 corrections added concurrent stderr draining, versioned
Loading/Failed/Disabled parity, all-outcome probe cleanup, handle-level
no-follow config I/O, explicit approval receipts, deterministic H3-A replay,
same-executable H3-B capability proof, target-workspace refresh evidence, and
T4-only production acceptance.

## Discarded Review Output

* The learnings reviewer could not access the workspace and returned no usable
  evidence
* Schema/CLI/docs findings against archived `054-F` and `055.*` artifacts were
  outside the reviewed `056-F`/`049-S` scope
* Findings that repeated a shared root cause were merged rather than counted
  as separate blockers

## Backlog and Dependency Changes

Shipment `049-S` still contains `056-F` and every task from `056.001-T`
through `056.020-T`.

* T0 now records ordered causal progress and explicit config-substitution
  approval
* T1 and `056.016-T` finish green; selected curative tasks own red/green
* H1 uses `src/embed/lifecycle.rs`, retryable `Failed`, and shared serialized
  retry across MCP and Generation sync
* Database and Workspace locks share one conservative identity policy, with
  approval-gated exact-lock legacy recovery
* managed-config signatures, generated fields, recovery primitives, and
  `cmd_upgrade` composition have distinct owners
* H3-A owns deterministic server replay only; H3-B1 requires the same exact
  CLI through a distinct documented mechanism; H3-B2 blocks shipment
* T4 solely owns restored-production acceptance, target refresh evidence, and
  at least one diagnostic-gate-off start
* Added dependency `056.003-T → 056.015-T`
* Removed dependency `056.019-T → 056.017-T`

## Files Modified

* `.backlogit/queue/056-F.md`
* `.backlogit/queue/056.001-T.md` through task-specific changed files ending
  at `.backlogit/queue/056.020-T.md`
* `.backlogit/hooks_queue.jsonl`
* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
* `docs/memory/2026-08-22/049-s-stage-review-fix-round3-memory.md`

The user-owned `.mcp.json`, `.backlogit/runtime/`, and malformed checkpoint
files remain excluded.

## Validation

* backlogit index rehydrated successfully with 484 artifacts
* shipment `049-S` membership contains `056-F` plus all 20 tasks
* dependency query matches the round-3 DAG; recursive cycle query returned no
  rows
* target doctor passed for `056-F` and all `056.001-T` through `056.020-T`
* authoring lint passed for the executable plan and deliberation
* workspace doctor reported only pre-existing orphan `013.008-T`, outside
  this shipment and untouched
* `git diff --check` passed before the final memory addition

## Next Gate

Stage only the intended queue, hook, plan, decision, and memory files; commit
without amending prior history; then run one fresh exact-HEAD standard
report-only review. A remaining P0/P1 means the three additional rounds did not
converge and must be reported to the operator.
