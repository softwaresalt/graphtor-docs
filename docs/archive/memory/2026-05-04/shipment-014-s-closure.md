---
type: session-memory
timestamp: 2026-05-04T00:00:00Z
agent: ship
shipment: 014-S
---

## Shipment 014-S — Large PDF Ingestion Performance Optimization

### Outcome

Merged as commit `c32eb19` via PR #23.

### Changes Delivered

- `src/parse/pdf.rs` — Complete optimization rewrite:
  - `FontSizeHistogram`: font-size sampling OutputDev, quantized to 0.5pt buckets
  - `PageTextAccumulator`: per-page text collection OutputDev (replaces extract_text_from_mem_by_pages)
  - `HeadingAwareOutput`: heading-aware section extractor with H1/H2 classification
  - `HeadingFontDetector`: lightweight uniformity confirmation scan
  - `PdfExtractBackend`: orchestrator for all three passes
  - `LARGE_PDF_THRESHOLD = 20 MiB`, `HISTOGRAM_SAMPLE_PAGES = 30`
  - Histogram bounded via `output_doc_page` loop (not `output_doc`)
  - Uniformity confirmed with `HeadingFontDetector` before page-based fallback
  - 57 unit tests in `#[cfg(test)]` module
- `src/logging/init.rs` — `EnvFilter`-based logging with `pdf_extract=error` noise suppression
- `src/bin/pdf_parse_timing.rs` — Benchmarking binary with zero-chunk division guard

### PR Review Fixes

Three P1 correctness bugs caught by Copilot review and fixed:
1. `output_doc` → `output_doc_page` loop for bounded histogram scan
2. `HeadingFontDetector` confirmation scan before `distinct_sizes <= 1` fallback
3. `pdf_extract=error` (not `warn`) to actually suppress WARN glyph noise

### Key Learnings

- `output_doc_page` loop + break is the only way to truly short-circuit page traversal
- `crate=warn` in EnvFilter shows WARN, not suppresses it — use `crate=error`
- Sample-then-confirm pattern for document structure detection

### Follow-up Items

None outstanding. All P1 and P2 findings addressed.
