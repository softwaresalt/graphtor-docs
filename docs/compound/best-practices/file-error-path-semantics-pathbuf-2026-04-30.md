---
title: "FileError path semantics — document absolute vs relative vs synthetic"
date: 2026-04-30
tags: [rust, pipeline, error-handling, pathbuf, api-design]
pr: 9
commit: 16d8aba
---

## Context

When `FileError::path` was changed from `String` to `PathBuf`, code review
surfaced that the same field was being populated with three distinct path
formats depending on the error site:

1. **Absolute path** — from `file.clone()` at parse-stage errors (I/O,
   path validation, UTF-8 decode, markdown parse failure)
2. **Source-root-relative path** — from `PathBuf::from(&path_str)` at
   load-stage errors (chunk or edge upsert failure)
3. **Synthetic identifier** — `format!("source:{source_id}").into()` for
   source-level acquisition failures

## Problem

Callers that receive a `FileError` cannot reliably determine which format
the `path` field uses without inspecting the `error` message or inferring
from context. Attempting to canonicalize or open a `source:…` path would
fail silently at runtime.

## Resolution Applied (PR #9)

Updated the `FileError::path` doc comment to document all three formats
and advise callers to inspect the value before use:

```rust
/// Path associated with this failure.
///
/// Three formats are possible depending on the failure site:
///
/// * **Absolute file path** — for I/O errors discovered during the parse stage.
/// * **Source-root-relative path** — for database errors during the load stage.
/// * **Synthetic source identifier** — formatted as `source:{source_id}`.
///
/// Callers should inspect the path value to determine which format applies.
pub path: std::path::PathBuf,
```

## Future Improvement (Backlog ARCH-004)

Consider replacing `PathBuf` with a typed enum:

```rust
pub enum FileErrorPath {
    Absolute(PathBuf),
    Relative(PathBuf),
    SourceId(String),
}
```

This would make path semantics explicit at the type level and prevent
silent misuse. Deferred to post-v0.1.0 as it is a more significant API
change.

## Lesson

When a `pub` field can hold values with different semantic meanings
depending on context, document all formats in the field's doc comment —
even if a proper enum would be better long-term. Ambiguous type semantics
are a hidden API contract problem that reviewers should flag on any type
change touching error structs.
