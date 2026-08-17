---
title: "Read-only-serve cross-process coordination — ownership and liveness model"
type: spike
date: 2026-08-16
time_box: "4h"
conclusion: "pivot"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["plan", "learnings"]
stash_id: "970AE45A"
tags:
  - read-only-serve
  - cross-process
  - concurrency
  - workspace-containment
---

## Goal

When two `graphtor-docs` processes open the same database read-only at the same
time, what ownership and liveness model should govern the read-only open
semantics so the documented "engine-enforced read-only" guarantee stays true?
This covers PR90 deferrals F2 (status/query read-only cross-process open) and F6
(stale-lock liveness and ownership), which hinge on the same primitive:
`EngineReadonlyGuard` uses the filesystem read-only attribute as a lock with no
cross-process reference counting.

## Success Criteria

* A concrete diagnosis of whether the F6 stale-restore gap is real, and how
  large its blast radius is today.
* A comparison of ownership/liveness models with a recommended direction.
* A clear statement of how this spike gates the related feature `5D98DBCC`.

## Scope Constraints

Read-only investigation. No production changes, no prototype branch. Analysis is
grounded in current `src/db/store.rs`, `src/path/security.rs`,
`src/workspace/serve_discovery.rs`, and the trust-boundary design doc.

## Investigation Approach

1. Read the guard acquire/drop path and the read-only open path in `store.rs`.
2. Reconstruct the two-process interleaving described in the F6 report and test
   it against the actual code semantics.
3. Cross-check the documented guarantee in the trust-boundary design doc.
4. Enumerate ownership/liveness options and weigh them against the repo's
   constitution (Principle III/IV) and its "favor simplicity and reliability"
   posture.

## Findings

### What Was Discovered

**The read-only guarantee is two-layered, and only one layer is authoritative.**

* App-level: `DataStore { access_mode: AccessMode::ReadOnly }` gates writes in
  application code (`src/db/store.rs:34`, `:197`).
* Filesystem-level: `EngineReadonlyGuard::lock` marks the db file and existing
  `-wal`/`-shm`/`-journal` sidecars read-only, capturing each path's exact
  original `fs::Permissions`, and restores them on `Drop`
  (`src/db/store.rs:472`, `:582`).

CozoDB's SQLite backend always opens `SQLITE_OPEN_READWRITE|CREATE`, so the
filesystem attribute — not the engine — is what actually denies OS-level writes.
The filesystem attribute is therefore defense-in-depth layered on top of the
authoritative app-level `AccessMode`.

**The F6 cross-process stale-restore gap is real.** Reconstructing the
interleaving against the code:

1. Process A opens first: file writable, guard captures `original = writable`,
   marks read-only.
2. Process B opens while A is live: file already read-only, guard captures
   `original = read-only`, marks read-only again.
3. A drops first: `Drop` restores A's captured `original = writable`. The file
   is now writable while B's `DataStore` is still alive and
   `is_engine_enforced_readonly()` still returns `true`
   (`src/db/store.rs:235` returns `self.engine_readonly_guard.is_some()`).
4. Any pooled connection Cozo lazily opens in B after step 3 opens against a
   now-writable file.

The trust-boundary design doc asserts "every connection — including later pool
refills — is genuinely denied write access at the OS/filesystem level"
(`docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`).
Under multi-process concurrent read that assertion is not upheld. This is a
correctness gap between the documented guarantee and the implemented behavior,
independent of the `5D98DBCC` external-path feature.

**The blast radius today is bounded, not acute.**

* The app-level `AccessMode::ReadOnly` guard still refuses writes regardless of
  the filesystem attribute; the stale attribute only weakens the OS-level
  belt-and-suspenders layer, it does not by itself let `serve` mutate data.
* In the stated operator workflow, `sync` (the only writer) runs outside any dev
  workspace, so no writer is ever concurrent with these reads — the WAL
  reader/writer coordination is unaffected.
* `open_sqlite` already self-heals a stale read-only attribute for a later
  write-mode open via `clear_stale_readonly_lock` (`src/db/store.rs`), so the
  restore failure cannot silently wedge a future writer.
* Concurrent multi-process read of the *same* file is incidental today (two
  serve/query processes in the same workspace); it only becomes *common* if the
  `5D98DBCC` external-path feature makes one shared db routinely opened from many
  workspaces at once.

### What Was Tried and Failed

