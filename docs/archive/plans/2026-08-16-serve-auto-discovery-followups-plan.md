---
title: "Serve auto-discovery follow-ups (PR90 deferrals)"
description: "Reduce ingestible-content classifier memory (behavior-preserving) and evaluate served-alias canonicalization"
date: 2026-08-16
source: "docs/decisions/2026-08-16-serve-auto-discovery-followups-deliberation.md"
stash_ids:
  - "B88E37BF"
  - "5868A7C5"
tags:
  - serve-discovery
  - performance
  - follow-up
---

## Problem Frame

Two PR90 deferrals in `src/workspace/serve_discovery.rs`:

* `source_has_ingestible_content` (`serve_discovery.rs:333`) eagerly walks the
  entire source tree, collects every format-matching relative path, then calls
  `graphtor_core::acquire::filter_files` once and checks non-empty — but the
  caller (`ServeMode` classification) only needs a boolean. Serve startup pays a
  full traversal and holds memory proportional to document count.
* Served database aliases: `discover_served_databases` (`serve_discovery.rs:91`)
  already canonicalizes and dedups entries via a `BTreeSet` union. Whether any
  further explicit canonicalization or reporting is needed is unevaluated.

## Requirements Trace

| Requirement | Implementation action |
|---|---|
| Eliminate O(document-count) memory in the boolean classifier | Unit B1: stream a boolean over the walk; drop the `Vec` of matching paths |
| Preserve exactly which files count as ingestible | Unit B1: test each format-matching path against the same compiled matcher `filter_files` uses (a shared reusable predicate exposed as an **additive public API** in the `graphtor_core` library crate, because the classifier lives in the `graphtor-docs` binary crate — see Plan Hardening Signals), so glob sets compile once and the aggregate "all files excluded" warning semantics are preserved — never call `filter_files` per file; no duplicated glob logic |
| Preserve fail-closed-on-walk-error semantics EXACTLY | Unit B1: keep the full walk; still return `false` on any `WalkDir` error |
| Decide whether aliases need more than dedup | Unit B2: evaluate, then implement diagnostic reporting or document no-op |

## Implementation Units

### Unit B1 — Reduce `source_has_ingestible_content` memory without changing semantics (code)

* Context (adversarial-review correction): a *traversal* short-circuit that
  returns `true` on the first eligible file is incompatible with the current
  fail-closed contract — an unreadable subtree encountered *after* the first
  eligible file (in walk order) would be skipped, flipping a partially-unreadable
  source from `false` (read-only, fail-closed) to `true`, which `classify_serve_
  postures` treats as eligible for the read-**write** `Generation` posture. That
  is a safety-degrading posture escalation and is rejected. The full,
  error-observing walk MUST be preserved.
* Changes: keep iterating the entire `WalkDir` and keep returning `false` on any
  walk error (fail closed, unchanged). Replace the `relative_candidates: Vec`
  accumulation + single batch `filter_files` call with a streaming boolean that
  preserves the batch semantics AND the aggregate warning behavior. To avoid
  divergence, prefer building the include/exclude matcher once (the same compiled
  matcher `filter_files` uses) and testing each format-matching relative path
  against it, ORing into a `found` flag while also tracking a
  `saw_format_candidate` flag — rather than calling `filter_files` per file, which
  would recompile the glob sets for every entry and emit a per-file "all files
  excluded" warning even when another file makes the source ingestible. To
  reproduce the batch warning semantics EXACTLY, emit the single aggregate "all
  files excluded" warning only when `saw_format_candidate && !found` (candidates
  existed but every one was excluded) and emit nothing when no format-matching
  candidate was seen — matching what `filter_files(&all_candidates, ...)` does
  today. If a shared reusable predicate is not exposed by
  `graphtor_core::acquire`, extract or expose one so the classifier and
  `filter_files` share a single matcher; do not fork the glob logic. Return
  `found` only when the walk completed without error. This removes memory
  proportional to document count; the full traversal is retained deliberately to
  preserve fail-closed semantics.
