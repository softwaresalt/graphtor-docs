---
title: "Stage ratification: 059-F rescoped-scope status normalization and successor-shipment assembly are Stage-owned (P-010 remediation)"
description: "Independent Stage review that ratifies the already-queued dispositions of the 059-F feasible scope, assigns blocked-to-queued status normalization and successor-shipment assembly to Stage, issues the durable Step 5.5 Mode R authorization naming the exact 10-item handoff set, supersedes the PR #113 051-S closure-timing precondition, and records the prior Ship-performed normalization as an un-legalized P-010 violation"
topic: "Ownership of superseded-chain status normalization and successor-shipment assembly for feature 059-F after the 2026-08-29 re-deliberation, following Copilot review comment 3888455427 on PR #114"
depth: "standard"
doc_type: "decision"
source: "pr:114 / feature:059-F / review-comment:3888455427"
decision_status: "decided"
linked_artifacts:
  - "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
  - "docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md"
  - "docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md"
  - ".backlogit/reconcile/051-S-safe-close-20260829-203729.md"
tags:
  - "role-boundary"
  - "p-010"
  - "shipment"
  - "backlogit"
  - "normalization"
  - "toctou"
  - "059-f"
  - "mode-r"
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

**Readiness (feature-family-wide query, not near-term-scope-only):** the dependency-ready query
was run across the **whole `059-F` feature family** — every `059.*` item plus the covering
feature, including the adjacent later-shipment items classified above — and returns exactly three
unblocked nodes: `059-F`, the sign-off gate `059.014-T`, and the adjacent later-shipment item
`059.013-T`. `059.013-T` appears **because the query is family-wide**, not because it is inside the
near-term scope; it stays excluded from that scope per the adjacent-items table above. Restricted
to the near-term scope alone the ready set is `059-F` + `059.014-T`. Every implementation unit
(U1–U6, U10, U11) is correctly `queued`-but-not-ready behind the sign-off gate and its dependency
chain. This matches the re-deliberation rule that dependent tasks may be queued but are not
executable until their dependencies are terminal.

**Acyclicity:** a Kahn topological ordering over the authoritative `item_deps` edge set
(`U7(done)/U14 → U1 → U2 → U6 → U3/U4 → U5/U10`, with `U11` after `U2`) consumes every node with no
back edge. The near-term DAG is acyclic and matches the governing decision's rescoped feasible DAG.

**Conclusion:** the current `queued` dispositions are semantically correct. No status change is
required; Stage ratifies them as Stage-owned planning disposition. These items are **not** added to
any shipment by this pass — successor-shipment assembly remains a future Stage Step 5.5 act, gated on
operator sign-off (`059.014-T`).

## Normalization Is Complete and Ratified; Assembly and Implementation Are Gated

Two separate things must not be conflated, and earlier wording in the plan and the governing
re-deliberation did conflate them:

| Act | Owner | State as of this decision | Gated on `059.014-T`? |
|---|---|---|---|
| `blocked → queued` status normalization of the 9 feasible units + covering feature | Stage-only (P-010) | **Already completed** — performed by Ship in violation, then **independently reviewed and ratified by Stage** on 2026-08-30 while `059.014-T` is still `queued` | **No** |
| Rewire of the feasible DAG (U8/U9 edge drops, U12 re-point) | Stage-only | **Already completed** and ratified | **No** |
| Successor-shipment assembly (Step 5.5) | Stage-only | **Not started** | **Yes** |
| Implementation of the rescoped `059-F` (U1 onward) | Ship, after assembly | **Not started** | **Yes** |

Normalization was never conditioned on sign-off. A `queued`-but-not-ready status is exactly the
intake-valid disposition that lets the sign-off gate function as a dependency edge: `059.001-T`
carries a `blocks` edge on `059.014-T`, so U1 stays unexecutable until the gate is `done` **because
of the edge**, not because the units were left `blocked`. Ratifying the normalization now therefore
grants no early execution authority. What sign-off gates is successor-shipment assembly and the
implementation that follows it.

Stage's ratification of the already-completed normalization does **not** retroactively legalize the
Ship mutation that produced it (see the historical violation record above and the four-entry record
in `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`).

## Mode R Authorization for Successor-Shipment Assembly (durable, exact-ID)

This section is the **durable, citable Step 5.5 Mode R authorization** required by
`.github/agents/.stage.agent.md` (Step 5.5, item 1, *Mode R — ratified existing-scope handoff*).
It exists because the ten items below were harvested in an earlier session, already exist in the
queue, and this PR intentionally creates **no** stash entry and **no** new harvest. Under Mode H the
scope guard would exclude them; Mode R is the sanctioned path for exactly this case. Manufacturing a
stash entry or a synthetic harvest to make Steps 1–5 look executed would itself be a P-005
violation, so no such input is created.

**Covering feature:** `059-F` — *Identity-bound no-follow handle for store.rs read-only permission
ops (sibling TOCTOU fix)*, rescoped by the 2026-08-29 re-deliberation (Option B).

