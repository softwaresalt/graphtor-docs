---
title: "Shipment 048-S Build Checkpoint — Implementation Complete"
description: "Session memory checkpoint recording implementation state for shipment 048-S after all backlog items were completed and quality gates passed"
date: 2026-08-17
shipment: "048-S"
branch: "feat/serve-auto-discovery-followups"
mode: "P-017 dark-factory"
---

**Date**: 2026-08-17
**Branch**: `feat/serve-auto-discovery-followups`
**Mode**: P-017 dark-factory (activation scope: 970AE45A, 5D98DBCC, B88E37BF, 5868A7C5 — last
activation-scope shipment)

## Status at Checkpoint

All implementation work for shipment 048-S is complete and committed. All backlog items are
`done`. Full quality gate sequence (fmt → clippy pedantic → test → audit) passes.

## Items Completed

| Item | Status | Commit |
|---|---|---|
| `055-F` (covering feature) | done | — |
| `055.001-T` (execution container) | done | — (delegated to subtasks below) |
| `055.001.001-ST` (library FileFilter API) | done | `0f6ae6d` |
| `055.001.002-ST` (binary streaming classifier) | done | `73454f4` |
| `055.002-T` (alias evaluation) | done | `e57f283` |

## Implementation Summary

### 055.001.001-ST — `graphtor_core::acquire::FileFilter`

* Added `pub struct FileFilter` with `new(include, exclude) -> Result<Self, GraphtorError>` and
  `is_match(&self, path: &Path) -> bool` in `src/acquire/filter.rs`.
* Refactored `filter_files` to build a `FileFilter` internally and call `is_match` per file —
  single source of truth, no forked glob logic.
* Exported `FileFilter` from `src/acquire/mod.rs`.
* RED-FIRST TDD: 9 new tests written against an `unimplemented!()` stub, confirmed failing,
  then implemented to green. All 12 pre-existing `filter_files` characterization tests
  (S026–S034 plus error cases) stayed green throughout — proof of behavior preservation.
* Additive, SemVer-minor: no existing public signature changed.

### 055.001.002-ST — Streaming `source_has_ingestible_content`

* Added a new private `stream_ingestible<I, E>(steps, matcher) -> bool` helper in
  `src/workspace/serve_discovery.rs` (binary crate). O(1) memory: a `found: bool` and a
  `format_candidate_count: usize`, no per-file `Vec`.
* `source_has_ingestible_content` now maps `walkdir::WalkDir` entries to
  `Result<Option<PathBuf>, walkdir::Error>` steps (`None` = non-candidate entry, `Some(path)` =
  format-matching candidate) and streams them through `stream_ingestible`, reusing the shared
  `graphtor_core::acquire::FileFilter` matcher instead of calling `filter_files` per file.
* Full error-observing walk retained — `Err` at any position (including AFTER an eligible
  candidate) forces `false`. No traversal short-circuit.
* Warning parity: emits the same `"filter produced empty file set — all files were excluded"`
  message under the same `input_files` field name (scalar count), exactly once, only when
  candidates existed but all were excluded.
* Characterization-first: 6 tests pinned the CURRENT (pre-refactor) behavior first and were
  confirmed passing against the OLD implementation before any refactor — including a
  differential test against a from-scratch reimplementation of the old batch algorithm.
* RED-FIRST for the new `stream_ingestible` abstraction: 7 tests written against an
  `unimplemented!()` stub (confirmed failing) before implementation, including the seam-driven
  regression case (eligible candidate observed before a later walk `Err`).
* All 44 `serve_discovery` tests green; stress-tested 10 consecutive runs with no flakiness.

**Tracing capture pitfall found and fixed**: the warning-capture test initially used
`EnvFilter::new("graphtor_core=warn")`, which is the WRONG crate name — `serve_discovery.rs` is
compiled into the **binary** crate (`graphtor_docs`, module path from crate name
`graphtor-docs`), not the `graphtor_core` **library** crate. Fixed to
`EnvFilter::new("graphtor_docs=warn")`. This is a distinct, more common bug than the
callsite-interest-cache race documented in
`docs/compound/tracing-callsite-interest-cache-parallel-test-race.md` (worth flagging during
compound-refresh — the existing learning doesn't call out the target-crate-name mismatch
failure mode, only the interest-cache race).

