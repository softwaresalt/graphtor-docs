---
title: "backlogit 1.10.1: moving a task to done immediately relocates it to archive/"
description: "backlogit 1.10.1's task done transition performs an immediate file rename from .backlogit/queue/ to .backlogit/archive/, well before the owning shipment is closed or a real merge SHA exists -- defer per-task --commit evidence writes accordingly"
source: "docs/compound/workflow-issues/backlogit-task-done-immediate-archive-relocation-2026-09-13.md"
doc_type: "learning"
problem_type: "tool-behavior-gotcha"
category: "workflow-issues"
component: "backlogit (task status transitions)"
root_cause: "backlogit 1.10.1 treats a task's status:done transition as equivalent to archival for that individual artifact -- the file is renamed queue/ -> archive/ immediately, independent of the owning shipment's own lifecycle state"
resolution_type: "workaround"
severity: "medium"
message: "n/a -- behavioral discovery, not an error"
file_path: ".backlogit/queue/{task-id}.md"
citations:
  - "docs/archive/memory/2026-09-13/2026-09-12-ship-049S-progress-checkpoint.md"
  - "compacted 2026-09-13, see docs/memory/compacted/2026-09-13-049-s-compacted.md"
  - ".github/agents/_ship.agent.md (Mutation Classification: archived-current-delivery-pending-finalization path)"
  - "docs/closure/2026-09-13-049-s-mcp-serve-handshake-post-merge-closure.md"
tags:
  - "backlogit"
  - "task-lifecycle"
  - "archival"
  - "commit-evidence"
---

## Problem

While implementing shipment `049-S`'s 8-task manifest, moving each task to
`status: done` (via `backlogit move <task-id> done` / the equivalent update
operation) immediately relocated that task's file from `.backlogit/queue/`
to `.backlogit/archive/` -- well before the shipment itself was closed, and
well before a real merge SHA existed to record as commit evidence. This is
easy to miss if a session assumes (by analogy with the shipment's own
`queued -> active -> shipped` lifecycle, which archives only at final
closure) that task archival also happens only at shipment close time.

## Root Cause

backlogit 1.10.1 treats a task-level `done` transition as archival for that
individual artifact, independent of the owning shipment's lifecycle state.
This is a different rule from the shipment-level lifecycle (see
`docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`): a
shipment only archives at final safe-close/cascade, but each of its member
tasks can already be sitting in `.backlogit/archive/` long before that,
while still correctly `parent_id`-linked to its covering feature and still
correctly referenced by the shipment's `custom_fields.items` manifest (the
manifest references task IDs, not a queue-vs-archive path).

## Resolution

Adopted a deliberate ordering rule for this and future shipments: defer ALL
per-task `--commit {sha}` evidence writes to the mandatory Step 6
post-merge `shipment-reconcile` safe-close phase, once the real merge SHA
actually exists, rather than writing an interim feature-branch SHA at
`done`-transition time. This matches the Ship agent template's own
Mutation Classification table, which already anticipates a task landing in
`.backlogit/archive/` before its commit evidence is finalized (the
"archived-current-delivery-pending-finalization" path for
`backlogit_update_item`).

## Prevention

* Do not assume a task stays in `.backlogit/queue/` until its owning
  shipment closes. Check the task's actual current path
  (`.backlogit/queue/` vs. `.backlogit/archive/`) rather than inferring it
  from the shipment's status.
* Write per-task commit-SHA evidence at Step 6 safe-close time (real merge
  SHA), not at `done`-transition time (only a feature-branch SHA would be
  available then).
* When computing a shipment's protected set or manifest membership, resolve
  by task ID and read from whichever of `.backlogit/queue/` or
  `.backlogit/archive/` currently holds that ID -- never assume queue-only.
