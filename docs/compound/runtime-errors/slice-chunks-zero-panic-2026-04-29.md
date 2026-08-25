---
title: "slice::chunks(0) panics at runtime — guard batch_size before chunking"
description: "Passing batch_size=0 to slice::chunks() causes a runtime panic; must clamp to 1 before use"
problem_type: "panic"
category: "runtime-errors"
component: "src/pipeline/mod.rs"
root_cause: "Rust's slice::chunks(n) panics if n=0; PipelineConfig.batch_size=0 is a valid user input that must be guarded"
resolution_type: "code_fix"
severity: "high"
message: "attempt to use chunk size 0 (panics in slice::chunks)"
file_path: "src/pipeline/mod.rs"
citations:
  - "docs/archive/plans/2026-08-24-pre-august-compaction/2026-04-29-pipeline-orchestration-plan.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/7"
tags:
  - "pipeline"
  - "panic"
  - "batch-size"
  - "slice"
---

## Problem

`slice::chunks(n)` in Rust panics at runtime if `n == 0`. The pipeline
`PipelineConfig.batch_size` defaults to a non-zero value, but a caller
can construct `PipelineConfig { batch_size: 0, .. }` and trigger the
panic. No compile-time protection exists — this is a pure runtime failure.

The panic message is:

```text
attempt to use chunk size 0
```

The failure appears deep in the batch loop, not at the point of configuration
construction, making it hard to diagnose without knowing this Rust invariant.

## Root Cause

Rust's standard library `slice::chunks()` method requires `chunk_size >= 1`
and panics otherwise. This is documented but easy to miss when `batch_size`
is treated as a "the default is fine" field.

## Resolution

Add an explicit guard at the top of `run()` that clamps `batch_size` to 1
when it is 0, and emits a warning:

```rust
let effective_batch_size = if config.batch_size == 0 {
    warn!("PipelineConfig.batch_size is 0; clamped to 1");
    1_usize
} else {
    config.batch_size
};
```

Use `effective_batch_size` throughout instead of `config.batch_size`.

## Prevention

- Any time `slice::chunks(n)` or `slice::windows(n)` is called with a
  user-supplied value, add a pre-condition guard that clamps or rejects `n=0`.
- Consider adding a `batch_size > 0` validation to `PipelineConfig` via a
  constructor or `TryFrom` impl that returns an error for invalid values.
- Add a test case: `PipelineConfig { batch_size: 0, .. }` should not panic
  and should process all documents (equivalent to `batch_size: 1`).
