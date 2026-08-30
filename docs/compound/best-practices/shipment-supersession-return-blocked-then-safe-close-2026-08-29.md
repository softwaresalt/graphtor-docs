---
title: "Shipment supersession pattern: return-blocked non-terminal manifest items, safe-close the evidence shipment, prepare the rescoped scope for Stage to assemble"
description: "When an active shipment's manifest mixes a done evidence task with a still-blocked feature/task that must never be cascade-archived, return the non-done items from the manifest first so shipment-reconcile's pre-mode expected_status check and safe-close protected-set computation both work correctly, then close the now-single-item shipment, identify the rescoped feasible scope, and hand it off — un-normalized — to Stage, which exclusively normalizes every superseded-chain member to an intake-valid status and decides on successor-shipment assembly, per both agents' NON-NEGOTIABLE P-010 role boundaries; if Ship mistakenly creates a shipment, operator approval satisfies P-005 destructiveness only — it never grants Ship the P-010 authority to delete it, so Ship must halt with ActionResult blocked and hand the deletion to the operator or a separately authorized recovery path, never execute it itself"
problem_type: "workflow-handoff"
category: "best-practices"
component: "shipment-reconcile skill + Ship Step 6 closure, backlogit shipment lifecycle"
root_cause: "shipment-reconcile's pre-mode classifies any manifest item whose status does not match expected_status as status-mismatch (HALT); a shipment whose original scope produced a mixed outcome (one item done, one item terminally blocked, the covering feature itself blocked) cannot be safe-closed as-is without first removing the non-terminal members from the manifest, and any superseded-dependency-chain member later folded into a successor shipment while still `blocked` will fail that successor's own future intake"
resolution_type: "workaround"
severity: "high"
message: "051-S manifest [059-F, 059.007-T, 059.008-T]: 059-F blocked, 059.008-T blocked (terminal evidence), 059.007-T done"
file_path: ".github/skills/shipment-reconcile/SKILL.md"
citations:
  - "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
  - "docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md"
  - "docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md"
  - "docs/memory/2026-08-30/stage-059-f-normalization-ratification-memory.md"
  - "docs/closure/2026-08-29-051-s-toctou-transition-closure.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/113"
  - "https://github.com/softwaresalt/graphtor-docs/pull/114"
tags:
  - "shipment"
  - "backlogit"
  - "ship-workflow"
  - "reconciliation"
  - "role-boundary"
  - "audit-trail"
---

## Problem

Shipment `051-S`'s manifest was `[059-F, 059.007-T, 059.008-T]` after a
feasibility spike produced a **mixed** outcome: `059.007-T` (U7) reached
`done`/archived, but `059.008-T` (U8) reached a **terminal `blocked`**
(structurally infeasible, not "not yet started"), and the covering feature
`059-F` stayed `blocked` as a result. A later re-deliberation decision
(`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`)
accepted U8's BLOCKED result as a decided input and authorized closing
`051-S` while rescoping the still-feasible units for a successor shipment.
Running `shipment-reconcile mode: pre` with `expected_status: done` directly
against the original three-item manifest would immediately `HALT` with
`status-mismatch` on both `059-F` and `059.008-T` — neither is `done`, and
neither should become `done` or be archived **by this `051-S` closure
operation**. That prohibition is scoped to the closure itself, not a
permanent claim about both items alike: `059-F` is a live feature, honestly
`blocked` pending operator sign-off, and is expected to reach `done` later
once sign-off lands and implementation completes on whatever successor
shipment a future Stage session assembles. Only `059.008-T`'s (U8) BLOCKED
status is the correct, **permanent** evidence record — it is not expected
to change, and is not re-evaluated by this closure or by sign-off.

