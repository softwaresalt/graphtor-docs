---
title: "049-S Stage review-fix round 1"
doc_type: memory
source: stage-review-fix-session
date: 2026-08-22
shipment: 049-S
feature: 056-F
---

## Outcome

The operator authorized three additional review-fix rounds. Exact-HEAD standard
review of `1bcadaa4213b9cc37c26c2bdd8f336af64e2c175` was `BLOCKED` with
two deduplicated P1 findings:

* The H0b legacy-live-old lock test had inverted pre-change polarity
* The H0a/H3-B cwd probe lacked a contrasting launch-directory control

Round 1 updates the executable plan, deliberation, feature, and task contracts.
No review PASS is claimed until the next committed HEAD is reviewed.

## Applied Corrections

* T0 uses a same-build foreign-directory control/treatment pair
* The wrapper records its CLI-assigned identity before mutation and the inner
  server identity separately
* The wrapper preserves cwd/env/args and bidirectional framing, propagates inner
  exit/pipe closure, and owns the complete isolated process tree
* T0 alone selects the branch; T1 confirms only T0-derived behavior
* H0b legacy-live-old behavior is an observed-red anchor
* Legacy pid-only recovery never terminates a process without verified
  ownership
* H1 uses explicit clone-shared typed load states instead of a bare lazy cell
* H3-B1 capability proof uses the temporary diagnostic entry and defers
  production-entry verification to T4
* T4 verifies the restored post-fix user-facing entry and includes H0c
  state-backup restoration in rollback
* Existing-install recovery artifacts have a contained, collision-resistant,
  permission-preserving lifecycle
* Documentation was split from code into `056.012-T` and `056.013-T`

## Backlog Changes

Shipment `049-S` now contains `056-F` and tasks `056.001-T` through
`056.013-T`.

* `056.012-T` owns diagnostics and evidence-selected H0c operator docs
* `056.013-T` owns managed-launch, existing-install, and H3 operator docs
* T4 depends on both documentation-only tasks

## Preserved State

The user-owned `.mcp.json` remains unstaged and unchanged by this work.
Tool-managed `.backlogit/runtime/` and checkpoint files remain excluded from
commits.

## Next Steps

1. Validate documentation, backlog artifacts, dependency edges, and diff scope
2. Commit round 1
3. Run a fresh exact-HEAD standard report-only review
4. If P0/P1 clears, run the mandatory adversarial review
5. If blockers remain, apply at most two more review-fix rounds and report
   non-convergence after round 3
