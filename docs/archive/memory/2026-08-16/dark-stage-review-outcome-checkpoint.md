---
title: "Dark-Stage checkpoint — plan-review + adversarial-review outcome"
type: session-checkpoint
date: 2026-08-16
agent: Stage
phase: post-review, pre-harvest
tags:
  - checkpoint
  - adversarial-review
---

## Review outcome

Both plans passed after one remediation cycle. Four independent cross-model
reviewers (anthropic claude-opus-4.8, google gemini-3.1-pro, openai gpt-5.6-sol,
xai grok-4.5) + a 3-reviewer post-remediation re-review satisfied the
adversarial-review requirement.

### Consensus decisions

* Reject-external-path decision (5D98DBCC): **PASS** (HIGH consensus) — waives/breaks
  workspace containment (Principle III MUST; IV NON-NEGOTIABLE ethos); exfiltration
  vector. Reshaped to honest-guarantee reliability hardening (Option D); Option C
  (single-owner serve + multiplexing transport) recorded as future direction.
* Plan A (readonly-serve-guarantee-hardening): **PASS** after remediation.
  Original draft's `is_engine_enforced_readonly() = access_mode==ReadOnly` overload
  was WRONG (open_sqlite_readonly = ReadOnly+guard None → false engine claim).
  Remediated: keep predicate = guard.is_some(); repository-wide honest-surface
  sweep (rustdocs + startup log + main.rs + design doc); characterization tests;
  F6 framed as documented residual (not closed); precise same-/cross-process
  qualification.
* Plan B (serve-auto-discovery-followups): **PASS** after remediation. Original
  traversal short-circuit broke fail-closed-on-walk-error → read-only→read-write
  Generation posture escalation. Remediated: full error-observing walk retained;
  only the O(document-count) Vec removed via streaming boolean with a shared
  reusable matcher; deterministic ordering test seam + differential test.

### Deferred (traceable stash entries created)

* `5905CDEE` (bug, medium) — symlink-swap TOCTOU in EngineReadonlyGuard lock/Drop.
* `F1CE20EC` (feature, low) — cross-workspace shared read-only serving via Option C
  + true F6 cross-process fix.

## Artifacts

* Spike: docs/decisions/2026-08-16-readonly-serve-cross-process-coordination-spike.md
* Deliberation A: docs/decisions/2026-08-16-shared-external-readonly-databases-deliberation.md
* Deliberation B: docs/decisions/2026-08-16-serve-auto-discovery-followups-deliberation.md
* Plan A: docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md (## Plan Review appended, PASS)
* Plan B: docs/exec-plans/2026-08-16-serve-auto-discovery-followups-plan.md (## Plan Review appended, PASS)

## Next steps

Harvest Plan A → feature + 2 tasks (A2 dep A1); Plan B → feature + 2 tasks (B1 || B2).
Create 2 queued shipments (Group A first: reliability/correctness priority; then
Group B). Archive consumed stash 970AE45A, 5D98DBCC, B88E37BF, 5868A7C5. Commit
Stage artifacts on main (no push). Keep 6 stowaway files uncommitted.
