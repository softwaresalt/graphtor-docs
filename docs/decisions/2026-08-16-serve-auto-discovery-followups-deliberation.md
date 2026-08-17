---
title: "Serve auto-discovery follow-ups (PR90 deferrals)"
description: "Whether the ingestible-content streaming memory-reduction refactor and served-alias evaluation form one coherent covering feature"
topic: "Serve auto-discovery follow-ups from PR90 review"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
stash_ids:
  - "B88E37BF"
  - "5868A7C5"
linked_artifacts:
  - "docs/exec-plans/2026-08-16-serve-auto-discovery-followups-decided-plan.md"
tags:
  - serve-discovery
  - performance
  - follow-up
---

## Problem Frame

Two low-priority tasks were deferred from the PR90 Copilot review, both landing
in `src/workspace/serve_discovery.rs`:

* `B88E37BF` — `source_has_ingestible_content` (line 333) eagerly walks the whole
  source tree via `WalkDir`, collects every format-matching relative path, then
  calls `graphtor_core::acquire::filter_files` once and checks non-empty. The
  caller (serve auto-discovery `ServeMode` classification) only needs a boolean,
  so serve startup pays a full traversal and retains memory proportional to
  document count. Optimization: keep the full error-observing walk but stream a
  boolean instead of accumulating the `Vec` of matching paths, removing the
  `O(document-count)` memory without weakening the fail-closed contract.
* `5868A7C5` — evaluate whether served database aliases need explicit
  canonicalization or reporting beyond the current dedup union in
  `discover_served_databases`.

The question: do these belong together as one covering feature and one release
unit, or should they be split?

## Research Findings

* Both tasks touch the same file and the same subsystem (serve auto-discovery),
  and both are PR90 deferrals — natural peers that would live in one pull request.
* `B88E37BF` is a concrete, testable performance change with a clear correctness
  invariant: the streaming refactor must preserve the existing "fail closed on any
  walk error" semantics (`serve_discovery.rs:333`), retain the full traversal (no
  short-circuit), and apply the same include/exclude glob semantics through the
  shared compiled matcher that `filter_files` uses (empty include = include all;
  exclude wins) rather than recompiling globs per file. Because the shared matcher
  must be exposed as an additive public API and `filter_files` is refactored to
  consume it, the blast radius extends beyond the boolean classifier to library
  filtering (all `filter_files` acquisition callers); this is contained by keeping
  the change additive and requiring `filter_files` result and warning behavior to
  stay unchanged (a differential test against the pre-refactor implementation).
* `5868A7C5` is investigative: `discover_served_databases` already canonicalizes
  every entry through `validate_path` and dedups via a `BTreeSet` union
  (`serve_discovery.rs:123`, tests `served_set_is_canonical_deduped_union...` and
  the `shared-alias`/`outside-alias` cases). The likely finding is that
  canonical-path dedup is already sufficient and the only possible gap is
  diagnostic reporting (surfacing a label/alias in `status` output). It is bounded
  by an explicit evaluate-then-decide acceptance criterion.
* Neither task touches containment, the read-only guard, or the write path, so
  they are independent of Group A.

## Options Evaluated

### Option A: one covering feature, two tasks

Group both under "Serve auto-discovery follow-ups (PR90 deferrals)."

* Pros: one coherent release unit; shared file context; single review and PR;
  reduces open-item count. (Low scheduling priority, but a moderate,
  security-sensitive ActionRisk — see Decision and Risks.)
* Cons: mixes a code-change task with an evaluation task — acceptable because the
  evaluation task is width-isolated and produces its own verifiable outcome.
* Effort: low. Fit: strong.

### Option B: two separate features/shipments

* Pros: strict single-responsibility per shipment.
* Cons: redundant overhead for two small, same-file, same-subsystem follow-ups.
* Effort: low but wasteful. Fit: weak.

## Trade-off Comparison

| Criterion | Option A | Option B |
|---|---|---|
| Coherence | High (same file/subsystem) | Low (artificial split) |
| Overhead | One PR | Two PRs |
| Grouping/overhead risk | Low | Low |
| Open-item reduction | Better | Worse |

The "Grouping/overhead risk" row compares only the grouping decision; the
intrinsic ActionRisk of Unit B1 is **moderate and security-sensitive** (see
Decision and Risks), not low.

## Decision

Adopt **Option A**: one covering feature "Serve auto-discovery follow-ups (PR90
deferrals)" with two width-isolated tasks — the `source_has_ingestible_content`
streaming memory-reduction refactor (code + regression test) and the served-alias
canonicalization evaluation (investigate-then-decide). The refactor keeps the full
error-observing `WalkDir` walk and only removes the `O(document-count)` `Vec` via a
streaming boolean; a traversal short-circuit that returns `true` on the first
eligible file is rejected because an unreadable subtree encountered later in walk
order would be skipped, flipping a partially-unreadable source from `false`
(read-only) to `true` (the read-**write** `Generation` posture) — a safety-degrading
escalation. Review confirms one coherent release unit, satisfying the grouping
guidance for Group B. No cross-task dependency: the two tasks are independent and
may execute in either order.

**Risk classification:** Group B (and specifically Unit B1) is **moderate and
security-sensitive**, not low-risk. Scheduling priority remains low — these are
small, deferred, same-file follow-ups — but `source_has_ingestible_content` gates
read-only vs read-**write** `Generation` posture, so a misclassification can
promote a partially-unreadable source from read-only to read-write. That, plus
the fact that reusing the shared matcher crosses the binary→library crate
boundary (requiring an additive `graphtor_core::acquire` public API), keeps the
ActionRisk moderate/security-sensitive even though the change is
behavior-preserving. B2 (alias evaluation) remains low-risk.

## Rejected Alternatives

* **Option B** — splitting two small same-file follow-ups into separate shipments
  adds overhead without reducing risk.

## Unresolved Questions

* The alias-evaluation task may conclude "no code change needed, current dedup is
  sufficient." That is an acceptable terminal outcome recorded in the task, not a
  reason to withhold the shipment.

## Risks and Mitigations

* Risk classification: Group B is **moderate, security-sensitive** (not low-risk)
  — Unit B1 gates read-only vs read-**write** `Generation` posture; B2 is
  low-risk.
* Risk: the streaming refactor subtly changes which sources classify as ingestible
  (and thus read-only vs generation posture). Mitigation: preserve the reviewed
  full-walk invariant — the entire `WalkDir` is always traversed and any walk error
  returns `false` (fail closed) — and never early-return on the first eligible file.
  Add the key regression case: an eligible file encountered BEFORE a later
  unreadable subtree still returns `false`. Regression tests also assert an
  excluded-only tree returns `false` and a fully readable eligible tree returns
  `true`, driven through a deterministic ordering seam because `WalkDir` iteration
  order is unspecified.