* Files: `src/workspace/serve_discovery.rs` (binary crate `graphtor-docs`), plus
  a small reusable matcher/predicate exposed as an **additive public API** in
  `graphtor_core::acquire` (library crate) — required because the binary cannot
  reach the currently private `build_glob_set`/`GlobSet` predicate — and a
  refactor of `filter_files` to consume that same predicate (single source of
  truth). ≤2 source files, code domain.
* Functions: `source_has_ingestible_content` (binary), plus a new public matcher
  type/constructor in `graphtor_core::acquire` (for example `FileFilter::new` /
  `FileFilter::is_match`) reused by a refactored `filter_files` (no duplicated
  glob logic).
* Tests (colocated `#[cfg(test)]`, characterization — prove identical
  classification; use a deterministic ordering seam):
  * excluded-only tree returns `false`.
  * a tree where an eligible file is encountered BEFORE an unreadable subtree
    returns `false` (fail-closed preserved even when an eligible file exists —
    the key regression guard for the rejected short-circuit). Because `WalkDir`
    filesystem iteration order is unspecified, drive this test through an injected
    ordered walk-result seam (or an equivalent portable deterministic fixture) so
    an accidental early-return regression cannot pass by luck of ordering.
  * include/exclude precedence (exclude wins; empty include includes all) matches
    the previous batch result.
  * differential check: for representative trees (nested relative paths,
    multi-segment include globs, union patterns, empty include), the streaming
    result equals the old `filter_files(&all_candidates, ...)`-then-non-empty
    result. Warning parity: exactly one aggregate "all files excluded" warning for
    an excluded-only tree (candidates existed, none passed), and zero warnings for
    the zero-candidate, ingestible, and walk-error cases — no spurious per-file
    warning.
  * library-crate unit tests (colocated in `graphtor_core::acquire`) for the new
    public matcher API in isolation — include/exclude precedence, empty include =
    all, union globs — so the exposed predicate is covered independently of the
    classifier and `filter_files` is proven to still consume it. The
    all-candidates-excluded case is exercised by iterating `is_match`; aggregation
    (the "all excluded" condition) stays caller-owned and is covered by the
    classifier's warning-parity tests, not by a public aggregate method.
* Execution posture: characterization-first — pin current behavior with tests,
  then refactor to the streaming boolean.
* Atomic milestone: `cargo test` passes; classification results are unchanged.

### Unit B2 — Evaluate served-alias canonicalization (investigation)

* Changes: evaluate whether served database aliases need explicit canonicalization
  or reporting beyond the current canonical-path dedup union. Confirm behavior
  against the existing tests (`served_set_is_canonical_deduped_union...`,
  `shared-alias`, `outside-alias`). Outcome is one of: (a) document that the
  current canonical-path dedup is sufficient (no code change), or (b) add a
  small, bounded diagnostic improvement (for example, surfacing the configured
  label/alias in `status` output) if a concrete gap is found.
* Files: at most `src/workspace/serve_discovery.rs` and/or a status-output path
  if (b); otherwise docs/decision note only.
* Execution posture: investigate-first — decide (a) or (b) before editing.
* Atomic milestone: a recorded conclusion; if (b), a passing test for the added
  reporting; if (a), a short rationale captured in the task and no code change.

## Dependency Graph

* B1 and B2 are independent (no ordering dependency). Both touch the same file, so
  Ship executes them sequentially on one branch, but neither blocks the other.
* No cycles.

## Decisions and Rationale

* **Characterization-first for B1** because the change must be behavior-preserving
  for a classifier that drives read-only vs generation posture — the tests must
  prove identical classification before and after.
* **Investigate-first for B2** because the existing dedup may already be
  sufficient; do not add code speculatively (Principle VI, single responsibility).

## Risks and Caveats

* Risk: the refactor changes which sources classify as ingestible and thus flips
  a source's read-only vs generation posture. Mitigation: the full error-observing
  walk is retained (no traversal short-circuit), and characterization tests assert
  identical results — including the fail-closed walk-error-with-eligible-file case
  and include/exclude precedence — plus a differential check against the previous
  batch `filter_files` result.
