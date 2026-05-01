---
title: "Concurrent Cargo Runs Produce Stale rlib Doc-Test Failures"
description: "Running two cargo test invocations concurrently causes doc-tests to fail with 'extern location does not exist' when the newer build changes the rlib fingerprint hash."
problem_type: "build_failure"
category: "build-errors"
component: "cargo doc-tests / rlib artifact cache"
root_cause: "A second cargo test run recompiled the library and produced a new rlib with a different hash in its filename (e.g. libgraphtor_core-a6044ba62d9ac9fb.rlib). Doc-tests compiled against the first run's rlib (libgraphtor_core-1448c4f8dd15af71.rlib) could not locate the replacement file."
resolution_type: "workaround"
severity: "medium"
message: "error: extern location for graphtor_core does not exist: target\\debug\\deps\\libgraphtor_core-1448c4f8dd15af71.rlib"
file_path: "target/debug/deps/"
citations:
  - "Session 85c761f8 — 008-S vector search implementation"
  - "logs/test-results.txt (prior concurrent run output)"
  - "PR #14 feat/vector-search"
tags:
  - "cargo"
  - "doc-tests"
  - "rlib"
  - "concurrent-builds"
  - "fingerprint"
---

## Problem

After two `cargo test` runs were started concurrently (one in the background, one
immediately after), the second run's doc-tests all failed with:

```
error: extern location for graphtor_core does not exist:
  D:\...\target\debug\deps\libgraphtor_core-1448c4f8dd15af71.rlib
```

This appeared for every doc-test (`src/lib.rs`, `src/embed/mod.rs`,
`src/path/security.rs`, `src/chunk/id.rs`). The failures are compile-time errors,
not runtime test failures.

The non-doc integration and unit tests all passed (`0 failed`).

## Root Cause

Cargo uses a content-addressed fingerprint hash in rlib filenames. When two `cargo
test` invocations run concurrently and both recompile the library (e.g., because
source files changed since the last build), each produces an rlib with a different
hash. The doc-test runner for the second build embeds the path to its own rlib, but
by the time it executes the doc-tests, the first build's rlib is already gone (or
vice versa). The result is a compile error for every doc-test that tries to
`extern crate graphtor_core`.

## Resolution

Run a single clean `cargo test` after all concurrent processes terminate. The doc-test
failures are not real — they are an artifact of the race. A fresh sequential run
produces a consistent rlib hash and all doc-tests compile and pass.

Steps:
1. Kill any stale cargo processes (see companion learning:
   `workflow-issues/cargo-artifact-lock-stale-process-2026-04-30.md`)
2. Run one clean test:
   ```powershell
   cargo test 2>&1 | Out-File logs\test-results.txt
   ```
3. Doc-test failures should be absent from the clean run.

## Prevention

- Do not run concurrent `cargo test` invocations against the same workspace.
- When reviewing test output that shows only doc-test compile errors and zero unit/
  integration test failures, suspect a stale-rlib race rather than a real code bug.
- Confirm by searching the output for `0 failed` in all non-doc test result lines
  before diagnosing the doc-test failures as genuine.