**Authorized `handoff_ids` — exactly 10 IDs, canonical (ID-sorted) member set:**

```text
059-F, 059.001-T, 059.002-T, 059.003-T, 059.004-T,
059.005-T, 059.006-T, 059.010-T, 059.011-T, 059.014-T
```

**Exact assembly order (parent-first, then dependency order, per Step 5.5 item 6):**

| # | ID | Unit | Why this position |
|---|---|---|---|
| 1 | `059-F` | covering feature | parent-first; supplied as the initial `items` list at creation |
| 2 | `059.014-T` | U14 sign-off gate | no in-set dependencies; must precede `059.001-T`, which depends on it |
| 3 | `059.001-T` | U1 | deps `059.014-T` (in set) + `059.007-T` (terminal prerequisite, see below) |
| 4 | `059.002-T` | U2 | deps `059.001-T` |
| 5 | `059.006-T` | U6 | deps `059.001-T`, `059.002-T`, `059.007-T` |
| 6 | `059.003-T` | U3 | deps `059.002-T`, `059.006-T` |
| 7 | `059.004-T` | U4 | deps `059.002-T`, `059.006-T` |
| 8 | `059.005-T` | U5 | deps `059.003-T`, `059.004-T`, `059.006-T` |
| 9 | `059.010-T` | U10 | deps `059.003-T`, `059.004-T`, `059.006-T` |
| 10 | `059.011-T` | U11 | deps `059.002-T` |

**Explicitly excluded — already decided, and never to be swept in by queue scanning:**

| ID | Disposition | Reason |
|---|---|---|
| `059.008-T` (U8) | terminal, stays `blocked` | the accepted terminal engine-open feasibility evidence; its `blocked` status *is* the decided result |
| `059.009-T` (U9) | deferred, stays `blocked` | engine-open integration removed from the near-term DoD; deps `059.013-T` + `059.006-T`; later separate shipment |
| `059.012-T` (U12) | deferred, `queued` | later separate shipment; dependency repointed to `059.014-T` so it becomes ready at the correct later gate |
| `059.013-T` (Option A) | deferred, `queued` | upstream/fork engine-open closure; later separate shipment; it is ready **only** in the family-wide query above |

**External terminal prerequisite (cited, NOT a member):** `059.007-T` (U7) is `done` and archived
(`.backlogit/archive/059.007-T.md`). `059.001-T` and `059.006-T` depend on it. Step 5.5 validation
condition (e) permits an out-of-set dependency on *an intentionally terminal prerequisite*; this
decision records `059.007-T` as exactly that. It is **not** added to the successor shipment — a
`done`/archived item is not re-shipped.

**Authorized Stage behaviour after `059.014-T` sign-off:**

Once `059.014-T` is `done`, Stage MAY enter Step 5.5 **directly under Mode R**, citing this section
by path, with Steps 1–5 logged as *not applicable — Mode R recovery path, authorized by
`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`*. Stage MUST
NOT invent stash entries, re-run `harvest` over these already-harvested items, or stand up any
synthetic harvest. Stage MUST then, in order:

1. Record the 10 IDs above verbatim as `handoff_ids` — the set is exactly this list; never widen it
   by scanning the queue for unassigned, ready-looking, or topically related `059.*` items.
2. Re-run the full Step 5.5 item-2 validation against live state at that time:
   (a) each ID exists as a live queue item; (b) each belongs to the `059-F` hierarchy named here;
   (c) none is a member of another queued/active shipment; (d) each carries a status ratified in a
   citable artifact — this decision; (e) no unresolved dependency outside the set except
   `059.007-T` (terminal prerequisite) and `059.014-T` (in-set gate). Reject on duplicates or any ID
   this section did not name. **Halt fail-closed** on any failure — no partial assembly.
3. Set `assembly_ids` = the validated `handoff_ids`, then create/reuse the shipment and add members
   in the exact order tabled above, and verify the read-back manifest matches the 10 IDs exactly.

**Validation snapshot at the time of this decision** (informational; re-verify at assembly time):
(a) all 10 present in `.backlogit/queue/`; (b) all under `059-F`; (c) no `059.*` item appears in any
open shipment manifest — the only queued shipments are `049-S`, `052-S`, `053-S`, all `056.*`;
(d) statuses ratified above; (e) the only out-of-set dependency edges are on `059.007-T`.

**Gate, restated:** this authorization supplies *scope*, never *gate relief*. It does not permit
assembly now. `059.014-T` is `queued`; until it is `done`, Stage assembles nothing. No shipment is
created, claimed, reused, or modified by this pass.

## Supersession of the PR #113 `051-S` Closure-Timing Requirement

