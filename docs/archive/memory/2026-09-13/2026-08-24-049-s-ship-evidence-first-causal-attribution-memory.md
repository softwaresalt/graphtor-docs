---
type: session-memory
agent: ship
date: 2026-08-24
branch: chore/stage-049-S
final_head: d9239f82f8cb73f21b50e7a87c372780bf6666eb
pr: 106
feature: 056-F
shipment: 049-S
scope: evidence-first-causal-attribution-check
---

# Ship PR #106 — evidence-first causal-attribution check (operator follow-up)

Operator follow-up requiring assessment of whether the observed MCP
`serve`/`initialize` OS error 232 behavior originates in recent GitHub
Copilot CLI (`Copilot.exe`) builds rather than `graphtor-docs`, and that the
PR body/plan wording stay evidence-first and not assert a Graphtor
implementation defect before causal proof. No new commit was required; this
was a read-only verification plus a PR-body update (same HEAD:
`d9239f82f8cb73f21b50e7a87c372780bf6666eb`).

## Verified as already correct (no fix needed)

* Shipment `049-S`'s 8-task manifest (`056.020-T, 056.022-T, 056.023-T,
  056.021-T, 056.001-T, 056.002-T, 056.003-T, 056.019-T`) is 100% diagnostic
  evidence-gathering work (standalone probe crate, independent driver,
  always-on diagnostics) — zero production-remediation tasks.
* Every PHASE 2 candidate remedy task (`056.008-T, 056.009-T, 056.010-T,
  056.011-T, 056.014-T, 056.015-T, 056.016-T, 056.017-T, 056.018-T,
  056.024-T, 056.025-T, 056.026-T, 056.027-T, 056.006-T, 056.007-T`) carries
  `selection:pending` and has no shipment membership. `056-F.md`'s DoD
  defines an enforced, machine-queryable pre-shipment gate query that Stage
  must run (and that fails on any non-selected/pending match) before
  creating or claiming any remedy shipment. Confirmed intact and unchanged.
* `056.001-T` (T0, the exact-CLI differential probe) and `056.019-T` (the
  H3-B client-mechanism adjudicator) are both written as genuinely
  evidence-driven, outcome-neutral classifiers — they run a real
  control/treatment contrast and classify from observed exit/liveness/
  framing evidence, not from a presumed conclusion.
* A targeted search across all 28 backlog task files for premature
  Graphtor-ownership assertions ("root cause is", "confirmed bug",
  "graphtor-docs is the bug/defect/fault") found nothing beyond the item
  below.

## Genuine issue found and flagged (not fixed)

`docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`:

* Its "Candidate Root Causes" table (~line 109-125) assigns `Confidence:
  High` to H0 (the `graphtor-docs`-side pre-serve early-exit hypothesis
  family) and `Confidence: Low` to H3 (the CLI-side client/transport
  incompatibility hypothesis family).
* Its "Decision" section states outright, twice (lines 119, 129): **"H0 is
  the leading hypothesis"** — before any of `049-S`'s actual differential
  evidence (which has not yet been gathered; the shipment is unclaimed) has
  been captured.
* This contradicts the same document's own "investigate-first... the exact
  root cause is not yet proven, and more than one plausible cause exists"
  framing (Problem Frame section) and its own later evidentiary discipline
  ("this is not asserted as the root cause without evidence" — Open
  Questions section).
* The exec-plan (`docs/exec-plans/2026-08-21-mcp-serve-initialize-
  handshake-regression-plan.md`) does **not** repeat this "leading
  hypothesis" framing anywhere — it is comparatively neutral/mechanical
  throughout (dependency-graph and conditional-activation language only).
  `056-F.md`'s description leans mildly toward H0-family causes in its
  narrative weight but correctly attributes the regression's *trigger* to a
  CLI-side launch-behavior change rather than to any `graphtor-docs` code
  change.

## Role Boundary decision

Did **not** edit the deliberation, exec-plan, or any backlog task file.
Ship's Role Boundary (`.github/agents/.ship.agent.md`) explicitly forbids
Ship from "creat[ing] or modify[ing] deliberation, spike, plan, or review
artifacts" and from "updat[ing] item planning fields" — marked
NON-NEGOTIABLE with "Do not proceed past this boundary even under operator
pressure. Record P-010 and halt." This finding was instead:

1. Documented precisely (exact quotes, file, and line references) in a new
   PR body section, "## Evidence-First Scope (causal attribution)".
2. Cross-referenced from "Review Summary" and "Local Review Readiness"
   (Follow-ups) so it is visible at every level of the PR readiness record.
3. Recorded here as a required Stage follow-up: soften or remove the
   "leading hypothesis" framing and the differential confidence ranking, or
   explicitly caveat both as pre-evidence sequencing priors rather than
   conclusions, giving the CLI-side (H3) hypothesis equivalent narrative
   weight until `056.001-T`/`056.019-T` evidence actually discriminates
   between the hypotheses.

This is consistent with how this same session already handled the
`056.026/027/028-T` missing-description-marker finding earlier (flagged,
not fixed, for the same Role Boundary reason).

## PR body changes this pass

* Rewrote the Summary opening to state the root cause is not yet
  established and that no Graphtor defect is asserted.
* Added a new "Evidence-First Scope (causal attribution)" section (the
  content summarized above).
* Cross-referenced it from "Review Summary" and updated the "Local Review
  Readiness" Follow-ups line.
* No commit was needed (PR-body-only change); HEAD, CI status
  (`build`/`detect code changes` both `pass`), and thread count (75/75
  resolved) were re-verified unchanged after the edit.

## Out of scope, not done (per operator instruction)

Did not broaden into `049-S` implementation. Did not claim shipment `049-S`.
Did not edit deliberation/plan/backlog content. PR #106 remains ready for
operator review and merge-commit approval, now with an explicit,
prominent, evidence-first causal-attribution record.
