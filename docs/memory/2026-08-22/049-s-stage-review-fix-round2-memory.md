---
title: "049-S Stage review-fix round 2"
doc_type: memory
source: stage-review-fix-session
date: 2026-08-22
shipment: 049-S
feature: 056-F
---

## Outcome

The exact-HEAD standard review of round 1 commit
`dddcac33a1e0adae27ef34f0870e7d279676ba7f` remained `BLOCKED`.
Additional correction round 2 reconciles the valid findings into the
executable plan, deliberation, feature, task boundaries, and dependency graph.
No review PASS is claimed until the next committed HEAD receives a fresh
standard review.

One user-authorized correction round remains after this round. If the final
round does not converge, report the non-convergence to the operator.

## Valid Review Findings Applied

* T0 config substitution needed no-follow/reparse checks, canonical
  containment, exclusive owner-only backup creation, sensitive-data redaction,
  and exact-byte-or-absence restoration
* A transparent wrapper needed independent full-duplex pumps, bounded buffers,
  continuous stderr drain, half-close propagation, child-exit coordination,
  and deadline process-tree cleanup
* The preflight inventory needed an exhaustive typed normal-exit seam,
  including pre-v4 and duplicate-intake exits
* Typed tracing could not replace unconditional fatal stderr because
  `RUST_LOG=off` can suppress tracing
* H1 needed one model owner shared by MCP handlers and Generation background
  sync, typed resolver outcomes, supervised task ownership, and non-blocking
  initialize
* H0b needed a red-first shared lock-policy harness for both Database and
  Workspace locks, with ambiguous identity remaining locked
* Managed config needed typed create/update/no-change/collision/path-violation
  outcomes rather than message sniffing
* Recovery artifacts needed component-by-component containment and
  owner-protected exclusive creation
* T4 needed exact production-entry parity rather than wrapper or temporary
  configuration evidence
* H3-A server framing and H3-B client cwd capability needed separate ownership;
  rmcp 1.8.x is incompatible with Rust 1.75
* H1, lock, config, recovery, H3, and documentation widths needed isolation to
  remain within the repository task-granularity contract

## Discarded Review Output

* The learnings reviewer could not access the workspace and returned no usable
  evidence
* The schema/CLI/docs reviewer inspected an explicitly excluded old memory
  artifact
* The template reviewer reported a stash mutation in
  `.backlogit/archive/stash.jsonl`, which was absent from the reviewed commit

These outputs did not enter the remediation queue.

## Backlog and Dependency Changes

Shipment `049-S` contains `056-F` and every task from `056.001-T` through
`056.020-T`.

* Added `056.014-T` for typed embedding resolver outcomes
* Added `056.015-T` for shared H1 `cmd_serve` and background-sync wiring
* Added `056.016-T` for the shared Database/Workspace lock-policy red harness
* Added `056.017-T` for typed managed-config mutation outcomes
* Added `056.018-T` for contained recovery-artifact creation
* Added `056.019-T` for H3-B client cwd capability adjudication
* Added `056.020-T` for the secure non-shipping actual-client probe harness

The authoritative execution spine is
`056.020 → 056.001 → 056.002`. H1 follows
`056.014 → 056.005 → 056.015`; H0b follows
`056.016 → 056.007`; managed launch follows
`056.019 → 056.017 → 056.018`, with `056.008` consuming the typed capability
and config outcomes before `056.009` integrates recovery. Documentation tasks
consume their respective branch contracts. T4 depends on `056.003` and every
task from `056.005` through `056.020`.

Backlogit accepted every edge with cycle detection. Index-backed membership and
edge inspection matched this DAG.

## Files Modified

* `.backlogit/hooks_queue.jsonl`
* `.backlogit/queue/049-S.md`
* `.backlogit/queue/056-F.md`
* `.backlogit/queue/056.001-T.md`
* `.backlogit/queue/056.003-T.md` through `056.009-T.md`, excluding unchanged
  `056.002-T.md`
* `.backlogit/queue/056.011-T.md` through `056.020-T.md`, excluding unchanged
  `056.010-T.md`
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
* `docs/memory/2026-08-22/049-s-stage-review-fix-round2-memory.md`

## Validation

* Backlog index refreshed after direct section-marker corrections
* Documentation authoring lint passed for the plan and deliberation
* Targeted backlog doctor passed for `056-F` and all tasks
  `056.001-T` through `056.020-T`
* Shipment membership includes all 20 tasks
* Dependency edges match the authoritative round-2 DAG

## Preserved State

The pre-existing user-owned `.mcp.json` modification remains unstaged and was
not changed by this round. Tool-managed `.backlogit/runtime/` and malformed
checkpoint files remain excluded from the correction commit.

## Next Steps

1. Lint this memory artifact and audit exact diff/staging scope
2. Commit correction round 2
3. Run a fresh exact-HEAD standard report-only review
4. If P0/P1 clears, run the mandatory adversarial review
5. If blockers remain, use the final authorized correction round and report
   non-convergence if it does not clear the gate
