---
title: "Ship post-merge transition — 051-S safe-close + 054-S rescoped shipment assembly (PR #111 / #113)"
date: "2026-08-29"
shipment: "051-S (closed) -> 054-S (created)"
feature: "059-F"
agent: "Ship"
status: "closure PR ready, not merged"
---

## Context

Acting as Ship for the post-merge transition/closure after two already-merged
PRs:

* **PR #111** (`72940e92d8fd19638a4cc25a40301a31babdbf1a`) — U7/U8 feasibility
  evidence (no production source; see
  `docs/memory/2026-08-29/ship-051-s-feasibility-blocked-memory.md` for the
  full U7 PASS / U8 BLOCKED narrative).
* **PR #113** (`92de0250e6e74d0f12a1126e040807ac83361629`) — Stage
  re-deliberation decision
  (`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`,
  Option B chosen) that decoupled `049-S` from `051-S` and planned (but did
  not execute — Stage does not mutate the active `051-S` manifest) the
  Ship-side transition performed in this session.

This session executes the "Ship-Side Transition" section of that decision
document verbatim, choosing its explicitly offered **alternative** path:
close `051-S` recording feasibility-complete, and assemble a fresh,
rescoped implementation shipment, rather than re-scoping `051-S` in place.

## Chosen transition and why

**Chosen**: close `051-S` (safe-close, single-artifact, never the cascade
`backlogit_ship_shipment`) after first returning `059-F` and `059.008-T`
from its manifest; then create a new shipment `054-S` for the rescoped
feasible implementation scope.

