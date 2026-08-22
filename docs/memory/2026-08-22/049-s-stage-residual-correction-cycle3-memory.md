---
title: "049-S Stage residual correction cycle 3"
doc_type: memory
source: stage-correction-session
date: 2026-08-22
shipment: 049-S
feature: 056-F
---

## Outcome

The report-only standard review of exact HEAD
`60f32c0ee5b795a3609497b092ce7d6f74ded7d7` was `BLOCKED`. Correction cycle 3
applied the final allowed review-fix pass to the authoritative plan,
deliberation, feature, and task contracts. No fresh PASS is claimed until the
next exact committed HEAD is reviewed.

## Merged Blocking Findings

* T0 used direct-server replay as a possible branch-selection source instead
  of requiring an actual target-CLI launch transcript
* The target-CLI cwd probe lacked an owned timeout, process cleanup boundary,
  and unconditional byte-for-byte config restoration
* H3-A required a framing red test without capturing the actual client's
  failing wire transaction
* Legacy pid-only locks could still age-evict a currently live writer
* H0c selected one fail-closed gate even though sequential gates can appear
  only after earlier workspace-state repairs
* Conditional tasks used `not-needed` as if it were a valid backlog status;
  task metadata supports `done` but not `not-needed`
* H1's proposed bare lazy cell did not account for `DocServer: Clone`

## Applied Corrections

* T0 now requires a bounded transparent actual-client wrapper with allowlisted
  diagnostics, raw framing capture, isolated process-tree cleanup, and config
  restoration on every outcome
* T1 holds `ChildStdin` explicitly and H3-A replays the captured client
  transaction
* `mcp_serve_ready` now means only preflight complete/about to call
  `serve_server`
* T4 requires three distinct actual-client launches correlated by CLI version,
  config identity, timestamp, PID when available, capture path, and result
* Conditional tasks move to `done` with `not-needed: <rationale>` comments
* Legacy live pid-only locks fail closed regardless of age; new lock records
  encode `start_time=<u64 epoch seconds>`
* H0c repairs and re-probes iteratively until initialize succeeds or the plan
  is explicitly rescaled
* H1 uses clone-shared per-server lazy state and has a split-before-code width
  guard
* Existing-install config refresh is backup-first and owns direct documentation
  updates
* The three reachable exit-2 guards get process tests; the defensive
  primary-None invariant gets no artificial injector
* H3-A rmcp changes add an explicit Rust 1.75 toolchain gate

## Review Adjudication

The Template Integrity and Learnings persona outputs were excluded from merged
findings where they reviewed files outside `13485d0..60f32c0` or incorrectly
reported the commit as stash-only. The `tools/list` parity suggestion was
rejected as outside this initialize-time regression scope. Clean
maintainability and architecture results were retained.

## Files Modified

* `.backlogit/queue/056-F.md`
* `.backlogit/queue/056.001-T.md` through `.backlogit/queue/056.011-T.md`
* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`

Backlogit appended update events to `.backlogit/hooks_queue.jsonl`. The
user-owned `.mcp.json` and tool-managed `.backlogit/runtime/` remain excluded.

## Next Steps

1. Re-run doclint, targeted backlog doctor, dependency verification, and
   whitespace/staged-file audits
2. Commit correction cycle 3
3. Run the final fresh report-only current-HEAD review
4. If P0/P1 clears, run the mandatory three-family adversarial re-review
5. If blockers remain, halt and hand them to the operator because the
   three-cycle review-fix cap is exhausted
