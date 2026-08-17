# Shipment 048-S Build Checkpoint — Implementation Complete

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

## Next Steps (not yet done at this checkpoint)

1. Runtime verification: exercise classifier posture across representative source trees
   including a later walk-error seam (via the new unit tests; consider an additional
   integration-level smoke pass).
2. Release-observability evidence: bounded post-deploy observation window per the exec plan
   (`docs/exec-plans/2026-08-16-serve-auto-discovery-followups-plan.md` § Runtime Verification
   and Closure).
3. Standard review (report-only) + mandatory adversarial review (3+ personas).
4. PR creation with `## Local Review Readiness` block, Copilot shadow-review cycle (blocking
   per operator directive), CI wait, merge-commit-only merge.
5. Post-merge closure: `post-merge/serve-auto-discovery-followups` branch, shipment-reconcile
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