A second, independent problem surfaced once the rescoped scope was
gathered: of the 10-item near-term scope (the 9 future Mode R shipment
`member_ids` — `059-F` + the eight implementation tasks
`059.001/002/003/004/005/006/010/011-T` — plus the then-queued sign-off
gate `059.014-T`, a `prerequisite_ids` entry never itself a shipment
member), **nine members — `059-F` plus the eight implementation
tasks — carried `status: blocked`**, not just the two returned from
`051-S` in this same session; most of them had already been returned
`blocked` from `051-S` in an *earlier* session, once the original
engine-boundary dependency chain (gating them on the now-terminally-
BLOCKED U8) was in force. The tenth item in this near-term count, the
sign-off gate `059.014-T`, was **created and remains `queued`** — it never
carried a superseded `blocked` status, required no normalization, and is
never a Mode R shipment member. A superseded
dependency-chain member's `status` field does not self-correct when the
*dependency edges* are later rewired (e.g. by a Stage re-deliberation
pass) — status and the dependency graph are independent in this schema.

## Root Cause

`shipment-reconcile`'s pre-mode and safe-close protocol assume every
manifest item that isn't `pre-archived` will reach the shipment's declared
`expected_status`. There is no built-in path for "this manifest item's
correct terminal state is `blocked`, not `done`, and it must be preserved
in queue, not archived, when the shipment closes." Separately, any
successor-scope member whose `blocked` status originated from a now-
superseded dependency chain will fail a **future** shipment's own intake
reconciliation (`expected_status: queued`) unless its status is normalized
first — regardless of *when* it was returned or how it came to be
`blocked`.

## Resolution

1. **Return non-terminal-`done` manifest items first**, one call per item,
   with an evidence-citing reason:
   ```bash
   backlogit shipment return-blocked --shipment 051-S --item 059-F --reason "..."
   backlogit shipment return-blocked --shipment 051-S --item 059.008-T --reason "..."
   ```
   `return-blocked` removes the item from `custom_fields.items` **without**
   changing its `status` (it stays exactly `blocked`) — this is the key
   property that makes it safe here, unlike `backlogit move`/`archive`.
   **Role-boundary authorization (2026-08-30, `242b5e3`)**: Ship's Role
   Boundary Allowed column now explicitly enumerates this narrow,
   status-preserving `return-blocked` operation, scoped to
   `shipment-reconcile`/safe-close and recording only the exact blocked
   reason that operation requires — it confers no broader item-planning
   authority (no scope/acceptance-criteria/dependency/priority change, and
   no blocked-to-queued normalization). This closes a latent policy gap a
   holistic PR #114 review identified; it does not retroactively legalize
   any of the four historical violations recorded below, none of which was
   the `return-blocked` call itself.
2. **Re-run `shipment-reconcile mode: pre`** with `expected_status: done`
   against the now-single-item manifest (`[059.007-T]`, already archived) —
   classifies as `pre-archived`, gate returns `PROCEED`.
3. **Run `safe-close`**: because the covering feature (`059-F`) is no longer
   in the manifest, the protected-set computation automatically treats
   `059-F` **and every other sibling task not in the manifest** (all
   returned/never-included `059.*` items) as protected. The baseline gate
   proves all of them are still in `.backlogit/queue/` before archiving
   anything; the verify-after-each invariant re-confirms it after the
   (here, no-op, since the sole manifest item was `pre-archived`) archival
   loop and after the shipment record itself is archived. This gives a
   **stronger** safety guarantee than manually eyeballing which items to
   protect — the mechanism is generic and doesn't need to know in advance
   which items are "supposed" to survive.