* Risk: B2 scope creep into alias normalization that is not needed. Mitigation:
  the acceptance criterion allows a documented no-op outcome; only implement (b)
  for a concrete, tested gap, and never print absolute internal/external paths in
  any added diagnostic (Principle III — no internal-path leakage).

## Runtime Verification and Closure

* Runtime surface: serve auto-discovery `ServeMode` classification and `status`
  output. No schema or CLI-flag change (unless B2 adds an opt-in diagnostic line).
* Verify before absorbed: `cargo test` for the new characterization + differential
  tests; confirm memory no longer scales with document count (no per-file `Vec`
  accumulation) while classification results and fail-closed behavior are
  unchanged. The full error-observing traversal is retained by design.
* Closure artifact: note in the shipment that this addresses the PR90 wave-7 perf
  finding (B88E37BF, memory reduction; traversal retained for fail-closed safety)
  and the alias evaluation (5868A7C5). Because the classifier chooses read-only
  vs read-**write** `Generation` posture, a bounded manual post-deploy
  observation window (below) is required even though there is no runtime rollout
  or data migration.

### Post-Deploy Observation Window (manual checklist)

* Owner: the developer merging the shipment (single-developer repo; no on-call
  rotation).
* Signals to watch: the resolved `ServeMode` per source (read-only vs
  `Generation`) and the aggregate "all files excluded" warning. Because startup
  logging may report aggregate counts rather than each source's classifier
  result, observe per-source by running `graphtor-docs serve`/`status` against
  each previously-served source in isolation (or add a temporary per-source
  classification log line) so the posture of every source is individually
  visible. Record baselines using source labels or relative identifiers, never
  absolute internal/external paths (Principle III).
* Expected baseline: for every previously-served source, the post-change
  `ServeMode` classification and warning output MUST equal the pre-change
  baseline captured on the same fixtures — read-only sources stay read-only,
  `Generation` sources stay `Generation`, excluded-only sources emit exactly one
  aggregate warning, and ingestible / zero-candidate / walk-error cases emit
  none.
* Window: observe the next 3 local `serve` startups (or 24 hours of local use,
  whichever comes first) after merge, comparing per-source classification and
  warning output against the baseline.
* Rollback trigger: ANY per-source posture change in EITHER direction versus
  baseline — read-only → `Generation` (the security-sensitive escalation) or
  `Generation` → read-only — any change to the set of `Generation` sources, or
  any spurious, missing, or differently-failing aggregate warning.
* Revert procedure: `git revert` the Unit B1 commit(s) — if B1 was decomposed
  into the library-API and binary-streaming subtasks, revert both in reverse
  dependency order (binary streaming first, then the library API / `filter_files`
  refactor) so the shared refactor is not left active — (behavior-preserving; no
  data, config, or schema state to unwind), rebuild, re-run the per-source
  comparison to confirm the baseline classification is restored, and reopen
  B88E37BF with the diverging fixture attached.
* Window-close outcome: at window close, record the outcome — healthy, degraded,
  or rolled-back — in the shipment closure artifact as releasability evidence.

## Plan Hardening Signals (REQUIRED)

* Public API, schema, or contract change: **present (additive)** — reusing the
  same compiled include/exclude matcher that `filter_files` uses crosses the
  binary→library crate boundary. `source_has_ingestible_content` lives in the
  binary crate `graphtor-docs` (`src/main.rs` → `mod workspace`), while the
  matcher and `filter_files` live in the library crate `graphtor_core`
  (`src/lib.rs` → `pub mod acquire`), where `build_glob_set` and the `GlobSet`
  predicate are currently private. The binary cannot reach them, so Unit B1 MUST
  add a small **additive public API** to `graphtor_core::acquire` — the minimal
  surface being a `pub struct FileFilter` with `pub fn new(include, exclude) ->
  Result<Self, GraphtorError>` and `pub fn is_match(&self, path: &Path) -> bool`
  — and refactor `filter_files` to consume it so there is a single source of
  truth. The caller computes the aggregate `saw_format_candidate && !found`
  condition itself from `is_match` results; do not add an aggregate-empty method
  to the public surface unless a concrete need appears (Principle VI — keep the
  surface minimal). This is additive-only — SemVer-minor, no breaking change to
  the existing `filter_files` signature. In-scope
  compatibility/documentation/testing work: `///` rustdoc on the new public type
  and methods (a usage example is optional for this small predicate),
  library-crate unit tests for the matcher in isolation (written first and
  observed to fail — red — before implementing `FileFilter`, per Principle II),
  and the binary-crate differential test proving the classifier's per-path result
  equals the old `filter_files(&all_candidates, ...)`-then-non-empty result. A
  per-file `filter_files` call or a forked/duplicated glob predicate is rejected
  (glob recompilation per entry and divergent warning semantics).
