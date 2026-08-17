---
title: "Serve auto-discovery follow-ups (PR90 deferrals) — decided plan"
description: "Decided plan: reduce ingestible-content classifier memory (behavior-preserving) and evaluate served-alias canonicalization"
date: 2026-08-16
decided: 2026-08-17
status: shipped
shipment: "048-S"
source: "docs/decisions/2026-08-16-serve-auto-discovery-followups-deliberation.md"
related:
  - "docs/decisions/2026-08-17-served-alias-canonicalization-evaluation.md"
supersedes: "docs/archive/plans/2026-08-16-serve-auto-discovery-followups-plan.md"
stash_ids:
  - "B88E37BF"
  - "5868A7C5"
tags:
  - serve-discovery
  - performance
  - follow-up
---

## Decision

`source_has_ingestible_content` (`serve_discovery.rs:333`) eagerly walked the
entire source tree, collected every format-matching relative path into a
`Vec`, then called `graphtor_core::acquire::filter_files` once and checked
non-empty — but the caller (`ServeMode` classification) only needs a boolean.
Decided fix: stream a boolean over the walk instead of accumulating a `Vec`,
while preserving the full error-observing traversal and the aggregate warning
semantics exactly. Served-database alias handling was evaluated separately and
concluded sufficient as-is (no code change).

## Constraints Preserved

* The entire `WalkDir` is always traversed; any walk error returns `false`
  (fail closed). No traversal short-circuit or first-eligible early return.
* Classification results are identical to the previous batch
  `filter_files(&all_candidates, ...)`-then-non-empty result for every
  representative tree (include/exclude precedence, empty include = all, union
  globs, nested relative paths).
* The aggregate "all files excluded" warning fires exactly once, only when
  format-matching candidates existed but none passed
  (`saw_format_candidate && !found`) — no spurious per-file warning, and none
  when no candidate was seen.
* No per-file `filter_files` recompilation of glob sets; the classifier and
  `filter_files` share one compiled matcher (single source of truth).

## Rejected Alternatives

* **Traversal short-circuit returning `true` on the first eligible file.**
  Rejected (adversarial-review consensus, HIGH confidence): an unreadable
  subtree encountered after an early eligible file would be skipped, flipping
  a partially-unreadable source from `false` (read-only, fail-closed) to
  `true` — which `classify_serve_postures` treats as eligible for the
  read-**write** `Generation` posture. That is a safety-degrading posture
  escalation and was rejected outright; the full, error-observing walk was
  retained.
* **Call `filter_files` per candidate file.** Rejected: recompiles the glob
  sets on every entry and would emit a per-file "all files excluded" warning
  even when another file makes the source ingestible, breaking warning
  parity with the previous batch behavior.
* **Add explicit alias canonicalization/reporting code (Unit B2).** Rejected
  as unnecessary after investigation — see Implementation below.

## Implementation (as shipped)

* **Library API** (`graphtor_core::acquire`, additive/SemVer-minor): added
  `pub struct FileFilter` with `new(include, exclude) -> Result<Self,
  GraphtorError>` and `is_match(&self, path: &Path) -> bool`; refactored
  `filter_files` to build a `FileFilter` internally so the classifier and
  `filter_files` share one compiled matcher. Crossing the binary→library
  boundary was required because `source_has_ingestible_content` lives in the
  `graphtor-docs` binary crate while the matcher lived only privately in the
  `graphtor_core` library crate.
* **Streaming classifier** (`src/workspace/serve_discovery.rs`): a private
  `stream_ingestible` helper tracks only a `found: bool` and a
  `format_candidate_count: usize` (O(1) memory) instead of accumulating a
  `Vec`. `WalkDir` entries map to `Result<Option<PathBuf>, walkdir::Error>`
  steps so the eligible-file-before-later-error regression is deterministically
  testable via an injected ordered seam (needed because filesystem walk order
  is unspecified, and the pre-existing Unix-only chmod test does not run on
  Windows).
* **Alias evaluation (investigate-first, concluded outcome (a)):** documented
  no-op. `discover_served_databases`'s canonical-path `BTreeSet` dedup already
  handles union assembly, shared-alias collapse, and outside-alias rejection;
  confirmed via existing passing tests. Decision doc:
  `docs/decisions/2026-08-17-served-alias-canonicalization-evaluation.md`.

## Verification, Rollback, and Monitoring

* Verified via `cargo test` (characterization tests pinning pre-refactor
  behavior first, a differential test against a from-scratch reimplementation
  of the old batch algorithm, and the eligible-file-before-later-error
  regression case); confirmed memory no longer scales with document count.
* **Post-deploy observation window** (required because the classifier gates
  read-only vs read-write `Generation` posture): owner is the developer
  merging the shipment; observe the next 3 local `serve` startups (or 24
  hours, whichever comes first), running `serve`/`status` against each
  previously-served source in isolation (since startup logging may report
  aggregate counts rather than each source's classifier result) and comparing
  per-source `ServeMode` classification and warning output against the
  pre-change baseline. Record baselines using source labels or relative
  identifiers, never absolute internal/external paths (Principle III).
* **Rollback trigger**: any per-source posture change in either direction
  versus baseline, any change to the set of `Generation` sources, or any
  spurious/missing/differently-failing aggregate warning.
* **Revert procedure**: `git revert` the streaming-classifier commit(s) (both
  the library-API and binary-streaming commits, in reverse dependency order),
  rebuild, re-run the per-source comparison to confirm baseline classification
  is restored.
* Window-close outcome (healthy / degraded / rolled-back) is recorded in the
  shipment closure artifact as releasability evidence. Stashed follow-up
  `8C2E313D` tracks the asynchronous window close-out.

## Plan Review Outcome

**PASS**, after one remediation cycle, reviewed by the same cross-model
reviewer set as the companion plan plus a post-remediation re-review. Round 1
raised one HIGH-confidence consensus P1 (the rejected traversal short-circuit,
above); resolved by retaining the full error-observing walk and adding the
regression + differential tests. A second Copilot review pass on the staging
PR raised two further plan-facing findings — the public-API/SemVer impact
(initially mislabelled absent; corrected to present/additive with the
binary→library crate-boundary detail) and a missing post-deploy observation
requirement (added, see above) — both resolved with no unresolved HIGH/MEDIUM
P0/P1 findings remaining.

## Shipped

Merged as shipment `048-S`, PR #101 (feature) and PR #102 (post-merge
closure), commits `ac8847b85ce2cea53a8f739530b35d3f6ea2ede4` and
`0cf49a81d5471026d17c81ea09db0d92f569a94b`. Full execution record:
`docs/closure/2026-08-17-serve-auto-discovery-followups-closure.md`,
`docs/closure/2026-08-17-serve-auto-discovery-followups-runtime-verification.md`,
and `docs/closure/2026-08-17-serve-auto-discovery-followups-post-merge-closure.md`.
Original plan (with full round-by-round review transcript and hardening
detail) archived at
`docs/archive/plans/2026-08-16-serve-auto-discovery-followups-plan.md`.
