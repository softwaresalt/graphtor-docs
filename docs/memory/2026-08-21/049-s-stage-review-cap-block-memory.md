---
doc_type: memory
source: orchestrator-session
title: Shipment 049-S staging review-cap block
date: 2026-08-21
backlog_refs:
  - 049-S
  - 056-F
  - 7BF1961D
---

## Outcome

Stash bug `7BF1961D` was harvested into feature `056-F` and queued
shipment `049-S`. Staging PR
[#106](https://github.com/softwaresalt/graphtor-docs/pull/106) remains
open and blocked. Three review-fix cycles completed, reaching the
repository hard cap. The final report-only review found **six** unresolved
P1 planning-contract defects (an earlier note of four omitted the
red-test-polarity and lock-age/start-time contracts), so Ship was not
invoked.

## Completed work

* Created and pushed the staging branch `chore/stage-049-S`
* Created PR #106 with plan, deliberation, feature, and task artifacts
* Expanded the shipment to tasks `056.001-T` through `056.008-T`
* Added a real MCP `initialize` exchange to the planned red harness
* Added plan-hardening and plan-review evidence
* Split runtime H0a, managed-launch H0a, H0b locking, H1 latency, and
  conditional diagnostics into separate tasks
* Added branch-sensitive H0/H1 evidence and observation criteria
* Replied to and resolved all 11 Copilot-authored review threads
* Marked PR #106 local readiness as `BLOCKED` at reviewed HEAD
  `9b56ffbc31b241f869a7515cd7f45ab75813a5bd`

## Blocking findings

Six unresolved P1 plan-contract defects (the earlier count of four omitted
findings 1 and 6):

1. Red-test polarity: the harness must always assert the successful
   `initialize` response; the reproduced broken-pipe / early-exit / timeout
   is diagnostic evidence only, never the expected assertion.
2. H0a proof coupling: the raw process harness cannot prove the
   managed-launch H0a fix without explicitly exercising the generated
   launch contract.
3. Existing-install migration: updating `managed_server_value` alone does
   not repair existing installations because binary upgrade does not
   rewrite `.mcp.json`.
4. H0c closure: the H0c branch has no tracked remediation that can reach
   the healthy handshake required by the shipment definition of done.
5. Authorized-root / launch-cwd containment: the plan required the child
   cwd to equal the project root while also requiring that cwd to be
   inside the project-root `.graphtor` directory; both conditions cannot
   hold.
6. Live-lock age semantics: adding process start-time without changing the
   age fallback can still evict a genuinely live long-running holder; a
   matching pid + start-time identity must stay live regardless of age.

## Files and surfaces changed

* `.backlogit/archive/stash.jsonl`
* `.backlogit/hooks_queue.jsonl`
* `.backlogit/queue/049-S.md`
* `.backlogit/queue/056-F.md`
* `.backlogit/queue/056.001-T.md` through
  `.backlogit/queue/056.008-T.md`
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`

The pre-existing `.mcp.json` modification and `.backlogit/runtime/`
directory were preserved and excluded from commits.

## Decisions and rationale

* Keep the work evidence-first. Do not implement a speculative
  protocol-version change; rmcp 1.5 negotiates the version internally.
* Preserve workspace containment and fail-closed startup gates.
* Use Engram direct mode for unified, graph, and dependency searches.
  The daemon did not reach ready state, but `ENGRAM_DIRECT=1` worked.
* Do not exceed the three-cycle review-fix cap or downgrade unresolved
  P1 findings to follow-ups.
* Do not invoke Ship until staging artifacts are on `origin/main` and
  the current-HEAD local readiness outcome is no longer `BLOCKED`.

## Failed or blocked approaches

* Normal merge of PR #106 was rejected by branch protection.
* Admin fallback was authorized and attempted, but GitHub rejected it
  while review conversations were unresolved.
* Hosted review conversations were subsequently addressed and
  resolved, but the final local review still found blocking plan
  defects.
* Engram daemon binding/status calls timed out twice. Direct mode
  remained usable, so no broad grep fallback was used for unified,
  graph, or dependency searches.

## Next steps

1. Start a new Stage review session with a reset review-fix budget.
2. Resolve the six P1 contracts (and the `056.003-T` title P3) without
   broadening implementation scope.
3. Run a fresh current-HEAD report-only review.
4. Update PR #106 readiness and obtain merge approval only after
   `P0=0, P1=0`.
5. Merge staging artifacts to `main`, then route shipment `049-S` to
   Ship for T0 evidence capture and test-first implementation.
