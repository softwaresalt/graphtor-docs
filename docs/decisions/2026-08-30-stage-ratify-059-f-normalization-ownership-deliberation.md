---
title: "Stage ratification: 059-F rescoped-scope status normalization and successor-shipment assembly are Stage-owned (P-010 remediation)"
description: "Independent Stage review that ratifies the already-queued dispositions of the 059-F near-term feasible scope, assigns blocked-to-queued status normalization and successor-shipment assembly to Stage, and records that the prior Ship-performed normalization remains an un-legalized P-010 violation"
topic: "Ownership of superseded-chain status normalization and successor-shipment assembly for feature 059-F after the 2026-08-29 re-deliberation, following Copilot review comment 3888455427 on PR #114"
depth: "standard"
doc_type: "decision"
source: "pr:114 / feature:059-F / review-comment:3888455427"
decision_status: "decided"
linked_artifacts:
  - "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
  - "docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md"
  - "docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md"
tags:
  - "role-boundary"
  - "p-010"
  - "shipment"
  - "backlogit"
  - "normalization"
  - "toctou"
  - "059-f"
---

## Problem Frame

The 2026-08-29 re-deliberation
(`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`,
Option B) rescoped feature `059-F` to a feasible near-term permission-mutation scope after
`059.008-T` (U8) recorded a terminal `BLOCKED` engine-open feasibility result. Ship then
transitioned `059-F` and the feasible units `059.001/002/003/004/005/006/010/011-T` from
`blocked` to `queued` so they would pass a successor shipment's intake reconciliation.

Copilot review comment `3888455427` on PR #114 (against
`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`)
correctly identifies that this `blocked → queued` normalization is an **unclassified Ship
backlog mutation** under fail-closed P-010 role enforcement, and that the reusable procedure must
assign **both** normalization **and** shipment assembly to Stage — leaving Ship to only identify
the affected items and hand them off.

This document is the Stage-side remediation: an independent review that ratifies the resulting
disposition, fixes the ownership language in the authoritative Stage decision and plan, and
records the historical violation honestly rather than retroactively legalizing it.

## Role-Boundary Analysis (why normalization is Stage-owned)

| Operation | `.ship.agent.md` Role Boundary | `.stage.agent.md` Role Boundary |
|---|---|---|
| Move task `blocked → queued` (status normalization) | Not in Ship Allowed (`Claim shipments, move tasks to active/done, close shipments, archive completed items`); not read-only → **fail-closed forbidden** (P-010) | In Stage Allowed (`Create, update, archive backlog items, stash entries, shipment manifests`) → **allowed** |
| Assemble successor shipment | Forbidden (`create shipments`) | Step 5.5 shipment assembly → **allowed** |
| Identify non-terminal manifest members and `return-blocked` (status-preserving) | Owner operation on an in-flight release unit → **allowed** | Not Stage's (Stage does not mutate an active shipment manifest) |

`return-blocked` removes an item from a shipment manifest **without** changing its status, so it is
inside Ship's owner authority. The subsequent `blocked → queued` normalization is a planning-shaping
status change with no shipment-lifecycle justification, which is exactly the mutation the Stage Role
Boundary enumerates as `update backlog items` and the Ship Role Boundary does not enumerate. Under
the fail-closed evaluation in `.github/instructions/role-enforcement.instructions.md`, an unlisted
state mutation is treated as forbidden. The Copilot comment is therefore correct.

## Authoritative Ownership Division (decided)

For any shipment supersession where a mixed done/terminal-blocked manifest is closed and a rescoped
feasible scope must be prepared:

* **Ship identifies, returns, and hands off.** Ship classifies the manifest, uses status-preserving
  `return-blocked` to remove non-terminal members from the manifest it owns, safe-closes the evidence
  shipment, and records in its closure/memory artifacts which items form the rescoped scope. Ship
  performs **no** `blocked → queued` normalization and assembles **no** successor shipment.
* **Stage normalizes and assembles.** Stage independently reviews the handed-off scope against the
  governing decision, normalizes every superseded-chain member's status to an intake-valid value
  (`blocked → queued`) where the only remaining blocker is a dependency edge on another in-scope
  member or an already-`done`/gate item, rewires dependency edges to the feasible DAG, and — only when
  the operator wants a fresh shipment — assembles the successor shipment (Step 5.5).

Items whose `blocked` status is itself the terminal, decided evidence (for example `059.008-T`/U8,
the infeasible engine-open feasibility spike, and the deferred `059.009-T`/U9) stay `blocked` and
stay out of the rescoped scope; Stage does not normalize those.

