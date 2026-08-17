---
title: "Serve auto-discovery follow-ups (PR90 deferrals)"
description: "Whether the ingestible-content short-circuit and served-alias evaluation form one coherent covering feature"
topic: "Serve auto-discovery follow-ups from PR90 review"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
stash_ids:
  - "B88E37BF"
  - "5868A7C5"
linked_artifacts:
  - "docs/exec-plans/2026-08-16-serve-auto-discovery-followups-plan.md"
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
  document count. Optimization: short-circuit and return `true` as soon as the
  first file passes both the format check and the include/exclude filter.
* `5868A7C5` — evaluate whether served database aliases need explicit
  canonicalization or reporting beyond the current dedup union in
  `discover_served_databases`.

The question: do these belong together as one covering feature and one release
unit, or should they be split?

## Research Findings

* Both tasks touch the same file and the same subsystem (serve auto-discovery),
  and both are PR90 deferrals — natural peers that would live in one pull request.
* `B88E37BF` is a concrete, testable performance change with a clear correctness
  invariant: the short-circuit must preserve the existing "fail closed on any
  walk error" semantics (`serve_discovery.rs:333`) and apply the same
  include/exclude glob semantics per file that `filter_files` applies in batch
  (empty include = include all; exclude wins). Blast radius is limited to the
  boolean classifier.
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

* Pros: one coherent low-risk release unit; shared file context; single review
  and PR; reduces open-item count.
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
| Risk | Low | Low |
| Open-item reduction | Better | Worse |

## Decision

Adopt **Option A**: one covering feature "Serve auto-discovery follow-ups (PR90
deferrals)" with two width-isolated tasks — the `source_has_ingestible_content`
short-circuit (code + regression test) and the served-alias canonicalization
evaluation (investigate-then-decide). Review confirms one coherent low-risk
release unit, satisfying the grouping guidance for Group B. No cross-task
dependency: the two tasks are independent and may execute in either order.

## Rejected Alternatives

* **Option B** — splitting two small same-file follow-ups into separate shipments
  adds overhead without reducing risk.

## Unresolved Questions

* The alias-evaluation task may conclude "no code change needed, current dedup is
  sufficient." That is an acceptable terminal outcome recorded in the task, not a
  reason to withhold the shipment.

## Risks and Mitigations

* Risk: the short-circuit subtly changes which sources classify as ingestible
  (and thus read-only vs generation posture). Mitigation: regression tests
  asserting an excluded-only tree returns `false` and a large tree returns `true`
  after inspecting only the first eligible file; preserve the fail-closed walk-
  error semantics exactly.
