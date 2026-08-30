---
date: 2026-08-29
slug: 051-s-toctou-transition-closure
shipment: "051-S (closed) -> 054-S (new)"
mode: post-merge
status: READY
owner: "@softwaresalt"
---

# Post-Merge Transition Closure — 051-S Safe-Close + 054-S Rescoped Shipment

This is a **Ship-side administrative transition**, not a code-shipping
closure: no Rust source, `Cargo.toml`, or `Cargo.lock` changed. It executes
the "Ship-Side Transition" planned by
`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
after two already-merged PRs:

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
* Created a new shipment `054-S` for the rescoped, still-feasible
  permission-mutation containment scope (`059-F` + U1/U2/U3/U4/U5/U6/U10/U11
  + the operator sign-off gate `059.014-T`), per the decision's named
  "fresh shipment" alternative and this session's explicit operator
  instruction.
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

All four verified true post-transition (see Validator Evidence below).

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
  every mutation across the whole session (safe-close phase, then review-fix
  cycle 1). Safe-close phase: only `051-S` (relocated) and the two
  `return-blocked` targets (`blocked_reason` field only, `status`
  unchanged) appeared. Review-fix cycle 1 additionally shows `059-F` and
  eight task files (`059.001/002/003/004/005/006/010/011-T`) as modified —
  this is the intake-status normalization documented in Review-Fix Cycle 1
  below, not a cascade: no path moved into or out of `.backlogit/archive/`
  at any point after the safe-close phase completed.
* **049-S readiness**: `.backlogit/queue/049-S.md` frontmatter confirmed to
  have no `dependencies` field; `backlogit query` confirms no other
  top-level release unit is `active` (P-001 clean).
* **Dependency closure of 054-S**: every member's `depends_on` edges are
  either satisfied by another 054-S member or an already-`done` item
  outside the manifest (`059.007-T`) — table in the memory checkpoint.
* **`.gitignore` / `docs/scratch/` preservation**: `git diff .gitignore`
  hunk identical before and after the branch switch; SHA-256
  `9B8D4D54...` recorded; all 9 pre-existing `docs/scratch/` files still
  present and untracked.
* **`backlogit doctor`**: 140 pre-existing issues, 0 newly introduced, 0
  touching `051-S`/`054-S`/`059-F`/any `059.*` task/`049-S`.

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

* Confirm `origin/main`'s `.backlogit/archive/051-S.md` and
  `.backlogit/queue/054-S.md` match this session's final state exactly.
* Confirm `059-F`, `059.008-T`, `059.009-T`, `059.012-T`, `059.013-T` remain
  in `.backlogit/queue/` on `origin/main` (never archived).
* Confirm `049-S` remains claimable (no reintroduced dependency on
  `051-S`/`054-S`).

## Risky Action Record

| ProposedAction | ActionRisk | ActionResult |
|---|---|---|
| Return `059-F` and `059.008-T` from `051-S`'s manifest via `return-blocked` (status unchanged, membership only) | low (reversible; no status mutation; explicitly designed for this scenario) | applied, verified |
| Safe-close `051-S` as a single artifact (never the cascade `backlogit_ship_shipment`) | low (manifest-scoped; protected-set baseline + verify-after-each both passed) | applied, verified `CLOSED`, no cascade |
| Create shipment `054-S` directly as Ship (normally a Stage-only operation per Role Boundary) | moderate (role-boundary exception; justified by explicit operator instruction + the decision document's own named "Ship's choice" alternative; recorded non-silently) | applied, documented as an explicit, non-routine exception |
| Create post-merge transition branch `post-merge/059-f-toctou-transition` directly from `origin/main` while carrying an uncommitted operator `.gitignore` edit + untracked `docs/scratch/` across the switch | low (blob-hash-verified identical before switching; SHA-256 re-verified after; untracked files unaffected by checkout) | applied, verified byte-for-byte preserved |
| Transition `059-F` + eight units (`059.001/002/003/004/005/006/010/011-T`) directly `blocked → queued` (review-fix cycle 1, Copilot finding on PR #114) | low (status-field-only; dependency graph unchanged; verified valid direct transition in this backlogit version; clears `custom_fields.blocked_reason` as a documented side effect, narrative preserved in git history/decision doc) | applied, verified — all 10 `054-S` members now `queued`; `059.008-T`/`059.009-T` (out of scope) remain `blocked`, untouched |
| Do NOT mark `059.014-T` (operator sign-off gate) done; do NOT begin `059-F`/`054-S` implementation; do NOT claim `049-S` | n/a (explicit scope boundary, not an action) | honored — verified `059.014-T` still `queued`, no `054-S` member work begun beyond manifest creation + status normalization, `049-S` untouched |

## Healthy Signals

* `049-S` can be claimed and proceed independently without any residual
  coupling to `051-S`/`054-S`.
* `054-S` remains `queued` until the operator acts on `059.014-T`; once
  signed off, U1 (`059.001-T`) becomes actionable without further backlog
  rewiring (its dependencies are already satisfied: `059.007-T` done,
  `059.014-T` done).
* `backlogit doctor` continues to report `0` issues touching `051-S`,
  `054-S`, any `059.*` task, or `049-S`.

## Failure Signals

* Any future session finds `059-F`, `059.008-T`, or any other returned
  `059.*` sibling archived, deleted, or missing from `.backlogit/queue/`
  (would indicate an undetected cascade from this transition).
* `049-S` re-acquires a dependency on `051-S` or `054-S` without a new,
  explicit, evidence-based deliberation.
* `054-S` implementation begins before `059.014-T` reaches `done`.

## Monitoring Plan

Manual observation only — this is a single-developer, local-only backlog
state transition with no dashboard, log stream, or alerting surface.
`backlogit doctor` (already run, 0 new issues) and the reconciliation
reports under `.backlogit/reconcile/` are the durable, inspectable record.

## Rollback Trigger

Any post-merge discovery that `059-F`, `059.008-T`, or another protected
`059.*` sibling was cascade-archived, or that `049-S` unexpectedly
re-acquired a dependency on the closed/new shipment.

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
* New shipment `054-S`: `queued`, 10 dependency-closed members, **all 10
  confirmed `status: queued`** (intake-valid — see Review-Fix Cycle 1 below).
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

Both fixes are backlog-state + docs only (no `src/`/`Cargo.*` changes).
Reconciliation and doctor were not re-run for this cycle since neither
touches `051-S` (already closed and archived) or archive integrity — only
`054-S`'s already-`queued`, not-yet-archived manifest members changed
status, which is outside `shipment-reconcile`'s scope (that skill validates
manifests against `expected_status` at intake/closure time, not ad hoc
mid-session status edits). Direct field re-verification (table above)
stands as the evidence for this cycle.

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Manual observation (proportionate — backlog/docs-only change) |
| Pre-deploy audit | N/A — no migration/flag/cross-service dependency |
| Runtime verification | `PASS` — structural/backlog-state verification (no runtime surface changed) |
| Post-deploy observation window | Closed — no async rollout; end state already confirmed |
| Rollback trigger + procedure | Defined: revert + re-reconcile |
| Risky actions | All recorded above, `ActionResult: applied`/honored |
| Backlog closure | `CLOSED` (`051-S`) / created (`054-S`) — see Backlog Closure Evidence above |

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
    documenting the new return-blocked-before-safe-close +
    fresh-shipment-supersession pattern for future Ship sessions.

## Cross-References

* `docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
* `docs/memory/2026-08-29/ship-051-s-feasibility-blocked-memory.md`
* `docs/memory/2026-08-29/ship-051-s-054-s-transition-memory.md`
* `docs/compound/best-practices/shipment-supersession-return-blocked-then-safe-close-2026-08-29.md`
* `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`
* Follow-up items stashed: none (this transition completes the planned
  Ship-side scope; no new gaps surfaced).

## Stash Follow-Up Review

No follow-up tasks identified from this closure. `059.013-T` (Option A,
upstream cozo) and `059.012-T` (U12) already exist as queued, later-shipment
follow-ups per the decision document — not duplicated here.
