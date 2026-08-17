---
title: "Read-only serve guarantee honesty (F2/F6 resolution)"
description: "Make the read-only serve contract authoritative to app-level AccessMode and correct the overstated OS-level guarantee"
date: 2026-08-16
source: "docs/decisions/2026-08-16-readonly-serve-cross-process-coordination-spike.md"
related:
  - "docs/decisions/2026-08-16-shared-external-readonly-databases-deliberation.md"
stash_ids:
  - "970AE45A"
  - "5D98DBCC"
tags:
  - read-only-serve
  - reliability
  - store
---

## Problem Frame

The spike (`970AE45A`) confirmed a correctness gap between the documented read-
only serve guarantee and the implemented behavior. The trust-boundary design doc
claims that for a `ReadOnly` database "every connection — including later pool
refills — is genuinely denied write access at the OS/filesystem level." That is
true for single-process serving, but `EngineReadonlyGuard`
(`src/db/store.rs:472`) uses the filesystem read-only attribute as a per-process
lock with no cross-process reference counting. Under concurrent multi-process
read of the same file, one process's `Drop` (`src/db/store.rs:582`) can restore
the file to writable while a peer process's `DataStore` is still alive and still
reports `is_engine_enforced_readonly() == true` (`src/db/store.rs:235`, which
returns `self.engine_readonly_guard.is_some()`).

The deliberation (`5D98DBCC`) rejected the external-path feature that would make
concurrent multi-process read common, on NON-NEGOTIABLE Principle III/IV grounds.
Concurrent multi-process read therefore stays incidental. The proportionate,
constitution-compliant fix is to make the read-only contract honest rather than
to build a cross-process refcount subsystem: the app-level `AccessMode` is the
authoritative read-only guarantee, and the filesystem attribute is defense-in-
depth that is robust while a single owning `DataStore` holds the guard and
best-effort whenever the same file is independently guarded more than once
(same- or cross-process — F6).

This plan was revised after multi-model adversarial review. The original draft
proposed redefining `is_engine_enforced_readonly()` as `access_mode == ReadOnly`;
reviewers correctly showed that is *more* deceptive (`open_sqlite_readonly` sets
`AccessMode::ReadOnly` with `engine_readonly_guard = None`, so it would falsely
report engine enforcement) and that no in-process change closes the cross-process
window. The scope below is honesty-only: correct every overstated read-only
contract surface and document F6 as a known limitation. It does not change guard
runtime behavior, does not overload the predicate, and does not claim to close
F6.

## Requirements Trace

| Requirement (from spike/deliberation) | Implementation action |
|---|---|
| App-level `AccessMode` remains the authoritative read-only guarantee | Unit A1: keep `is_engine_enforced_readonly()` == `guard.is_some()`; do not overload it |
| Every read-only contract surface states the guarantee honestly | Unit A1: repository-wide sweep — correct the `open_engine_readonly` rustdoc, `is_engine_enforced_readonly` rustdoc, the `open_sqlite_readonly` rustdoc, the `EngineReadonlyGuard` field/type docs, the "filesystem lock active" startup log, and any overstated read-only claim in the serve entry point (`main.rs` `open_serve_databases`); Unit A2: correct the design doc |
| F6 is recorded as a known best-effort limitation, not a closed gap | Unit A1 tests pin honest invariants; Unit A2 documents the limitation |
| No new durable cross-process state; guard runtime behavior unchanged | Unit A1: no change to `EngineReadonlyGuard::lock`/`Drop` |
| Distinguish app-level read-only from engine-enforced only if a caller needs it | Unit A1: investigate call sites; add a separate `is_read_only()` predicate only on demonstrated need |

## Implementation Units

### Unit A1 — Make the read-only contract honest across all surfaces (code)

