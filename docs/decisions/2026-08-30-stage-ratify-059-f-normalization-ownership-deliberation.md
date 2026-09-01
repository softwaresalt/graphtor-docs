---
title: "Stage ratification: 059-F rescoped-scope status normalization and successor-shipment assembly are Stage-owned (P-010 remediation)"
description: "Independent Stage review that ratifies the already-queued dispositions of the 059-F feasible scope, assigns blocked-to-queued status normalization and successor-shipment assembly to Stage, issues the durable Step 5.5 Mode R authorization naming two disjoint exact sets (8 task-only member_ids plus 2 prerequisite_ids, a 10-ID audit union; the partial-feature covering root 059-F is protected, never a member), supersedes the PR #113 051-S closure-timing precondition, and records the prior Ship-performed normalization as an un-legalized P-010 violation"
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

| Operation | `_ship.agent.md` Role Boundary | `_stage.agent.md` Role Boundary |
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
(`backlogit sync` then `backlogit query`) for the 10-item near-term context — defined here, and
only here, as the **9 future shipment members** (`059-F` + `059.001/002/003/004/005/006/010/011-T`)
plus the **queued sign-off gate** `059.014-T`. That count is a verification convenience, never a
Mode R assembly set: the gate is a prerequisite and is excluded from shipment membership.
Authoritative status and dependency edges:

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
| `blocked → queued` status normalization of the covering feature + eight implementation tasks = **nine normalized members** (`059-F`, `059.001-T`, `059.002-T`, `059.003-T`, `059.004-T`, `059.005-T`, `059.006-T`, `059.010-T`, `059.011-T`) | Stage-only (P-010) | **Already completed** — performed by Ship in violation, then **independently reviewed and ratified by Stage** on 2026-08-30 while `059.014-T` is still `queued` | **No** |
| Rewire of the feasible DAG (U8/U9 edge drops, U12 re-point) | Stage-only | **Already completed** and ratified | **No** |
| Successor-shipment assembly (Step 5.5) | Stage-only | **Not started** | **Yes** |
| Implementation of the rescoped `059-F` (U1 onward) | Ship, after assembly | **Not started** | **Yes** |

**The normalized scope is nine members, not ten.** `059.014-T` is **not** one of the normalized
members: it was never `blocked`, was never normalized, and has remained `queued` throughout. It is
the operator sign-off gate — a **prerequisite**, not a member of the normalized scope and not a
member of the successor shipment. Any count that reaches ten by adding `059.014-T` to the nine
normalized members is conflating the member set with the gate set.

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
`.github/agents/_stage.agent.md` (Step 5.5, item 1, *Mode R — ratified existing-scope handoff*).
It exists because the items below were harvested in an earlier session, already exist in the
queue, and this PR intentionally creates **no** stash entry and **no** new harvest. Under Mode H the
scope guard would exclude them; Mode R is the sanctioned path for exactly this case. Manufacturing a
stash entry or a synthetic harvest to make Steps 1–5 look executed would itself be a P-005
violation, so no such input is created.

Per the Mode R contract, this authorization names **two disjoint exact sets** — `member_ids` and
`prerequisite_ids` — and nothing else. `handoff_ids` is their union and exists only as an audit and
citation convenience; it is never an assembly set. `assembly_ids` is exactly the validated
`member_ids`.

**Covering feature:** `059-F` — *Identity-bound no-follow handle for store.rs read-only permission
ops (sibling TOCTOU fix)*, rescoped by the 2026-08-29 re-deliberation (Option B).

**Authorized `member_ids` — exactly 8 task IDs, listed in the authoritative parent-first
dependency assembly order. These and only these become `assembly_ids`:**

```text
059.001-T, 059.002-T, 059.006-T, 059.003-T,
059.004-T, 059.005-T, 059.010-T, 059.011-T
```

`059.006-T` precedes `059.003-T`, `059.004-T`, `059.005-T`, and `059.010-T` because each of those
depends on it. This list is published in dependency order — not ID-sorted — because Mode R records
`member_ids` verbatim and treats them as the ordered candidate `assembly_ids` and as the CLI
`--items` string, so the set and the ordered transport input must be one and the same.

**The covering feature `059-F` is deliberately NOT a member.** This is a **partial-feature
shipment**: `059-F` retains five live children outside the manifest (`059.008-T` and `059.009-T`
are `blocked`; `059.012-T`, `059.013-T`, and `059.014-T` are `queued`). Under P-015 the covering
feature plus every unshipped sibling form the **protected set**, which safe-close must leave intact
in `.backlogit/queue/`. Including `059-F` in the manifest would make safe-close archive it and
strand those queued children under an archived parent. The `049-S` precedent likewise excluded its
covering feature `056-F`.

**Authorized `prerequisite_ids` — exactly 2 IDs. Gates, never members, never counted in the
manifest:**

