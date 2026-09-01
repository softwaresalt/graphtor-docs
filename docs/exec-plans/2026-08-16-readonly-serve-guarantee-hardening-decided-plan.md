---
title: "Read-only serve guarantee honesty (F2/F6 resolution) — decided plan"
description: "Decided plan: make the read-only serve contract authoritative to app-level AccessMode and correct the overstated OS-level guarantee"
date: 2026-08-16
decided: 2026-08-17
status: shipped
shipment: "047-S"
source: "docs/decisions/2026-08-16-readonly-serve-cross-process-coordination-spike.md"
related:
  - "docs/decisions/2026-08-16-shared-external-readonly-databases-deliberation.md"
supersedes: "docs/archive/plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md"
stash_ids:
  - "970AE45A"
  - "5D98DBCC"
tags:
  - read-only-serve
  - reliability
  - store
---

## Decision

`EngineReadonlyGuard` (`src/db/store.rs:472`) uses the filesystem read-only
attribute as a per-process lock with no cross-process reference counting, so
the documented "genuinely denied write access at the OS/filesystem level"
guarantee did not hold once the same file was independently guarded more than
once. The deliberation (`5D98DBCC`) rejected the external-path feature that
would have made concurrent multi-process read of one shared file common
rather than incidental, so the decided fix is **honesty-only**: the app-level
`AccessMode` remains the authoritative read-only guarantee; the filesystem
attribute is defense-in-depth that is robust only while a single owning
`DataStore` holds the guard and best-effort whenever the same file is
independently guarded more than once (same- or cross-process — the PR90 **F6**
deferral). No guard runtime behavior changed and no cross-process refcount
subsystem was built.

## Constraints Preserved

* `is_engine_enforced_readonly()` keeps meaning `guard.is_some()` — it is
  NOT overloaded to `access_mode == ReadOnly`.
* `EngineReadonlyGuard::lock` / `Drop` bodies are unchanged; exact-permission
  capture/restore and non-empty-sidecar preservation stay intact.
* App-level `AccessMode::ReadOnly` continues to refuse `mutate` regardless of
  guard state; the single-process byte-identical read-cycle behavior is
  unchanged (no cross-process closure is asserted).
* Principle III/IV containment is untouched — no external-path capability,
  `validate_path` unaltered.

## Rejected Alternatives

* **Redefine `is_engine_enforced_readonly()` as `access_mode == ReadOnly`.**
  Rejected (adversarial-review consensus, HIGH confidence): `open_sqlite_readonly`
  intentionally yields `AccessMode::ReadOnly` with guard `None`, so this would
  falsely advertise engine enforcement — the opposite of the honesty goal.
* **Build a cross-process refcount subsystem to close F6.** Rejected as
  disproportionate once the external-path feature was rejected — it would add
  durable cross-process state with its own crash-recovery and TOCTOU failure
  modes for a scenario that stays incidental.
* **Scope an in-process-only F6 mitigation.** Rejected: the harmful restore is
  performed by an independent peer guard on the same file, so a process-local
  ownership check could address only same-process duplication, not the
  cross-process window; true closure needs shared ownership/liveness
  coordination and is deferred (stash `F1CE20EC`).

## Implementation (as shipped)

* **Unit A1** (`src/db/store.rs`): repository-wide sweep correcting the
  `is_engine_enforced_readonly` rustdoc, the `open_engine_readonly` rustdoc and
  startup log, the `EngineReadonlyGuard` struct/field docs, and the serve entry
  point (`main.rs`) wherever an overstated read-only claim existed. Colocated
  characterization tests pin: engine-readonly reports `true`; `open_sqlite_readonly`
  reports `false` (predicate not overloaded); `AccessMode::ReadOnly` refuses
  `mutate`; the existing single-process byte-identical read cycle is unchanged.
  No separate `is_read_only()` accessor was added — the call-site survey found
  no caller needing to distinguish app-level read-only from engine-enforced.
* **Unit A2** (`docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`):
  replaced the unconditional OS-level write-denial claim with the honest
  statement above, linking the spike, the deliberation, and the deferred stash
  items `F1CE20EC` (true cross-process fix) and `5905CDEE` (symlink TOCTOU).

## Verification, Rollback, and Monitoring

* Verified via `cargo test` (new characterization tests plus the existing
  `open_engine_readonly_*` suite) and a live runtime check: built the release
  binary, ran `serve --read-only` against a throwaway fixture workspace, and
  confirmed the exact qualified log wording in real process output.
* **Bounded post-deploy observation window** (manual; no dashboard or
  alerting exists for this single-developer, local-only tool): the first 10
  `graphtor-docs serve` invocations against a `ReadOnly`-posture database, or
  14 days post-merge, whichever comes first; owner `@softwaresalt`. As of
  this writing the window remains **open** (well short of the threshold) —
  it closes per its own criteria with an owner-recorded
  healthy/degraded/rolled-back outcome.
* **Rollback trigger**: the qualified log wording fails to render, is
  misread as reintroducing an unconditional guarantee, or a future
  log-scraping/alerting integration silently stops matching because of the
  appended qualification — observed during the bounded window.
* **Rollback procedure**: a single-commit **text-only revert** with zero
  guard/runtime impact (`EngineReadonlyGuard::lock`/`Drop` untouched).
  Restoring the original unconditional "filesystem lock active" wording is
  explicitly OUT of scope for rollback — that is precisely the overstated
  claim this shipment corrects; a too-long/confusing qualified message is
  instead rolled back to a SHORTER but still-qualified message, never to the
  unconditional claim.
* F6 is recorded as an intentional, honest, best-effort residual — not closed.
  Genuine closure (a coordination primitive or single-owner serve topology) is
  tracked as follow-up stash `F1CE20EC`; the adjacent symlink-swap TOCTOU is
  tracked separately as stash `5905CDEE`.

## Plan Review Outcome

**PASS**, after one remediation cycle, reviewed by four independent
cross-model reviewers (anthropic / google-alt-provider / openai / xai) plus a
post-remediation re-review satisfying the adversarial-review requirement.
Round 1 raised four HIGH-confidence P1 findings — the predicate-overload
alternative (rejected above), a test spec that could not produce a true RED
phase, an incomplete contract-surface sweep, and a pre-existing symlink TOCTOU
now deferred as `5905CDEE` — all resolved before Round 2 re-review passed with
only non-blocking P3 advisories remaining.

## Shipped

Merged as shipment `047-S`, PR #97, commit `704b95a6c1e2930079d6f3a602ab66e9682d4916`.
Releasability status at closure: `READY_WITH_CONDITIONS` — the one open
condition is the post-deploy observation window above (see "Verification,
Rollback, and Monitoring"), not yet closed as of the closure record's date.
Full execution record (compacted 2026-09-01, see
`docs/closure/2026-09-01-047-s-048-s-closure-summary.md`): originally
`docs/closure/2026-08-17-047-s-post-merge-closure.md` and
`docs/closure/2026-08-17-047-s-release-observability-evidence.md`, now
archived at
`docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-047-s-post-merge-closure.md`
and
`docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-047-s-release-observability-evidence.md`.
Original plan (with full round-by-round review transcript and hardening detail)
archived at `docs/archive/plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md`.