**Superseded requirement.** The PR #113 transition authority
(`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`,
§ *Ship-Side Transition*, step 2 and its struck "alternative") required `051-S` to be resolved
**only after** `059.014-T` sign-off. The Ship-owned reconcile artifact
`.backlogit/reconcile/051-S-safe-close-20260829-203729.md` cites that section as its closure
authority, but `051-S` was archived on 2026-08-29 while `059.014-T` was — and still is — `queued`.
The later "enacted" wording added on 2026-08-30 scoped the sign-off condition to successor
assembly; it did **not** by itself establish prior authority to drop the closure precondition.
The review is correct: PR #113 cannot serve as the stated authorization for the timing that
actually occurred.

**Decision (Stage, 2026-08-30): the timing requirement is explicitly superseded.** This section is
the "explicit decision that supersedes the timing requirement". Stage does **not** restore `051-S`.

**Evidence-based rationale.**

1. **`051-S` was an evidence shipment, not the implementation shipment.** Its post-return manifest
   was `[059.007-T]` — the U7 feasibility unit, `done` and already archived by PR #111. Its purpose
   (produce U7/U8 feasibility evidence) was complete. The sign-off gate `059.014-T` exists to gate
   *implementation of the rescoped scope*, which `051-S` no longer carried.
2. **The safe-close archived only delivered work.** Per the reconcile record, the run archived
   exactly one item — `059.007-T`, already `done`/pre-archived — plus the `051-S` shipment record
   itself. The destructive cascade `backlogit_ship_shipment` was never called, and the 14-member
   protected set was verified intact before and after.
3. **Non-terminal members were returned status-preservingly, not resolved.** `059-F` and
   `059.008-T` were removed from the manifest with `return-blocked`; neither status changed (both
   stayed `blocked`) and neither was archived. Closing the shipment therefore consumed no
   nonterminal work and pre-empted no decision about it.
4. **The closure accepted no residual risk and began no implementation.** Residual-risk acceptance
   lives solely in the Accepted-Residual-Risk Record and its sign-off gate `059.014-T`, which
   remains `queued` and unsigned. No `059-F` implementation unit moved to `active` or `done`, and
   none can while `059.014-T` is open. Nothing the sign-off gate protects was consumed by the
   closure.

**Honest sequencing record.** There was still a real sequencing mismatch: the closure ran on
2026-08-29 under a written precondition that had not yet been superseded, and the reconcile
artifact's cited authority was therefore stale at the moment it was written. That is recorded here
as fact, not excused. Ship also could not have cured it unilaterally — re-scoping or creating a
successor shipment is Stage-only under fail-closed P-010. The correct sequence would have been:
Stage supersedes the timing requirement first, then Ship safe-closes.

**Current rule (authoritative from this decision forward).**

* Closure of an **evidence** shipment MAY precede `059.014-T` sign-off, provided it archives only
  delivered (`done`) members, returns every non-terminal member status-preservingly, accepts no
  residual risk, and starts no implementation.
* `059.014-T` gates **successor-shipment assembly and implementation of the rescoped `059-F`
  scope only**. It has never gated status normalization, and it no longer gates evidence-shipment
  closure.
* Closure of an **implementation** shipment carrying the rescoped scope remains gated on sign-off.

**This is not a retroactive security sign-off.** Nothing here signs off the Accepted-Residual-Risk
Record, accepts the engine-open leaf/intermediate-directory redirection residual, or advances
`059.014-T`, which stays `queued`. It supersedes a *shipment-lifecycle timing precondition* only.
The four standing Ship violations remain un-legalized, and the tracked closure path `059.013-T` is
unaffected.

## Scope of This Stage Pass

* Planning/backlog mutation only. No source, test, or config changes; no shipment created, claimed,
  closed, or archived; no PR body or thread edits; no merge.
* No edits to Ship closure/compound/memory artifacts; the compound-doc Step 4 remediation is
  Ship-owned follow-up.
* `.gitignore` and `docs/scratch/` are preserved untouched.

### Reconciliation pass addendum (2026-08-30, PR #114 HEAD `242b5e3`)

This decision was extended in a follow-on Stage reconciliation pass that added, above: the
feature-family-wide relabel of the readiness query; the normalization-complete vs
assembly/implementation-gated split; the durable Step 5.5 Mode R authorization with the exact
10-ID handoff set and assembly order; and the supersession of the PR #113 `051-S` closure-timing
precondition. That pass changed **no** item status, **no** dependency edge, and **no** shipment;
`059.014-T` remains `queued` and unsigned. It touched only Stage-owned
decision/plan/memory/backlog artifacts — the Ship-owned reconcile record
`.backlogit/reconcile/051-S-safe-close-20260829-203729.md`, closure artifacts, compound
best-practice docs, Ship transition memory, and agent definitions were left unmodified; the
superseding authority is recorded here instead.

## Follow-up Left to Ship

Ship, on its next cycle, should correct
`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
Step 4 so the reusable procedure hands the un-normalized, `return-blocked` scope to Stage and lets
Stage perform the `blocked → queued` normalization, consistent with the ownership division decided
here. That is a Ship closure/compound edit and is intentionally out of scope for this Stage pass.
