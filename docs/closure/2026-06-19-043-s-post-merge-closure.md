---
date: 2026-06-19
slug: 043-s-post-merge-closure
shipment: 043-S
mode: post-merge
status: READY
owner: copilot
---

# Operational Closure — 043-S Triage and suppress post-042-S unmaintained transitive audit advisories

## Change Summary

PR `#71` merged shipment `043-S` at
`544138486a7360b4fdc9141628d9c97b8dbec298`.

Closure scope for this session:

* `043-S` (shipment)
* `043-C` (chore)
* `043.001-T` — verify number_prefix upgrade feasibility (proved infeasible)
* `043.002-T` — suppress remaining unmaintained advisories with rationale + review date

The shipped PR carried the implementation (`audit.toml` + `.github/workflows/ci.yml`),
review history, and shipment execution history. Post-merge closure adds shipment
archival, knowledge gardening, and release-closure records only. No application
runtime surface changed — the shipment touches the CI audit gate and the
human-readable suppression record only.

## Merge Confirmation

* PR `#71` state: `MERGED` (mergedAt `2026-06-20T05:45:27Z`)
* Merge commit: `544138486a7360b4fdc9141628d9c97b8dbec298`
* True merge commit — parents `2874a2f` (prior `main` tip) + `2666073` (PR HEAD)
* Merge strategy: **merge commit** (P-009 honored — squash/rebase NOT used)
* `--admin` bypass required: yes — branch protection `REVIEW_REQUIRED` was bypassed
  with explicit operator P-014 approval (operator account has admin bypass)
* Merge commit confirmed as an ancestor of `origin/main`
  (`git merge-base --is-ancestor` exit 0)

## Pre-Merge Readiness Gate (§1.9 — defense-in-depth re-check at HEAD 2666073)

| Check | Result |
| --- | --- |
| No pending Copilot review request (`reviewRequests.nodes` empty) | ✅ PASS |
| Latest Copilot review covers HEAD `2666073` (review @ `05:26:17Z`) | ✅ PASS |
| Zero unresolved Copilot review threads (6/6 resolved, no further pages) | ✅ PASS |
| CI `build` SUCCESS at HEAD `2666073` | ✅ PASS |

## Backlog Closure Actions

* Ran `shipment-reconcile` pre-mode (GI gate) before shipping — all 3 manifest
  items present in queue with expected statuses (`043-C` active, both tasks done),
  no orphans, zero queue/archive ID collisions → `PROCEED`
* Claimed shipment `043-S` (`queued` → `active`), then
  `backlogit shipment ship 043-S --sha 5441384` (`active` → `shipped`)
* Archived `043.001-T`, `043.002-T`, `043-C`, `043-S` to `.backlogit/archive/`
  with merge-SHA traceability; all carry `status: archived`
* Ran `shipment-reconcile` post-mode (GR gate) — all 4 items present in archive,
  absent from queue, P-007 archive-deletion guard clean → `PROCEED`
* Re-ran the level-1 ID collision scan post-ship — **zero rows** (the renumber
  `035-C → 043-C` held; no archive clobbering occurred)
* Committed backlog archival as a discrete commit on `main`
  (`chore: archive 043-S backlog artifacts (post-merge closure)`)

## Source Artifact Cleanup

* The archived `043-C` carries **no** `source_stash_id` and **no**
  `source_deliberation_id` custom field, so the explicit-link source-artifact
  cleanup protocol (Ship Step 6.7) did not trigger. No automatic archival of
  source artifacts was performed.
