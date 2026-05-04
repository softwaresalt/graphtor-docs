---
title: "Confirming Uniform-Looking Sample Before Fallback with HeadingFontDetector"
date: 2026-05-04
tags: [rust, pdf, algorithm, sampling]
---

## Context

When sampling only the first N pages of a document to detect font-size variety (for heading detection), a sample that looks uniform (all body text, one font size) does not mean the whole document is uniform. Pages beyond the sample window may contain headings.

## Pattern

When a sampled histogram shows `distinct_sizes <= 1`, confirm by scanning the remaining pages with a lightweight `HeadingFontDetector` before committing to the page-based fallback:

```rust
let really_uniform = if distinct_sizes <= 1 && page_count > HISTOGRAM_SAMPLE_PAGES {
    let threshold = body_font_size * H2_RATIO;
    let mut detector = HeadingFontDetector::new(threshold);
    for page_num in (HISTOGRAM_SAMPLE_PAGES + 1)..=page_count {
        pdf_extract::output_doc_page(&doc, &mut detector, page_num)?;
        if detector.found_heading {
            break; // stop as soon as any heading is found
        }
    }
    !detector.found_heading
} else {
    distinct_sizes <= 1
};
```

`HeadingFontDetector` is cheap: no string accumulation, just one float comparison per character, with an early break.

## Why This Matters

A document with 30 pages of title/abstract/bibliography followed by 150 pages of section-structured content would be incorrectly chunked as page-based without this confirmation scan. The heading structure after page 30 would be lost.

## Trade-off

The confirmation scan adds at most one full pass over the remaining pages in the false-positive case. For truly uniform documents (entire document has no headings), this scan runs to completion but correctly confirms the fallback. For most structured documents, the scan short-circuits at the first heading.
