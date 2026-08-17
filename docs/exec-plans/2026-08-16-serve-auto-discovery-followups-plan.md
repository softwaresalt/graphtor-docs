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
| Preserve exactly which files count as ingestible | Unit B1: reuse `filter_files` per file (single-path slice) — no duplicated glob logic |
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
  against it, ORing into a `found` flag — rather than calling `filter_files`
  per file, which would recompile the glob sets for every entry and emit a
  per-file "all files excluded" warning even when another file makes the source
  ingestible. If a shared reusable predicate is not exposed by
  `graphtor_core::acquire`, extract or expose one so the classifier and
  `filter_files` share a single matcher; do not fork the glob logic. Return
  `found` only when the walk completed without error. This removes memory
  proportional to document count; the full traversal is retained deliberately to
  preserve fail-closed semantics.
* Files: `src/workspace/serve_discovery.rs`, plus a small reusable predicate in
  `graphtor_core::acquire` if one must be exposed to avoid forking glob logic
  (≤2 files, code domain).
* Functions: `source_has_ingestible_content`, plus at most one small shared
  matcher/predicate reused with `filter_files` (no duplicated glob logic).
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
    result, and no spurious per-file "all files excluded" warning is emitted when
    the source is ingestible.
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
  and the alias evaluation (5868A7C5); no monitoring/rollback needed.

## Plan Hardening Signals (REQUIRED)

* Public API, schema, or contract change: absent — internal classifier and
  optional diagnostic output only.
* Security, auth, permission, or compliance-sensitive behavior: absent — no
  containment or read-only-enforcement change; classification correctness is
  covered by characterization tests.
* Migration, backfill, destructive data/config action, or irreversible step: absent.
* External integration, operator checkpoint, or external dependency: absent.
* High runtime, rollout, or rollback risk: absent — behavior-preserving refactor
  plus an optional diagnostic line.

Requires plan hardening: no

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

No unresolved HIGH/MEDIUM P0/P1 findings remain. Cleared for harvest and shipment.
