---
title: "Harden .vscode/settings.json pip auto-approval (9CEC208C) — decided plan"
description: "Decided plan: remove the blanket chat.tools.terminal.autoApprove pip:true entry, restoring least-privilege for the terminal auto-approve allow-list"
date: 2026-08-24
decided: 2026-08-29
status: shipped
shipment: "050-S"
source: "docs/decisions/2026-08-24-vscode-pip-autoapprove-hardening-deliberation.md"
supersedes: "docs/archive/plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md"
stash_ids:
  - "9CEC208C"
deliberation_id: "003-DL"
tags:
  - security
  - config
  - vscode
  - auto-approve
  - hardening
  - chore
---

## Decision

`.vscode/settings.json` `chat.tools.terminal.autoApprove` held a blanket
`"pip": true` entry matched as a substring against the command, auto-approving
any command line containing `pip` (e.g. `pip install <attacker-package>`) —
effectively blanket remote-code-execution approval inside agent sessions,
since pip executes arbitrary build-backend/`setup.py` code at install time.
The sibling entries are anchored regexes (`matchCommandLine: true`); the bare
`pip` key was the outlier. Confirmed pre-existing (predates the `047-S`
stowaway carry-over per `git show 33bbb37:.vscode/settings.json`).

**Option-A fallback taken**: the blanket entry was removed outright with no
replacement anchored pattern added — the documented `.scripts/*.py` clone
workflows invoke `python .scripts/*.py`, not `pip`, so no concrete pip command
line required auto-approval.

## Constraints Preserved

* The three existing anchored `.scripts/*.py` auto-approve patterns are
  byte-for-byte unchanged.
* `.vscode/settings.json` remains valid JSON.
* No bare-substring/prefix `pip` auto-approve key remains; a representative
  `pip install <pkg>` command line matches no auto-approve entry.

## Rejected Alternatives

* **Add one anchored `/^<exact reviewed pip command line>$/` replacement
  pattern.** Not needed — the documented clone workflows never invoke a
  concrete pip command, so no anchored replacement pattern exists to add;
  removal outright (Option-A) is the correct endpoint per the plan's own
  fallback criterion, avoiding a speculative pattern.

## Implementation (as shipped)

* **Unit 1** (`.vscode/settings.json`): removed the `"pip": true` key from
  `chat.tools.terminal.autoApprove` (commit `f1e0c0879925cd37981a06e0d935a5f3558fac5e`,
  "fix(config): remove blanket pip auto-approve grant"). The three anchored
  `.scripts/*.py` patterns were left byte-for-byte unchanged.

## Verification, Rollback, and Monitoring

* Verified: `.vscode/settings.json` parses as valid JSON; the three existing
  anchored patterns are byte-for-byte unchanged (diff review); a
  representative `pip install <pkg>` command line matches no remaining
  auto-approve entry.
* **Rollback trigger**: any documented `.scripts/*.py` clone workflow
  encounters an unexpected approval prompt.
* **Rollback procedure (fail-closed)**: keep `pip` denied and use manual
  per-invocation approval; if repeated automation is demonstrably required,
  add exactly one separately reviewed, exact anchored command-line entry
  (`/^...$/` with `matchCommandLine: true`) — **never restore the blanket
  `"pip": true` grant**.
* Owner: single developer/operator. Validation window: the next agent
  session that exercises the `.scripts/*.py` clone workflow. No monitoring
  system applies; closure is a manual inspection item.

## Plan Review Outcome

**PASS**, findings summary P0=0, P1=0, P2=0, P3=2 (advisory: any future
replacement pip pattern must be fully anchored with `matchCommandLine: true`;
confirm no other bare-substring auto-approve keys exist post-edit — both
already covered by the plan's own acceptance criteria).

## Shipped

Merged as shipment `050-S`, PR #109, merge commit
`4fba2500797c46fe2bd9d79e1e8e1ca350367725`. Follow-ups: none; no stash
entries created.

Original plan (with full `## Plan Hardening` and `## Plan Review` detail)
archived at
`docs/archive/plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md`.

## Known Exceptions (Ship Role Boundary)

Two artifacts cite the original plan path and were intentionally left
untouched by this 2026-09-01 compaction pass under Ship's Role Boundary:

* `docs/decisions/2026-08-24-vscode-pip-autoapprove-hardening-deliberation.md`
  — a deliberation artifact; Ship's Role Boundary forbids creating or
  modifying deliberation, spike, plan, or review artifacts (Planning row).
* `.backlogit/archive/057-F.md` (`references:` field) and
  `.backlogit/archive/057.001-T.md` — archived backlogit records; Ship's
  Role Boundary permits only the narrow commit-only `backlogit_update_item`
  write during shipment-reconcile safe-close, not general field edits
  (Backlog row).

Both citations degrade gracefully (the original plan file was relocated, not
deleted) and are recorded here for a future Stage session to reconcile if
desired.
