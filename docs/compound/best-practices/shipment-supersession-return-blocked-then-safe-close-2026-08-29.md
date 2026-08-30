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
4. **Assemble the fresh, rescoped shipment** for the still-feasible scope
   (`backlogit shipment create --items ...`). Per this workspace's Ship Role
   Boundary, creating shipments is normally Stage's job — but Ship's own
   Step 0.5 fallback path explicitly permits direct assembly **when the
   operator explicitly confirms bypassing Stage**, and the governing
   decision document itself named this as a sanctioned "Ship's choice"
   alternative. Record the explicit operator authorization in the closure
   report; do not silently create shipments as a matter of routine.

## Prevention

When a shipment's manifest will end in a mixed done/blocked/terminal-blocked
outcome, plan the closure as "return non-`done` items → reconcile pre →
safe-close → create successor shipment for the returned/rescoped items,"
not as a single `shipment ship`/cascade call. This keeps the blocked feature
and any terminally-blocked evidence task visible in the queue (never
archived) while still letting the shipment record for the completed
evidence-gathering scope close cleanly.