### 055.002-T — Served-alias canonicalization evaluation

* Investigate-first, concluded outcome **(a)**: documented no-op, no code change.
* Evidence: `discover_served_databases`'s canonical-path `BTreeSet` dedup already handles union
  assembly, shared-alias collapse, and outside-alias rejection (verified via existing passing
  tests: `served_set_is_canonical_deduped_union_of_candidates_and_root_scan`,
  `explicit_database_entry_matching_an_auto_discovered_file_collapses_to_one_entry`,
  `explicit_database_entry_outside_graphtor_but_inside_project_root_is_rejected`, junction/`..`
  rejection tests). `DatabaseSource` carries only `id` + `path` — no other config that dedup
  collapse could silently lose.
* Decision doc: `docs/decisions/2026-08-17-served-alias-canonicalization-evaluation.md`.

## Quality Gates (all green)

* `cargo fmt --all -- --check`: PASS
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`: PASS
* `cargo test --all-targets`: PASS (362 lib + 215 bin + all integration test binaries, 0 failures)
* `cargo audit` with the CI-equivalent `--ignore` allowlist (see `audit.toml` /
  `.github/workflows/ci.yml`): PASS. Raw `cargo audit` without the allowlist reports the same
  7 pre-existing, already-tracked advisories documented in `audit.toml` (owned by task
  013.008-T and the 035-C deliberation) — unrelated to this shipment, not introduced by it.

### RED-phase evidence (traceability)

Per Constitution Principle II, RED-then-GREEN was directly observed in-session, not merely
asserted:

* `FileFilter` (055.001.001-ST): `cargo test --lib acquire::filter` was run against the
  `unimplemented!()` stub BEFORE implementation and showed `12 passed; 9 failed` — all 9
  failures were the new `FileFilter` tests panicking with the exact `unimplemented!()` message;
  all 12 pre-existing `filter_files` tests stayed green throughout. After implementation, the
  same command showed `21 passed; 0 failed`.
* `stream_ingestible` (055.001.002-ST): `cargo test --bin graphtor-docs
  workspace::serve_discovery::tests::stream_ingestible` was run against the `unimplemented!()`
  stub BEFORE implementation and showed `0 passed; 7 failed` — all 7 new tests panicking with
  the exact stub message. After implementation, the full `serve_discovery` module (44 tests,
  including the 6 characterization tests confirmed passing against the PRE-refactor
  implementation first) showed `44 passed; 0 failed`.

## Adversarial Review (mandatory, 7 personas, cross-model)

Dispatched in parallel: Security Reviewer (`claude-opus-4.6`), Constitution Reviewer
(`gpt-5.5`), Correctness Reviewer (`claude-sonnet-4.6`), Architecture Strategist (`gpt-5.4`),
Rust Reviewer (`claude-sonnet-5`), Concurrency Reviewer (`gemini-3.1-pro-preview`),
Schema-CLI-Docs Coupling Reviewer (`grok-4.5`).

* **Security/Constitution lens**: PASS, no findings — fail-closed contract verified sound.
* **Correctness lens**: PASS_WITH_FINDINGS. One genuine **P1**: a proactively-added performance
  optimization (skip `is_match`/count once `found` was already `true`) caused
  `format_candidate_count` to under-count once a match was found — benign for current behavior
  (the warning only fires when `!found`) but a latent semantic-drift risk. **Resolved**: reverted
  the optimization; `format_candidate_count` is now always an accurate total (the
  concurrency/performance reviewer separately confirmed the optimization's benefit was
  negligible at this codebase's realistic scale, so reverting has no real cost).
* **Rust idiom lens**: PASS_WITH_FINDINGS. One **P1 (MEDIUM confidence, self-disclosed as
  unverified — no execute access)**: flagged `FileFilter::is_match`'s `match &Option` shape as a
  plausible `clippy::pedantic::option_if_let_else` violation. **Verified and refuted**:
  explicitly ran `cargo clippy --all-targets -- -D warnings -D clippy::pedantic -D
  clippy::option_if_let_else` — this lint DOES fire on the flagged lines, but ALSO fires on 12
  pre-existing, unrelated call sites across the codebase, proving `option_if_let_else` is not
  actually enabled by this repository's `-D clippy::pedantic` gate (confirmed clean, exit 0,
  without the extra explicit flag). False positive, no fix needed. Also applied two small P3
  cleanups from this review: imported `FileFilter` once via `use` instead of repeating the
  fully-qualified path at each use site.
* **Architecture lens**: PASS_WITH_FINDINGS. One P2 (duplicated format-alias normalization
  across the binary/library boundary) — verified via `git show bab9577:...` to be byte-for-byte
  pre-existing, unmodified by this shipment; correctly out of scope per task boundaries.
* **Performance/concurrency lens**: PASS. Confirmed O(1) memory achieved, no concurrency surface
  introduced, test-only `Arc<Mutex<...>>` log-capture helper is sound (poison-recovery via
  `PoisonError::into_inner`).
* **Schema-CLI-docs coupling lens**: PASS_WITH_FINDINGS. One genuine **P2**: the runtime
  verification report's "Follow-Up Recommendations" section linked to a
  `...-release-observability.md` file that was never created (release-observability content was
  folded into the closure doc instead, leaving a dangling cross-reference). **Resolved**:
  retargeted the link to the actual closure doc's Monitoring Plan/Rollback sections. Also
  flagged (and now documented) that the aggregate warning's `tracing` target changed from
  `graphtor_core::acquire::filter` to `graphtor_docs::workspace::serve_discovery` — expected
  given the crate-boundary refactor, now called out explicitly in the runtime-verification
  report's new "Observability Note" section for `RUST_LOG` users.

All P0/P1 findings across all 7 reviewers are resolved (one genuine fix; one empirically-refuted
false positive). Remaining P2/P3 findings are either fixed (broken link, tracing-target note,
import consistency, memory-checkpoint frontmatter) or explicitly acknowledged as pre-existing/
out-of-scope/advisory-only with rationale recorded here.

## Next Steps (remaining before merge)

1. PR creation with `## Local Review Readiness` block referencing the final HEAD SHA (after the
   adversarial-review fix commit).
