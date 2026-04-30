---
title: "Clippy pedantic rejects as_millis() as u64 — use try_from with fallback"
description: "clippy::cast_possible_truncation fires on Duration::as_millis() as u64 because as_millis() returns u128"
problem_type: "lint_error"
category: "best-practices"
component: "src/pipeline/mod.rs"
root_cause: "Duration::as_millis() returns u128; casting to u64 with 'as' triggers cast_possible_truncation under clippy::pedantic"
resolution_type: "code_fix"
severity: "medium"
message: "error: casting `u128` to `u64` may truncate the value"
file_path: "src/pipeline/mod.rs"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/7"
tags:
  - "clippy"
  - "pedantic"
  - "duration"
  - "u128"
  - "tracing"
---

## Problem

When passing elapsed milliseconds to `tracing::info!` structured fields,
the natural approach is:

```rust
elapsed_ms = start.elapsed().as_millis() as u64,
```

Under `clippy::pedantic`, this triggers:

```text
error: casting `u128` to `u64` may truncate the value
  --> src/pipeline/mod.rs:151:24
   |
   = help: if this is intentional allow the lint with `#[allow(clippy::cast_possible_truncation)]`
   = help: ... or use `try_from` and handle the error accordingly
```

`Duration::as_millis()` returns `u128` because duration can theoretically
exceed `u64::MAX` milliseconds (~585 million years). Clippy pedantic considers
the `as` cast unsafe even though in practice it will never overflow.

## Root Cause

`clippy::pedantic` includes `cast_possible_truncation` which disallows
all numeric narrowing casts via `as` without explicit acknowledgment.
The `tracing` crate serializes `u128` poorly across some backends, making
`u64` the preferred type for structured log fields.

## Resolution

Use `u64::try_from(...).unwrap_or(u64::MAX)` to handle the conversion
explicitly. `unwrap_or` is not banned by `clippy::unwrap_used` (which only
catches `.unwrap()`):

```rust
elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
```

This satisfies clippy, is safe, and provides a meaningful fallback
(u64::MAX ≈ 584 million years) that will never occur in practice.

Alternatively, if sub-millisecond precision is not needed, use `as_secs()`
which already returns `u64`:

```rust
elapsed_s = start.elapsed().as_secs(),
```

## Prevention

- Establish `u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)` as the
  project-standard pattern for millisecond elapsed fields in tracing spans.
- Never use `as_millis() as u64` directly — the cast will always fail clippy
  pedantic in this codebase.
- Consider a project-local helper: `fn elapsed_ms(d: Duration) -> u64 { u64::try_from(d.as_millis()).unwrap_or(u64::MAX) }`
