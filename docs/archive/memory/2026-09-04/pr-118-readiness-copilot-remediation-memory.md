---
title: "PR #118 readiness: local review, Copilot shadow-review remediation, merge-approval gate"
description: "Records the full PR lifecycle execution for fix/graphtor-startup-checkpoint-recovery through PR creation, CI monitoring, Copilot shadow-review discovery and remediation, and the §1.9/P-018 readiness gate, stopping at the explicit P-014 merge-approval gate per operator instruction"
source: "docs/archive/memory/2026-09-04/pr-118-readiness-copilot-remediation-memory.md"
doc_type: "memory"
date: "2026-09-04"
agent: "ship"
---

## Outcome

PR #118 (`chore(startup): fix startup checkpoint recovery and quarantine
legacy checkpoints`, `fix/graphtor-startup-checkpoint-recovery` → `main`) is
**merge-ready** as of head SHA `f2d42474b272084aa77eaeaef6021b515ca5e4dc`.
No merge was executed. Explicit operator approval is still required per the
operator's original instruction to stop at the post-readiness gate.

## Session scope

No shipment was claimed or mutated (`048-S`/`049-S` untouched). This was a
chore-branch-only PR lifecycle execution: local review → PR creation → CI/
review remediation → merge-readiness presentation, per explicit operator
instruction not to merge without a separately confirmed P-014 approval.

## Key discovery this session: repository ruleset auto-engages Copilot review

`gh api repos/softwaresalt/graphtor-docs/rules/branches/main` revealed a
repository ruleset (id `13816903`) that:
- auto-triggers Copilot code review on every push (`copilot_code_review.
  review_on_push: true`)
- requires all review threads resolved before merge
  (`required_review_thread_resolution: true`)
- requires 0 approving reviews (`required_approving_review_count: 0`)
- requires the `detect code changes` status check
- forces `allowed_merge_methods: ["merge"]` (merge-commit only — matches P-009)

Classic branch-protection API (`.../branches/main/protection`) returns 404
("not protected") on this repo — that check alone is **insufficient** to
conclude Copilot/thread-resolution is not engaged. The newer Rulesets API
(`/rules/branches/{branch}`) must also be checked. Recording this as a
compound-learning candidate for a future session if this pattern recurs.

## Timeline

1. Local review (report-only, 3-reviewer adversarial consensus) on the base
   diff — `READY_WITH_FOLLOWUPS`, P0=0/P1=0. Two in-scope fixes applied
   directly; one out-of-scope finding (ad hoc scripts) captured as stash
   entry `CCAC612D` per P-021 C2. Committed `3080dc8`.
2. PR #118 created at `3080dc8`. CI (`build`, `detect code changes`,
   `pipeline topology gate`) all passed (~8m5s for `build`).
3. Copilot auto-reviewed at `3080dc8` (🟡 Changes recommended, 5 comments,
   `mergeStateStatus: BLOCKED` due to unresolved-thread ruleset requirement).
   Classified each finding against P-021 C1/C3 against the branch's own diff:
   - 4 findings were genuine defects **within files this branch itself
     modified** (`.mcp.json` backslash path; `.mcp.json` missing
     `${workspaceFolder}` env bindings; `.mcp.json`/`start.sh` sync-removal
     inconsistency; stale-tense compound-doc paragraph) — all in-scope per
     C1/C3, fixed directly.
   - 1 finding (stash entry `CCAC612D`'s `Kind: chore` text vs. actual
     `kind: task` field) was **not fixed**: editing a captured stash entry is
     outside Ship's create-only stash authority under the Role Boundary
     (confirmed against the single-write capture invariant and the Mutation
     Classification table's explicit "including the one it just created"
     restriction). Replied with the discrepancy and resolved the thread,
     flagging it for Stage's `CCAC612D` triage instead.
4. Applied the 4 fixes, re-ran full quality gates (cargo check/fmt/clippy/
   test — all pass, no Rust source touched), committed `f2d4247`, pushed.
5. Replied to and resolved all 5 review threads via GraphQL
   (`addPullRequestReviewThreadReply` + `resolveReviewThread`).
6. CI re-ran and passed at `f2d4247` (~7m5s for `build`).
   `mergeStateStatus` → `CLEAN`. Copilot re-reviewed automatically
   (`review_on_push`) at `f2d4247`: 🔵 informational, **0 new comments**.
7. Ran `autoharness gate copilot-review 118 --repo softwaresalt/graphtor-docs
   --enforcement auto --max-wait 900` → `SATISFIED: PASS`.
8. Updated the PR body: new `## Copilot Shadow-Review Remediation` section,
   `## Local Review Readiness` block updated to `Reviewed HEAD:
   f2d42474b272084aa77eaeaef6021b515ca5e4dc`, `Shadow review: clean`,
   follow-ups list extended with the Stage triage note for `CCAC612D`'s
   text/field mismatch.
9. Ran the full §1.9.1 GraphQL readiness query (single page, `hasNextPage:
   false`, 5 threads all resolved, `reviewDecision: null` — consistent with
   the ruleset's `required_approving_review_count: 0`). All 5 §1.9.4 checks
   pass:
   - Check 1 (coverage): `Reviewed HEAD` matches current `headRefOid`. ✅
   - Check 2 (outcome): `READY_WITH_FOLLOWUPS`, P0=0/P1=0. ✅
   - Check 3 (follow-ups explicit): `CCAC612D` + residual notes listed. ✅
   - Check 4 (full local build evidence): cargo check/fmt/clippy/test
     recorded, code-changing PR. ✅
   - Check 5 (P-018 Copilot gate): `SATISFIED: PASS`. ✅
10. Did **not** merge. No explicit P-014 approval signal was present in the
    operator's instructions (operator explicitly said to stop at this gate
    pending confirmation). Remained on
    `fix/graphtor-startup-checkpoint-recovery` throughout; never checked out
    `main`.

## Branch / PR state at session end

- Branch: `fix/graphtor-startup-checkpoint-recovery`, clean, pushed, HEAD
  `f2d42474b272084aa77eaeaef6021b515ca5e4dc`
- PR #118: OPEN, `mergeable: MERGEABLE`, `mergeStateStatus: CLEAN`
- All CI checks: pass (`build`, `detect code changes`, `pipeline topology gate`)
- All review threads: resolved (5/5)
- P-018 gate: `SATISFIED: PASS`
- §1.9 readiness gate: all 5 checks pass
- Merge: **not executed** — awaiting explicit operator approval (P-014)

## Next steps

- Await explicit operator merge approval referencing PR #118 at HEAD
  `f2d4247...`.
- On approval: re-run the last-mile §1.9 re-check (confirm HEAD hasn't
  advanced), confirm merge-commit strategy (P-009), execute merge, then
  proceed to post-merge closure (Step 6) — including creating a
  `post-merge/{feature_slug}` branch for closure artifacts, never committing
  directly to `main`.
- Stage follow-up (not performed by Ship): triage stash entry `CCAC612D`
  (ad hoc scripts disposition) and correct its `Kind: chore` → `Kind: task`
  text mismatch during that same triage pass.
