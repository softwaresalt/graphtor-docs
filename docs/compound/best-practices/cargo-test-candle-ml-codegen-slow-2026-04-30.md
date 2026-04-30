---
title: "cargo test takes 60+ minutes with candle ML dependencies — run compiled binary instead"
description: "Full cargo test on graphtor-docs triggers LLVM codegen for candle-transformers (ML deps) on every run; skip codegen by running the compiled test binary directly"
problem_type: "slow_build"
category: "best-practices"
component: "tests/"
root_cause: "candle-core, candle-transformers, candle-nn compile with full LLVM optimization even in dev profile; cargo test triggers this codegen on every dependency change"
resolution_type: "workaround"
severity: "medium"
message: "Compiling candle-transformers ... [takes 60+ minutes]"
file_path: "Cargo.toml"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/10"
tags:
  - "cargo-test"
  - "candle"
  - "ml"
  - "build-time"
  - "codegen"
  - "workaround"
---

## Problem

Running `cargo test` in the graphtor-docs workspace takes 60+ minutes because
the candle ML crates (`candle-core`, `candle-transformers`, `candle-nn`)
perform full LLVM machine-code generation during compilation, even in the `dev`
profile. This makes using `cargo test` as a routine quality gate impractical
during development.

## Root Cause

Candle is a pure-Rust ML framework that implements GPU/CPU tensor operations.
Its crates are large and require heavy LLVM optimization to produce usable
performance. The Rust toolchain generates LLVM IR for every crate regardless
of profile, but candle's tensor kernels amplify this cost significantly.

`cargo check` and `cargo clippy` are fast (~25–30 seconds) because they
generate type-checked LLVM IR without emitting machine code.

## Resolution

### Fast iteration during development

Use `cargo check` and `cargo clippy` for the inner loop. They catch all
type errors and most logic errors without full codegen:

```powershell
cargo check
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
```

### Running tests without re-compiling

After the initial build succeeds, find and run the compiled test binary
directly — it skips the codegen phase entirely:

```powershell
# Find the test binary (built once via cargo test --no-run)
Get-ChildItem target\debug\deps\graphtor_core-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1

# Run it directly
.\target\debug\deps\graphtor_core-<hash>.exe
```

Or use `cargo test --no-run` to compile once, then re-run the binary
on subsequent test iterations:

```powershell
cargo test --no-run   # compile only (still slow on first run)
# ...iterate on code...
.\target\debug\deps\graphtor_core-<hash>.exe  # fast re-run
```

### CI strategy

CI runs `cargo test` once per PR push where the full build is acceptable.
Developers should not wait for the full test suite locally on every change —
use `cargo check` + `cargo clippy` for rapid feedback.

## Prevention

- Never block development on `cargo test` output for logic changes that
  `cargo check` already validates.
- If test compile time becomes a blocker, consider splitting the workspace
  into a `graphtor-core` crate (with tests) and a `graphtor-ml` crate
  (candle deps only), so `cargo test -p graphtor-core` skips candle codegen.
- Document the `--no-run` + direct binary approach in the contributing guide
  for new developers.
