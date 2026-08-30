---
type: circuit-breaker
title: "Stage circuit-breaker: PR #113 review-fix cap reached, 7 new unresolved threads"
timestamp: "2026-08-30T01:47:07Z"
date: "2026-08-29"
agent: "Stage"
skill: "direct (circuit-breaker recording only)"
breaker_type: "skill-managed (review-fix cycle cap, per pull-request/github-pr-automation Section 1.8: limit 3)"
operation: "PR #113 Stage review-fix convergence"
attempts: 3
status: "halted; awaiting operator decision"
feature: "059-F"
shipment: "051-S (active, not mutated)"
pr: 113
repo: "softwaresalt/graphtor-docs"
branch: "chore/stage-059-f-redeliberation"
head: "46ab09461968523e9482d156e34e5814440e962e"
---

## Operation

PR #113 (`chore/stage-059-f-redeliberation`) Stage review-fix convergence against
Copilot shadow-review comments, governed by the review-fix cycle cap (limit 3;
`.github/instructions/github-pr-automation.instructions.md` Section 1.8, mirrored by
`circuit-breaker.instructions.md`'s "Review-fix cycles per task: 3").

## Failure Chain (cycles 1-3, cap reached)

### Cycle 1 — HEAD `869f035` — six threads fixed
U1 gated on U7 PASS + `059.014-T` (U8 BLOCKED = accepted input); U9 named `059.013-T`/U6 +
future safe API (DEFERRED); U3/U4/U5/U6/U10 + exec-plan dropped U9/U8-gate, U2/U11 aligned;
U12 repointed `059.008-T` → `059.014-T` (acyclic); 056-F removed stale 049-S↔051-S edge and
superseded `050-S -> 051-S -> 049-S`; control #3 serve trust-boundary guidance landed and
gated in `059.014-T`.

### Cycle 2 — HEAD `41026a4` — five new re-review threads fixed
Trust-boundary scope corrected: protect the workspace root namespace **and every parent
directory** (leaf write bit alone insufficient); threat is **not** serve-only —
`cmd_sync` reaches the same `open_sqlite` reopen via `with_locked_database_store`
(`src/main.rs:603-617`). Stage memory disclosed the `059.012-T` repoint. 056-F active
exec-plan (`2026-08-21-...regression-plan.md`) superseded at both sequencing surfaces.

### Cycle 3 — HEAD `46ab094` — three new re-review threads fixed
Root's-parent-directory trust boundary added (U7 no-follow bootstrap ambiently trusts the
opened root's parent); sign-off gate (`059.014-T`) named both leaf-swap and
intermediate-directory-swap forms; readiness HEAD refreshed. PR body recorded:
**"Review-fix cycle cap reached (3/3)."**

### Post-cycle-3 re-review at current HEAD `46ab094` — 7 new unresolved threads surfaced
A further Copilot re-review pass against the cycle-3 HEAD surfaced **7 new threads**, none
of which existed before cycle 3 closed. Per the cap, no fourth review-fix cycle was
attempted.

## Seven Current Unresolved Thread IDs (GraphQL, `reviewThreads`, all `isResolved: false`)

Confirmed via `gh api graphql` against PR #113 at HEAD `46ab09461968523e9482d156e34e5814440e962e`
(21 total threads; 14 resolved across cycles 1–3; 7 unresolved below).

| # | Thread node ID | Comment databaseId | File : line | Category |
|---|---|---|---|---|
| 1 | `PRRT_kwDORiB5E86deAwT` | `3888193657` | `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md:192` | Read-only status/query store opens omitted from trust boundary (linked 1/4) |
| 2 | `PRRT_kwDORiB5E86deAwZ` | `3888193668` | `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md:330` | Read-only status/query store opens omitted from trust boundary (linked 2/4) |
| 3 | `PRRT_kwDORiB5E86deAwc` | `3888193674` | `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md:381` | Read-only status/query store opens omitted from trust boundary (linked 3/4) |
| 4 | `PRRT_kwDORiB5E86deAwm` | `3888193686` | `.backlogit/queue/059.014-T.md:24` | Read-only status/query store opens omitted from trust boundary (linked 4/4 — sign-off precondition) |
| 5 | `PRRT_kwDORiB5E86deAwu` | `3888193696` | `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md:417` | `059.013-T` wrongly made a near-term rescope precondition (contradicts later/non-blocking classification) |
| 6 | `PRRT_kwDORiB5E86deAwx` | `3888193701` | `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md:410` | Rescoped manifest task-count error: lists 8 units (U1/U2/U3/U4/U5/U6/U10/U11), decision prose says 9 |
| 7 | `PRRT_kwDORiB5E86deAw0` | `3888193709` | `.backlogit/queue/059.001-T.md:29` | U1 acceptance contract missing explicit `cap-primitives` dependency/duplicate-tree check |

**Linked-comment grouping (4 of 7 are one defect class):** threads 1–4 all restate the same
gap — `status` and every query subcommand call `DataStore::open_sqlite_readonly`
(`src/main.rs:2760-2790`, `2958-2992`), which reaches the same bare-path
`open_sqlite_instance` as the write-mode path, but the trust-boundary doc, the residual
statement, compensating Control #3, and the `059.014-T` sign-off precondition all still
describe the threat as `serve`-plus-write-mode-only. Threads 5–7 are three independent,
narrower defects (precondition sequencing error, an off-by-one manifest count, and a missing
acceptance-criterion line item).

## Files Involved

- `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
- `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
- `.backlogit/queue/059.014-T.md`
- `.backlogit/queue/059.001-T.md`
- (unchanged this pass; listed for reference only — no edits made to any of the above)

## PR / HEAD

- PR: #113, `softwaresalt/graphtor-docs`, branch `chore/stage-059-f-redeliberation`
- HEAD verified identical across local `git rev-parse HEAD` and `gh pr view 113
  --json headRefOid`: `46ab09461968523e9482d156e34e5814440e962e`

## P0/P1 Classification Status

**Not yet adjudicated.** The 7 threads are Copilot shadow-review comments; per
`github-pr-automation.instructions.md` Section 1.3, each requires classification as
Valid / Partial / Invalid / Informational before a P0–P3 severity can be assigned, and per
Section 1.9.4 Check 2, only the **Local Review Readiness** block's own `Blocking findings`
field is authoritative for the merge gate — that field remains `P0=0, P1=0` from the Stage
plan-review persona pass (Security Lens, Scope Boundary, Constitution, Correctness, Template
Integrity; no P0/P1 found there). The 4-linked trust-boundary gap is plausibly a
security-completeness (Correctness/Security Lens) P1-class candidate given it affects the
sign-off gate's own precondition language, but this session did **not** run reviewer
adjudication — that determination is explicitly out of scope for this halt (no fix, no
backlog remedy task, no re-review dispatch was performed).

## Reason No Fourth Review-Fix Cycle Was Attempted

The review-fix cycle cap (limit 3, per `github-pr-automation.instructions.md` Section 1.8 and
`circuit-breaker.instructions.md`'s "Review-fix cycles per task: 3") was reached at cycle 3
(HEAD `46ab094`). Per that protocol: *"When the review-fix cycle limit is reached, unresolved
comments must be surfaced explicitly in the readiness summary and converted into follow-up
items or operator-visible residual-risk notes... they only remain merge-blocking if the
operator explicitly elevated shadow review to blocking status."* This task's explicit mandate
is circuit-breaker recording only — no fix, no backlog remedy task creation, no reply to or
resolution of the 7 threads, no review re-request, no merge. Per
`circuit-breaker.instructions.md` Escalation Protocol: stop, log (this file), prompt operator.

## Operator Choices Needed

1. **Accept the 4-linked trust-boundary gap as a real defect** (status/query read-only opens
   are in-scope for the trust boundary, the residual statement, Control #3, and the
   `059.014-T` sign-off precondition) and authorize a **new, separately-scoped Stage pass**
   (outside this cap) to fix docs/backlog language — or **explicitly defer** it as a follow-up
   item with documented rationale if the operator judges the read-only exposure acceptable
   for the sign-off gate as currently worded.
2. **Decide disposition of threads 5–7** (059.013-T precondition-sequencing wording, 8-vs-9
   task count, missing `cap-primitives` acceptance line) — fix, defer as follow-ups, or
   decline with rationale.
3. **Decide whether to elevate shadow review to merge-blocking** for PR #113, or accept the
   default advisory posture and proceed to merge readiness with the 7 threads recorded as
   follow-ups (per Section 1.8 cap-reached default).
4. **Confirm no fourth automated review-fix cycle should be attempted** for this PR without
   explicit operator instruction, consistent with the circuit-breaker halt.

## Preservation Status

- No source, backlog artifact, dependency graph, plan, or task content was altered this
  session. `.backlogit/queue/059.014-T.md` and `.backlogit/queue/059.001-T.md` were read
  only (via GraphQL review-comment context), not edited.
- No reply was posted to, and no thread was resolved for, any of the 7 unresolved GitHub
  review threads listed above.
- No backlog remedy task was created; no review was requested; no merge was attempted or
  performed.
- Scratch artifacts `docs/scratch/pr113-threads.json` and
  `docs/scratch/pr113-body-current.md` were created for this investigation and are excluded
  from this commit (scratch is git-ignored / excluded per task instruction).
- `.gitignore` shows a pre-existing uncommitted local modification (operator's own change,
  unrelated to this task); left untouched and excluded from this commit per task instruction.
- `docs/memory/` currently holds 47 files (~270 KB), above the `context-efficiency` mandatory
  compaction trigger (>40 files). `compact-context` was evaluated but **deliberately deferred**
  this pass: (a) a same-day compaction already ran at 2026-08-29 11:45 (
  `docs/memory/compacted/2026-08-29-050-s-memory-compaction.md`), (b) this task's explicit
  mandate is circuit-breaker recording only with no dependency/plan/task alteration, and (c)
  the task instruction directs excluding active `051-S`/`059-F`/`056-F` records — a safe
  compaction pass would need to scope precisely to non-active records, which is out of scope
  for a narrowly-bounded halt task. Recommend a dedicated `compact-context` pass in a
  follow-up session that explicitly protects `051-S`/`059-F`/`056-F` and any other active
  checkpoints.

## Next Safe Action

Halt. Do not fix, reply, resolve, request review, merge, or alter dependency/plan/task
content for PR #113 or its 7 open threads. Update the PR #113 **Local Review Readiness**
block to `Outcome: BLOCKED` at the current HEAD, citing the review-fix cycle cap and the 7
unresolved threads as the blocking condition, and await explicit operator direction per the
"Operator Choices Needed" section above before any further Stage or Ship action on this PR.
