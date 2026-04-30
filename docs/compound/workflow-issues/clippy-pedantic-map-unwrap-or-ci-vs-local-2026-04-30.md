---
title: "clippy::map_unwrap_or fires on CI (Linux Rust 1.95) but not locally on older toolchain"
description: "Result.map(f).unwrap_or(d) must be Result.map_or(d, f) under clippy::pedantic — CI catches this when local toolchain does not"
problem_type: "lint_error"
category: "workflow-issues"
component: "src/sync/mod.rs"
root_cause: "clippy::map_unwrap_or (part of pedantic group) changed lint activation threshold between Rust toolchain versions; Linux CI on Rust 1.95.0 triggered it while local Windows toolchain did not"
resolution_type: "code_fix"
severity: "medium"
message: "error: called `map(<f>).unwrap_or(<a>)` on a `Result` value. This can be done more directly by calling `map_or(<a>, <f>)` instead"
file_path: "src/sync/mod.rs"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/10"
tags:
  - "clippy"
  - "pedantic"
  - "map_unwrap_or"
  - "map_or"
  - "ci"
  - "toolchain"
---

## Problem

Local `cargo clippy` passed cleanly, but CI (Linux, Rust 1.95.0) failed with:

```text
error: called `map(<f>).unwrap_or(<a>)` on a `Result` value.
       This can be done more directly by calling `map_or(<a>, <f>)` instead
  --> src/sync/mod.rs:234:9
   |
   |         SystemTime::now()
   |             .duration_since(UNIX_EPOCH)
   |             .map(|d| d.as_secs())
   |             .unwrap_or(0)
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/...
   = note: `-D clippy::map-unwrap-or` implied by `-D clippy::pedantic`
```

The code compiled and worked correctly — this is a purely stylistic lint
under `clippy::pedantic`.

## Root Cause

`clippy::map_unwrap_or` is part of the `pedantic` group. Its activation
appears to be version-dependent — older local toolchains may not surface it
while the CI toolchain (pinned to a newer Rust stable) does. When the
project uses `-D clippy::pedantic` in the CI gate, even a single new lint
activation across toolchain updates causes a CI failure.

## Resolution

Replace the two-step `.map(f).unwrap_or(d)` chain with the single
`.map_or(d, f)` combinator:

```rust
// Before (triggers clippy::map_unwrap_or)
SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)

// After (correct)
SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_secs())
```

Note: argument order in `map_or` is `(default, mapping_fn)` — the default
value comes **first**, unlike `.unwrap_or(default)` which comes last.

## Prevention

- Always use `map_or(default, fn)` and `map_or_else(default_fn, fn)` instead
  of `.map(f).unwrap_or(d)` and `.map(f).unwrap_or_else(d)`.
- The same rule applies to `Option`: `opt.map(f).unwrap_or(d)` → `opt.map_or(d, f)`.
- Run `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` locally
  on the same Rust version as CI (check `rust-toolchain.toml` or CI workflow)
  to catch version-sensitive pedantic lints before push.
- When CI uses a newer Rust than the local development machine, new pedantic
  lints may activate without local warning — keep toolchains synchronized.
