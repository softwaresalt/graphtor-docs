---
date: 2026-08-29
slug: 051-s-toctou-transition-closure
shipment: "051-S (closed)"
mode: post-merge
status: READY
owner: "@softwaresalt"
---

# Post-Merge Transition Closure — 051-S Safe-Close + Rescoped-Scope Prep

This is a **Ship-side administrative transition**, not a code-shipping
closure: no Rust source, `Cargo.toml`, or `Cargo.lock` changed. It executes
the "Ship-Side Transition" planned by
`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
after two already-merged PRs, adapted per a Copilot shadow-review
correction (Review-Fix Cycle 2 below) that established shipment assembly
for the rescoped scope must be performed by a future **Stage** session, not
by Ship:

* **PR [`#111`](https://github.com/softwaresalt/graphtor-docs/pull/111)**
  merged `72940e92d8fd19638a4cc25a40301a31babdbf1a` (merge commit, 2 parents
  `92c4003` + `c4b6a51` — P-009 compliant) — U7/U8 feasibility evidence, no
  production source.
* **PR [`#113`](https://github.com/softwaresalt/graphtor-docs/pull/113)**
  merged `92de0250e6e74d0f12a1126e040807ac83361629` (merge commit, 2 parents
  `72940e9` + `9eef6db` — P-009 compliant) — re-deliberation decision and
  graph correction (`049-S` decoupled from `051-S`).

Both confirmed `MERGE_CONFIRMED` via `gh pr view {111,113} --json
state,mergedAt,mergeCommit` (`state: MERGED`) and `git merge-base
--is-ancestor {sha} origin/main` (exit 0 for both).

## Summary of the Change

* Closed shipment `051-S` (U7/U8 feasibility spike) via the `shipment-
  reconcile` safe-close protocol — **never** the cascade
  `backlogit_ship_shipment` — after first returning its two non-`done`
  members (`059-F`, `059.008-T`) from the manifest via `backlogit shipment
  return-blocked` so neither could be mistakenly archived.
* Prepared the rescoped, still-feasible permission-mutation containment
  scope (`059-F` + U1/U2/U3/U4/U5/U6/U10/U11 + the operator sign-off gate
  `059.014-T`) as **individual, unshipped, `queued`, dependency-closed**
  backlog items. **Ship does not create the successor shipment itself** —
  an earlier version of this session did (shipment `054-S`), which a
  Copilot shadow review correctly identified as a P-010 role-boundary
  violation; it was reverted (deleted) — see Review-Fix Cycle 2.
* No implementation work began. `059.014-T` (operator sign-off) was neither
  marked done nor bypassed. `049-S` was not claimed.

## Invariants to Preserve

* `059-F` and every `059.*` sibling task not explicitly named `done`/PASS
  must never be silently archived or cascade-closed as a side effect of
  closing `051-S`.
* `049-S` must have zero remaining dependency on `051-S`.
* The operator's dirty `.gitignore` edit and untracked `docs/scratch/`
  files must survive the branch switch byte-for-byte.
* No shipment closure may use the cascade `backlogit_ship_shipment`.
* Ship must not create shipments (P-010, NON-NEGOTIABLE).

All five verified true post-transition (see Validator Evidence below).

## Validator Evidence (Runtime Verification)

No runtime/source surface changed — this closure is a backlog-state and
documentation transition. Runtime-verification's adapter-based probes are
not applicable (no binary, MCP tool, or CLI behavior changed). The
equivalent structural verification for this scope is the
`shipment-reconcile` protocol plus direct backlog-state assertions,
performed and re-confirmed in this session:

* **Protected-set integrity**: baseline gate (before any archival) and
  verify-after-each invariant (after the shipment-record archival) both
  confirmed all 14 protected artifacts (`059-F` + 13 non-U7 `059.*`
  siblings) remained in `.backlogit/queue/`, never in
  `.backlogit/archive/`. See
  `.backlogit/reconcile/051-S-safe-close-20260829-203729.md`.
* **No cascade**: `git status --short -- ".backlogit/"` inspected after
  every mutation across the whole session (safe-close phase, review-fix
  cycle 1, review-fix cycle 2). Safe-close phase: only `051-S` (relocated)
  and the two `return-blocked` targets (`blocked_reason` field only,
  `status` unchanged) appeared. Review-fix cycle 1 additionally shows
  `059-F` and eight task files (`059.001/002/003/004/005/006/010/011-T`) as
  modified — intake-status normalization, not a cascade. Review-fix cycle 2
  additionally shows the erroneously-created `054-S` deleted — a corrective
  revert, not a cascade: no protected-set path ever moved into or out of
  `.backlogit/archive/` at any point.
* **049-S readiness**: `.backlogit/queue/049-S.md` frontmatter confirmed to
  have no `dependencies` field; `backlogit query` confirms no other
  top-level release unit is `active` (P-001 clean).
* **Dependency closure of the rescoped scope**: every member's `depends_on`
  edges are either satisfied by another scope member or an already-`done`
  item outside the scope (`059.007-T`) — table in the memory checkpoint.
  This holds independent of shipment membership.
* **`.gitignore` / `docs/scratch/` preservation**: `git diff .gitignore`
  hunk identical before and after the branch switch; SHA-256
  `9B8D4D54...` recorded; all 9 pre-existing `docs/scratch/` files still
  present and untracked.
* **`backlogit doctor`**: 140 pre-existing issues, 0 newly introduced, 0
  touching `051-S`/`059-F`/any `059.*` task/`049-S`.
* **No Ship-created shipment survives**: `backlogit get 054-S` returns
  "artifact not found"; `backlogit sync` reports `517` artifacts, matching
  the pre-session baseline.

**Verdict**: `PASS` (structural/backlog-state verification; no
runtime-surface adapters applicable to this scope). No manual checkpoints
applicable (no OAuth/payment/email/external-service flow in scope).

## Pre-Deploy Audits

Not applicable — no feature flag, migration, schema, or cross-service
dependency. `Full local build: not applicable` (no Rust source changed).

## Deployment / Rollout Path

Merge-only, once approved. No build, release, or restart of the
`graphtor-docs` binary is involved — this is a backlog/documentation state
transition that takes effect the moment the closure PR merges to `main`.

## Post-Deploy Checks

* Confirm `origin/main`'s `.backlogit/archive/051-S.md` matches this
  session's final state exactly, and that `.backlogit/queue/054-S.md`
  does **not** exist (Ship-created shipment reverted).
* Confirm `059-F`, `059.008-T`, `059.009-T`, `059.012-T`, `059.013-T` remain
  in `.backlogit/queue/` on `origin/main` (never archived).
* Confirm `049-S` remains claimable (no reintroduced dependency on
  `051-S`).

## Risky Action Record

| ProposedAction | ActionRisk | ActionResult |
|---|---|---|
| Return `059-F` and `059.008-T` from `051-S`'s manifest via `return-blocked` (status unchanged, membership only) | low (reversible; no status mutation; explicitly designed for this scenario) | applied, verified |
| Safe-close `051-S` as a single artifact (never the cascade `backlogit_ship_shipment`) | low (manifest-scoped; protected-set baseline + verify-after-each both passed) | applied, verified `CLOSED`, no cascade |
| Create shipment `054-S` directly as Ship | **high (P-010 role-boundary violation)** — Ship's Role Boundary is NON-NEGOTIABLE and forbids shipment creation with no operator-confirmation carve-out | **reverted** (review-fix cycle 2): `backlogit delete 054-S --force`; verified removed from queue and index (`backlogit sync`: `517` artifacts, matching pre-session baseline) |
| Create post-merge transition branch `post-merge/059-f-toctou-transition` directly from `origin/main` while carrying an uncommitted operator `.gitignore` edit + untracked `docs/scratch/` across the switch | low (blob-hash-verified identical before switching; SHA-256 re-verified after; untracked files unaffected by checkout) | applied, verified byte-for-byte preserved |
| Transition `059-F` + eight units (`059.001/002/003/004/005/006/010/011-T`) directly `blocked → queued` (review-fix cycle 1, Copilot finding on PR #114) | low (status-field-only; dependency graph unchanged; verified valid direct transition in this backlogit version; clears `custom_fields.blocked_reason` as a documented side effect, narrative preserved in git history/decision doc) | applied, verified — all 10 rescoped-scope members now `queued`; `059.008-T`/`059.009-T` (out of scope) remain `blocked`, untouched |
| Do NOT mark `059.014-T` (operator sign-off gate) done; do NOT begin `059-F` implementation; do NOT claim `049-S`; do NOT create a successor shipment | n/a (explicit scope boundary, not an action) | honored (after review-fix cycle 2 correction) — `059.014-T` still `queued`, no implementation begun, `049-S` untouched, no shipment exists for the rescoped scope |

## Healthy Signals

* `049-S` can be claimed and proceed independently without any residual
  coupling to `051-S`.
* The rescoped scope (`059-F` + 8 units + `059.014-T`) stays `queued` and
  dependency-closed until a future Stage session assembles it into a
  shipment and the operator acts on `059.014-T`; once both happen, U1
  (`059.001-T`) becomes actionable without further backlog rewiring.
* `backlogit doctor` continues to report `0` issues touching `051-S`, any
  `059.*` task, or `049-S`.
* No shipment named `054-S` (or any other Ship-created shipment) exists.

## Failure Signals

* Any future session finds `059-F`, `059.008-T`, or any other returned
  `059.*` sibling archived, deleted, or missing from `.backlogit/queue/`
  (would indicate an undetected cascade from this transition).
* `049-S` re-acquires a dependency on `051-S` without a new, explicit,
  evidence-based deliberation.
* A future Ship session creates a shipment directly instead of redirecting
  to Stage (repeat P-010 violation).
* Implementation of the rescoped scope begins before `059.014-T` reaches
  `done` and/or before a Stage session has assembled it into a shipment.

## Monitoring Plan

Manual observation only — this is a single-developer, local-only backlog
state transition with no dashboard, log stream, or alerting surface.
`backlogit doctor` (already run, 0 new issues) and the reconciliation
reports under `.backlogit/reconcile/` are the durable, inspectable record.

## Rollback Trigger

Any post-merge discovery that `059-F`, `059.008-T`, or another protected
`059.*` sibling was cascade-archived, that `049-S` unexpectedly
re-acquired a dependency on `051-S`, or that a shipment exists for the
rescoped scope that Ship created rather than Stage.

## Rollback Procedure

`git revert` the closure PR's merge commit (backlog-state-only diff, safely
revertible); re-run `shipment-reconcile mode: pre` against `051-S`'s
original three-item manifest to re-establish the pre-transition baseline
before re-attempting the transition.

## Validation Window

None open — the transition is complete and self-verifying (reconciliation
reports + doctor + direct field checks all already confirm the end state).
No async rollout.

## Owner

`@softwaresalt` (sole maintainer).

## Backlog Closure Evidence

* Pre-mode: `.backlogit/reconcile/051-S-pre-20260829-203640.md` — `PROCEED`.
* Safe-close: `.backlogit/reconcile/051-S-safe-close-20260829-203729.md` —
  `CLOSED` (14-member protected set verified intact; shipment archived as
  its own single artifact; merge SHA `92de025` recorded; never the cascade
  `backlogit_ship_shipment`).
* Post-mode: `.backlogit/reconcile/051-S-post-20260829-203815.md` —
  `PROCEED`.
* Rescoped scope: 10 dependency-closed items, **all 10 confirmed
  `status: queued`** (intake-valid — see Review-Fix Cycle 1 below),
  **unshipped** (no successor shipment created by Ship — see Review-Fix
  Cycle 2 below).
* `049-S`: confirmed `queued`, zero dependencies, independently claimable.

## Review-Fix Cycle 1 (Copilot shadow review, PR #114)

Two related findings on the initial PR #114 diff, both fixed and pushed in
one follow-up commit:

1. **`054-S` manifest not intake-valid**: `059-F` and all eight
   U1/U2/U3/U4/U5/U6/U10/U11 units were still `status: blocked` (carried
   over from the pre-PR#113 U8-gated dependency chain; their status field
   was never refreshed even though PR #113 already rewired the underlying
   dependency edges). Ship's own Step 0.5 intake reconciliation
   (`expected_status: queued`) would `HALT` on this the moment `054-S` is
   claimed. **Fixed**: `backlogit move <id> --status queued` on all nine
   affected items (verified valid direct `blocked → queued` transition; no
   status mutation to out-of-scope `059.008-T`/`059.009-T`, which correctly
   remain `blocked`).
2. **Compound doc omitted the fix's own prerequisite**: the newly-authored
   `docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
   documented "return-blocked → safe-close → assemble successor" without
   the status-restore step, so following it verbatim would reproduce
   finding 1. **Fixed**: added an explicit "restore to intake-valid status"
   step (and a final "verify intake-readiness" step) to the compound entry.

## Review-Fix Cycle 2 (Copilot shadow review, PR #114 — P-010 remediation)

Two further findings, both fixed and pushed in a second follow-up commit:

1. **Mandatory / P-010 violation**: creating shipment `054-S` directly as
   Ship violates the NON-NEGOTIABLE role boundary. Both
   `.github/agents/.ship.agent.md`'s Role Boundary table ("create
   shipments" is Forbidden; "Do not proceed past this boundary even under
   operator pressure. Record P-010 and halt.") and the canonical
   `.github/policies/workflow-policies.md` P-010 definition ("Ship MUST
   NOT: Create backlog items, create shipments..."; "Do not proceed past
   the boundary even if the operator requests work outside scope —
   redirect to the correct agent instead.") are unconditional. This
   session's earlier reasoning — that an explicit operator instruction plus
   `.ship.agent.md`'s own Step 0.5 fallback text constituted a valid
   exception — was incorrect; the canonical policy document takes
   precedence and has no such carve-out. **Fixed**: `054-S` deleted
   (`backlogit delete 054-S --force`); verified removed from
   `.backlogit/queue/` and the index (`backlogit sync`: `517` artifacts,
   matching the pre-session baseline). The 10-item rescoped scope remains
   prepared (`queued`, dependency-closed) as individual, unshipped backlog
   items for a future Stage session to assemble.
2. **Compound doc generalization**: the normalization step added in
   Review-Fix Cycle 1 was scoped only to manifest members *returned* in
   this session, but eight of the nine affected items were actually
   returned from `051-S` in an *earlier*, already-merged session. **Fixed**:
   generalized the compound entry's normalization guidance to cover every
   successor-scope member whose `blocked` state originated from the
   superseded dependency chain (regardless of when it was returned), and
   replaced the "Ship creates the successor shipment" framing with "Ship
   prepares the scope; Stage assembles the shipment."

All five review threads across both cycles (2 in cycle 1, 3 in cycle 2)
were replied-to with the fix commit SHA and resolved via the GraphQL
`resolveReviewThread` mutation.

Both cycles' fixes are backlog-state + docs only (no `src/`/`Cargo.*`
changes). Reconciliation and doctor were not re-run for these cycles since
neither touches `051-S` (already closed and archived) or archive
integrity — the changes were status-field normalization (cycle 1) and a
shipment-record creation-then-deletion (cycle 2), both outside
`shipment-reconcile`'s scope (that skill validates manifests against
`expected_status` at intake/closure time, not ad hoc mid-session status
edits or non-manifest shipment lifecycle). Direct field re-verification
(tables above; `backlogit get 054-S` → not found; `backlogit sync` → `517`
artifacts) stands as the evidence for both cycles.

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Manual observation (proportionate — backlog/docs-only change) |
| Pre-deploy audit | N/A — no migration/flag/cross-service dependency |
| Runtime verification | `PASS` — structural/backlog-state verification (no runtime surface changed) |
| Post-deploy observation window | Closed — no async rollout; end state already confirmed |
| Rollback trigger + procedure | Defined: revert + re-reconcile |
| Risky actions | All recorded above, `ActionResult: applied`/`reverted`/honored |
| Backlog closure | `CLOSED` (`051-S`); rescoped scope prepared but **unshipped** (no shipment created — see Backlog Closure Evidence above) |

**Releasability status**: `READY` — all transition work is complete,
verified, and requires no further conditions. The closure PR is presented
for operator review only; **it is not merged by this session** per explicit
task instruction.

## Source Artifact Cleanup

Reviewed `custom_fields` on the covering feature `059-F`: no
`source_stash_id` or `source_deliberation_id` key present (this feature's
provenance is tracked via `references:`/`links:` to
`docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`
and the 2026-08-29 re-deliberation, not via `custom_fields`). Nothing to
archive under this protocol — logged as "not present → skip."

## Documentation / Knowledge Graduation Review

* `docs/ARCHITECTURE.md` — no structural change; not touched.
* `AGENTS.md` — no agent or skill change; not touched.
* `docs/design-docs/` — no new durable design decision to graduate this
  session; the re-deliberation decision document itself already carries the
  durable rationale.
* `docs/product-specs/` — no requirement change.
* `docs/compound/` —
  * Upvoted (re-verified) `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`
    — the branch-directly-from-origin/main technique worked identically a
    second time.
  * Added
    `docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
    documenting the return-blocked-before-safe-close + status-normalization
    pattern for future Ship sessions, revised twice during this PR's review-fix
    cycles: cycle 1 added the intake-status normalization step; cycle 2
    generalized it to cover all superseded-chain members (not just those
    returned in the same session) and corrected the framing from "Ship
    creates the successor shipment" to "Ship prepares the scope; Stage
    assembles the shipment" (P-010 compliance).

## Cross-References

* `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
* `docs/memory/2026-08-29/ship-051-s-feasibility-blocked-memory.md`
* `docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md`
* `docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
* `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`
* Follow-up items stashed: **none, by design** (see Stash Follow-Up Review
  below — creating a stash entry is also a Stage-only operation under
  P-010).

## Stash Follow-Up Review

One genuine follow-up exists — **a future Stage session must assemble the
rescoped 10-item scope (`059-F` + U1/U2/U3/U4/U5/U6/U10/U11 + `059.014-T`)
into a successor shipment** — but this closure does **not** create a stash
entry for it. `.github/policies/workflow-policies.md`'s P-010 definition
lists "Perform stash operations, triage, or deliberation" under **Ship MUST
NOT**, the same unconditional list that forbids shipment creation (see
Review-Fix Cycle 2). Recording this follow-up as a stash entry would repeat
the same category of role-boundary violation this closure just remediated.
Instead, the follow-up is recorded here, in the memory checkpoint's "Open
questions" section, and in the PR description, as plain documentation for
the operator/Stage to act on directly. `059.013-T` (Option A, upstream
cozo) and `059.012-T` (U12) already exist as queued, later-shipment
follow-ups per the decision document — not duplicated here.