| ID | State at authorization time | Satisfaction condition |
|---|---|---|
| `059.007-T` (U7) | `done` and archived (`.backlogit/archive/059.007-T.md`) | **Already satisfied.** Absence from the live queue is expected for a satisfied prerequisite and is not a validation failure — Step 5.5 condition 2a(a) does not apply to prerequisites. `059.001-T` and `059.006-T` depend on it; condition 2a(e) permits that out-of-set edge because this decision records it as an intentionally terminal prerequisite. |
| `059.014-T` (U14) | `queued` — the operator sign-off gate | **Not yet satisfied.** It must itself become `done` and archived after operator sign-off before it counts as satisfied. It is never normalized into a member to make assembly proceed. |

**`handoff_ids` — the auditable union of the two sets above (10 IDs) and nothing more:**

```text
059.001-T, 059.002-T, 059.003-T, 059.004-T, 059.005-T,
059.006-T, 059.007-T, 059.010-T, 059.011-T, 059.014-T
```

`handoff_ids` is a citation and audit convenience only. It is **never** `assembly_ids`, no ID is
added to a shipment merely because it appears in the union, and neither set is derived from the
other. The two sets are disjoint: no ID appears in both.

**No shipment is created, populated, or handed off until BOTH prerequisites are satisfied** —
`059.007-T` already is; `059.014-T` is not, and stays a gate until operator sign-off moves it to
`done`/archived.

**Exact assembly order — the 8 `member_ids` only (parent-first, then dependency order, per Step 5.5
item 6). Neither a `prerequisite_ids` entry nor the covering feature appears in this order:**

| # | ID | Unit | Why this position |
|---|---|---|---|
| 1 | `059.001-T` | U1 | first implementation unit; both its dependencies (`059.007-T`, `059.014-T`) are prerequisites outside the member set, satisfied before assembly begins |
| 2 | `059.002-T` | U2 | deps `059.001-T` |
| 3 | `059.006-T` | U6 | deps `059.001-T`, `059.002-T` (in set) + `059.007-T` (prerequisite) |
| 4 | `059.003-T` | U3 | deps `059.002-T`, `059.006-T` |
| 5 | `059.004-T` | U4 | deps `059.002-T`, `059.006-T` |
| 6 | `059.005-T` | U5 | deps `059.003-T`, `059.004-T`, `059.006-T` |
| 7 | `059.010-T` | U10 | deps `059.003-T`, `059.004-T`, `059.006-T` |
| 8 | `059.011-T` | U11 | deps `059.002-T` |

The covering feature `059-F` is **not** supplied as the initial `items` list at creation: this is a
partial-feature shipment, so `059-F` stays in the P-015 protected set (see the membership
correction addendum below).

**Explicitly excluded — already decided, and never to be swept in by queue scanning:**

| ID | Disposition | Reason |
|---|---|---|
| `059.008-T` (U8) | terminal, stays `blocked` | the accepted terminal engine-open feasibility evidence; its `blocked` status *is* the decided result |
| `059.009-T` (U9) | deferred, stays `blocked` | engine-open integration removed from the near-term DoD; deps `059.013-T` + `059.006-T`; later separate shipment |
| `059.012-T` (U12) | deferred, `queued` | later separate shipment; dependency repointed to `059.014-T` so it becomes ready at the correct later gate |
| `059.013-T` (Option A) | deferred, `queued` | upstream/fork engine-open closure; later separate shipment; it is ready **only** in the family-wide query above |

**Why `059.014-T` is a prerequisite and never a member:** once sign-off lands it moves to `done` and
is archived, so it can never satisfy the live-queue member validation in Step 5.5 condition 2a(a).
Treating it as a member would force either a validation failure or an illegitimate exception. Its
gating force comes from the `blocks` dependency edge `059.001-T ← 059.014-T`, not from shipment
membership. Restating the prerequisite rules from the table above: a prerequisite is satisfied,
never shipped, and never converted into a member to make assembly proceed; and `059.007-T` being
`done`/archived — absent from the live queue — is the expected shape of a satisfied prerequisite,
not a validation failure.

**Authorized Stage behaviour after `059.014-T` sign-off:**

Once `059.014-T` is `done` and archived — and therefore both `prerequisite_ids` entries are
satisfied — Stage MAY enter Step 5.5 **directly under Mode R**, citing this section
by path, with Steps 1–5 logged as *not applicable — Mode R recovery path, authorized by
`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`*. Stage MUST
NOT invent stash entries, re-run `harvest` over these already-harvested items, or stand up any
synthetic harvest. Stage MUST then, in order:

1. Record the two sets above verbatim — the 8 `member_ids` and the 2 `prerequisite_ids`. Never
   widen either set by scanning the queue for unassigned, ready-looking, or topically related
   `059.*` items, and never promote a prerequisite into the member set.
2. Re-run the full Step 5.5 item-2 validation, applying each set's own rules against live state at
   that time. **2a — every `member_ids` entry:** (a) exists as a live queue item; (b) belongs to the
   `059-F` hierarchy named here; (c) is not a member of another queued/active shipment; (d) carries
   a status ratified in a citable artifact — this decision; (e) has no unresolved dependency outside
   the declared sets except the `prerequisite_ids` entries `059.007-T` and `059.014-T`.
   **2b — every `prerequisite_ids` entry:** (a) is `done` or otherwise satisfied, resolved in the
   queue *and* the archive (absence from the live queue is expected, not a failure — condition
   2a(a) must not be applied here); (b) is never added to the shipment or counted in the manifest;
   (c) if still queued or unsatisfied, assembly halts and waits. Reject on duplicates, on any
   intersection between the sets, or on any ID this section did not name. **Halt fail-closed** on
   any failure — no partial assembly.
