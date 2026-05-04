---
title: "Suppressing WARN-Level Noise from a Specific Crate with EnvFilter"
date: 2026-05-04
tags: [rust, tracing, logging, pdf_extract]
---

## Context

The `pdf_extract` crate emits WARN-level messages for every unresolvable glyph in a PDF. In a document-heavy pipeline this floods the log with thousands of lines. The fix requires setting the crate's log level to ERROR, not WARN.

## Pattern

In `EnvFilter` format strings, `crate_name=level` sets the maximum level for that crate. To suppress WARN messages from `pdf_extract`:

```rust
format!("{base_level},pdf_extract=error")
```

**Not** `pdf_extract=warn` — that still allows WARN messages through (it sets the minimum level TO warn, meaning WARN and above are shown).

## Why the Off-by-One Matters

`pdf_extract=warn` means "show WARN and above from pdf_extract" — it does NOT suppress WARN. Only `pdf_extract=error` suppresses WARN (shows ERROR and above only).

In Quiet mode where the base level is `error`, using `pdf_extract=warn` actually widens the filter for that crate, showing WARN from pdf_extract while everything else is ERROR.

## Integration Pattern

```rust
impl LogVerbosity {
    fn filter_string(self) -> String {
        let level = self.as_tracing_level();
        // pdf_extract emits WARN-level glyph messages; clamp to ERROR.
        format!("{level},pdf_extract=error")
    }
}
```

Override at runtime with `RUST_LOG=pdf_extract=warn` when debugging glyph issues.