## Historical Violation Record (not retroactively legalized)

The prior Ship-performed `blocked → queued` normalization of `059-F` and
`059.001/002/003/004/005/006/010/011-T` **remains a P-010 role-boundary violation**. This decision
does **not** grant retroactive approval and does **not** reclassify that mutation as Ship-allowed.
Stage is **affirming the resulting disposition after independent review** because the resulting
statuses are, on their own merits, the semantically correct dispositions under the governing
re-deliberation — not because the operation that produced them was legitimate. The remediation of the
reusable Ship procedure that still embeds this mutation
(`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`,
Step 4) is a Ship-owned closure/compound edit and is left to Ship; Stage does not edit Ship
closure/compound/memory artifacts.

## Independent Verification of the Ratified Dispositions

Stage re-read the governing re-deliberation and re-queried the live backlog index
(`backlogit sync` then `backlogit query`) for the 10-item near-term scope. Authoritative status and
dependency edges:

| Item | Status | Depends on (blocks edges) | Correct? |
|---|---|---|---|
| `059-F` (feature) | queued | — | Yes — rescoped covering feature, awaiting sign-off + implementation |
| `059.014-T` (sign-off gate) | queued | — | Yes — ready gate awaiting operator sign-off |
| `059.001-T` U1 | queued | `059.007-T` (done), `059.014-T` | Yes — queued, not ready until sign-off done |
| `059.002-T` U2 | queued | `059.001-T` | Yes — queued, not ready |
| `059.006-T` U6 | queued | `059.001-T`, `059.002-T`, `059.007-T` (done) | Yes — queued, not ready |
| `059.003-T` U3 | queued | `059.002-T`, `059.006-T` | Yes — queued, not ready |
| `059.004-T` U4 | queued | `059.002-T`, `059.006-T` | Yes — queued, not ready |
| `059.005-T` U5 | queued | `059.003-T`, `059.004-T`, `059.006-T` | Yes — queued, not ready |
| `059.010-T` U10 | queued | `059.003-T`, `059.004-T`, `059.006-T` | Yes — queued, not ready |
| `059.011-T` U11 | queued | `059.002-T` | Yes — queued, not ready |

Adjacent items, confirmed unchanged:

* `059.008-T` (U8) `blocked` — terminal accepted engine-open feasibility evidence; stays blocked.
* `059.009-T` (U9) `blocked` — deferred engine-open integration (deps `059.006-T` + `059.013-T`);
  later separate shipment.
* `059.012-T` (U12) `queued` — later separate shipment; dependency repointed from the terminally
  blocked U8 to the sign-off gate `059.014-T`.
* `059.013-T` (Option A) `queued` — later separate shipment; upstream/fork engine-open closure.

**Readiness:** a dependency-ready query over the near-term scope returns only `059-F`, `059.013-T`,
and the sign-off gate `059.014-T` as unblocked; every implementation unit (U1–U6, U10, U11) is
correctly `queued`-but-not-ready behind the sign-off gate and its dependency chain. This matches the
re-deliberation rule that dependent tasks may be queued but are not executable until their
dependencies are terminal.

**Acyclicity:** a Kahn topological ordering over the authoritative `item_deps` edge set
(`U7(done)/U14 → U1 → U2 → U6 → U3/U4 → U5/U10`, with `U11` after `U2`) consumes every node with no
back edge. The near-term DAG is acyclic and matches the governing decision's rescoped feasible DAG.

**Conclusion:** the current `queued` dispositions are semantically correct. No status change is
required; Stage ratifies them as Stage-owned planning disposition. These items are **not** added to
any shipment by this pass — successor-shipment assembly remains a future Stage Step 5.5 act, gated on
operator sign-off (`059.014-T`).

## Scope of This Stage Pass

* Planning/backlog mutation only. No source, test, or config changes; no shipment created, claimed,
  closed, or archived; no PR body or thread edits; no merge.
* No edits to Ship closure/compound/memory artifacts; the compound-doc Step 4 remediation is
  Ship-owned follow-up.
* `.gitignore` and `docs/scratch/` are preserved untouched.

## Follow-up Left to Ship

Ship, on its next cycle, should correct
`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
Step 4 so the reusable procedure hands the un-normalized, `return-blocked` scope to Stage and lets
Stage perform the `blocked → queued` normalization, consistent with the ownership division decided
here. That is a Ship closure/compound edit and is intentionally out of scope for this Stage pass.
