---
title: "clippy::useless_conversion Fires on CI But Not Locally Due to Rust Version Skew"
description: "Newer Rust toolchains promote clippy::useless_conversion, catching .into_iter() on types that already implement Iterator — missed by older local toolchain."
problem_type: "lint_failure"
category: "workflow-issues"
component: "src/pipeline/mod.rs"
root_cause: "CI uses a newer Rust toolchain than the local dev environment; clippy::useless_conversion was promoted/tightened in Rust 1.95, flagging .into_iter() calls that are redundant when zip() accepts IntoIterator directly."
resolution_type: "code_fix"
severity: "medium"
message: "warning: useless conversion to the same type: `Vec<Vec<f32>>` / consider removing `.into_iter()`"
file_path: "src/pipeline/mod.rs"
citations:
  - "PR #14: feat(db): vector storage and semantic search (016-F, 008-S)"
  - "commit 0ea00ea: fix(pipeline): remove useless .into_iter() call"
  - "CI run: https://github.com/softwaresalt/graphtor-docs/actions/runs/25228296101"
tags:
  - "clippy"
  - "rust-version-skew"
  - "ci"
  - "into_iter"
  - "useless_conversion"
---

## Problem

CI (`cargo clippy`) failed with:

```
warning: useless conversion to the same type: `Vec<Vec<f32>>`
  --> src/pipeline/mod.rs:462:18
   |
   | for batch in vecs.into_iter().chunks(EMBED_BATCH_SIZE) {
   |                   ^^^^^^^^^^^^ help: consider removing `.into_iter()`
   = note: `-D clippy::useless_conversion` implied by `-D clippy::pedantic`
```

The local clippy pass (Gate 2) passed cleanly. The CI run failed immediately after push.

## Root Cause

CI was running a newer Rust toolchain (1.95+) while the local environment ran an older version. The `clippy::useless_conversion` lint was tightened in the newer release to flag `.into_iter()` calls on types where the receiver already implements `Iterator` or where the call site accepts `IntoIterator` directly. In this case, `vecs` is a `Vec<Vec<f32>>` and `.into_iter()` produces a `std::vec::IntoIter<Vec<f32>>` — the conversion is redundant since `itertools::Itertools::chunks` accepts `IntoIterator`.

The lint is part of `clippy::pedantic` which is denied in this workspace via `-- -D clippy::pedantic`.

## Resolution

Remove the `.into_iter()` call at the affected site:

```rust
// Before (triggers lint on Rust 1.95+)
for batch in vecs.into_iter().chunks(EMBED_BATCH_SIZE) {

// After
for batch in vecs.chunks(EMBED_BATCH_SIZE) {
```

Single-character change in `src/pipeline/mod.rs`. After applying:

```
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
# exit 0, no warnings
```

## Prevention

1. **Pin CI toolchain to match local**: Add a `rust-toolchain.toml` at the workspace root specifying the exact channel (e.g., `channel = "1.85.0"`). This prevents version drift between local and CI. If intentionally tracking stable, document the minimum required version explicitly.

2. **Avoid `.into_iter()` on owned collections in iterator chains**: When passing a collection to a function that accepts `IntoIterator`, omit `.into_iter()`. The trait bound is satisfied by the collection directly. Prefer `vecs.chunks(N)` over `vecs.into_iter().chunks(N)`.

3. **Run `cargo +stable clippy` locally** before pushing if the local toolchain differs from `stable`. This catches lint promotions that land in new stable releases.

4. **Watch for `clippy::pedantic` lint promotions in Rust release notes**: When upgrading the toolchain, scan the release notes for newly-promoted pedantic lints and proactively audit the codebase.