2. Copilot shadow-review cycle (blocking per operator directive for this session).
3. CI wait, merge-commit-only merge.
4. Post-merge closure: `post-merge/serve-auto-discovery-followups` branch, shipment-reconcile
   safe-close for `048-S` (archive-only, never cascade `backlogit_ship_shipment`), compound
   refresh, compact-context.

## Decisions and Rationale

* Kept `055.001-T` as a non-executing container per its own recorded post-cap decomposition
  (PR #96 finding E) — executed only its two subtasks directly, matching Stage's design.
* Chose to decouple `stream_ingestible`'s aggregation logic from `walkdir::DirEntry` (which
  cannot be constructed outside the `walkdir` crate) via a `Result<Option<PathBuf>, E>` step
  abstraction — this is what makes the eligible-file-before-later-error regression testable
  deterministically on Windows (the pre-existing `#[cfg(unix)]`-gated chmod-based test for a
  sibling scenario does not run on this Windows dev machine at all).

## Failed Approaches

* None — implementation proceeded directly per Stage's plan without blocking failures, aside
  from the tracing EnvFilter crate-name bug (caught and fixed within the same task, not a
  separate failed attempt requiring rollback).

## Files Modified

* `src/acquire/filter.rs` (FileFilter + tests)
* `src/acquire/mod.rs` (export)
* `src/workspace/serve_discovery.rs` (streaming classifier + tests)
* `docs/decisions/2026-08-17-served-alias-canonicalization-evaluation.md` (new)
