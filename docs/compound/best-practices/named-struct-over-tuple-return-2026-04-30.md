---
title: "Named struct vs tuple return in private pipeline helpers"
date: 2026-04-30
tags: [rust, pipeline, refactor, best-practices]
pr: 9
commit: 16d8aba
---

## Context

`process_batch` in `src/pipeline/mod.rs` originally returned a raw tuple
`(usize, usize, Vec<FileError>)`. The fields were positional and had no
self-documenting names, making call sites read as `source_docs += batch_docs`
where `batch_docs` was the first element of a destructured tuple.

## Pattern

Replace tuple returns from private helpers with named structs when:

1. The tuple has 3 or more elements.
2. The elements are not all the same type.
3. The struct is used in a loop accumulation context (where misaligned
   field assignment would be a silent bug).

```rust
// Before — error-prone positional access
fn process_batch(...) -> (usize, usize, Vec<FileError>) { ... }
let (batch_docs, batch_chunks, batch_errors) = process_batch(...);

// After — named, self-documenting, extensible
struct BatchResult {
    docs_processed: usize,
    chunks_loaded: usize,
    errors: Vec<FileError>,
}
fn process_batch(...) -> BatchResult { ... }
let result = process_batch(...);
source_docs += result.docs_processed;
```

## Scoping Rule

The struct does NOT need to be `pub` or `pub(crate)` if it is only used within
the same module. A plain `struct` declaration is private to the module — correct
for implementation details of `mod.rs` helpers.

## Applicability

Apply this pattern whenever a helper function's return type requires the caller
to destructure positionally. Clippy pedantic does not enforce this, so it
requires manual judgement during review.
