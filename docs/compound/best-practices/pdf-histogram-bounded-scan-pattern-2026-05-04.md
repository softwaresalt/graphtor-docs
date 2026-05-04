---
title: "Bounding PDF Font-Size Histogram Scan with output_doc_page Loop"
date: 2026-05-04
tags: [rust, pdf, performance, pdf_extract]
---

## Context

When extracting font-size statistics from a PDF for heading detection, the naive approach is to call `pdf_extract::output_doc(&doc, &mut histogram)`. This scans all pages even if the `OutputDev` implementation has an early-return guard in `output_character` — the page traversal loop itself still iterates every page.

## Pattern

Use `output_doc_page` in a bounded loop instead of `output_doc` for any sampling pass:

```rust
let page_count = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
let sample_end = page_count.min(HISTOGRAM_SAMPLE_PAGES);
for page_num in 1..=sample_end {
    pdf_extract::output_doc_page(&doc, &mut histogram, page_num)
        .map_err(|e| GraphtorError::Parse {
            message: format!("pdf font-size scan failed on page {page_num}: {e}"),
            path: Some(source_path.into()),
        })?;
}
```

Keep the guard in `output_character` as defense-in-depth. Use `output_doc_page` + early `break` for scans that should stop when a condition is met (e.g., `HeadingFontDetector`).

## Why This Matters

For a 200-page PDF, `output_doc` traverses all 200 pages even if the character accumulation is a no-op after page 30. The `output_doc_page` loop truly caps the traversal at 30 pages and reduces scan cost proportionally.

## Trap

An `OutputDev` guard (`pages_seen > LIMIT`) in `output_character` does NOT prevent `output_doc` from traversing all pages. It only prevents character accumulation. Page begin/end callbacks still fire for every page.
