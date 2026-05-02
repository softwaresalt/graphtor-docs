---
title: ".markdown extension canonicalization needed before format allow-list check"
tags: [pipeline, format-filtering, rust, bug-pattern]
date: 2026-05-02
---

## Problem

The graphtor-docs pipeline dispatch matched both `"md"` and `"markdown"` in
the parse arm, but the format allow-list (`formats: ["md","pdf","docx"]`)
compared raw file extensions. Files with `.markdown` extension were silently
skipped under default settings because `"markdown" ∉ ["md","pdf","docx"]`.

## Root Cause

Two separate subsystems used different comparison strategies:

- **`is_format_allowed()`** — called with the raw extension string
- **Parse dispatch** — handled `"md" | "markdown"` as equivalent

The allow-list check ran before the parse dispatch, so `.markdown` files were
filtered out before they ever reached the dispatch arm.

## Fix Pattern

Canonicalize the extension to a normalized form **before** the allow-list
check:

```rust
// In process_batch(), after extracting ext:
let ext = file
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

// Canonicalize aliases before allow-list check
let canonical_ext = if ext == "markdown" { "md" } else { &ext };

if !is_format_allowed(formats, canonical_ext) {
    skipped_by_format += 1;
    continue;
}
```

Also add `"markdown"` to `VALID_FORMATS` in `config/validation.rs` and to the
`VALID` list in `acquire/plan.rs` so users can explicitly specify it in
`sources.yaml`.

## Complement: Case normalization in validation

Both `validate_formats()` and `validate_format_list()` did exact-case
comparison against their allow-lists while `is_format_allowed()` used
`eq_ignore_ascii_case`. Fix: normalize input to `.to_ascii_lowercase()` before
membership checks in all three functions.

## Evidence

Copilot low-confidence comment on PR #19, independently confirmed and fixed
in commit `ce25f99`, 2026-05-02.