* Changes in `src/db/store.rs`, behavior-preserving for the guard:
  * Keep `is_engine_enforced_readonly()` returning `self.engine_readonly_guard.is_some()`.
    Do **not** redefine it as `access_mode == ReadOnly` — that would make
    `open_sqlite_readonly` stores (`AccessMode::ReadOnly`, guard `None`) falsely
    report engine enforcement.
  * Correct its rustdoc to state precisely that it reports whether *this handle*
    currently holds the filesystem read-only guard, and that the OS-level
    guarantee is robust only while a single owning `DataStore` holds the guard;
    whenever the same file is guarded by more than one independent guard —
    same-process (two independent `open_engine_readonly` calls on one path) or
    cross-process — a peer guard dropping first can restore the file to writable
    (F6). Do not scope the caveat to "cross-process" only.
  * Correct the `open_engine_readonly` rustdoc (`src/db/store.rs:197`+): the claim
    that "every connection … including ones opened later from its connection
    pool … becomes a genuine engine-enforced read-only connection" holds only for
    a single owning guard; qualify it as best-effort whenever the same file is
    independently guarded more than once (same- or cross-process) and
    cross-reference the F6 limitation.
  * Qualify the `open_engine_readonly` startup log ("filesystem lock active") so
    it does not imply an unconditional guarantee.
  * Repository-wide sweep: grep for other overstated read-only claims and correct
    them too — at minimum the `open_sqlite_readonly` rustdoc (`src/db/store.rs:138`,
    which correctly notes it is app-level only — verify it stays accurate), the
    `EngineReadonlyGuard` struct/field docs, and any read-only guarantee wording
    in the serve entry point (`main.rs` `open_serve_databases`). Do not leave a
    corrected predicate beside an uncorrected caller doc.
  * Investigate-first: enumerate callers of `is_engine_enforced_readonly()`
    (`impact_analysis`/grep). Add a separate app-level `is_read_only()`
    (`access_mode == AccessMode::ReadOnly`) predicate **only if** a caller
    genuinely needs to distinguish app-level read-only from engine-enforced;
    otherwise record that none does and add nothing.
* Files: `src/db/store.rs`, plus the serve entry point (e.g. `src/main.rs`) only
  if the sweep finds overstated read-only wording there (≤2 files, code domain).
* Functions: `is_engine_enforced_readonly` (doc only) and rustdoc/log text on
  `open_engine_readonly`; optionally one new small `is_read_only` accessor.
* Tests (colocated `#[cfg(test)]`, characterization — pin the honest contract):
  * `open_engine_readonly` store reports `is_engine_enforced_readonly() == true`.
  * `open_sqlite_readonly` store reports `is_engine_enforced_readonly() == false`
    (locks in that the predicate was NOT overloaded).
  * app-level `AccessMode::ReadOnly` refuses `mutate` regardless of guard.
  * the existing single-process byte-identical read-cycle behavior is unchanged
    (reuse `src/db/store.rs:823`+; do not assert any cross-process closure).
* Execution posture: characterization-first (pin current honest invariants, then
  correct the doc/log text so the code and its contract agree).
* Atomic milestone: `cargo test` green; a grep-backed call-site list confirms no
  caller depended on a meaning that changed (none did — the meaning is unchanged).

### Unit A2 — Correct the read-only serve guarantee documentation (docs)

* Changes: rewrite the "Read-only serve hardening" section of
  `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`.
  Replace the unconditional "every connection — including later pool refills — is
  genuinely denied write access at the OS/filesystem level" claim with the honest
  statement: app-level `AccessMode` is authoritative; the filesystem read-only
  attribute is fully robust for single-process serving and best-effort
  defense-in-depth under concurrent multi-process read (F6). Record that
  cross-process refcounting is intentionally not implemented (disproportionate
  once the external-path feature was rejected) and link the spike, the
  deliberation, and the deferred stash items `F1CE20EC` (true fix / Option C) and
  `5905CDEE` (symlink TOCTOU).
* Files: the one design doc.
* Execution posture: documentation edit; verify links resolve.
* Atomic milestone: the section no longer claims unconditional OS-level write
  denial under concurrency and cites the spike + deliberation.

## Dependency Graph

* A2 depends on A1 (the design doc must match the corrected in-code rustdoc/log
  wording).
* No cycles.

## Decisions and Rationale

* **Do not overload `is_engine_enforced_readonly()`.** `open_sqlite_readonly`
  intentionally yields `AccessMode::ReadOnly` with no guard; redefining the
  predicate would falsely advertise engine enforcement — the opposite of the
  honesty goal (adversarial-review consensus, HIGH confidence).
