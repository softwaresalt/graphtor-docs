---
title: "Shipment supersession pattern: return-blocked non-terminal manifest items, safe-close the evidence shipment, prepare the rescoped scope for Stage to assemble"
description: "When an active shipment's manifest mixes a done evidence task with a still-blocked feature/task that must never be cascade-archived, return the non-done items from the manifest first so shipment-reconcile's pre-mode expected_status check and safe-close protected-set computation both work correctly, then close the now-single-item shipment and normalize every superseded-chain member of the rescoped feasible scope to an intake-valid status — leaving shipment assembly itself to Stage, per Ship's NON-NEGOTIABLE P-010 role boundary"
problem_type: "workflow-handoff"
category: "best-practices"
component: "shipment-reconcile skill + Ship Step 6 closure, backlogit shipment lifecycle"
root_cause: "shipment-reconcile's pre-mode classifies any manifest item whose status does not match expected_status as status-mismatch (HALT); a shipment whose original scope produced a mixed outcome (one item done, one item terminally blocked, the covering feature itself blocked) cannot be safe-closed as-is without first removing the non-terminal members from the manifest, and any superseded-dependency-chain member later folded into a successor shipment while still `blocked` will fail that successor's own future intake"
resolution_type: "workaround"
severity: "low"
message: "051-S manifest [059-F, 059.007-T, 059.008-T]: 059-F blocked, 059.008-T blocked (terminal evidence), 059.007-T done"
file_path: ".github/skills/shipment-reconcile/SKILL.md"
citations:
  - "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
  - "docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/113"
  - "https://github.com/softwaresalt/graphtor-docs/pull/114"
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
4. **Normalize EVERY superseded-chain member of the rescoped scope to an
   intake-valid status** — not just the members returned from `051-S` in
   the current session. Identify the full rescoped scope from the
   governing decision document (the feature plus every unit named
   feasible), regardless of which prior session returned each one from a
   shipment or whether it was ever in a shipment at all. For each member
   whose *only* remaining blocker is a dependency edge on another scope
   member or an already-`done`/gate item (not a terminal/evidence-based
   block), transition it back to `queued` directly:
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
   the rescoped scope entirely. Skipping any affected member (for example,
   normalizing only the members returned in the current session while
   leaving pre-existing `blocked` siblings untouched) reproduces the same
   intake-mismatch failure for a future shipment attempt.
5. **Hand off shipment assembly to Stage — do NOT assemble it as Ship.**
   `.github/policies/workflow-policies.md`'s P-010 definition is
   unconditional: **"Ship MUST NOT: Create backlog items, create
   shipments..."**, and **"Do not proceed past the boundary even if the
   operator requests work outside scope — redirect to the correct agent
   instead."** This holds even when `.ship.agent.md`'s own Step 0.5
   fallback text describes an operator-confirmed direct-assembly path, and
   even when a governing decision document frames fresh-shipment assembly
   as "Ship's choice" — the canonical policy document takes precedence
   over both. **A prior version of this pattern created the successor
   shipment directly as Ship; a Copilot shadow review correctly flagged it
   as a P-010 violation, and it was reverted** (`backlogit delete <id>
   --force`). The correct handoff: leave the normalized scope as
   individual, `queued`, dependency-closed backlog items with no shipment
   membership, and record in the closure/memory artifacts (not a stash
   entry — stash operations are equally Stage-only under the same P-010
   list) that a future Stage session must assemble them into a shipment.
6. **Verify intake-readiness of the prepared scope**: confirm every
   scope member's `status` equals `queued` (or `active` if some are
   already claimed elsewhere) — the same check Step 0.5 will perform when
   Stage's successor shipment is later claimed — so the handoff is
   genuinely actionable, not just structurally present.

## Prevention

When a shipment's manifest will end in a mixed done/blocked/terminal-blocked
outcome, plan the closure as "return non-`done` items → reconcile pre →
safe-close → **normalize every superseded-chain member of the rescoped
scope to `queued` (not just this session's returns)** → **hand the prepared
scope to Stage for shipment assembly (never assemble it as Ship)** →
**verify the prepared scope's own intake-readiness**," not as a single
`shipment ship`/cascade call and not by creating a successor shipment
directly. This keeps the blocked feature and any terminally-blocked
evidence task visible in the queue (never archived), ensures the prepared
scope is genuinely executable once shipped, and keeps shipment creation
where the role boundary requires it — with Stage, not Ship.
