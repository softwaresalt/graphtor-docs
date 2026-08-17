---
title: "Serve Auto-Discovery Follow-Ups — Pre-Merge Operational Closure"
description: "Releasability evidence, monitoring plan, and rollback procedure for shipment 048-S"
date: 2026-08-17
mode: "pre-merge"
shipment: "048-S"
feature: "055-F"
readiness: "READY"
---

## Summary of Change

Two PR90 deferrals in `src/workspace/serve_discovery.rs`, executed as shipment `048-S`:

* **`055.001-T`** (execution container, delegated to two subtasks):
  * **`055.001.001-ST`** — added an additive `graphtor_core::acquire::FileFilter` public API
    (`new`/`is_match`) and refactored `filter_files` to consume it as the single source of truth.
  * **`055.001.002-ST`** — refactored `source_has_ingestible_content` into an O(1)-memory
    streaming boolean (`stream_ingestible`) that reuses the shared `FileFilter` instead of
    accumulating a `Vec` of every candidate path and calling `filter_files` once as a single
    batch over that `Vec` (the pre-refactor approach). The full error-observing `WalkDir` walk
    is retained; any walk error still fails closed to `false`.
* **`055.002-T`** — investigate-first evaluation of served-database alias handling; concluded the
  existing canonical-path dedup is sufficient (documented no-op, no code change).

## Invariants to Preserve

* **Fail-closed on any walk error** — `source_has_ingestible_content` (and its resolved
  `ServeMode`) must return `false`/`ReadOnly` if the walk encounters ANY error, regardless of
  whether an eligible candidate was already observed earlier in the walk. This is the
  safety-critical invariant: a break here could promote a partially-unreadable source from
  `ReadOnly` to the read-**write** `Generation` posture.
* **No traversal short-circuit** — the entire `WalkDir` is always traversed; `true` is never
  returned before the walk completes without error.
* **Classification identical to before the refactor** — the set of sources that classify
  ingestible (and thus `Generation`-eligible) must not change.
* **Aggregate "all files excluded" warning parity** — exactly one warning, carrying the same
  `input_files` scalar count field name and message text `filter_files` itself uses, only when
  format-matching candidates existed but all were excluded.
* **No containment change** — path validation, workspace-root containment, and the served-database
  dedup/rejection semantics (`discover_served_databases`) are unchanged (055.002-T made no code
  change).

## Validator Evidence (from Runtime Verification)

* Report: `docs/closure/2026-08-17-serve-auto-discovery-followups-runtime-verification.md`
* Verdict: **PASS**
* Adapter: CLI/command, manual subprocess invocation against the real compiled binary
* Surfaces verified: `serve` startup posture resolution log (`resolved serve posture ...`) and the
  aggregate exclusion warning log, across three representative scenarios (ingestible, excluded-only,
  zero-candidate) plus a platform-independent unit-level seam test for the later-walk-error
  regression case (Windows cannot reliably simulate unreadable subtrees via ACLs; the existing
  `#[cfg(unix)]`-gated sibling test covers real-filesystem confirmation on Linux CI).
* No BLOCKED prerequisites.

## Pre-Deploy Audits

* No feature flags, migrations, or rollout gates apply — this is a same-process, behavior-preserving
  refactor plus one additive library API. No data or schema changes.
* No dependent services or cross-service boundaries are affected.
* Monitoring plan (below) is complete.

## Deployment / Rollout Path

Merge-only. `graphtor-docs` is a single-developer, locally-run MCP server binary — there is no
staged rollout, canary, or remote deployment pipeline. "Deployment" here means the next time the
developer runs `cargo build`/`serve` from the merged `main` branch.

## Post-Deploy Checks (first concrete observations)

1. Run `graphtor-docs serve` (or `status`) against each real, previously-served source in
   isolation and confirm the resolved `ServeMode` (`Generation` vs `ReadOnly`) and any aggregate
   exclusion warning match the pre-change baseline for that source.
2. Confirm no unexpected `Generation` promotions or demotions appear in the `resolved serve
   posture` log line's `generation_count`/`readonly_count` fields relative to the pre-merge
   baseline.

## Risky Action Record