4. **Identify the full rescoped scope and hand it, un-normalized, to
   Stage — do NOT run `backlogit move <id> --status queued` as Ship.**
   Fail-closed P-010 role enforcement
   (`.github/instructions/role-enforcement.instructions.md`) treats any
   backlog state mutation not listed in Ship's Allowed column as
   forbidden. `.github/policies/workflow-policies.md` lists "move tasks to
   active/done" as Ship-allowed; a `blocked → queued` planning-shaping
   status change is a different, unlisted mutation and therefore defaults
   to forbidden for Ship. It is, however, explicitly Stage-allowed
   ("update backlog items"). **A prior version of this pattern had Ship
   run this normalization directly. A later, independent Stage
   ratification
   (`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`)
   confirmed that mutation was itself an un-legalized P-010 violation —
   distinct from the shipment-creation violation in step 6 below — even
   though the resulting `queued` disposition turned out, on independent
   review, to be semantically correct. Do not repeat it.** Instead, Ship:
   1. Identifies the full rescoped scope from the governing decision
      document (the feature plus every unit named feasible), regardless of
      which prior session returned each item from a shipment or whether it
      was ever in a shipment at all;
   2. Records, for every scope member, its current `status` value and its
      full `depends_on` dependency-edge context (which edges are satisfied,
      which are internal to the scope, which point at an already-`done`
      item outside the scope);
   3. Hands this off in the closure/memory artifacts — never as a stash
      entry, since stash operations are equally Stage-only under the same
      P-010 list — so Stage can independently review and decide
      normalization for itself.
   4. **Never directly edits a returned item's `blocked_reason` or other
      `custom_fields` planning text, even for a "clarifying wording
      fix."** That is also an unclassified item-planning mutation,
      fail-closed forbidden for Ship. **A prior version of this pattern
      had Ship do exactly this** — rewording `059.008-T`'s
      `blocked_reason` for clarity after it had already been returned
      from `051-S`. A later, independent Stage ratification
      (`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`,
      converged `63f933a`, persisted as durable tracked evidence in
      `303106c`) confirmed that edit was itself an un-legalized
      **fourth**, distinct P-010 violation — separate from the
      status-normalization violation above and the
      shipment-creation/deletion violations in step 6 below — even
      though the resulting text turned out, on independent review, to be
      semantically correct. If a returned item's planning-field wording
      needs correction, hand off the proposed text and rationale to
      Stage in the closure/memory artifacts instead of editing it
      directly.
5. **Stage normalizes every superseded-chain member of the handed-off
   scope to an intake-valid status.** This step belongs to Stage, not
   Ship; it is described here only so the reusable procedure documents
   the full two-agent handoff. Stage identifies each member whose *only*
   remaining blocker is a dependency edge on another scope member or an
   already-`done`/gate item (not a terminal/evidence-based block) and
   transitions it directly:
   ```bash
   backlogit move <id> --status queued
   ```
   This is a valid direct `blocked → queued` transition in this backlogit
   version (verified — no intermediate `active` hop required, unlike the
   `queued → done` FSM constraint documented in
   `backlogit-status-transitions-2026-05-02.md`). It clears any
   `custom_fields.blocked_reason` on that item as a side effect (verified);
   preserve the narrative in git history / the governing decision doc
   instead of relying on the live field. Stage does **not** apply this to
   items whose block is itself the terminal, decided evidence (e.g. a
   feasibility spike that proved infeasible) — those stay `blocked` and
   stay **out of** the rescoped scope entirely. Skipping any affected
   member (for example, normalizing only the members returned in the
   current session while leaving pre-existing `blocked` siblings
   untouched) reproduces the same intake-mismatch failure for a future
   shipment attempt.
