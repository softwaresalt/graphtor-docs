---
title: "Enforce checkpoint schema and lifecycle transitions at the tool boundary"
description: "Schema-less compatibility and replacement resolved records can leave malformed or falsely active checkpoints that block pipeline recovery"
source: "docs/compound/workflow-issues/checkpoint-schema-and-lifecycle-controls-2026-09-03.md"
doc_type: "learning"
problem_type: "workflow_state_corruption"
category: "workflow-issues"
component: "backlogit checkpoint lifecycle"
root_cause: "Schema-less checkpoint creation bypassed V1 validation, while completion created a separate resolved record instead of resolving the original active checkpoint"
resolution_type: "design_change"
severity: "high"
message: "checkpoint validation failed; needs_quarantine is nonzero or an obsolete active cursor remains"
file_path: ".backlogit/checkpoints/"
citations:
  - ".backlogit/archive/checkpoints/checkpoint-20260429-214618.json"
  - ".backlogit/archive/checkpoints/checkpoint-20260429-215617.json"
  - ".backlogit/archive/checkpoints/checkpoint-20260701-064559.json"
  - ".backlogit/archive/checkpoints/checkpoint-20260822-073402.json"
  - ".backlogit/archive/checkpoints/checkpoint-20260822-090657.json"
  - ".backlogit/archive/checkpoints/checkpoint-20260822-092508.json"
  - ".backlogit/checkpoints/checkpoint-20260829-163933.json"
  - ".backlogit/checkpoints/checkpoint-20260829-165829.json"
  - ".github/instructions/backlogit.instructions.md"
tags:
  - "backlogit"
  - "checkpoints"
  - "schema-validation"
  - "recovery"
  - "lifecycle"
---

## Problem

Pipeline startup failed closed because six historical checkpoint files were
parseable JSON but invalid under the current CheckpointV1 schema. The files
used legacy or consumer-specific top-level fields, omitted required identity
and timestamp fields, and used statuses such as `blocked` and
`superseded-closed` that are not valid checkpoint lifecycle states.

A separate valid checkpoint made a completed Stage session appear
interrupted. The session created an `active` checkpoint and later created a
second `resolved` checkpoint with the same session, phase, shipment, feature,
branch, and task set. Because the original filename was never transitioned
through `backlogit checkpoint resolve`, it remained a false active recovery
candidate.

## Root Cause

Backlogit preserves backward compatibility by writing a state dump without
`schema_version: 1` verbatim and without validation. That compatibility path
allowed older agents to persist arbitrary checkpoint structures. Checkpoint
listing reported the invalid records through `needs_quarantine`, but returned
success, and the general backlog doctor did not gate checkpoint validity.

The lifecycle contract was also not enforced at creation. A caller could
create a new checkpoint already marked `resolved` rather than resolving the
existing active checkpoint. This produced two records for one cursor and left
the obsolete record active.

## Resolution

With explicit operator approval, quarantine all six schema-invalid records
using `backlogit checkpoint quarantine`. Quarantine moved the original bytes
and disposition metadata into `.backlogit/archive/checkpoints/`; it did not
discard the historical evidence. An unfiltered checkpoint listing then
reported `needs_quarantine: 0`.

The August 29 active record (`checkpoint-20260829-163933.json`) was a
lifecycle leak, not evidence of an interrupted Stage run: its later resolved
peer (`checkpoint-20260829-165829.json`, same session, same
shipment/feature) proved that the same session completed successfully. The
owning Stage recovery workflow has since resolved the original active
filename directly, and a subsequent unfiltered checkpoint listing confirmed
`status: resolved` with no active cursor remaining. An Orchestrator must
still never silently infer or mutate owner state on a future recurrence of
this pattern.

The highest-priority recurrence control -- reject schema-less checkpoint
creation by default and require an explicit legacy-import path -- is already
captured in the backlogit stash for upstream follow-up, as reported by the
operator. Do not create a duplicate stash entry in this workspace.

## Prevention

* Require `schema_version: 1` for ordinary checkpoint creation and reserve
  schema-less input for an explicit legacy-import operation
* Create checkpoints only in the `active` state; use governed `resolve` or
  `abandon` transitions on the same filename for terminal disposition
* Add a checkpoint validation command that exits nonzero when any record needs
  quarantine, and run it in startup and CI gates
* Reject or report duplicate active cursors for the same agent, session, and
  phase
* Keep domain data under the open `context` object and use only the official
  checkpoint creation operation
* Enumerate checkpoints without a status filter before recovery selection so
  malformed records cannot disappear behind filtering
