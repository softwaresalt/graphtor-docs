---
title: "Switch PDF Pass 2 from output_doc to output_doc_page Loop for Page-Boundary State"
description: "Pattern for using output_doc_page in a per-page loop instead of output_doc to preserve heading state across PDF page boundaries in PdfExtractBackend"
date: 2026-05-05
tags: [rust, pdf, pdf_extract, heading-detection, architecture]
---

## Context

The original `PdfExtractBackend::parse` used `pdf_extract::output_doc(&doc, &mut heading_output)`
for Pass 2 (heading-aware extraction). This works for most PDFs but has two subtle issues:

1. **Heading state is reset between pages** — `begin_page` resets y/x tracking but the
   heading accumulator (`current_heading`, `current_heading_level`) is preserved. However,
   `output_doc` internally calls `begin_page`/`end_page` per page, so state does persist.
   The issue is that `output_doc`'s internal loop is opaque — errors are attributed to the
   whole document rather than to a specific page.

2. **Large-file quality bypass** — a `LARGE_PDF_THRESHOLD` guard routed files ≥ 20 MiB
   directly to `extract_by_pages` (page-based chunking), silently losing heading structure
   for large technical documents.

## Pattern

Replace `output_doc` with an explicit `output_doc_page` loop for both Pass 2 paths:

```rust
// Pass 2 — heading-aware extraction
let mut heading_output = HeadingAwareOutput::new(body_font_size);
for page_num in 1..=page_count {
    pdf_extract::output_doc_page(&doc, &mut heading_output, page_num).map_err(|e| {
        GraphtorError::Parse {
            message: format!("pdf heading-aware extraction failed: {e}"),
            path: Some(source_path.into()),
        }
    })?;
}
let sections = heading_output.finish();

// Uniform-font fallback — per-page chunking
let mut acc = PageTextAccumulator::new();
for page_num in 1..=page_count {
    pdf_extract::output_doc_page(&doc, &mut acc, page_num).map_err(|e| {
        GraphtorError::Parse {
            message: format!("pdf per-page extraction failed: {e}"),
            path: Some(source_path.into()),
        }
    })?;
}
```

Remove any large-file fast-path that bypasses heading detection — quality should not
degrade based on file size.

## Why This Matters

- **Per-page error attribution**: `output_doc_page` returns an error for a specific page
  number; `output_doc` returns a document-level error with no page context.
- **No quality bypass**: all PDFs receive full heading-aware extraction regardless of size.
  The `LARGE_PDF_THRESHOLD` constant should be retained as a performance hint for backend
  selection (PDFium vs pdf-extract) only — never as a quality gate.
- **Testability**: the `output_doc_page` loop can be simulated in unit tests with manual
  `begin_page` / `output_character` / `end_page` calls, enabling regression tests for
  cross-page heading persistence.

## Regression Test

Always add a test that verifies heading state persists across page boundaries:

```rust
#[test]
fn heading_aware_heading_state_persists_across_page_boundaries() {
    let mut output = HeadingAwareOutput::new(10.0);
    // Page 1: emit H1 heading
    output.begin_page(1, &mb, None).unwrap();
    emit_text(&mut output, "Introduction", 18.0, 72.0, 720.0);
    output.end_page().unwrap();
    // Page 2: emit body text
    output.begin_page(2, &mb, None).unwrap();
    emit_text(&mut output, "Body text.", 10.0, 72.0, 600.0);
    output.end_page().unwrap();
    let sections = output.finish();
    // Body section must inherit the H1 heading from page 1
    let chunks = sections_to_chunks(sections, "test.pdf").unwrap();
    assert!(chunks.iter().any(|c| c.heading_hierarchy.contains(&"Introduction".to_string())));
}
```

## Evidence

- PR #28: `feat(pipeline): switch PDF Pass 2 to output_doc_page loop`
- `src/parse/pdf.rs`: `PdfExtractBackend::parse` — both heading-aware and fallback paths
- `tests/parse_pdf_test.rs`: `heading_aware_heading_state_persists_across_page_boundaries`
- Related: `pdf-histogram-bounded-scan-pattern-2026-05-04.md` (same pattern for Pass 1)
