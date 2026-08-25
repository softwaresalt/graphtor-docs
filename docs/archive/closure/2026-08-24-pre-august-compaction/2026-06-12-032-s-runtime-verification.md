---
date: 2026-06-12
slug: 032-s-runtime-verification
shipment: 032-S
surface: cli
mode: manual
status: PASS
merge_commit: 2592cfd7404663cb4a28deac11eef8a39fc975cd
owner: copilot
---

# Runtime Verification — 032-S Release sync hardening

## Verification Target

Verify the shipped CLI runtime surfaces for release sync hardening:

* shared embedding-model resolution behaves consistently for sync and prewarm
* incremental `sync` progress stays on stderr and preserves metrics JSON on stdout
* full-sync stage announcements make long-running sync progress visible
* incremental source filtering behavior remains intact after the sync-path changes

## Preconditions

* PR `#67` merged at `2592cfd7404663cb4a28deac11eef8a39fc975cd`
* Shipment `032-S` scope:
  * `041-F`
  * `041.001-T`
  * `041.002-T`
  * `041.003-T`
  * `041.004-T`
  * `041.005-T`
  * `041.006-T`
  * `041.007-T`
  * `041.008-T`
* Closure verification ran on branch `post-merge/041-release-sync-hardening`

## Commands Attempted

```text
cargo test --test sync_progress_test
cargo test --test embedding_resolver_parity_test
cargo test --test sync_incremental_source_filter_test
```

## Expected Behavior

* `sync` emits operator-facing progress on stderr without corrupting structured stdout
* sync and prewarm share the same embedding-model resolution behavior, including `--no-embed`
* full-sync announces coarse stage boundaries instead of appearing stalled
* incremental sync source filtering still selects only the requested source entries

## Observed Behavior

All targeted runtime verification commands passed on the merged code:

* `cargo test --test sync_progress_test`
  * passed all progress-reporting coverage, including metrics/stdout preservation and full-sync stage announcements
* `cargo test --test embedding_resolver_parity_test`
  * passed the sync/prewarm resolver parity coverage
* `cargo test --test sync_incremental_source_filter_test`
  * passed the incremental source-filter behavior check

## Evidence

* the targeted progress suite confirms stderr-only progress output and bounded full-sync stage announcements
* the resolver parity test confirms sync and prewarm share embedding-model lookup behavior
* the source-filter regression test confirms incremental sync still respects filtered source selection

## Verdict

**PASS**

## Recommended Next Action

Carry this verification result into the post-merge closure PR for shipment `032-S`.
