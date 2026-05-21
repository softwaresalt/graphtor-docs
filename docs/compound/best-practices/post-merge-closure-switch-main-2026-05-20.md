---
title: "Post-merge closure should end on main"
description: "After a post-merge closure PR lands, return the local worktree to main so the next pipeline run starts from the correct branch context"
problem_type: "workflow-handoff"
category: "best-practices"
component: "ship workflow"
root_cause: "Post-merge closure can finish with the local worktree still on a feature or post-merge branch, which leaves the operator one step away from the next pipeline run and increases the chance of pulling or continuing work from the wrong branch"
resolution_type: "design_change"
severity: "medium"
message: "A post-merge closure completes successfully but the local repository remains on the closure branch instead of returning to main"
file_path: ".github\\agents\\ship.agent.md"
citations:
  - "docs\\memory\\2026-05-20\\pr-47-closure-ready-blocked-memory.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/47"
tags:
  - "post-merge"
  - "git"
  - "branch-hygiene"
  - "ship-workflow"
---

## Problem

Post-merge closure can finish in a technically correct state while still
leaving the operator on the closure branch. That makes the next pipeline
run awkward because the repository is not already positioned on `main`
for the expected `git pull` and fresh-cycle startup.

## Root Cause

The merge itself closes the release unit, but it does not change the
local branch context automatically. When the Ship workflow stops at
"merged" without one final branch reset step, the local worktree remains
on the feature or closure branch even though the source of truth has
already moved to `main`.

## Resolution

Treat `git switch main` as the final local workflow step after a
post-merge closure finishes. Do that after the closure PR merge is
confirmed and after any branch-specific verification is complete. If
local edits are present, let them carry over only when the switch is
safe; otherwise stop and report the blocking files instead of forcing the
branch change.

## Prevention

Make "return to `main`" part of the standard Ship closure checklist. The
goal is not to pull automatically in every case; the goal is to leave the
repository on the correct branch so the operator can run `git pull` and
start the next pipeline cycle without branch cleanup first.
