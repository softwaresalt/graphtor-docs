---
title: "Stage 056.011 H3-A — PR #108 review-fix cycle 1 (declared-MSRV DAG correction)"
description: "Planning-only remediation of the five current PR #108 Copilot review threads: description markers, 29-task scope reconciliation, server/discover wording, and the dedicated 053-S declared-MSRV release unit ahead of 052-S"
doc_type: "memory"
date: "2026-08-29"
agent: "stage"
status: "superseded"
superseded_by: "docs/memory/2026-08-29/stage-056-011-h3a-pr108-reviewfix-cycle-2-memory.md"
branch: "chore/stage-056-011-h3a"
base_head: "4750d3d6f076464ac266776ffa7674486522e310"
backlog_refs:
  - "056-F"
  - "056.004-T"
  - "056.011-T"
  - "056.029-T"
  - "049-S"
  - "052-S"
  - "053-S"
---

One narrow review-fix cycle. Planning and backlog only: no Rust, source,
workflow, or build change; no push, PR body edit, GitHub reply, thread
resolution, merge, or admin fallback. `.gitignore` carried a pre-existing
working-tree modification and was never staged, committed, reverted, or
stashed.

## Threads addressed

| # | Thread | Disposition |
|---|---|---|
| 1 | `056.011-T` lost `BEGIN:description` markers | Restored via backlogit `sections` mutation; duplicate unmarked prose removed |
| 2 | Active "Reviewed artifact identity" still said 28 tasks | Reconciled to `056.001-T`..`056.029-T` (29) and now names the PHASE 1.6 MSRV unit |
| 3 | `server/discover` described inconsistently | One evidence-backed wording across plan, both decisions, and memory |
| 4 | Cargo 1.75 cannot parse edition-2024 rmcp 1.5 (substantive) | DAG corrected: dedicated `053-S` executes before `052-S` |
| 5 | `056.029-T` had no description markers | Restored the same way as thread 1 |

## Substantive correction (thread 4)

Stable Cargo accepted `edition = "2024"` only from Rust/Cargo 1.85. The
workspace declares `rust-version = "1.75"`, `Cargo.lock` resolves `rmcp 1.5.0`,
and the vendored `rmcp-1.5.0/Cargo.toml` declares `edition = "2024"` with no
`rust-version`. The previously mandatory `cargo +1.75.0 check --all-targets`
entry gate on `052-S` was therefore a deterministic failure, not a measurement,
and `052-S` could never have reached adapter implementation.

The DAG was corrected instead of deferring the blocker:

* `056.029-T` re-scoped from an unshipped evidence-consumer into the
  declared-MSRV / dependency-compatibility resolution task.
* New queued shipment `053-S` (PHASE 1.6), sole member `056.029-T`.
* Resolution options belong to implementation: raise the declared
  `rust-version` to a truthful floor and align docs/CI, **or** select an
  MSRV-compatible rmcp strategy through a bounded decision. Neither is
  preselected, implemented now, or silently waived. Any rmcp version change
  stays reviewed inside `056.029-T` or a named split if width requires it.
* `056.011-T` now validates against the **resolved** declared floor, never a
  hard-coded `+1.75.0`.
* Claims that a failed MSRV gate merely produced a T4-fan-in follow-up were
  removed from `056-F`, `056.004-T`, `052-S`, the plan, and both decisions.

## Edges encoded

```text
050-S -> 051-S -> 049-S -> [PHASE 1.5: 056.028-T, assembled after 049-S closes] -> 053-S -> 052-S
```

* shipment `blocks`: `051-S -> 050-S` (pre-existing), `049-S -> 051-S`
  (pre-existing), `053-S -> 049-S` (new), `052-S -> 053-S` (new),
  `052-S -> 049-S` (pre-existing, retained)
* task `blocks`: `056.011-T -> 056.029-T` (new), plus pre-existing
  `056.011-T -> 056.003-T` and `056.011-T -> 056.028-T`
* `056.004-T -> 056.029-T` retained (T4 fan-in)

PHASE 1.5 assembly gate is explicit and unchanged: Stage creates the
`056.028-T` shipment manifest only after `049-S` closes, and at assembly adds
that shipment as an explicit `053-S` dependency before claim. No PHASE 1.5
manifest was invented in this pass. `051-S` was not bypassed or removed and
`049-S` retains exactly its eight frozen members.

## Validation

* `backlogit_sync_index` clean; `backlogit_doctor` reports only the
  pre-existing unrelated `013.008-T` orphan.
* Dependency graph: 145 nodes / 248 edges / **0 cycles**.
* Shipment membership verified: `049-S` = 8 members, `052-S` = `056.011-T`,
  `053-S` = `056.029-T`.
* Selection gate: `056.011-T` passes (`selection:selected`); `056.029-T`
  carries no `selection:*` label by design, matching the pre-existing
  `056.028-T` non-cause-selected precedent, and that exemption is now stated
  explicitly in the artifacts.
* `git diff --check` clean; line endings preserved per file (plan/backlog LF,
  the 2026-08-29 decision CRLF).
* Focused report-only planning review recorded in the plan: **P0=0, P1=0**.

## Degraded tooling

`C:\Tools\engram.exe` was unavailable for this session: the daemon failed to
reach Ready within 30 s on three consecutive attempts
(`workspace-status`, two `search` invocations). Per the circuit-breaker
threshold, retries stopped and the documented fallback was used — backlogit
read-only SQL for backlog queries plus exact file reads for document scans. No
engram-derived claim appears in this pass.

## Next steps

1. Orchestrator pushes the branch and updates the PR body (replacement wording
   supplied separately in the cycle report).
2. Orchestrator replies to and resolves the five PR #108 threads after the push.
3. `056.029-T` implementation (in `053-S`) picks exactly one bounded MSRV
   resolution and records it in a new dated `docs/decisions/` artifact.