6. **Stage decides on and assembles the successor shipment — Ship never
   assembles it.** `.github/policies/workflow-policies.md`'s P-010
   definition is unconditional: **"Ship MUST NOT: Create backlog items,
   create shipments..."**, and **"Do not proceed past the boundary even if
   the operator requests work outside scope — redirect to the correct
   agent instead."** An earlier revision of `.ship.agent.md`'s Step 0.5
   included an operator-confirmed direct-assembly fallback that permitted
   Ship to assemble a shipment itself once the operator confirmed intent;
   that carve-out was removed in
   `ea47df004755e155947a51be0e36e362601279de`. Under the current,
   authoritative Step 0.5, Ship's only choices at shipment-assembly time
   are to **select an existing Stage-prepared shipment** or to
   **halt/redirect to Stage** — there is no operator-confirmation path
   that permits Ship to assemble a shipment directly, even when a
   governing decision document frames fresh-shipment assembly as "Ship's
   choice." The canonical policy document and the current Step 0.5 text
   are in agreement, not in tension.

   **If Ship ever creates a successor shipment in violation of this
   boundary, do not compound the error by deleting it unilaterally.**
   Deleting a backlog artifact (`backlogit delete <id> --force`) is itself
   a destructive action under Constitution Principle VII / P-005 and the
   strict-safety `ProposedAction`/`ActionRisk: destructive` contract — it
   requires real-time operator approval before execution, regardless of
   whether the artifact being deleted was itself created in error.
   **A prior version of this pattern ran that delete without obtaining
   approval; that remains a separate, unresolved P-005 violation, not a
   compliant remediation of the P-010 finding it was responding to** —
   deleting a P-010 violation does not exempt the deletion from its own
   destructive-command approval gate.
   **Operator approval addresses only the command's destructiveness
   (P-005); it never grants role authority a Role Boundary withholds
   (P-010).** `backlogit_delete_item` is Forbidden for Ship regardless of
   approval state — an approved destructive command is still a P-010
   violation when the acting agent's Role Boundary forbids that category
   of mutation. The correct response to a mistakenly created shipment is,
   therefore, one of:
   * halt, record the exact `ProposedAction` (the identified artifact and
     the deletion it would require), `ActionRisk: destructive`, and
     `ActionResult: blocked`, and hand the cleanup to the operator or to a
     separately authorized recovery executor/path — Ship itself must
     never execute the deletion, even after approval is granted, because
     approval cannot substitute for the P-010 authority Ship's Role
     Boundary withholds; or
   * leave the artifact in place, unclaimed and unshipped, and hand it to
     Stage/the operator as a recovery item in the closure/memory
     artifacts, recording the mistaken creation as an open P-010 finding
     until the operator or Stage resolves it.

   Never instruct Ship to delete its own mistaken artifact as a matter of
   routine remediation, and never instruct Ship to run the deletion itself
   even once approval is obtained. The correct handoff for the legitimate case (no
   mistaken shipment exists) is precisely the step 4/5 sequence, not a
   shortcut around it: Ship leaves and hands off, in the closure/memory
   artifacts, every scope member's **un-normalized** current `status` and
   full dependency-edge context — no shipment membership, no status
   mutation performed by Ship; Stage independently validates that handoff
   and performs any normalization it decides is warranted (`blocked →
   queued` for superseded-chain members whose block is not terminal
   evidence); only **after** Stage's normalization do the resulting
   `queued`, dependency-closed items constitute the prepared scope that a
   future Stage session assembles into a successor shipment.
7. **Once Stage completes normalization, verify intake-readiness of the
   prepared scope**: confirm every scope member's `status` equals `queued`
   (or `active` if some are already claimed elsewhere) — the same check
   Step 0.5 will perform when Stage's successor shipment is later claimed
   — so the handoff is genuinely actionable, not just structurally
   present.

## Audit-Trail Caveat: Hook/Log Replay Is Incomplete for Deletions and Custom-Field Mutations (broadened, 2026-08-30)

A later holistic correctness review of PR #114 (2026-08-29, post-closure)
confirmed a **backlogit audit-trail limitation** directly relevant to
step 6's `backlogit delete <id> --force` remediation path:
`.backlogit/hooks_queue.jsonl` (seq `1157`) and
`.backlogit/logs/054-S.jsonl` record only the artifact's
`create_artifact`/`shipment_created` event — the subsequent `backlogit
delete 054-S --force` emitted **no** deletion or tombstone event in
either file. Replaying either file in isolation would therefore falsely
infer the deleted artifact remains queued.

A further frozen-diff reconciliation pass (2026-08-30) found the gap is
**not unique to deletion**: `.backlogit/hooks_queue.jsonl` contains
**zero** entries tagged `"custom_fields"` anywhere in the file. The
`return-blocked` calls in step 1 above (which mutate
`custom_fields.blocked_reason`) each leave an `item_blocked` entry in the
affected item's own per-item log (`.backlogit/logs/<id>.jsonl`) but no
corresponding entry in the central `hooks_queue.jsonl`; and the direct
Ship `blocked_reason` edit that produced the fourth historical P-010
violation (step 4 above) left **no** entry in either log at all.

**This is a tool limitation, not something to work around by hand-editing
append-only hook/log files or inventing a synthetic tombstone or any
other event** — either action would corrupt tool-managed state this
workspace's `backlogit` overlay treats as authoritative.

