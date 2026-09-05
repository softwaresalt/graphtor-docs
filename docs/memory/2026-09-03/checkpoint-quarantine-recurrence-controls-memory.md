---
title: "Checkpoint quarantine and recurrence controls session"
description: "Records operator-approved quarantine of six legacy checkpoints and the resulting compound learning"
source: "docs/memory/2026-09-03/checkpoint-quarantine-recurrence-controls-memory.md"
doc_type: "memory"
date: "2026-09-03"
---

## Outcome

Quarantined six parseable but CheckpointV1-invalid records with explicit
operator approval. `backlogit checkpoint list` now reports
`needs_quarantine: 0`.

Captured the causes, remediation, and recurrence controls in
`docs/compound/workflow-issues/checkpoint-schema-and-lifecycle-controls-2026-09-03.md`.
The highest-priority upstream control, rejecting schema-less checkpoint
creation by default, is already represented in the backlogit stash according
to the operator and was not duplicated locally.

## Files modified

* Moved six legacy records from `.backlogit/checkpoints/` to
  `.backlogit/archive/checkpoints/`
* Added six tool-generated quarantine disposition records under
  `.backlogit/archive/checkpoints/`
* Added the checkpoint schema and lifecycle compound learning

## Decisions

* Treat malformed checkpoints as schema-invalid historical evidence, not
  corrupt JSON
* Preserve their bytes through quarantine rather than deleting them
* Treat `checkpoint-20260829-163933.json` as a valid stale active lifecycle
  record because a later resolved record proves that its Stage session
  completed
* Leave owner-scoped resolution of the stale active checkpoint to Stage

## Open work

* Stage must resolve the stale active August 29 checkpoint through the
  owner-scoped recovery protocol
* Backlogit follow-up should enforce schema-versioned creation and governed
  lifecycle transitions at the CLI boundary