Considered treating the filesystem attribute as the primary read-only guarantee
and "fixing" it with a cross-process advisory refcount file (a counter per db
path, incremented on acquire, permissions restored only at zero — the fix the
stash entry proposes). Rejected as the recommended primary direction: it adds
durable cross-process state (a new counter file), which brings crash-recovery,
staleness, and TOCTOU concerns of its own, and it doubles down on a defense-in-
depth layer as if it were authoritative. That is more machinery and more failure
modes to make "concurrent multi-process read" robust — value that only
materializes if the (separately rejected) external-path feature ships.

### Remaining Unknowns

* Whether any caller genuinely needs a separate app-level `is_read_only()`
  predicate (investigate-first in the plan) — no caller may currently need it.
* Whether the operator wants to pursue the single-owner serve topology (Option C)
  that would truly eliminate cross-process concurrent-file access — deferred to
  stash `F1CE20EC`. Adversarial review resolved the earlier open question about
  an in-process fix: there is none; the honest-contract framing is the correct
  proportionate response.

## Recommendation

**Conclusion:** pivot. **Confidence:** high (diagnosis); high (reframed direction
after adversarial review).

Do not adopt a cross-process filesystem-attribute refcount as the primary model,
and do not overstate what an in-process change can achieve. Multi-model review
confirmed the F6 diagnosis and established that **no in-process change closes the
cross-process writable window**: the harmful restore is performed by the *peer*
process whose guard captured `original = writable`; a process cannot distinguish
a stale lock from a live peer's lock without shared ownership/liveness state.
Therefore:

1. **Keep the app-level `AccessMode` as the single authoritative read-only
   guarantee, and keep `is_engine_enforced_readonly()` meaning intact.** It must
   continue to report whether *this* handle holds the filesystem guard
   (`guard.is_some()`). Do NOT redefine it as `access_mode == ReadOnly`:
   `open_sqlite_readonly` deliberately sets `AccessMode::ReadOnly` with
   `engine_readonly_guard = None`, so that redefinition would make a store that
   is explicitly *not* engine-enforced falsely report engine enforcement — the
   opposite of honesty. If a distinct app-level read-only query is genuinely
   needed by a caller, add a separately named `is_read_only()` predicate
   (investigate-first) rather than overloading the engine method.
2. **Make the read-only contract honest across every surface, and record F6 as a
   documented best-effort limitation.** Correct the overstated claims in the
   `open_engine_readonly` rustdoc ("including ones opened later from its
   connection pool"), the `is_engine_enforced_readonly` rustdoc, the
   "filesystem lock active" startup log, and the trust-boundary design doc: the
   filesystem attribute is fully robust for single-process serving and
   best-effort defense-in-depth under concurrent multi-process read. Do not claim
   the window is closed.
3. **Defer, do not attempt, the true cross-process fix here.** Genuinely closing
   the window requires an ownership/liveness coordination primitive or the
   single-owner serve topology (Option C). Both are out of this unit and are
   captured as future work (stash `F1CE20EC`).

Adversarial review also surfaced an adjacent, pre-existing **symlink-swap TOCTOU**
in `EngineReadonlyGuard::lock`/`Drop` (`fs::set_permissions` follows links; the
main db at index 0 is not re-checked with `is_reparse_point`). It is a distinct
security-mechanism change requiring its own spike and is deferred to stash
`5905CDEE` rather than folded into the honesty unit.

This spike **gates `5D98DBCC`**: because the external-path feature would turn
concurrent multi-process read of one shared file from incidental into common, it
must not ship on top of an overstated read-only guarantee. Since the deliberation
rejects the external-path relaxation on constitutional grounds (see
`docs/decisions/2026-08-16-shared-external-readonly-databases-deliberation.md`),
concurrent multi-process read stays incidental and the correct, proportionate
response is the honest-guarantee hardening above — not a refcount subsystem.

## Next Steps

* Promote to `impl-plan` for a bounded reliability-hardening unit
  (`docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md`).
* Capture the "app-level AccessMode is authoritative; FS attribute is
  defense-in-depth" boundary as a compound learning.

## References

* `src/db/store.rs` — `AccessMode` (34), `open_engine_readonly` (197),
  `is_engine_enforced_readonly` (235), `EngineReadonlyGuard` (472), `Drop` (582),
  `clear_stale_readonly_lock`.
* `src/path/security.rs:143` — `validate_path` containment enforcement.
* `src/workspace/serve_discovery.rs:91` — `discover_served_databases`.
* `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md` — read-only serve hardening guarantee.
* `docs/compound/git-pull-blocked-by-sqlite-wal-lock.md` — SQLite WAL/sidecar cross-process fragility.