* **No in-process F6 fix; document it instead.** Review established that the
  harmful restore is performed by a peer process; no single-process change can
  close the window without ownership/liveness coordination. Documenting F6 as a
  known best-effort limitation is the proportionate, honest response. The true
  fix is deferred (stash `F1CE20EC`).
* **No cross-process refcount.** Rejected as disproportionate once the
  external-path feature is rejected; it adds durable state with crash-recovery
  and TOCTOU failure modes for a scenario that stays incidental.
* **Keep the guard unchanged.** It is correct and valuable for single-process
  serving (existing byte-identical read-cycle tests). This unit targets the
  *advertised contract*, not the mechanism.

## Risks and Caveats

* Risk: correcting only some contract surfaces leaves others overstated.
  Mitigation: Unit A1 explicitly enumerates all surfaces (predicate rustdoc,
  `open_engine_readonly` rustdoc, startup log) and A2 covers the design doc.
* Risk: the honesty-only scope is seen as thin. Mitigation: it fixes a real
  overstated-guarantee defect that could mislead future security reasoning; the
  genuine mechanism fixes (symlink TOCTOU, cross-process coordination) are
  captured as traceable follow-ups (`5905CDEE`, `F1CE20EC`) rather than rushed
  into this unit.
* Risk: an `is_read_only()` accessor is added speculatively. Mitigation:
  investigate-first — add it only for a demonstrated caller need (Principle VI).

## Runtime Verification and Closure

* Runtime surface: `serve`/`status` read-only behavior (library-level `DataStore`
  contract). No CLI flag, schema, or guard runtime-behavior change.
* Verify before absorbed: `cargo test` for the new characterization + existing
  read-only tests; confirm the single-process read-only serve cycle still leaves
  the db byte-identical.
* Closure artifact: note in the shipment that this makes the read-only guarantee
  honest (corrects overstated rustdocs, startup log, and the design doc) and
  documents PR90 F2/F6 as a known best-effort limitation under concurrent
  multi-process read; it does NOT close the F6 window (deferred: `F1CE20EC`) and
  does not address the symlink TOCTOU (deferred: `5905CDEE`). No monitoring or
  rollback needed (no runtime rollout, no data migration).

## Plan Hardening Signals (REQUIRED)

* Public API, schema, or contract change: **present** — public rustdoc contract
  wording on `is_engine_enforced_readonly` / `open_engine_readonly` is corrected
  (behavior unchanged); optional additive `is_read_only()` accessor.
* Security, auth, permission, or compliance-sensitive behavior: **present** —
  touches the read-only enforcement guarantee documentation (Principle III
  adjacent); no behavior change.
* Migration, backfill, destructive data/config action, or irreversible step: absent.
* External integration, operator checkpoint, or external dependency: absent.
* High runtime, rollout, or rollback risk: absent — documentation/contract-wording
  change, no rollout.

Requires plan hardening: yes

## Plan Hardening

Hardening was required because the plan touches the read-only enforcement
guarantee documentation (security/permission-sensitive, Principle III adjacent)
and public rustdoc contract text.

### Risk triggers and protected invariants

* Trigger: corrections to the advertised read-only contract (rustdocs, log,
  design doc).
* Invariant to preserve: guard runtime behavior is UNCHANGED — no edit to
  `EngineReadonlyGuard::lock`/`Drop`; exact-permission capture/restore and
  non-empty-sidecar preservation stay intact.
* Invariant to preserve: `is_engine_enforced_readonly()` keeps meaning
  `guard.is_some()`; it is NOT overloaded to `access_mode == ReadOnly`.
* Invariant to preserve: app-level `AccessMode::ReadOnly` continues to refuse
  `mutate`; single-process byte-identical read-cycle behavior is unchanged (the
  plan does not assert any cross-process closure).
* Invariant to preserve: Principle III/IV containment is untouched — no
  external-path capability, `validate_path` unaltered.

### Learnings and instructions consulted

* `docs/compound/git-pull-blocked-by-sqlite-wal-lock.md` — `.db-wal`/`.db-shm`
  are ephemeral; cross-process file/attribute coordination is fragile (reinforces
  the "no durable refcount" decision).
