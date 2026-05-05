---
title: "PDF Pass 2: Page-by-Page Refactor to Enable Heading Detection on Large Files"
date: 2026-05-04
tags: [pdf, parsing, performance, heading-detection]
status: research
stash_id: 56CA4B2E
---

## Problem

`PdfExtractBackend` skips heading-aware extraction for PDFs ≥ 20 MiB
(`LARGE_PDF_THRESHOLD`). This means large PDFs always receive
`heading_hierarchy: ["Page N"]` chunks instead of real section titles,
degrading MCP search result quality.

## Root Cause

The large-file guard exists because **Pass 2** — `HeadingAwareOutput` — uses
`pdf_extract::output_doc`, which is a full O(n-pages) traversal:

```rust
// src/parse/pdf.rs — current Pass 2
let mut heading_output = HeadingAwareOutput::new(body_font_size);
pdf_extract::output_doc(&doc, &mut heading_output)  // all pages, O(n)
```

For a 500-page large PDF where PDFium is unavailable, this is too slow. The
histogram pass is not the bottleneck — it already uses `output_doc_page` in a
bounded 30-page loop and is O(1) with respect to document size.

Notably, the `extract_by_pages` fallback used for large files **also** calls
`output_doc` (all pages), so the large-file guard only avoids the histogram
and heading-aware pass — not the full traversal cost itself.

## Proposed Fix

Refactor Pass 2 to use `output_doc_page` in a loop, mirroring Pass 1:

```rust
// Proposed Pass 2 — O(n) but incremental, same as extract_by_pages
let mut heading_output = HeadingAwareOutput::new(body_font_size);
for page_num in 1..=page_count {
    pdf_extract::output_doc_page(&doc, &mut heading_output, page_num)?;
}
let sections = heading_output.finish();
```

`HeadingAwareOutput` already maintains heading state across pages (`last_h1`,
`last_h2` equivalents), so incremental page-by-page feeding is correct.

## Impact

| Change | Effect |
|---|---|
| Remove `LARGE_PDF_THRESHOLD` guard in `PdfExtractBackend` | Heading-aware extraction runs for all file sizes |
| Remove `LARGE_PDF_THRESHOLD` guard in `parse_pdf_document` | PDFium path no longer needed as a quality workaround for large files; PDFium still useful for performance |
| Chunk IDs for previously-ingested large PDFs change | `#page=` → `#section=` for docs with detectable headings; `sync --force` required |

## Non-Impact

- `PdfiumBackend` remains valuable: it loads documents lazily, avoiding the
  full `Document::load_mem` RAM spike for very large files. The proposed change
  improves quality for the PDFium fallback path; it does not eliminate the
  need for PDFium as a performance optimization.
- The 30-page histogram sample limit stays — it is already appropriate.
- The uniform-font fallback (`HeadingFontDetector` confirmation scan) stays —
  it correctly handles documents with no font-size variation.

## Acceptance Criteria

- [ ] `PdfExtractBackend::parse` removes the `LARGE_PDF_THRESHOLD` fast-path.
- [ ] Pass 2 uses `output_doc_page` in a bounded loop over all pages.
- [ ] Existing unit tests pass unchanged.
- [ ] A regression test confirms a synthetic large-page PDF produces
      section-based chunks, not page-based chunks.
- [ ] `docs/incremental-sync.md` updated to note `sync --force` needed when
      upgrading from a version that used the old large-file path.

## References

- `src/parse/pdf.rs` — `PdfExtractBackend::parse` (lines ~715–799)
- `src/parse/pdf.rs` — `LARGE_PDF_THRESHOLD` constant (line ~57)
- `src/parse/pdf.rs` — `HISTOGRAM_SAMPLE_PAGES` constant (line ~49)
- Prior analysis: session 2026-05-04, "Why can't we perform a histogram from
  the first 30 pages of a PDF regardless of how big it is?"
