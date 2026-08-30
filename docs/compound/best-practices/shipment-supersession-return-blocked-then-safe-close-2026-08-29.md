---
title: "Shipment supersession pattern: return-blocked non-terminal manifest items, safe-close the evidence shipment, prepare the rescoped scope for Stage to assemble"
description: "When an active shipment's manifest mixes a done evidence task with a still-blocked feature/task that must never be cascade-archived, return the non-done items from the manifest first so shipment-reconcile's pre-mode expected_status check and safe-close protected-set computation both work correctly, then close the now-single-item shipment, identify the rescoped feasible scope, and hand it off — un-normalized — to Stage, which exclusively normalizes every superseded-chain member to an intake-valid status and decides on successor-shipment assembly, per both agents' NON-NEGOTIABLE P-010 role boundaries; never instruct Ship to delete a mistakenly created shipment without real-time operator approval"
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
neither should ever become `done` or be archived (the feature is honestly
blocked pending operator sign-off; U8's BLOCKED status is itself the
correct, permanent evidence record).

A second, independent problem surfaced once the rescoped scope was
gathered: **every unit in that scope carries `status: blocked`**, not just
the two returned from `051-S` in this same session — most of them had
already been returned `blocked` from `051-S` in an *earlier* session, once
the original engine-boundary dependency chain (gating them on the now-
terminally-BLOCKED U8) was in force. A superseded dependency-chain member's
`status` field does not self-correct when the *dependency edges* are later
rewired (e.g. by a Stage re-deliberation pass) — status and the dependency
graph are independent in this schema.

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
   destructive-command approval gate. The correct response to a
   mistakenly created shipment is one of:
   * halt and request explicit real-time operator approval for the
     deletion before running it, or
   * leave the artifact in place, unclaimed and unshipped, and hand it to
     Stage/the operator as a recovery item in the closure/memory
     artifacts, recording the mistaken creation as an open P-010 finding
     until the operator or Stage resolves it.

   Never instruct Ship to delete its own mistaken artifact as a matter of
   routine remediation. The correct handoff for the legitimate case (no
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
mistakenly created shipment without real-time operator approval)** →
**verify the prepared scope's own intake-readiness**," not as a single
`shipment ship`/cascade call and not by mutating status or creating a
successor shipment directly as Ship. This keeps the blocked feature and any
terminally-blocked evidence task visible in the queue (never archived),
ensures the prepared scope is genuinely executable once shipped, and keeps
both status normalization and shipment creation where the role boundary
requires them — with Stage, not Ship. If Ship nonetheless creates an
out-of-boundary artifact by mistake, treat correcting it as its own
destructive action requiring approval, not a self-authorized cleanup step.