* Deliberation `002-DL` ("Triage post-042-S unmaintained transitive audit
  advisories", linked stash `964597B1`) is the evident — but **unlinked** —
  source for this chore. It remains `queued` and carries an unresolved
  operator-directed open question (narrow `013.008-T` to lz4_flex-only and fix
  its stale `blocked_reason`). It was **not** auto-archived: heuristic stale-item
  archival is explicitly out of scope, and the open question is still live. It is
  recorded under Follow-Up Items for Stage/operator intake.

## Knowledge Gardening

* `compound-refresh` (mode=apply) — see
  `docs/closure/2026-06-19-043-s-compound-refresh.md`
  * `docs/compound/cargo-audit-workspace-config-limitation.md` → **update**:
    added the `--deny warnings` allowlist-hardening pattern, the `^0.22` install
    pin, corrected the stale git2 `RUSTSEC-2026-0008` entry (resolved/dropped),
    refreshed example + evidence to the 043-S suppression set.
  * `docs/compound/backlogit-level1-id-collision-across-parent-types.md` →
    **update (light)**: added a confirmed-resolution section validating that the
    renumber held and `shipment ship` archived cleanly.
* Design-doc graduation: **not warranted** — durable decision rationale already
  exists at `docs/decisions/2026-06-18-unmaintained-transitive-audit-advisories-deliberation.md`,
  and the reusable patterns are captured in the two compound learnings above.
* `compact-context` (target=all): **assessed, no compaction performed** —
  `docs/memory/` holds 3 loose files (~13 KB), the `043-S` unit has only 2
  checkpoints, and no file exceeds 14 days. All triggers (40 files / 500 KB /
  14 days / 10-checkpoint mandatory) are below threshold. Session knowledge is
  already durable via the committed memory checkpoint, the two refreshed compound
  learnings, and this closure record.

## Invariants to Preserve

1. CI audit gate runs `cargo audit ... --deny warnings` so the `--ignore` set is
   an explicit allowlist; any NEW unmaintained/unsound advisory fails CI
2. cargo-audit stays pinned to the `^0.22` line in CI
3. `audit.toml` remains the authoritative human-readable suppression record with
   2026-09-18 review dates
4. `013.008-T` remains the separate owner of the `lz4_flex` (RUSTSEC-2026-0041)
   vulnerability path
5. the resolved git2 0.19 advisory (RUSTSEC-2026-0008) stays dropped
6. no shipped `043-S` artifact reappears in `.backlogit/queue/`

## Verification

| Check | Status | Notes |
| --- | --- | --- |
| CI `build` on merged HEAD `2666073` | ✅ | green (verified pre-merge) |
| `.backlogit/archive/043-S.md` status | ✅ | `archived`, commit `5441384` |
| `.backlogit/archive/043-C.md` status | ✅ | `archived`, commit `5441384` |
| `.backlogit/archive/043.001-T.md` / `043.002-T.md` | ✅ | `archived` |
| shipped scope absent from `.backlogit/queue/` | ✅ | only `002-DL`, `013.008-T` remain |
| P-007 archive-deletion guard | ✅ | no archive deletions |
| level-1 ID collision scan (post-ship) | ✅ | zero rows |

> No application runtime surface changed (CI/build-config only), so
> `runtime-verification` was not invoked.

## Deployment / Rollout Path

CI-configuration change. No application deployment step. The audit gate behavior
takes effect on the next CI run against `main`.

## Rollback Trigger

A legitimate advisory is masked by the allowlist, the `--deny warnings` gate
blocks CI on an advisory that should be suppressed, or the cargo-audit `^0.22`
pin breaks installation.

## Rollback Procedure

```text
git revert -m 1 544138486a7360b4fdc9141628d9c97b8dbec298
backlogit sync --cwd .
```

## Owner

Derek Williams (softwaresalt)

## Follow-Up Items

* **[for Stage/operator]** Deliberation `002-DL` open question: narrow
  `013.008-T` to lz4_flex-only and fix its stale `blocked_reason`. Once resolved,
  `002-DL` becomes eligible for archival (it is fully consumed by the shipped
  `043-C`). Captured here rather than stashed directly — stash intake is Stage's
  domain (Ship role boundary).
* **[scheduled 2026-09-18]** Re-review all suppressed unmaintained advisories
  (adler, bincode, fxhash, number_prefix, paste) — check whether the upstream
  blockers (cozo/swapvec, jieba-rs, hf-hub/indicatif, candle/tokenizers/gemm)
  released fixes so suppressions can drop.
* **[ongoing]** `013.008-T` remains the blocked owner of the `lz4_flex`
  (RUSTSEC-2026-0041) vulnerability path.
* **[repo hygiene, non-blocking]** `allow_rebase_merge=true` in repo settings —
  P-009 requires merge-commit-only. Recommend disabling rebase merge (squash is
  already disabled) so the merge-commit invariant is enforced by configuration.
