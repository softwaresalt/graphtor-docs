---
title: "Avoid env var mutation in parallel Rust tests"
description: "Mutating process-wide environment variables in Rust tests causes flakiness since tests run in parallel by default"
problem_type: "test_flakiness"
category: "best-practices"
component: "src/parse/pdf.rs"
root_cause: "std::env::set_var/remove_var mutates process-wide state; Rust test threads run concurrently and can observe each other's env changes"
resolution_type: "design_change"
severity: "medium"
message: "Copilot review: env var mutation causes flaky behavior if other tests read or write the same env var concurrently"
file_path: "src/parse/pdf.rs"
citations:
  - "PR #25 review comment PRRT_kwDORiB5E85_QxYw"
tags:
  - "testing"
  - "parallelism"
  - "env-vars"
  - "flakiness"
  - "rust"
---

## Problem

Tests for `load_pdfium()` used `std::env::remove_var("GRAPHTOR_PDFIUM_PATH")` to ensure the first search path missed, then restored the value afterward. Since Rust's `cargo test` runs tests in parallel by default, any other test reading this env var concurrently would observe the removed value — causing intermittent failures.

## Root Cause

`std::env::set_var` and `std::env::remove_var` mutate **process-wide** state. Rust tests share a single process and run on parallel threads. There is no built-in synchronization for env var access between test threads.

## Resolution

Redesigned tests to **not mutate env vars at all**. Instead, tests handle both outcomes gracefully:

```rust
#[test]
fn pdfium_load_returns_not_available_without_panic() {
    // Do NOT mutate env vars — handle both DLL-present and DLL-absent.
    let result = PdfiumBackend::load_pdfium();
    if let Err(e) = result {
        assert!(matches!(e, PdfiumBindError::NotAvailable(_)));
    }
    // If DLL is found, Ok is fine — the invariant is no panic.
}
```

## Prevention

When testing code that reads env vars:

1. **Design for both states** — write assertions that pass whether the env var is set or not
2. **If env var control is essential**, use `#[serial]` from the `serial_test` crate or refactor the function to accept a config parameter instead of reading env directly
3. **Prefer dependency injection** — pass search paths as parameters to `load_pdfium()` for testing, use env var only in the production caller
4. **Never save/restore env vars** as a "mutex" — it's not thread-safe and creates a false sense of safety
