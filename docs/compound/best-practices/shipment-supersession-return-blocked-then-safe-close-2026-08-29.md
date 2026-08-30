---
title: "Shipment supersession pattern: return-blocked non-terminal manifest items, safe-close the evidence shipment, assemble a fresh rescoped implementation shipment"
description: "When an active shipment's manifest mixes a done evidence task with a still-blocked feature/task that must never be cascade-archived, return the non-done items from the manifest first so shipment-reconcile's pre-mode expected_status check and safe-close protected-set computation both work correctly, then close the now-single-item shipment and create a new shipment for the rescoped feasible scope under explicit operator authorization"
problem_type: "workflow-handoff"
category: "best-practices"
component: "shipment-reconcile skill + Ship Step 6 closure, backlogit shipment lifecycle"
root_cause: "shipment-reconcile's pre-mode classifies any manifest item whose status does not match expected_status as status-mismatch (HALT); a shipment whose original scope produced a mixed outcome (one item done, one item terminally blocked, the covering feature itself blocked) cannot be safe-closed as-is without first removing the non-terminal members from the manifest"
resolution_type: "workaround"
severity: "low"
message: "051-S manifest [059-F, 059.007-T, 059.008-T]: 059-F blocked, 059.008-T blocked (terminal evidence), 059.007-T done"
file_path: ".github/skills/shipment-reconcile/SKILL.md"
citations:
  - "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
  - "docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/113"
tags:
  - "shipment"
  - "backlogit"
  - "ship-workflow"
  - "reconciliation"
  - "role-boundary"
---

## Problem

Shipment `051-S`'s manifest was `[059-F, 059.007-T, 059.008-T]` after a
feasibility spike produced a **mixed** outcome: `059.007-T` (U7) reached
`done`/archived, but `059.008-T` (U8) reached a **terminal `blocked`**
(structurally infeasible, not "not yet started"), and the covering feature
`059-F` stayed `blocked` as a result. A later re-deliberation decision
(`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`)
accepted U8's BLOCKED result as a decided input and authorized closing
`051-S` while rescoping the still-feasible units onto a fresh shipment.
Running `shipment-reconcile mode: pre` with `expected_status: done` directly
against the original three-item manifest would immediately `HALT` with
`status-mismatch` on both `059-F` and `059.008-T` — neither is `done`, and
neither should ever become `done` or be archived (the feature is honestly
blocked pending operator sign-off; U8's BLOCKED status is itself the
correct, permanent evidence record).

## Root Cause

`shipment-reconcile`'s pre-mode and safe-close protocol assume every
manifest item that isn't `pre-archived` will reach the shipment's declared
`expected_status`. There is no built-in path for "this manifest item's
correct terminal state is `blocked`, not `done`, and it must be preserved
in queue, not archived, when the shipment closes."

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
4. **Restore the rescoped-feasible returned members to an intake-valid
   status BEFORE assembling the successor shipment** (this step is
   easy to miss — a Copilot review on the first pass of this pattern
   correctly caught its absence). `return-blocked` only detaches manifest
   membership; it deliberately does **not** touch `status`, so any item
   that was `blocked` under the OLD (now-superseded) dependency chain stays
   `blocked` after being returned. If those items are then folded straight
   into a new shipment's manifest while still `blocked`, the successor
   shipment can never pass Ship's own Step 0.5 intake reconciliation
   (`shipment-reconcile mode: pre` with `expected_status: queued` — see
   `.ship.agent.md` primary-path step 6 and `shipment-reconcile/SKILL.md`'s
   pre-mode `status-mismatch` classification, which halts on any manifest
   item whose `status` doesn't equal the expected value). Completing a
   downstream gate task (e.g. an operator sign-off) does **not** itself
   transition these members — `status` and the dependency graph are
   orthogonal in this schema.

   The supported workflow: for every returned item whose *only* remaining
   blocker is a dependency edge in the manifest it's about to join (not a
   terminal/evidence-based block), transition it back to `queued` directly:
   ```bash
   backlogit move <id> --status queued
   ```
   This is a valid direct `blocked → queued` transition in this backlogit
   version (verified — no intermediate `active` hop required, unlike the
   `queued → done` FSM constraint documented in
   `backlogit-status-transitions-2026-05-02.md`). It clears any
   `custom_fields.blocked_reason` on that item as a side effect (verified);
   preserve the narrative in git history / the governing decision doc
   instead of relying on the live field. Do **not** apply this to items
   whose block is itself the terminal, decided evidence (e.g. a feasibility
   spike that proved infeasible) — those stay `blocked` and stay **out of**
   the successor manifest.
5. **Assemble the fresh, rescoped shipment** for the now-`queued` feasible
   scope (`backlogit shipment create --items ...`). Per this workspace's
   Ship Role Boundary, creating shipments is normally Stage's job — but
   Ship's own Step 0.5 fallback path explicitly permits direct assembly
   **when the operator explicitly confirms bypassing Stage**, and the
   governing decision document itself named this as a sanctioned "Ship's
   choice" alternative. Record the explicit operator authorization in the
   closure report; do not silently create shipments as a matter of routine.
6. **Verify intake-readiness before presenting the shipment as executable**:
   confirm every manifest member's `status` equals `queued` (or `active` if
   already claimed) — the same check Step 0.5 will perform later — so the
   successor shipment doesn't merely exist, but can actually be claimed and
   built without an immediate `RECONCILE_FAIL`.

## Prevention

When a shipment's manifest will end in a mixed done/blocked/terminal-blocked
outcome, plan the closure as "return non-`done` items → reconcile pre →
safe-close → **restore the rescoped-feasible returned items to `queued`** →
create successor shipment → **verify the successor's own intake-readiness**,"
not as a single `shipment ship`/cascade call and not by assembling the
successor immediately after `return-blocked` without the status-restore
step. This keeps the blocked feature and any terminally-blocked evidence
task visible in the queue (never archived) while still letting the
shipment record for the completed evidence-gathering scope close cleanly,
and it ensures the successor shipment is genuinely executable, not just
structurally present.
