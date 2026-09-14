---
date: 2026-09-14
slug: 054-s-mcp-probe-ci-post-merge-closure
shipment: 054-S
feature: 056-F
mode: post-merge
status: READY_WITH_CONDITIONS
owner: "@softwaresalt"
compaction: done
---

# Post-Merge Closure — Shipment 054-S: Dedicated CI job for the standalone mcp-probe crate

PR [`#125`](https://github.com/softwaresalt/graphtor-docs/pull/125)
(`Add dedicated CI job for the standalone mcp-probe crate — shipment 054-S`,
`feat/054-s-mcp-probe-ci` → `main`) merged at
`2872de3379589c2b5a91a879861bbbb345e9171c` (merge commit, merge-commit
strategy per Constitution Principle XI / P-009), merged at
`2026-09-14T06:09:02Z`.

**Merge confirmation**: `gh pr view 125 --json state,mergedAt,mergeCommit`
returned `state: MERGED`, `mergeCommit.oid:
2872de3379589c2b5a91a879861bbbb345e9171c`. Independently confirmed via
`git fetch origin main` + `git merge-base --is-ancestor
2872de3379589c2b5a91a879861bbbb345e9171c origin/main` (exit 0), per the Merge
Confirmation Gate.

## Prerequisite Staging Publication

Stage had prepared `054-S` locally but could not push directly to `main`
(branch-protection rule requires PR + `detect code changes` check) and could
not open a PR itself (P-010). Ship published the 2 pending Stage commits via
an ordinary staging branch/PR before claim:

- Branch `chore/publish-054-s` from `543e080`.
- PR [`#124`](https://github.com/softwaresalt/graphtor-docs/pull/124) —
  merged via merge commit `c876d8505f6e0a1ebf3322cf21d8e1695b8e54dd`.
- 3 Copilot review findings on this PR were out of scope for a
  publication-only PR and were captured as P-021 deferred entries (stash
  `7A883184`, `8E8C6272`, `81588CD4`) rather than fixed inline.
- Local `main` refreshed after merge; `.backlogit/queue/054-S.md` verified
  present on `origin/main` with exact manifest `[056.028-T]`.

## Summary of the Change

Shipment `054-S` delivered a dedicated GitHub Actions CI job for the
standalone `tools/mcp-probe` diagnostic crate (own `Cargo.toml`/`Cargo.lock`,
outside the root workspace, so the primary fmt→clippy→test→audit pipeline
never reached it). Covering feature `056-F` (Phase 1.5 evidence
infrastructure); sole manifest member `056.028-T`. Feature `056-F` and all
other `056-*` siblings were intentionally excluded from the manifest
(protected set, P-015 partial-feature-shipment pattern).

### Changed Files

- `.github/workflows/mcp-probe-ci.yml` (new) — `probe-ci` job (Rust 1.75
  `fmt --check` / `clippy --all-targets -D warnings -D clippy::pedantic` /
  `test` / `build` against `tools/mcp-probe/Cargo.toml`, plus `cargo audit`
  against `tools/mcp-probe/Cargo.lock` via a separate `stable` toolchain
  scoped only to audit tooling to avoid the crate's pinned 1.75.0 MSRV
  conflicting with cargo-audit ≥0.22's rustc 1.88+ requirement) and
  `probe-ci-selftest` job (test-first seeded-violation red/green proof).
  Path-filtered on `tools/mcp-probe/**` and the workflow file itself; `mold`
  linker installed in both jobs to satisfy the repo-wide `.cargo/config.toml`
  linker override.
- `scripts/mcp-probe-ci-seeded-violation-check.sh` (new) — bash self-test:
  extracts the real `run:` command body from the workflow's clippy step
  (`extract_run_body`, comment-excluding, mawk-compatible), executes it
  against a seeded-violation copy of the crate (RED proof) and the clean
  crate (GREEN proof) — never a hardcoded mirror of the command.
- `scripts/mcp-probe-ci-seeded-violation-check.ps1` (new) — PowerShell twin
  of the same extraction/substitution/execution architecture
  (`Get-RunBody`/`ConvertTo-SingleLineCommand`), used locally as a
  proxy-validation vehicle (real cargo available on Windows, unlike the
  WSL environment used for the `.sh` script's structural checks).

No production Rust/probe source was modified, per the task's explicit
CI/workflow-only contract.

## Quality Gates (this session, against merged `main` content)

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` — pass.
- `cargo test` — pass.
- `actionlint` on the new workflow — pass.
- `bash -n` on the new script — pass.
- CI (PR #125, final HEAD `e7d0caa`): all 5 checks green, including
  `mcp-probe CI seeded-violation self-test` (authoritative confirmation the
  bash extraction/execution logic is correct against real `ubuntu-latest`
  cargo — the WSL environment used locally lacks cargo/rustup entirely and
  could only validate the script's structural/symlink logic).

## Review

- Local `review` gate (standard mode, iterative across 3 re-review rounds
  as new commits landed) plus GitHub-hosted Copilot review (advisory
  shadow review, `DARK_MODE_ACTIVE` scope).
- P-018 Copilot-Review Completion Gate: iterated through 3 rounds of newly
  surfaced threads (10 total across the PR's lifetime) as commits landed;
  every actionable in-scope finding was fixed and the thread resolved via
  GraphQL; every out-of-scope finding was captured per P-021 (see below).
  Final gate run: **`SATISFIED: PASS`** at HEAD `e7d0caa`.
- In-scope fixes applied directly (not deferred): CI trigger paths,
  `--locked`/`--deny warnings` flags, PR-body staleness, a symlink-guard
  root-item gap (`.ps1`), a `%TEMP%`-path-with-spaces tokenization bug
  (`.ps1`/`.sh`), and an `-ErrorAction SilentlyContinue` error-suppression
  bug (`.ps1`).
- Out-of-scope findings deferred per P-021 C2 (6 total, all committed to
  `.backlogit/stash.jsonl`, all citing the P-021 C1 out-of-scope rationale):
  `7A883184`, `8E8C6272`, `81588CD4` (staging PR #124 findings), `787C92F9`
  (pre-existing clippy lint debt — reused across 2 separate findings with the
  same root cause), `D948EC0B` (backlogit `status_delta` gap), `FF798FA3`
  (missing `concurrency:` group — verified no existing workflow in the repo
  has one).

## Backlog Closure Evidence

- `056.028-T` — **fully closed**. `backlogit move 056.028-T --status done`
  (task's own status-routing auto-relocated it to
  `.backlogit/archive/056.028-T.md` before the merge SHA existed —
  the documented `current-delivery-pending-finalization` scenario).
  `backlogit update 056.028-T --commit 2872de3379589c2b5a91a879861bbbb345e9171c`
  (commit-only frontmatter write, verified). `backlogit archive 056.028-T`
  (final archive markers). Final state: `status: archived`,
  `archived_status: done`, `archived_from:
  .backlogit/queue/056.028-T.md`, `commit:
  2872de3379589c2b5a91a879861bbbb345e9171c`. Evidence source:
  `current-delivery-post-terminal`, fail-closed provenance proof passed
  (membership, Ship-owned completion evidence, no foreign delivery, merge
  proven on `origin/main`, record-consistent). Full detail:
  `.backlogit/reconcile/054-S-safe-close-20260914T061736Z.md`.
- `054-S` (shipment record) — **NOT closed**. `backlogit move 054-S
  --status shipped` was refused by the installed backlogit CLI
  (`v1.10.1-0.20260823032255-b07729386a31+dirty`,
  `shipment_shipped_requires_envelope` guard — see
  `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`).
  Only the cascade `backlogit shipment ship` can perform this transition,
  and it is P-015-forbidden here: `054-S`'s manifest (`[056.028-T]`,
  task-only, zero feature members) does not qualify for the verified
  fully-covered-root exception (the sole task's ancestry does not lead back
  to any manifest-member root feature — its covering feature `056-F` is not
  in the manifest at all). No explicit, real-time, operator-authorized
  cascade exception was available in this AFK/dark-mode session, and the
  operator's pre-authorization was scoped to *ordinary merge approval*,
  explicitly excluding destructive actions. **Open halt handoff**:
  `.backlogit/reconcile/054-S-halt-20260914T061736Z.md` (`status: open`) —
  requires an explicit operator decision (authorize the one-time cascade
  exception with acknowledged `parent_id`-clearing risk on `056-F`'s
  out-of-manifest siblings, accept `054-S` remaining `status: active`
  indefinitely, or await a backlogit fix). Protected set (`056-F` and its
  out-of-manifest siblings) verified fully intact; no cascade occurred.

## Runtime Verification

Not applicable. This shipment is a CI/workflow-only change (new GitHub
Actions job plus two local self-test scripts); it touches no production
runtime surface, no server code path, and no deployed artifact. No
`runtime-verification` validator evidence is produced or required.

## Releasability Evidence

`READY_WITH_CONDITIONS`:

- The delivered CI job itself is fully green and merged — no condition on
  the shipped artifact.
- The **condition** is entirely backlog-bookkeeping: `054-S`'s shipment
  record remains `status: active` pending the operator decision recorded in
  the open halt handoff above. This does not block, weaken, or roll back
  the delivered work; it blocks only the shipment container's own terminal
  bookkeeping state.
- Monitoring: none required beyond normal CI health (the new job runs on
  every PR touching `tools/mcp-probe/**` or the workflow file itself).
- Rollback: revert PR #125's merge commit
  (`2872de3379589c2b5a91a879861bbbb345e9171c`) if the new CI job proves
  disruptive; no other production surface is affected.
- Owner: `@softwaresalt`.
- Validation window: none beyond the CI job's own green run, already
  observed on the merge commit.

## Documentation / Knowledge Graduation Review

Reviewed `docs/ARCHITECTURE.md`, `AGENTS.md`, `docs/design-docs/`,
`docs/product-specs/`, and `CHANGELOG.md` for relevance. No updates applied:
this is CI/tooling-internal infrastructure for a diagnostic crate with no
user-facing behavior, API, or architectural surface change, and no existing
document references `tools/mcp-probe` CI coverage that would need
correction.

## Compound Refresh

Reviewed `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`
against this session's evidence. Classification: **KEEP** (reconfirmed, not
superseded or invalidated) — this session independently reproduced the exact
`shipment_shipped_requires_envelope` constraint against a second, unrelated
shipment. Appended a "Reconfirmation — 054-S Closure" evidence section to
that file (additive only; no rewrite of the existing constraint
description).

## Follow-Up Handoff for Stage (Ship does not mutate stash/backlog)

The following are recorded here, in the session memory checkpoint, and in
the PR's local-review-readiness follow-up field, for a **future Stage
session** to triage/harvest. Ship performed no stash mutation beyond the
capture-only P-021 entries already created during execution (C5 carve-out):

1. Stash `7A883184` — deferred finding from staging PR #124 (see PR #124
   review thread for detail).
2. Stash `8E8C6272` — deferred finding from staging PR #124.
3. Stash `81588CD4` — deferred finding from staging PR #124.
4. Stash `787C92F9` — pre-existing clippy lint debt (`-A
   clippy::module_name_repetitions` scoped too broadly), surfaced twice
   across independent review rounds on PR #125.
5. Stash `D948EC0B` — backlogit `status_delta` gap noted during P-018
   remediation (threadless capture).
6. Stash `FF798FA3` — missing `concurrency:` group control on the new
   workflow (and, per the finding, potentially on other existing workflows
   repo-wide) — verified no existing workflow currently has one; a
   repo-wide concurrency-control convention decision is out of scope for
   this CI-only shipment.
7. **Shipment-closure tooling gap** — `054-S`'s open halt handoff
   (`.backlogit/reconcile/054-S-halt-20260914T061736Z.md`) requires an
   explicit operator/Stage-facilitated decision to reach terminal shipment
   closure. This is not a stash item; it is a standing backlog-tooling
   condition that should be resolved (ideally by a backlogit CLI/version
   change restoring a non-cascading shipment-status transition, or by a
   deliberate, documented, one-time operator-authorized cascade) before the
   next partial-feature, task-only shipment closure is attempted.

## Source Artifact Cleanup (handoff only — Ship does not mutate stash)

`054-S`'s `custom_fields.source_stash_id` / `source_deliberation_id` (and
plural equivalents) were not present on the shipment record at claim time
(it was Stage-assembled directly from the `056-F` planning artifacts, not
from a discrete stash entry) — no source-stash retirement handoff is
recorded for this shipment.

## Compaction (P-020)

`done`. `compact-context` (`target: memory`) invoked immediately after this
artifact's creation. Consolidated the three fresh 054-S memory files (Stage's
assembly + recovery/resume memory, plus this session's Ship lifecycle memory)
into `docs/memory/compacted/2026-09-14-054-s-compacted.md`; verbose originals
moved byte-for-byte to `docs/archive/memory/2026-09-14/`. Live backlog-tooling
artifacts (`.backlogit/reconcile/054-S-*`) and this closure artifact itself
were left untouched (not memory; not yet stale).

## 053-S Eligibility

- **Task-level prerequisite** (per `053-S`'s own body text: "member
  readiness must verify that `056.028-T` is terminal"): **SATISFIED** —
  `056.028-T` is `status: archived`, `archived_status: done`.
- **Shipment-level `dependencies` field** (`053-S.dependencies: [049-S,
  054-S]`): **NOT YET SATISFIED** for the `054-S` edge — `054-S` remains
  `status: active`, not `shipped`/archived, pending the open halt handoff
  above. `049-S`'s edge is satisfied (`archived`, `archived_status:
  shipped`).
- Ship did **not** claim `053-S` and made no manifest, planning, or status
  changes to `053-S` or `052-S`. Their manifests and dependency ordering are
  unchanged from before this session.

## Final Verification

- `git fetch origin main` + `git merge-base --is-ancestor
  2872de3379589c2b5a91a879861bbbb345e9171c origin/main` — exit 0.
- `git status --short -- ".backlogit/"` after item-level archival — only
  the expected `056.028-T` relocation and `hooks_queue.jsonl` change; no
  protected-set path appears as an unexpected deletion (P-007).

## Cross-References

- `.backlogit/reconcile/054-S-safe-close-20260914T061736Z.md`
- `.backlogit/reconcile/054-S-halt-20260914T061736Z.md` (open)
- `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`
- PR #124 (staging publication), PR #125 (release)
