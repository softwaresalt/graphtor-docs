---
title: "Post-Merge Closure Artifacts Must Use Feature Branches and PRs — Not Direct Main Push"
description: "The branch protection rule applies to ALL commits including docs-only closure work. There is no exception for memory docs, compound learnings, or backlog updates."
problem_type: "process_violation"
category: "workflow-issues"
component: "ship-agent/closure"
root_cause: "Post-merge closure commits pushed directly to main using admin bypass instead of going through a feature branch and PR"
resolution_type: "process_correction"
severity: "high"
message: "Bypassed rule violations for refs/heads/main: Changes must be made through a pull request."
file_path: "docs/archive/memory/2026-05-05/pr28-pr31-review-followups.md"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/commit/a35d5dd"
  - "https://github.com/softwaresalt/graphtor-docs/pull/32"
date: 2026-05-05
tags:
  - workflow
  - git
  - branch-protection
  - process
  - closure
---

## Problem

After merging PRs #30 and #31 (Copilot review follow-up fixes), post-merge closure artifacts
were committed and pushed directly to `main`:

```text
git add docs/compound/... docs/memory/...
git commit -m "docs(memory): post-merge closure ..."
git push origin main   # ← VIOLATION
```

Git allowed this via branch protection bypass (operator has admin rights), but it violates
P-011: **all changes must go through feature branches and PRs**.

The remote correctly flagged it:

```text
remote: Bypassed rule violations for refs/heads/main:
remote:
remote: - Changes must be made through a pull request.
```

## Root Cause

A prior session note described "post-merge backlog commits bypass branch protection with a
'Bypassed rule violations' warning — this is expected for backlog-only commits." This was
a rationalization of a pre-existing bad habit, not a sanctioned exception. It was treated
as precedent and repeated.

## Resolution

**All file changes — including docs-only closure work — must go through a feature branch and PR.**

```bash
# After merging the feature PR:
git checkout main
git pull origin main
git checkout -b chore/closure-{shipment-slug}

# Write compound learnings, session memory, backlog updates
# ... edit files ...

git add docs/compound/... docs/memory/...
git commit -m "docs(memory): post-merge closure for {shipment}"
git push origin chore/closure-{shipment-slug}

gh pr create --base main --head chore/closure-{shipment-slug} \
  --title "docs(memory): post-merge closure for {shipment}" \
  --body-file logs/pr-body.txt
# Wait for CI → operator approval → merge
```

This violation cannot be cleanly undone without force-pushing, which is also forbidden
(P-011). When it occurs: acknowledge to the operator, store this learning, apply the correct
pattern going forward.

## Prevention

- Branch protection applies to **all** commits — there is no "docs-only" exception.
- "Docs-only" commits can still introduce errors (broken links, wrong filenames, stale
  content) that CI or review would catch.
- Admin bypass capability is for emergency hotfixes only — not routine agent workflow.
- Normalizing direct pushes erodes the trust boundary between agent and repository.
- When in doubt: if a file changed, it needs a PR.
