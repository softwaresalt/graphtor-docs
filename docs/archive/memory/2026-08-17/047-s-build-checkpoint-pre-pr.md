---
date: 2026-08-17
shipment: 047-S
feature: 054-F
branch: feat/054-f-readonly-serve-guarantee-honesty
phase: build-complete-pre-pr
---

# 047-S Build Checkpoint — Read-Only Serve Guarantee Honesty

## Summary

All backlog items for shipment `047-S` are implemented, tested, reviewed, and
committed on `feat/054-f-readonly-serve-guarantee-honesty` (branched from
`main` at `33bbb37484464d84d14219938008d70faab12eee`). Ready to push and open
the PR.

## Tasks Completed

* `054.001.001-ST` — RED-first contract test for qualified startup-log
  wording. Observed FAILING against current code (commit `adcd1ee`), then
  green after implementation (commit `65751dc`).
* `054.001.002-ST` — Implemented honest read-only contract surfaces:
  corrected rustdoc on `is_engine_enforced_readonly`, `open_engine_readonly`,
  `EngineReadonlyGuard` struct docs; new `ENGINE_READONLY_OPEN_LOG_MESSAGE`
  qualified startup-log constant. `EngineReadonlyGuard::lock`/`Drop` bodies
  byte-identical to `main` (verified via diff). `is_engine_enforced_readonly()`
  unchanged (`guard.is_some()`); zero external callers confirmed via grep.
* `054.001.003-ST` — Release-observability evidence at
  `docs/closure/2026-08-17-047-s-release-observability-evidence.md` (owner,
  baseline, bounded observation window, failure threshold, rollback
  trigger/procedure) — compacted 2026-09-01, now archived at
  `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-047-s-release-observability-evidence.md`,
  see `docs/closure/2026-09-01-047-s-048-s-closure-summary.md`.
* `054.002-T` — Corrected "Read-only serve hardening" section of
  `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`;
  linked spike/deliberation + deferred stash IDs `F1CE20EC`/`5905CDEE`.
* `054.001-T` (execution container) and `054-F` (covering feature) both
  marked `done` — all child work complete.
* `src/main.rs`'s `open_serve_databases` investigated and confirmed to need
  NO changes (no overstated wording found there).

## Stowaways (authorized, applied via `git stash apply` from
`4791882bad8291bae5d26cc0a096d37c25e54cc4`, not popped)

Committed as two coherent commits:
* `7f5de00` `chore(agents): route tier2/tier3 models and enable gemini
  adversarial reviewer` — `.autoharness/config.yaml` + 3 agent frontmatter files.
* `27b84b5` `chore(config): ignore copilot-tracking artifacts and clean stale
  vscode settings` — `.gitignore` + `.vscode/settings.json`.

All 4 original stash entries verified intact (`git stash list` shows all 4,
none popped/dropped).

## Review

* Standard + mandatory adversarial review: 6 independent reviewers across 4
  model providers (Anthropic haiku-4.5/opus-4.7/opus-4.8, Google
  gemini-3.1-pro-preview as alt-provider, xAI grok-4.6, OpenAI gpt-5.6-sol)
  covering constitution/security, security, correctness, architecture,
  Rust, and scope/schema-doc-coupling lenses.
* No P0/P1 consensus findings. Cycle-1 fixes applied (commit `ff3d597`):
  removed 2 new rustdoc private-intra-doc-link warnings, fixed a
  docstring/implementation mismatch on the test's retry helper, narrowed
  `EnvFilter` to `graphtor_core=info`, cleaned stale red-phase comment text.
* Post-remediation re-review (cycle 1 of max 2): 2 reviewers, verdict CLEAN
  (one trivial non-blocking P3 documentation note, explicitly non-behavioral).
* 4 follow-up items stashed for Stage triage: `9CEC208C` (pre-existing `pip:
  true` blanket auto-approve in `.vscode/settings.json`, high priority,
  unrelated to 047-S), `C365AB98` (pre-existing duplicate `.gitignore`
  `.engram/` entry), `3FFE51B4` (model-routing config drift inherited from
  the stowaway stash content itself), `B883681D` (optional `main.rs` F6
  cross-reference, cosmetic).

## Quality Gates (all green on HEAD `ff3d597`)

* `cargo fmt --all -- --check` — clean
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` — clean
* `cargo test --all-targets` — 555+ tests, 0 failures (verified across
  multiple full-suite runs plus 15x stress runs of the new test alone under
  default parallel execution — 0 flakes after the `EnvFilter` +
  `rebuild_interest_cache` + bounded-retry fix)
* `cargo audit` — 1 vulnerability (`lz4_flex` RUSTSEC-2026-0041) + 6
  unmaintained-crate warnings, all CONFIRMED PRE-EXISTING on `main` baseline
  (verified by running `cargo audit` on `main` directly) — not introduced by
  this shipment, out of scope to fix here.

## Notable Engineering Finding (worth compounding)

`tracing`'s callsite `Interest` cache is process-wide and sticky: a log
call-site shared with many sibling tests that call it WITHOUT installing a
subscriber can get permanently cached `Interest::never()` by whichever thread
touches it first, silently dropping a scoped `with_default` subscriber's own
events under parallel `cargo test` — even after `rebuild_interest_cache()`
and an `EnvFilter` (forces per-event evaluation) fixes, empirically ~50-80%
per-attempt failure was still observed. Resolved with a bounded retry loop
(25 attempts, fresh temp DB per attempt) plus the EnvFilter/rebuild
combination — 15/15 stress runs clean. Candidate for `docs/compound/`.

## Next Steps

1. Push branch, create PR with Local Review Readiness block.
2. Request Copilot shadow review (operator elevated it to blocking per task
   instructions) and poll/address per GitHub PR automation instructions.
3. Wait for CI, remediate if needed.
4. Merge (merge-commit strategy) after last-mile readiness re-check.
5. Post-merge closure: `post-merge/054-f-readonly-serve-guarantee-honesty`
   branch, safe-close shipment `047-S`, compound-refresh, compact-context.
6. Do NOT start or claim `048-S`.
