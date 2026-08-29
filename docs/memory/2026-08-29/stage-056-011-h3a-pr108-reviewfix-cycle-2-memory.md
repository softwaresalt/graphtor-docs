---
title: "Stage memory: PR #108 H3-A review-fix cycle 2"
description: "Current dependency-closed H3-A and Rust 1.75 planning record"
doc_type: "memory"
session_date: "2026-08-29"
agent: "stage"
status: "current"
supersedes:
  - "docs/memory/2026-08-29/stage-056-011-h3a-selection-memory.md"
  - "docs/memory/2026-08-29/stage-056-011-h3a-recovery-memory.md"
  - "docs/memory/2026-08-29/stage-056-011-h3a-pr108-reviewfix-memory.md"
current_backlog_memory_key: "stage-056-011-h3a-pr108-reviewfix-cycle-2-2026-08-29"
backlog_refs:
  - "056-F"
  - "056.004-T"
  - "056.011-T"
  - "056.028-T"
  - "056.029-T"
  - "056.030-T"
  - "056.031-T"
  - "056.032-T"
  - "056.033-T"
  - "049-S"
  - "052-S"
  - "053-S"
linked_artifacts:
  - "docs/decisions/2026-08-29-mcp-serve-discover-preinitialize-evidence.md"
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
tags:
  - stage
  - review-fix
  - h3a
  - msrv
  - rust-1.75
---

## Scope

Planning and backlog correction only. No Rust, source, workflow, or runtime
implementation was performed; no build, test, lint, push, PR body edit, GitHub
reply, merge, admin fallback, or destructive action occurred.

## Current Decision

Cargo 1.75 cannot parse the current edition-2024 `rmcp 1.5.0` manifest. This is
an established incompatibility, not a pending measurement. Rust 2021 and
`rust-version = "1.75"` remain non-negotiable. Raising the floor is outside
Feature 056 and requires a separately approved constitutional amendment.

`056.029-T` now owns only `Cargo.toml`, `Cargo.lock`, and a dated dependency
decision that selects a compatible release, pin, or narrowly approved patch.
If that choice needs source or API migration, Stage creates a named bounded
follow-up and blocks the task rather than widening it.

## Authoritative Routing

```text
050-S -> 051-S -> 049-S -> PHASE 1.5 (056.028-T) -> 053-S -> 052-S

056.020-T -> 056.028-T -> 056.029-T
056.029-T -> {056.030-T, 056.031-T, 056.032-T, 056.033-T, 056.011-T}
056.003-T -> 056.011-T
056.028-T -> 056.011-T
053-S -> 052-S
{056.011-T, 056.028-T, 056.029-T, 056.030-T, 056.031-T, 056.032-T,
 056.033-T} -> 056.004-T
```

The authoritative Phase 1.5 edge is `056.028-T -> 056.029-T`. Its shipment
manifest remains deliberately uncreated until `049-S` closes. Before `053-S`
claim, member readiness verifies `056.028-T` is terminal and then verifies the
`056.029-T` successor dependencies. `052-S` starts only after all five `053-S`
members close. There is no direct Phase 1.5 shipment-to-`052-S` edge.

## Width Ownership

* `056.029-T`: dependency decision and Cargo manifest/lock resolution only
* `056.030-T`: `.github/workflows/ci.yml` only
* `056.031-T`: `README.md` and `docs/architecture.md` only
* `056.032-T`: `AGENTS.md`, `.github/copilot-instructions.md`, and the
  constitutional instruction only
* `056.033-T`: three generic Rust authoring-instruction files only

The feature inventory is 33 tasks, `056.001-T` through `056.033-T`. Shipment
`053-S` contains the five prerequisite tasks in dependency-closed order. T4
has direct fan-in from the four newly split tasks as well as `056.029-T`.

## Supersession

The earlier `stage-056-011-h3a-selection-memory.md`,
`stage-056-011-h3a-recovery-memory.md`, and
`stage-056-011-h3a-pr108-reviewfix-memory.md` records are preserved as
historical truth but marked `status: superseded` and point here. The backlog
memory key `stage-052-S-h3a-selection-2026-08-29` is superseded by the current
key above. None of those records is actionable for Phase 1.5 routing, a
`052-S` Rust 1.75 entry gate, or shipment placement of `056.029-T`.

## Search and PR Notes

Engram is degraded for this session: the sole permitted
`C:\Tools\engram.exe --workspace "C:\Source\GitHub\graphtor" status` attempt
failed because `status` is not a recognized subcommand. This pass used
backlogit structured queries and exact known-file reads only.

PR #108 was not read or edited. The Orchestrator should retain a current-head
reference to `0db36b7`, identify `053-S` as the five-member prerequisite unit,
state that Phase 1.5 is enforced by `056.028-T -> 056.029-T`, and state that
the Rust 1.75 floor is preserved through an MSRV-compatible dependency strategy.

## Preservation

The pre-existing `.gitignore` SHA-256 was
`9B8D4D547ACCD743356F02B5F3BDFB44D9154CDE11BB841C81104D9DA0013EC2`.
It was not staged, reverted, or stashed.
