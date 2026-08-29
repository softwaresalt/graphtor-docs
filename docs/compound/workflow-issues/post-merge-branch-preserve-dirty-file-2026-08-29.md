---
title: "Branch directly from origin/main to preserve a dirty operator edit across post-merge closure"
description: "When local main is stale and a tracked file has an uncommitted operator edit that must survive byte-for-byte, verify the file's committed blob is identical between the current branch and origin/main, then branch straight off origin/main instead of updating local main first"
problem_type: "workflow-handoff"
category: "workflow-issues"
component: "ship workflow — Step 6.0 post-merge branch protocol"
root_cause: "Step 6.0 documents 'git checkout main; git pull; git checkout -b post-merge/{slug}', but when local main is many commits behind origin/main and the working tree carries an uncommitted edit to a tracked file that must never be staged/reverted/stashed (e.g. an operator .gitignore change), routing through local main adds an unnecessary pull step and risk window without adding safety"
resolution_type: "workaround"
severity: "low"
message: "local branch behind origin/main; working tree has uncommitted edit that must remain untouched (M .gitignore)"
file_path: ".github/agents/.ship.agent.md"
citations:
  - "docs/memory/2026-08-29/ship-050-s-recovery-memory.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/109"
tags:
  - "post-merge"
  - "git"
  - "branch-hygiene"
  - "ship-workflow"
  - "concurrency"
---

## Problem

Post-merge closure for shipment `050-S` started with the local worktree on
the merged feature branch (`chore/ship-050-pip-autoapprove`, tip `adee5e8`,
behind `origin/main`'s new merge commit `4fba250`) and a dirty, uncommitted
`.gitignore` edit that the operator required to remain untouched — not
staged, committed, reverted, or stashed — through the entire closure. Local
`main` was 10 commits behind `origin/main`. Naively following "checkout
main; pull; checkout -b post-merge/{slug}" would checkout a branch whose
tracked `.gitignore` content might differ from the feature branch's, risking
a merge/conflict prompt on the dirty file, and adds an extra fast-forward
step that isn't needed when going straight to `origin/main`.

## Root Cause

The generic Step 6.0 wording assumes local `main` is reasonably current and
doesn't call out what to check before switching when a tracked file has
uncommitted local modifications that must be preserved exactly.

## Resolution

Before switching branches, verify the *committed* blob for the sensitive
file is identical between the current HEAD and the target ref:

```
git rev-parse HEAD:.gitignore
git rev-parse origin/main:.gitignore
```

If the two hashes match, the file's tracked content hasn't diverged, so the
working-tree diff applies cleanly regardless of which of the two commits is
checked out — `git checkout -b post-merge/{slug} origin/main` (skipping
local `main` entirely) will carry the uncommitted edit across untouched.
Confirm afterward with `git status --short` (same `M`/`??` entries as
before) and a SHA-256 hash of the dirty file to prove byte-for-byte
preservation. This also sidesteps the local-`main`-pull step entirely when
branching directly off `origin/main` is safe and sufficient.

If the hashes differ, do NOT switch — halt and report the conflicting file
to the operator instead of stashing or force-checking-out over it.

## Prevention

Add this blob-hash-comparison check as an explicit sub-step of Step 6.0
whenever the working tree carries an uncommitted edit to a tracked file that
must survive the branch switch unchanged, instead of assuming `checkout
main; pull` is always safe first.