* Security, auth, permission, or compliance-sensitive behavior: **present** — the
  classifier gates read-only vs read-**write** `Generation` serve posture. The
  *consequence severity* if the invariant breaks is HIGH (P1): any refactor that
  drops the fail-closed full-walk error observation could silently escalate a
  partially-unreadable source from read-only to read-write. The *ActionRisk*
  (mitigated blast radius) is moderate — the change is additive-only,
  behavior-preserving, and revertible via `git revert` (one B1 commit, or both
  subtask commits in reverse order if decomposed), and the fail-closed contract
  and identical classification are covered by characterization tests plus the
  eligible-file-before-later-error regression case. High consequence with
  moderate residual risk after mitigation is consistent, not contradictory.
* Migration, backfill, destructive data/config action, or irreversible step: absent.
* External integration, operator checkpoint, or external dependency: absent.
* High runtime, rollout, or rollback risk: low — behavior-preserving refactor
  plus an optional diagnostic line, with no rollout or migration and a
  `git revert` rollback path (one B1 commit, or both subtask commits if
  decomposed). A bounded **manual post-deploy observation window**
  is nonetheless required (see "Runtime Verification and Closure") because the
  classifier gates read-only vs read-**write** `Generation` posture.

Requires plan hardening: yes

## Plan Hardening

Hardening was required because `source_has_ingestible_content` gates the read-only
vs read-**write** `Generation` serve posture. A refactor that weakened the
fail-closed full-walk error observation would be a security-relevant posture
escalation (Principle III adjacent), so the memory-only optimization must be
hardened against silently dropping that contract.

### Risk triggers and protected invariants

* Trigger: refactoring the classifier that drives read-only vs read-write posture.
* Invariant to preserve: the entire `WalkDir` is always traversed; any walk error
  returns `false` (fail closed). No traversal short-circuit or first-eligible
  early return.
* Invariant to preserve: identical classification — the streaming boolean equals
  the previous `filter_files(&all_candidates, ...)`-then-non-empty result for every
  representative tree (include/exclude precedence, empty include = all, union
  globs, nested relative paths).
* Invariant to preserve: aggregate warning semantics — the shared compiled matcher
  is built once; no per-file `filter_files` recompilation. Emit the single
  aggregate "all files excluded" warning only when format-matching candidates
  existed but none passed (`saw_format_candidate && !found`), matching the old
  batch call — no spurious per-file warning and no warning when no candidate was
  seen.

### Learnings and instructions consulted

* `docs/compound/best-practices/reparse-point-fail-closed-containment-2026-07-16.md`
  — fail-closed-on-`WalkDir`-error is a deliberate short-circuit, not error
  propagation; preserve it exactly.
* `.github/instructions/constitution.instructions.md` — Principles II, III, VI.
* `.github/instructions/rust.instructions.md` / `technology-rust` — no `unwrap` in
  library code for the touched tests; propagate via `Result`.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction: replace the `Vec` accumulation + single batch `filter_files`
  call with a streaming boolean over the full walk, testing each format-matching
  relative path against the shared compiled matcher.
  * targets: `src/workspace/serve_discovery.rs` (binary crate); an **additive
    public matcher API** in `graphtor_core::acquire` (library crate) plus a
    `filter_files` refactor to consume it; colocated characterization tests and a
    library-crate unit test for the new public API.
  * change_kind: local edit plus an additive public API (behavior-preserving
    refactor of a security-relevant classifier; SemVer-minor library surface,
    no breaking change).
  * rollback: `git revert` the B1 change on its one branch (both subtask commits
    in reverse dependency order if decomposed); no data or config touched.
  * approval_required: no (non-destructive).
  * ActionRisk: moderate — behavior must be provably identical; a regression
    would escalate serve posture.
  * ActionResult: planned.

