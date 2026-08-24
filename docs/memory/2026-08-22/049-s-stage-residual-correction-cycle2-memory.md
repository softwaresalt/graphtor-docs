---
title: "049-S Stage residual correction cycle 2"
doc_type: memory
source: stage-correction-session
date: 2026-08-22
shipment: 049-S
feature: 056-F
---

## Outcome

Correction cycle 2 reconciled the implementation plan, deliberation, feature,
and task contracts after the current-HEAD standard review blocked staging.
The plan remains `draft` until a fresh report-only review clears the exact
committed HEAD.

## Decisions

* T1 is a reusable open-stdin initialize driver, not one fixture that must
  mechanically green every causal branch
* Repository-code branches require observed-red/green proof; operational-only
  H0c and H3-B require bounded actual-client before/after evidence
* `mcp_serve_ready` is a structured preflight signal emitted immediately before
  `serve_server`; `/mcp show graphtor-docs` remains the completed-handshake proof
* H3-B1 keeps managed `cwd` generation and existing-install refresh active when
  a supported CLI honors `cwd`; H3-B2 closes both tasks
* A healthy observation requires three successful starts; 24 hours with fewer
  than three starts is incomplete rather than healthy
* Verified live lock identity is never age-evicted; a live-but-hung holder
  requires operator-confirmed process recovery
* H1 tests use an injected loader seam and must retain Rust 1.75 compatibility

## Files Modified

* `.backlogit/queue/056-F.md`
* `.backlogit/queue/056.001-T.md` through `.backlogit/queue/056.011-T.md`
* `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
* `docs/memory/2026-08-21/049-s-stage-residual-correction-cycle1-memory.md`
* `docs/memory/2026-08-22/circuit-break-engram-context-search.md`

The user-owned `.mcp.json` change and tool-managed `.backlogit/runtime/`
directory remain excluded from staging.

## Dependency State

* `056.006-T` depends on `056.002-T` and `056.003-T`
* `056.008-T` depends on `056.002-T` and `056.011-T`
* `056.009-T` depends on `056.008-T`
* `056.011-T` depends on `056.001-T` and `056.002-T`
* `056.004-T` depends on all eight diagnostic and conditional tasks

## Failed Approach

Six Engram searches used the unsupported `--region context` value and returned
the same invalid-parameter error. The circuit breaker opened for Engram search
in this session. Details are recorded in
`docs/memory/2026-08-22/circuit-break-engram-context-search.md`; no further
Engram search or broad grep substitution is permitted in this session.

## Validation

The plan and deliberation pass authoring-profile documentation lint. Targeted
backlog doctor checks pass for the feature and dependency-sensitive tasks, and
the dependency query matches the intended DAG.

## Next Steps

1. Audit the intended diff while excluding `.mcp.json` and
   `.backlogit/runtime/`
2. Commit correction cycle 2 with the required trailers
3. Run a fresh report-only standard review against the exact new HEAD
4. If standard review clears P0/P1, run the mandatory three-family adversarial
   re-review before changing review metadata or pushing staging PR #106