3. Set `assembly_ids` = the validated `member_ids` (never `handoff_ids`, and never including a
   `prerequisite_ids` entry), then create/reuse the shipment and add members in the exact 8-item
   order tabled above, and verify the read-back manifest matches those 8 IDs exactly — same count,
   same IDs, no extras, no prerequisite present, and no covering feature.

**Validation snapshot at the time of this decision** (informational; re-verify at assembly time):
(a) all 8 `member_ids` present as live items in `.backlogit/queue/`; (b) all under `059-F`; (c) no
`059.*` item appears in any open shipment manifest — the only queued shipments are `049-S`, `052-S`,
`053-S`, all `056.*`; (d) statuses ratified above; (e) the only out-of-set dependency edges are on
the `prerequisite_ids` entries `059.007-T` and `059.014-T`. Prerequisite state: `059.007-T` is
`done`/archived (satisfied); `059.014-T` is `queued` (**not** satisfied), so assembly stays blocked.

**Gate, restated:** this authorization supplies *scope*, never *gate relief*. It does not permit
assembly now. Assembly stays blocked while any `prerequisite_ids` entry is unsatisfied: `059.007-T`
is satisfied (`done`/archived), but `059.014-T` is `queued`, so until it is `done` and archived
Stage assembles nothing. No shipment is created, claimed, reused, or modified by this pass.

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
assembly/implementation-gated split; the durable Step 5.5 Mode R authorization and assembly order;
and the supersession of the PR #113 `051-S` closure-timing
precondition. That pass changed **no** item status, **no** dependency edge, and **no** shipment;
`059.014-T` remains `queued` and unsigned. It touched only Stage-owned
decision/plan/memory/backlog artifacts — the Ship-owned reconcile record
`.backlogit/reconcile/051-S-safe-close-20260829-203729.md`, closure artifacts, compound
best-practice docs, Ship transition memory, and agent definitions were left unmodified; the
superseding authority is recorded here instead.

### Mode R partition-alignment addendum (2026-08-30, PR #114 HEAD `378444e`)

The Mode R contract in `.github/agents/_stage.agent.md` (named `.stage.agent.md` at the time of
that commit; renamed during the autoharness 1.5.0 merge-install) was corrected by commit `378444e` to require
**two disjoint exact sets** rather than a single handoff list. That reconciliation pass had written
the authorization as one 10-ID `handoff_ids` set that folded the sign-off gate `059.014-T` into the
member list and into the assembly order. This addendum records the alignment applied here:

* `member_ids` is exactly 8 — the eight implementation tasks — and is the only
  thing that becomes `assembly_ids`.
* `prerequisite_ids` is exactly 2 — `059.007-T` (already `done`/archived, satisfied) and
  `059.014-T` (`queued` until operator sign-off, then `done`/archived). Neither is ever a member.
* `handoff_ids` is the 10-ID auditable union of those two sets and nothing more; it is never an
  assembly set.
* The assembly order lists the 8 member IDs only, in parent-first dependency order;
  `059.014-T` was removed from position 2.
* The normalized-scope count is nine members. `059.014-T` was never normalized: it has been `queued`
  since creation and is a prerequisite, not a member.

### Partial-feature membership correction addendum (2026-08-31, PR #114 review)

A later PR #114 review finding established that the covering feature `059-F` MUST NOT be a shipment
member. `059-F` retains five live children outside the manifest (`059.008-T`, `059.009-T`,
`059.012-T`, `059.013-T`, `059.014-T`), which makes this a **partial-feature shipment**. Under
P-015 the covering feature and every unshipped sibling form the protected set that safe-close must
leave intact; including `059-F` in the manifest would archive it and strand those children under an
archived parent. Superseding the counts recorded immediately above:

* `member_ids` is exactly **8** task IDs — `059-F` removed.
* `handoff_ids` is therefore a **10**-ID union (8 members + 2 prerequisites), a different
  composition from the earlier, unrelated 10-ID set that had folded `059.014-T` into the members.
* `member_ids` is published in **dependency order**, not ID order, because Mode R consumes the list
  verbatim as the ordered `assembly_ids` and CLI `--items` string.
* Mode R assembly may be **task-only**; `.github/agents/_stage.agent.md` was corrected in the same
  pass to include a covering feature only when the shipment fully covers it.
* The `049-S` precedent, which excluded its covering feature `056-F`, is the governing pattern.

No item status, dependency edge, shipment, or sign-off state changed in this pass either.

## Follow-up Left to Ship

Ship, on its next cycle, should correct
`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
Step 4 so the reusable procedure hands the un-normalized, `return-blocked` scope to Stage and lets
Stage perform the `blocked → queued` normalization, consistent with the ownership division decided
here. That is a Ship closure/compound edit and is intentionally out of scope for this Stage pass.