**Why this alternative over "re-scope `051-S` in place"**: the operator's
task instructions for this session explicitly directed "safe-close the
now-completed feasibility shipment record ... and create a new queued
near-term shipment," which is verbatim the decision document's named
alternative ("if a fresh shipment ID is preferred, Ship instead closes
`051-S` ... and ... assembles a new implementation shipment so `059-F` has
single-shipment membership"). This also produces a cleaner backlog record:
`051-S` now represents exactly what it delivered (U7/U8 feasibility
evidence), and `054-S` represents exactly the newly-rescoped, sign-off-gated
implementation scope — no manifest carries a mix of "evidence, done" and
"implementation, not yet started" semantics.

**Explicit Ship shipment-creation authorization note (P-010 boundary)**:
Ship's Role Boundary table forbids "create shipments" by default (that is
Stage's job). However, Ship's own Step 0.5 fallback path explicitly permits
direct shipment assembly "if the operator explicitly confirms they want to
bypass Stage," and the governing decision document itself frames the
fresh-shipment path as "Ship's choice." The current session's operator
instructions explicitly directed this exact action ("create a new queued
near-term shipment containing the sign-off gate plus dependency-closed
feasible implementation unit(s)"), which is read as that explicit
confirmation. This is recorded here as a deliberate, non-silent exception,
not a routine practice — future sessions should not treat "Ship creates
shipments" as default behavior.

## Old 051-S final state

* `status: archived`, `archived_status: done`,
  `commit: 92de0250e6e74d0f12a1126e040807ac83361629` (PR #113 — the merge
  that authorized this closure; the underlying U7/U8 evidence itself was
  produced by PR #111, `72940e92d8fd19638a4cc25a40301a31babdbf1a` — both
  SHAs recorded in the reconciliation reports since the archive record has
  only one `commit` field).
* Final manifest: `[059.007-T]` only (the two non-`done` original members,
  `059-F` and `059.008-T`, were returned from the manifest via
  `backlogit shipment return-blocked` *before* closure — their `status`
  fields were **not** touched by the return, both remain exactly `blocked`).
* File relocated `.backlogit/queue/051-S.md` → `.backlogit/archive/051-S.md`.

## New shipment created

* **ID**: `054-S`
* **Title**: "059-F near-term: store.rs permission-mutation TOCTOU
  containment (U1/U2/U3/U4/U5/U6/U10/U11), gated on 059.014-T operator
  sign-off"
* **Status**: `queued`
* **Priority**: `high`
* **Members** (10, dependency-closed as verified below):
  `059-F`, `059.001-T` (U1), `059.002-T` (U2), `059.003-T` (U3),
  `059.004-T` (U4), `059.005-T` (U5), `059.006-T` (U6), `059.010-T` (U10),
  `059.011-T` (U11), `059.014-T` (operator sign-off gate).
* **Explicitly excluded** (per the decision — remain for later, separate
  shipments and are NOT touched by this session): `059.008-T` (U8, terminal
  evidence, stays blocked/unshipped), `059.009-T` (U9, engine-open,
  re-pointed to `059.013-T`), `059.012-T` (U12, custom_flags leaf-primitive
  proof, re-pointed to `059.014-T`), `059.013-T` (Option A upstream cozo,
  non-blocking follow-up).

### Dependency-closure verification (before creating 054-S)

Confirmed via `backlogit query` that every enacted dependency edit from
PR #113 is already consistent with this manifest (no further backlog
rewiring needed):

| Item | Depends on | Satisfied by |
|---|---|---|
| 059.001-T (U1) | 059.007-T (done, outside manifest), 059.014-T (in manifest) | outside-manifest dep already `done`; in-manifest dep present |
| 059.002-T (U2) | 059.001-T | in manifest |
| 059.003-T (U3) | 059.002-T, 059.006-T | in manifest |
| 059.004-T (U4) | 059.002-T, 059.006-T | in manifest |
| 059.005-T (U5) | 059.003-T, 059.004-T, 059.006-T | in manifest |
| 059.006-T (U6) | 059.001-T, 059.002-T, 059.007-T (done, outside manifest) | in-manifest + already-done |
| 059.010-T (U10) | 059.003-T, 059.004-T, 059.006-T | in manifest |
| 059.011-T (U11) | 059.002-T | in manifest |
| 059.014-T (gate) | (none) | n/a — ready for operator action |

No member depends on an item outside the manifest that is not already
terminal (`done`). The manifest is **dependency-closed**.

Task width (<=2h): not re-derived in this session — these are pre-existing
tasks already decomposed by Stage's original harvest/plan-review, with the
2026-08-25 PR #107 test-scenario split explicitly bounding U5/U10/U11 to
fewer than four independently countable scenarios each. Ship did not
re-scope or re-split any task; only manifest membership was changed.

## Survival / status of 059-F and units

| Item | Status after this session | Notes |
|---|---|---|
| 059-F (feature) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Returned from 051-S, added to 054-S; still gated on 059.014-T sign-off before U1 can start (dependency graph gates readiness, not the status field) |
| U1 (059.001-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S; deps `059.007-T`(done)+`059.014-T`(queued) |
| U2 (059.002-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U3 (059.003-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U4 (059.004-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U5 (059.005-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U6 (059.006-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U7 (059.007-T) | `done`, archived (unchanged) | Pre-archived by PR #111; sole item that was ever a manifest member of the now-closed 051-S |
| U8 (059.008-T) | `blocked` (unchanged, terminal) | Returned from 051-S; NOT added to 054-S; NOT archived; remains visible in queue as the accepted-residual evidence record |
| U9 (059.009-T) | `blocked` (unchanged) | Not in 054-S; deps already re-pointed (PR #113) to 059.006-T + 059.013-T; later separate shipment |
| U10 (059.010-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U11 (059.011-T) | `queued` (transitioned from `blocked`, review-fix cycle 1) | Added to 054-S |
| U12 (059.012-T) | `queued` (unchanged) | Not in 054-S; deps already re-pointed (PR #113) to 059.014-T; later separate shipment |
| U13 (059.013-T, Option A) | `queued` (unchanged) | Not in 054-S; non-blocking follow-up, later separate shipment |
| U14 (059.014-T, sign-off gate) | `queued` (unchanged) | Added to 054-S; operator sign-off gate — **not marked done, not bypassed, by this session** |

### Review-fix cycle 1 (Copilot shadow review, PR #114)

Copilot's shadow review on PR #114 correctly identified that `054-S`'s
manifest, as originally assembled, could not pass Ship's own future Step 0.5
intake reconciliation: `059-F` and all eight U1/U2/U3/U4/U5/U6/U10/U11 units
were still `status: blocked` (a holdover from the OLD U8-gated dependency
chain that PR #113 already rewired away from at the *dependency-edge*
level, but their `status` field was never refreshed to match). `.ship.agent.md`
primary-path step 6 runs `shipment-reconcile mode: pre` with
`expected_status: queued` at intake, which halts with `status-mismatch` on
any manifest member whose status isn't `queued`/`active` — completing
`059.014-T` does not itself flip these members' status. Fixed by
transitioning all nine affected items directly `blocked → queued` via
`backlogit move <id> --status queued` (verified as a valid direct
transition in this backlogit version; no intermediate `active` hop
required). Re-verified afterward: all 10 `054-S` members now `status:
queued`; `059.008-T`/`059.009-T` (out-of-scope, correctly excluded from
`054-S`) remain `blocked`; `059.012-T`/`059.013-T` (out-of-scope) remain
`queued`, untouched. A second Copilot finding on the same root cause (the
new compound entry's procedure omitted this normalization step) was fixed
by adding an explicit "restore to intake-valid status" step to
`docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`.
Both fixes pushed as a follow-up commit on this PR; reconciliation
re-verified clean (see updated Reconciliation section below).

**Never cascade-closed or archived**: verified before and after every
mutation that `059-F` and all 13 non-U7 `059.*` siblings remained present in
`.backlogit/queue/` (never moved to `.backlogit/archive/`). See the
safe-close reconciliation report for the full baseline + verify-after-each
evidence.

## 049-S readiness

Confirmed independently ready, no residual dependency on `051-S`:

* `.backlogit/queue/049-S.md` frontmatter has **no `dependencies` field at
  all** (PR #113 already removed the `051-S` `blocks` edge).
* `backlogit shipment list --status queued` shows `049-S` with no
  `dependencies` key in its JSON record.
* `backlogit query "SELECT ... WHERE status='active' ..."` confirms no other
  top-level release unit is `active` (P-001 clean) — `049-S` is free to be
  claimed independently whenever the operator/Ship next picks it up. This
  session does **not** claim or begin `049-S` work (out of scope per the
  task instructions).

## Reconciliation (pre / safe-close / post)

* `.backlogit/reconcile/051-S-pre-20260829-203640.md` — `PROCEED` (sole
  remaining manifest item `059.007-T` pre-archived; 0 orphans).
* `.backlogit/reconcile/051-S-safe-close-20260829-203729.md` — `CLOSED`
  (14-member protected set — `059-F` + 13 non-U7 siblings — proven intact
  at baseline and after every mutation; shipment record archived as its own
  single artifact with commit SHA recorded; never the cascade
  `backlogit_ship_shipment`).
* `.backlogit/reconcile/051-S-post-20260829-203815.md` — `PROCEED` (archive
  file present, no deletions).
* `backlogit doctor`: 140 pre-existing issues found both before and after
  this session's mutations, none newly introduced, none touching `051-S`,
  `054-S`, `059-F`, any `059.*` task, or `049-S` (all pre-existing
  `archived_from_self_ref` / orphan debt on unrelated legacy items —
  explicitly out of scope for this transition).
* `backlogit sync`: ran before and after the transition; both succeeded
  (`Indexed 517 artifacts` before, `Indexed 518 artifacts` after — the `+1`
  is the newly-created `054-S`).

## Branch / commits / PR

* Prior local branch: `chore/stage-059-f-redeliberation` (already fully
  merged into `origin/main` via PR #113; left untouched, not deleted).
* New branch: `post-merge/059-f-toctou-transition`, created directly from
  `origin/main` (`git checkout -b post-merge/059-f-toctou-transition
  origin/main`) per Step 6.0's post-merge branch protocol, adapted per the
  compound learning
  `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`:
  verified `origin/main:.gitignore` blob (`ea76354`) matched the prior
  branch's committed `.gitignore` blob (also `ea76354`, i.e. the diff's
  "before" side) before switching, so the operator's dirty `.gitignore` edit
  (`+.backlogit/checkpoints/`, `+.backlogit/runtime/`) carried across
  byte-for-byte — confirmed via `git diff` (identical hunks before/after)
  and `Get-FileHash .gitignore` (`9B8D4D54...`). The 9 untracked
  `docs/scratch/` files were unaffected by the branch switch (untracked
  files are not touched by `git checkout`).
* Commits on this branch (backlog-only + docs, no `src/`/`Cargo.*` changes):
  will be committed after this memory/closure write, see task list below.
* No PR opened yet as of this checkpoint — created next, marked **DO NOT
  MERGE** per the task instructions; awaiting operator approval.

## Review / CI / readiness

* No Rust source changed — `Full local build: not applicable (backlog +
  docs only)`.
* `markdownlint` run on all newly-created/edited Markdown files before
  commit (see closure artifact for exact file list and result).
* Local Review Readiness block will be written into the PR body for the
  current HEAD once commits land, per
  `.github/instructions/github-pr-automation.instructions.md` §1.9.
* PR explicitly **not merged** this session per task instructions — closure
  PR is presented `READY` for operator review/approval only.

## Preservation confirmations

* `.gitignore`: dirty working-tree edit preserved byte-for-byte across the
  branch switch (verified: diff hunk identical, SHA-256 recorded).
* `docs/scratch/`: all 9 pre-existing untracked files still present,
  untouched, still untracked (not staged/committed by this session).
* `059.008-T`, `059.009-T`, `059.012-T`, `059.013-T`: none archived, none
  status-mutated at all this session (the `return-blocked` calls on
  `059.008-T` only removed manifest membership and recorded a
  `blocked_reason`, never changed `status`; `059.009-T`/`059.012-T`/
  `059.013-T` were never touched).
* `059-F`: NOT archived, but its `status` **was** deliberately mutated this
  session in two distinct steps — first `return-blocked` (membership-only,
  `status` stayed `blocked`), then a separate, explicit
  `backlogit move 059-F --status queued` in review-fix cycle 1 (see that
  section above) to make `054-S` intake-valid. This is a real status
  change, not a preservation — called out explicitly here so it isn't
  conflated with the status-preserving `return-blocked` operation.
* No cascade archival occurred at any point (verified via
  `git status --short -- ".backlogit/"` before and after every mutation —
  see the safe-close reconciliation report for the full diff evidence).

## Files modified this session (backlog + docs only)

* `.backlogit/queue/051-S.md` → archived to `.backlogit/archive/051-S.md`
* `.backlogit/queue/059-F.md` — `return-blocked` recorded `blocked_reason`
  (status unchanged at that point), then review-fix cycle 1 transitioned
  `status: blocked -> queued` (`blocked_reason` cleared as a side effect of
  that transition) — see Review-fix cycle 1 above
* `.backlogit/queue/059.008-T.md` — `blocked_reason` updated (status
  unchanged — stays `blocked`, correctly excluded from `054-S`)
* `.backlogit/queue/059.001-T.md` through `059.006-T.md`, `059.010-T.md`,
  `059.011-T.md` — review-fix cycle 1 transitioned each `status: blocked ->
  queued` (`blocked_reason` cleared as a side effect where present)
* `.backlogit/queue/054-S.md` (new)
* `.backlogit/hooks_queue.jsonl` — backlogit-managed append-only event log
* `.backlogit/reconcile/051-S-pre-20260829-203640.md` (new)
* `.backlogit/reconcile/051-S-safe-close-20260829-203729.md` (new)
* `.backlogit/reconcile/051-S-post-20260829-203815.md` (new)
* `docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
  (new)
* `docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md` (this file)
* `docs/closure/2026-08-29-051-s-toctou-transition-closure.md` (new,
  written next)
* **No files under `src/`, `Cargo.toml`, or `Cargo.lock` were changed.**
  `.gitignore` (operator's dirty edit) and `docs/scratch/` (untracked) left
  exactly as found.

## Open questions / not performed this session (explicitly out of scope)

* `059-F` U1-onward implementation (`054-S` scope) — **not started**; gated
  on operator sign-off `059.014-T`, which this session does **not** mark
  done or bypass.
* `049-S` evidence work — **not started**; only readiness was verified.
* `059.013-T` (Option A upstream cozo investigation) — untouched, remains
  queued for its own later, non-blocking shipment.
