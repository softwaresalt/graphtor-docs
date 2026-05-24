---
title: PR58 Copilot runtime hardening memory
date: 2026-05-24
branch: feat/multi-database-runtime-hardening
pr: 58
---

# PR58 Copilot runtime hardening memory

## Completed work

* Resolved the two unresolved Copilot threads on PR #58
* Committed and pushed `4f850b4695fe56f8365af470758d0f8040a7462c`
* Replied to and resolved threads `PRRT_kwDORiB5E86EWy3m` and `PRRT_kwDORiB5E86EWy3t`
* Re-requested Copilot review with GraphQL `requestReviewsByLogin` using `botLogins: ["copilot-pull-request-reviewer"]`

## Files changed

* `src/main.rs`
* `src/lock.rs`

## Decisions

* Scoped sync-time database locking to a per-database callback helper so each database lock drops as soon as that database finishes syncing
* Treated the `WorkspaceLock::acquire` issue as a documentation clarification, not a behavior change, because concurrent live `.replacing` ownership is a real contention case
* Added a regression test in `src/main.rs` to verify the sync helper releases the database lock after the callback returns

## Validation

* `cargo fmt --all -- --check` ✅
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` ✅
* `cargo test` ✅
* `cargo audit` ❌ — pre-existing dependency advisories remain (`lz4_flex` vulnerability plus unmaintained transitive crates)

## Failed or alternate approaches

* `gh pr edit 58 --add-reviewer copilot` and `--add-reviewer "@copilot"` did not work in this environment
* REST `requested_reviewers` also failed for Copilot
* GraphQL `requestReviewsByLogin` with `botLogins` succeeded and is the working re-review path here

## Current status

* No unresolved Copilot threads remain on PR #58
* Fresh Copilot review for commit `4f850b4` is still pending after polling
* Current head check runs show `build` and `Prepare` succeeded, while `Agent` is still in progress

## Next steps

* Wait for the pending Copilot review on `4f850b4` to complete
* Wait for the in-progress `Agent` check run to finish
* Decide whether to handle the pre-existing `cargo audit` findings separately before merge approval
