---
title: "Stage recovery: H3-A planning pass"
description: "Recovery record for the interrupted H3-A selection, MSRV, and review remediation"
doc_type: "memory"
session_date: "2026-08-29"
agent: "stage"
backlog_refs:
  - "049-S"
  - "052-S"
  - "056.004-T"
  - "056.011-T"
  - "056.029-T"
linked_artifacts:
  - "docs/decisions/2026-08-29-mcp-serve-discover-preinitialize-evidence.md"
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
tags:
  - stage
  - h3a
  - mcp
  - planning
  - recovery
---

## Outcome

Recovered the interrupted H3-A planning pass above `db5baa0`. H3-A remains
selected as `cause:h3a-transport`: redacted actual-client stderr records
preflight and model completion, `starting MCP STDIO server`, a pre-initialize
`server/discover` request at id 0, rmcp `ExpectedInitializeRequest`, exit 2,
and pipe closure.

## Corrected planning semantics

* `056.011-T` remains queued and `selection:selected`; `052-S` remains queued
  with it as the sole member, after `049-S` and PHASE 1.5 (`056.028-T`)
* `049-S` retains exactly its eight evidence members and continues to depend on
  `051-S`
* `056.011-T` alone runs the actual current-pin `cargo +1.75.0 check
  --all-targets` before H3-A work
* A nonzero result blocks and returns `052-S` to Stage; it cannot be waived or
  used to claim adapter MSRV compatibility
* Unshipped `056.029-T` consumes and dispositions that one redacted result for
  T4 fan-in and carries no `selection:*` label
* The private binary adapter handles only pre-initialize `server/discover`;
  response shape remains exact-Copilot evidence, with `-32601` only a
  standards-informed candidate
* The transparent diagnostic wrapper is valid for `056.011-T` before/after
  evidence only; T4 restored-production acceptance remains wrapper-free

## Review and validation

Report-only reviewers covered correctness, architecture/dependencies,
scope/constitution, agent-native parity, schema/CLI/docs coupling, and Rust
planning. Remediated P1s covered wrapper/T4 separation, single MSRV evidence
ownership, private binary visibility, module rustdoc ownership, and T4 closure
ownership. Final re-review reported `P0=0, P1=0`.

Remaining non-blocking risks are the unmeasured MSRV gate, unproven exact-client
response shape and probe cadence, the future PHASE 1.5 shipment dependency, and
conditional pinned-rmcp concurrency verification. No source, workflow, build,
task-claim, shipment-close, merge, or admin action occurred.

## Handoff

Stage may commit the planning artifacts locally. Stage role policy forbids the
requested push and PR creation; Ship must perform those actions. The pre-existing
`.gitignore` operator change was not modified or staged. Its SHA-256 remained
`9B8D4D547ACCD743356F02B5F3BDFB44D9154CDE11BB841C81104D9DA0013EC2`.