### Added verification, closure, and rollback detail

* Verification: run the colocated characterization + differential tests, including
  the eligible-file-before-later-unreadable-subtree case (driven by the injected
  ordered walk-result seam) and the excluded-only-returns-`false` case; confirm the
  streaming result equals the old batch result and no spurious per-file warning is
  emitted. Confirm memory no longer scales with document count (no per-file `Vec`).
* Rollback: `git revert` the B1 commit(s) — one commit, or both the library-API
  and binary-streaming subtask commits reverted in reverse dependency order if B1
  was decomposed; B2 reverts independently. No runtime rollout or migration.
  A bounded manual post-deploy observation window (see "Post-Deploy Observation
  Window" under Runtime Verification and Closure) with owner, baseline, rollback
  trigger, and revert procedure is required because the classifier gates
  read-only vs read-write posture.
* Residual risk: none beyond the covered invariants; B2 stays bounded to an
  evaluate-then-decide outcome and never prints absolute internal/external paths
  (Principle III).

## Plan Review

Gate decision: **PASS** (after one remediation cycle). Reviewed alongside Plan A
by the same cross-model reviewer set plus a post-remediation re-review.

### Round 1 finding (resolved)

* **P1 (HIGH, consensus)** — the proposed traversal short-circuit ("return `true`
  on the first eligible file") is incompatible with the fail-closed-on-any-walk-
  error contract: an unreadable subtree encountered after an early eligible file
  would be skipped, flipping a partially-unreadable source from `false`
  (read-only) to `true`, which `classify_serve_postures` treats as eligible for
  the read-**write** `Generation` posture — a safety-degrading escalation.
  **Resolved:** the full error-observing walk is retained; only the
  `O(document-count)` `Vec` is removed via a streaming boolean; a regression test
  (eligible file before an unreadable subtree ⇒ `false`) plus a differential test
  vs the old batch result are required.

### Round 2 (post-remediation re-review)

* Constitution (claude-opus-4.8) and Scope (gemini-3.1-pro): **PASS** — posture-
  escalation risk removed; memory-only fix is right-sized; B2 remains bounded.
* Correctness (gpt-5.6-sol): refinements — the ordering test needs an injected
  deterministic seam (WalkDir order is unspecified), and per-file `filter_files`
  would recompile globs and change aggregate warning behavior. **Resolved:** the
  plan now requires a shared reusable matcher (no per-file recompilation / no
  spurious warnings) and an injected ordered walk-result seam for the test.

### Round 3 (PR #96 second Copilot review — bounded remediation cycle 2)

Copilot's second review of the staging PR raised two plan-facing findings
(suppressed comments, no live threads):

* **Public API impact (was mislabelled "absent")** — reusing the shared compiled
  matcher crosses the binary→library crate boundary, so it requires an additive
  public API in `graphtor_core::acquire`, not an internal-only change.
  **Resolved:** Plan Hardening Signals now marks the change **present (additive)**,
  names the `graphtor-docs` binary vs `graphtor_core` library split, and adds the
  compatibility (SemVer-minor, `filter_files` refactor to a single source of
  truth), documentation (`///` rustdoc), and testing (library-crate unit test +
  binary-crate differential test) work. Requirements Trace, Unit B1
  Files/Functions/Tests, and the ProposedAction targets/change_kind agree.
* **Missing post-deploy observation** — the classifier chooses read-only vs
  read-**write** `Generation`, so a bounded observation window is warranted.
  **Resolved:** a manual "Post-Deploy Observation Window" (owner, signals,
  expected baseline, window, rollback trigger, revert procedure) is added under
  Runtime Verification and Closure; the "no monitoring/rollback needed" and
  rollout/rollback signals are corrected to reference it.

No unresolved HIGH/MEDIUM P0/P1 findings remain. Cleared for harvest and shipment.