**Source-of-truth ordering for deletes and custom-field mutations
(corrected, narrower claim than the prior wording)**: prefer the
**artifact store and current structured query state** (`backlogit get
<id>`, `backlogit sync` artifact count, direct file existence /
`git status --short` against `.backlogit/queue/<id>.md` and
`.backlogit/archive/<id>.md`) over **replay-only hook/log history**
whenever confirming that a delete, `return-blocked` call, or other
`custom_fields`/planning-field mutation occurred and completed. Hook/log
replay is proven reliable only for `create_artifact` and top-level
`status`-change events; the prior wording's claim that it "remains
reliable for creation, status-change, and other non-delete mutations" is
not proven and is withdrawn — `custom_fields`/`blocked_reason` mutations
are inconsistently captured, sometimes only in a per-item log and
sometimes not at all.

**Required evidence going forward**: any future operator-approved
destructive recovery of a mistakenly created backlog artifact, **or any
manifest/custom-field mutation on an existing artifact (`return-blocked`,
direct `blocked_reason`/`custom_fields` edits, etc.)**, MUST capture
explicit **before/after structured query evidence** at the time of the
action (e.g. `backlogit get <id>` before and after; `backlogit sync`
artifact count before and after; file-existence/`git status --short` on
the artifact's queue/archive path before and after; a before/after diff
of the mutated field's text) — do not rely on the hook/log stream alone
to prove the mutation occurred, since neither `backlogit delete --force`
nor `custom_fields` edits are guaranteed to emit a corresponding
lifecycle event. Never synthesize or tamper with hook/log entries to
compensate for a gap in captured evidence.

## Prevention

When a shipment's manifest will end in a mixed done/blocked/terminal-blocked
outcome, plan the closure as "return non-`done` items → reconcile pre →
safe-close → **identify the full rescoped scope and hand it, un-normalized,
to Stage (never run `blocked → queued` normalization as Ship)** → **Stage
normalizes every superseded-chain member to `queued` and decides on
shipment assembly (never assemble it as Ship, and never delete a
mistakenly created shipment itself, even with real-time operator
approval — approval satisfies P-005 destructiveness only and never grants
Ship the P-010 authority to run the deletion)** →
**verify the prepared scope's own intake-readiness**," not as a single
`shipment ship`/cascade call and not by mutating status or creating a
successor shipment directly as Ship. This keeps the blocked feature and any
terminally-blocked evidence task visible in the queue (never archived),
ensures the prepared scope is genuinely executable once shipped, and keeps
both status normalization and shipment creation where the role boundary
requires them — with Stage, not Ship. If Ship nonetheless creates an
out-of-boundary artifact by mistake, treat correcting it as its own
destructive action requiring approval — and, even once approved, as an
action Ship must hand off rather than execute, never a self-authorized
cleanup step.

## Mode R / Role-Boundary Reconciliation Addendum (2026-08-30, `242b5e3`/`537daaf`)

Two structural gaps this pattern exposed have since been closed at the
agent-definition and Stage-decision level; both are cross-referenced here,
not re-litigated:

* **Successor-shipment assembly path.** Step 4/5/6's "hand it,
  un-normalized, to Stage ... a future Stage session assembles it" no
  longer has to wait for a fresh harvest. `.github/agents/.stage.agent.md`
  now supports a durable **Step 5.5 Mode R** ratified-existing-scope
  handoff for exactly this recovery case; `member_ids` (9), `prerequisite_ids`
  (2), assembly order, and exclusions for `059-F` are named in
  `docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`
  § *Mode R Authorization for Successor-Shipment Assembly*. **As first
  recorded here this bullet cited a single 10-ID `handoff_ids` set that
  folded the sign-off gate `059.014-T` into the member list; `378444e`/
  `3fb4fd0` corrected that to the disjoint `member_ids`/`prerequisite_ids`
  sets above, with `handoff_ids` demoted to their 11-ID audit union — see
  the "Mode R Partition-Alignment Correction" addendum below.** Mode R
  supplies *scope*, never gate relief — assembly stays blocked until
  **both** `prerequisite_ids` entries are satisfied: `059.007-T` already is
  (`done`/archived); the operator sign-off gate `059.014-T` is not, until
  it too reaches `done`/archived.
* **Continuity and source-artifact retirement handoff wording.** Ship's
  Continuity Allowed scope now explicitly covers Ship-owned checkpoints
  from a prior session for the same shipment/PR (owner and scope validated
  before resolving), not only the current session. The source-artifact
  retirement handoff Ship records for Stage (step 4 above) now reads both
  the singular and plural `source_stash_id`/`source_stash_ids` and
  `source_deliberation_id`/`source_deliberation_ids` fields (union +
  dedupe) and defaults to the state-appropriate **archive** action
  (`stash_archive` or the equivalent artifact archive) — never removal —
  consistent with `stash_remove` being destructive/deprecated per
  `.github/instructions/backlogit.instructions.md`.

Neither change retroactively legalizes any of the four historical
violations recorded in this entry's citations (status normalization,
`054-S` shipment creation, its unapproved deletion, `059.008-T`
`blocked_reason` mutation) — all four remain standing, un-legalized
historical record. Closure of the `051-S` **evidence** shipment (this
entry's own subject) may precede the `059.014-T` sign-off gate; that gate
governs successor-shipment assembly and implementation of the rescoped
scope only, never the historical normalization or this evidence-shipment
closure, and this is not a retroactive security sign-off — see
`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`
§ *Supersession of the PR #113 `051-S` Closure-Timing Requirement* for the
full evidence-based rationale.

## Mode R Partition-Alignment Correction (2026-08-30, `378444e`/`3fb4fd0`)

New Mode R review findings raised after `537daaf` were resolved by this
agent-contract/Stage-state pair, further sharpening the assembly-path
prevention step above:

* **Mode R is now a disjoint two-set contract, fail-closed.** `378444e`
  corrected `.github/agents/.stage.agent.md`'s Step 5.5 Mode R to require
  the ratifying authorization to name `member_ids` (items intended for
  shipment; these and only these become `assembly_ids`) and
  `prerequisite_ids` (external gates/terminal prerequisites — never
  shipped, never counted in the manifest) as disjoint exact sets, with
  `handoff_ids` demoted to an auditable union of the two and nothing more.
  Practitioners following this pattern must never treat a sign-off gate or
  other prerequisite as a shipment member just because it appears in a
  combined handoff citation.
* **Fail-closed add/manifest behavior distinguishes Mode R from Mode H.**
  Any add failure, a member concurrently assigned to another shipment,
  status drift, or a manifest read-back discrepancy now halts Mode R
  assembly immediately and unpublishes nothing partial — the
  skip-and-record-the-reason tolerance this pattern's own Step 6 describes
  applies to fresh-harvest Mode H only, never to a Mode R recovery handoff.
* **Stage's mutation classification is now complete**, closing the gap that
  produced the original `move_item` stash-fallback defect below:
  `create_item`, `update_item`, `append_comment`, `add_dependency`/
  `remove_dependency`, `add_link`/`remove_link`, `move_item` (ratified
  status on a work item, or complete/archive a `backlog-md` work item —
  never a stash-archival fallback), `archive_item`, `stash`/`stash_edit`,
  `deliberate`, `harvest_stash`, `stash_archive` are Allowed; `delete_item`
  and `track_commit` are explicitly Forbidden for Stage.
* **Ship's evidence-only commit-tracking authority is now explicit.**
  Ship's Allowed column names `backlogit_track_commit` (or the registry
  `commit` field) as recording the actual, already-`origin/main`-confirmed
  merge commit SHA of Ship's current shipment or its member tasks, and
  nothing else — no planning-field or status authority.
* **The invalid `move_item` stash fallback this pattern originally relied
  on is removed.** `backlogit_move_item` operates on backlog work-item
  IDs, so it was never a valid fallback when `backlogit_stash_archive` is
  unavailable — a hex stash ID is not a work-item ID. The corrected
  default (folded into Step 4/6 above) is: leave the stash entry
  untouched, record a retirement handoff naming the entry and its
  promotion target, and report the missing capability as a block.
* **`3fb4fd0` aligned the `059-F` Mode R authorization to this contract**:
  `member_ids` exactly 9 (`059-F` + the eight implementation tasks),
  `prerequisite_ids` exactly 2 (`059.007-T`, already satisfied;
  `059.014-T`, the operator sign-off gate, not yet satisfied),
  `handoff_ids` their 11-ID auditable union only, and the normalized-scope
  count corrected to nine.

None of this retroactively legalizes the four historical violations
recorded in this entry's citations — all four remain standing,
un-legalized historical record, unchanged by this addendum.