| Action | Risk | Mitigation | Result |
|---|---|---|---|
| Refactor `source_has_ingestible_content` (gates read-only vs read-write `Generation` posture) | Moderate, security-sensitive (per deliberation risk classification) — consequence severity HIGH if fail-closed contract breaks, but mitigated to moderate residual risk | Characterization tests pinned pre-refactor behavior first; RED-FIRST tests for the new streaming abstraction including the eligible-then-error regression seam; full error-observing walk retained; `git revert`-able (behavior-preserving, no data/config/schema state) | **Applied** — all tests green, runtime-verified against the real binary across 3 scenarios plus the seam test |
| Add additive public API `graphtor_core::acquire::FileFilter` (crosses binary→library crate boundary) | Low — additive only, SemVer-minor, no breaking change to any existing signature | RED-FIRST tests for the new type; `filter_files` refactored to consume it with all 12 pre-existing characterization tests staying green | **Applied** |
| `055.002-T` alias evaluation | Low | Investigate-first; concluded no code change was warranted (Principle VI) | **Applied** (no-op) |

## Healthy Signals

* `cargo test --all-targets` green (362 lib + 215 bin + all integration binaries).
* `resolved serve posture` log line's `generation_count`/`readonly_count` match the pre-change
  baseline for every previously-served source.
* No unexpected aggregate "all files were excluded" warnings, and no missing warnings where one
  is expected (excluded-only case).

## Failure Signals

* Any per-source `ServeMode` change in **either** direction versus baseline — `ReadOnly` →
  `Generation` (the security-sensitive escalation) or `Generation` → `ReadOnly`.
* Any change to the set of sources resolving to `Generation`.
* Any spurious, missing, or differently-worded aggregate exclusion warning.
* Any new panic, error, or `cargo clippy`/`cargo test` regression surfaced after merge (unlikely
  given full local gate coverage, but the observation window exists precisely because the
  classifier is safety-sensitive).

## Monitoring Plan

* **Signals**: per-source `ServeMode` classification and the aggregate exclusion warning, observed
  via `graphtor-docs serve`/`status` stderr output (this repository has no hosted dashboard —
  single-developer, local-only tool).
* **Method**: run `serve`/`status` against each previously-served source in isolation (or add a
  temporary per-source classification log line) so every source's posture is individually visible,
  since aggregate startup logging reports only totals.
* **Baseline**: the pre-change classification and warning output captured on the same fixtures
  before this shipment merged (all `serve_discovery` regression tests, unchanged and green, serve
  as the automated form of this baseline; the manual runtime-verification scenarios in this
  session serve as the observed real-binary form).
* **Owner**: the developer merging the shipment (single-developer repository; no on-call rotation).
* **Alert threshold / rollback trigger**: ANY per-source posture change versus baseline in either
  direction, or any spurious/missing/differently-worded aggregate warning.

## Rollback Trigger

Any per-source `ServeMode` posture change versus the pre-merge baseline (in either direction), any
change to the set of `Generation` sources, or any spurious/missing/differently-failing aggregate
exclusion warning, observed during the validation window below.

## Rollback Procedure

`git revert` the shipment's commits — in reverse dependency order if needed (binary streaming
classifier commit `73454f4` first, then the library `FileFilter`/`filter_files` commit `0f6ae6d`)
so the shared refactor is never left partially active. The change is behavior-preserving with no
data, config, or schema state to unwind. After reverting: rebuild, re-run the per-source comparison
to confirm the pre-change baseline classification is restored, and reopen a follow-up stash entry
with the diverging fixture attached.

## Validation Window

Observe the next 3 local `serve` startups (or 24 hours of local use, whichever comes first) after
merge, comparing per-source classification and warning output against the baseline established by
the regression test suite and this session's manual runtime verification.

## Window-Close Outcome

To be recorded by the developer at window close as one of: healthy, degraded, or rolled-back. This
pre-merge closure artifact defines the window; the outcome itself is recorded in post-merge closure
or a follow-up note once the observation window has elapsed (post-merge, asynchronous to this Ship
session — see Follow-Ups below).

## Releasability Evidence

| Requirement | Status |
|---|---|
| Runtime verification | **Satisfied** — PASS, see runtime-verification report |
| Monitoring plan | **Satisfied** — defined above |
| Rollback trigger + procedure | **Satisfied** — defined above |
| Validation window + owner | **Satisfied** — defined above |
| Pre-deploy audit | **Satisfied** — no migration/flag/rollout risk |

## Readiness Status

**READY.** No conditions block merge. The post-deploy observation window is a manual,
asynchronous follow-up the developer performs after merge (recorded as a follow-up item below);
it does not block the merge decision itself, consistent with the exec plan's framing of this as a
bounded manual checklist rather than a release gate.

## Follow-Ups

* **Post-deploy observation window close-out**: after the 3-startup/24-hour window elapses,
  record the healthy/degraded/rolled-back outcome. This is an asynchronous, low-effort manual
  check appropriate for a stash follow-up rather than blocking this PR.
