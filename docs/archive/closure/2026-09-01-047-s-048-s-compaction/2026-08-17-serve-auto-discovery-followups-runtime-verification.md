---
title: "Serve Auto-Discovery Follow-Ups — Runtime Verification"
description: "Runtime verification of the streaming ingestible-content classifier and served-alias evaluation for shipment 048-S"
date: 2026-08-17
shipment: "048-S"
feature: "055-F"
verdict: "PASS"
---

## Scope

Runtime surface: serve auto-discovery `ServeMode` classification (`source_has_ingestible_content`,
`classify_serve_postures`) and the `serve` CLI's `resolved serve posture` / aggregate exclusion
warning log lines. No schema, CLI flag, or public MCP tool contract changed. `055.002-T` produced
no code change (documented no-op), so it has no runtime surface to verify beyond confirming the
existing behavior is unchanged (already covered by the full `serve_discovery` regression suite).

## Environment Prechecks

* Build artifact under test: `target\debug\graphtor-docs.exe`, built from
  `feat/serve-auto-discovery-followups`. Initially verified at commit `73454f4`
  (post-refactor); Scenario 2 was re-verified again after the Copilot-review tracing-target fix
  (see Scenario 2's "Observed" block and Observability Note below, which reflect that final
  re-verification, not the initial commit).
* `cargo build --bin graphtor-docs` completed successfully immediately before verification.
* No external services, network, credentials, or fixtures required — `serve` is a local STDIO
  MCP process; verification uses closed-stdin subprocess invocation (the same technique
  `tests/serve_posture_gating_test.rs` already uses) against disposable temp workspaces.

## Adapter and Execution Mode

**CLI / command adapter**, `mode=manual` (direct subprocess invocation, not the automated test
harness) to observe real stderr log output end-to-end against the actual compiled binary, as a
defense-in-depth check beyond the 44 `serve_discovery` unit tests and existing
`serve_posture_gating_test.rs` integration tests (all of which also passed — see Quality Gates
in the build checkpoint memory).

## Scenarios Executed

### Scenario 1 — Ingestible source → `Generation` posture

* Workspace: `docs/guide.md` present; `sources.yaml` configures a `local` source over `./docs`
  with `formats: [md]`, no include/exclude restriction.
* Expected: the source is classified ingestible; its resolved target promotes to `Generation`.
* Observed:

  ```text
  INFO graphtor_docs: resolved serve posture discovered_count=1 generation_count=1 readonly_count=0
  ```

* No aggregate exclusion warning present. **Matches expectation.**

### Scenario 2 — Excluded-only source → `ReadOnly` + exactly one aggregate warning

* Workspace: `docs/guide.md` present; `sources.yaml` sets `include: ["nomatch/**"]` so the one
  format-matching candidate is excluded by the include filter.
* Expected: classifier returns `false` (excluded-only); exactly one aggregate warning is emitted
  carrying the scalar format-candidate count (`input_files=1`).
* Observed:

  ```text
  WARN graphtor_core::acquire::filter: filter produced empty file set — all files were
  excluded input_files=1
  ```

* Exactly one warning line, `input_files=1` (the scalar candidate count, not a per-file list),
  under the exact same explicit tracing target (`graphtor_core::acquire::filter`) as the
  pre-existing `filter_files` S032 warning. **Matches expectation exactly** — same message text,
  same field name, AND same target, confirming true warning parity end-to-end through the real
  binary, not just in the unit-level capture tests.

### Scenario 3 — Zero format-candidate source → `ReadOnly`, no warning

* Workspace: `docs/notes.txt` present (a `.txt` file; source `formats: [md]`, so zero
  format-matching candidates exist at all); an empty placeholder db pre-created at the resolved
  target path so the CLI's unrelated "phantom default" candidate filter (`main.rs`, a pre-existing
  behavior unrelated to this change) does not drop the entry before the posture-summary log line.
* Expected: classifier returns `false` (zero candidates observed); **no** aggregate warning
  (there is nothing to report — a zero-candidate tree is not the same case as an
  all-excluded tree).
* Observed:

  ```text
  INFO graphtor_docs: resolved serve posture discovered_count=1 generation_count=0 readonly_count=1
  ```

* No "all files were excluded" warning anywhere in stderr. **Matches expectation.** (The
  subsequent `open_engine_readonly` error in this scenario's output is an artifact of the
  placeholder file not being a valid SQLite database — expected given the manual test setup, and
  unrelated to classifier behavior, which had already logged its posture decision by that point.)

### Scenario 4 — Later walk-error seam (eligible file before an unreadable subtree)

* This scenario cannot be reproduced deterministically via real filesystem permissions on this
  Windows verification machine: Windows ACL-based unreadable-directory simulation is unreliable
  and non-portable for automated verification, which is exactly why the pre-existing sibling test
  `source_with_an_unreadable_subtree_stays_read_only` in this same file is already
  `#[cfg(unix)]`-gated (it self-skips on Windows even in CI when DAC is bypassed).
* Equivalent evidence: the deterministic, platform-independent unit-level seam test
  `stream_ingestible_false_when_error_follows_an_eligible_candidate` drives this exact scenario
  (an eligible candidate observed, followed by a walk error) through the new `stream_ingestible`
  abstraction without depending on real permissions, and asserts `false` is returned — this is
  the safety-critical regression this shipment exists to protect (Constitution-adjacent: a
  traversal short-circuit would otherwise escalate a partially-unreadable read-only source to the
  read-write `Generation` posture). This test passed in the full quality-gate run (see build
  checkpoint memory) and was stress-tested across 10 consecutive full-suite runs with no
  flakiness.
* On Linux CI (where the `#[cfg(unix)]` sibling test runs), that test provides an additional,
  real-filesystem confirmation of the same fail-closed contract using actual unreadable
  permissions, complementing the portable seam test.

## Verdict

**PASS.** All three CLI-observable scenarios that could be exercised on this Windows verification
machine behaved exactly as specified, using the actual compiled binary rather than only the unit
test harness. The one seam-only scenario (later walk-error after an eligible candidate) is
covered by a deterministic, platform-independent unit test plus a real-filesystem Unix-only
sibling test that runs in CI; both assert the identical fail-closed contract this shipment is
designed to preserve.

## Follow-Up Recommendations

None required for merge. See the Monitoring Plan and Rollback Trigger/Procedure sections of the
pre-merge operational closure
(`docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-closure.md`) for the bounded post-deploy
manual observation window the exec plan requires given the classifier's read-only-vs-generation
safety sensitivity. (Release-observability content is consolidated into that closure artifact
rather than a separate file for this shipment.)

## Observability Note

The aggregate exclusion warning fires from `stream_ingestible` in the BINARY crate
(`src/workspace/serve_discovery.rs`), but uses an explicit `target:
"graphtor_core::acquire::filter"` override in its `tracing::warn!` call so it appears under the
exact same tracing target as the LIBRARY crate's pre-existing `filter_files` S032 warning. This
preserves true observability parity: an operator's existing `RUST_LOG=graphtor_core=warn`
configuration continues to surface this warning after this shipment, confirmed directly above in
Scenario 2's re-verified output.