* `.github/instructions/constitution.instructions.md` — Principles III/IV, V, VI.
* `.github/instructions/rust.instructions.md` / `technology-rust` — error
  handling and no-`unwrap` in library code for the touched tests.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction: correct the overstated read-only contract text (rustdocs on
  `is_engine_enforced_readonly` and `open_engine_readonly`, the "filesystem lock
  active" startup log) and, only on demonstrated caller need, add an additive
  `is_read_only()` accessor.
  * targets: `src/db/store.rs` (rustdoc/log text; optional accessor; colocated
    characterization tests).
  * change_kind: local edit (contract-wording correction; no guard behavior
    change).
  * rollback: revert the single-file change; no data or config touched.
  * approval_required: no (non-destructive, `ActionRisk: moderate`).
  * ActionRisk: moderate — contract-wording change on a security-relevant
    method, but no destructive or containment impact.
  * ActionResult: planned.
* ProposedAction: rewrite the trust-boundary doc guarantee section.
  * targets: `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`.
  * change_kind: docs edit.
  * rollback: revert the doc change.
  * approval_required: no. ActionRisk: low. ActionResult: planned.

### Added verification, closure, and rollback detail

* Verification: run the new colocated characterization tests (engine-readonly
  reports `true`; `open_sqlite_readonly` reports `false`; `AccessMode::ReadOnly`
  refuses `mutate`) plus the existing `open_engine_readonly_*` suite; confirm the
  single-process byte-identical read cycle is unchanged. Do not assert any
  cross-process closure.
* Call-site review is part of A1 — enumerate callers of
  `is_engine_enforced_readonly()` and confirm none depend on a meaning that
  changed (the meaning is unchanged); decide whether an additive `is_read_only()`
  is warranted.
* Rollback: single-commit revert per unit; no runtime rollout, migration, or
  monitoring required.
* Deferred (out of this unit, tracked): the true cross-process F6 fix / Option C
  (`F1CE20EC`) and the symlink-swap TOCTOU guard hardening (`5905CDEE`).

## Plan Review

Gate decision: **PASS** (after one remediation cycle). Reviewed by four
independent cross-model reviewers plus a post-remediation re-review, satisfying
the adversarial-review requirement (>= 3 independent reviewers, cross-model
diversity: anthropic / google-alt-provider / openai / xai).

### Round 1 findings (resolved)

* **P1 (HIGH, all reviewers)** — original draft redefined
  `is_engine_enforced_readonly()` as `access_mode == ReadOnly`; that is *more*
  deceptive because `open_sqlite_readonly` sets `AccessMode::ReadOnly` with guard
  `None`. **Resolved:** predicate meaning kept as `guard.is_some()`; overload
  rejected; optional additive `is_read_only()` only on demonstrated need.
* **P1 (HIGH)** — test spec could not produce a RED phase and used the benign
  guard ordering. **Resolved:** replaced with characterization tests pinning the
  honest contract; no cross-process closure asserted.
* **P1 (HIGH)** — correcting only the design doc left other surfaces overstated.
  **Resolved:** repository-wide surface sweep (predicate + `open_engine_readonly`
  rustdocs, `open_sqlite_readonly` rustdoc, guard docs, startup log, `main.rs`
  serve entry point) + design doc; F6 framed as documented residual, not closed.
* **P1 (HIGH, security/gemini)** — pre-existing symlink-swap TOCTOU in the guard.
  **Resolved by deferral:** distinct security-mechanism change captured as stash
  `5905CDEE` (not rushed into this unit).
* **PASS (HIGH consensus)** — the reject-external-path decision is
  constitutionally and security correct.

### Round 2 (post-remediation re-review)

* Constitution reviewer (claude-opus-4.8): **PASS**. Scope reviewer
  (gemini-3.1-pro): **PASS** — "Plan A is now right-sized."
* Correctness reviewer (gpt-5.6-sol): refinements — the contract sweep must be
  repository-wide (add `open_sqlite_readonly` rustdoc + `main.rs`), and the F6
  qualification must not be scoped to "single-process" (two independent
  same-process guards also reproduce F6). **Resolved** in this revision.
* Residual: P3 advisories only (enforcement-tier wording accuracy, differential
  glob-test coverage) — non-blocking, folded into the plan text.

No unresolved HIGH/MEDIUM P0/P1 findings remain. Cleared for harvest and shipment.
