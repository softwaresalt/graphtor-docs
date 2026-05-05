---
title: "pdf-extract 0.10: API Usage and Two-Pass Heading-Aware Parsing Pattern"
---

## Context

- PR #13 — PDF ingestion pipeline (007-S): initial `extract_text_from_mem` + page-chunking
- PR #18 — Streaming PDF heading-aware chunking (013-S): two-pass `OutputDev` architecture

## API Overview

### Legacy: extract_text_from_mem (not used in current implementation)

```rust
// Single string with \x0c form-feed page delimiters
let text = pdf_extract::extract_text_from_mem(bytes)?;
for (page_idx, page_text) in text.split('\x0c').enumerate() { /* ... */ }
```

### Legacy per-page API: extract_text_from_mem_by_pages (not used in current implementation)

```rust
// Returns Vec<String> — one String per page, no form-feed delimiters
let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)?;
for (page_idx, page_text) in pages.iter().enumerate() { /* ... */ }
```

### OutputDev trait (custom two-pass processing)

```rust
// Trait signature (all 6 methods required; stroke/fill have defaults)
pub trait OutputDev {
    fn begin_page(&mut self, page_num: u32, media_box: &MediaBox,
                  art_box: Option<(f64, f64, f64, f64)>) -> Result<(), OutputError>;
    fn end_page(&mut self) -> Result<(), OutputError>;
    fn output_character(&mut self, trm: &Transform, width: f64,
                        spacing: f64, font_size: f64, char: &str) -> Result<(), OutputError>;
    fn begin_word(&mut self) -> Result<(), OutputError>;
    fn end_word(&mut self) -> Result<(), OutputError>;
    fn end_line(&mut self) -> Result<(), OutputError>;
    // stroke/fill have default no-op impls
}

// Process all pages — NOTE: not used in current implementation (see below)
pdf_extract::output_doc(&doc, &mut my_output_dev)?;
// Single page — used for both passes in current implementation (output_doc_page loop)
pdf_extract::output_doc_page(&doc, &mut my_output_dev, page_num)?;

// Load document (via lopdf re-export — `pub use lopdf::*`)
let doc = pdf_extract::Document::load_mem(bytes)?;
```

### Transform matrix

```rust
// Transform = euclid::Transform2D<f64, Space, Space>
// Fields: m11, m12, m21, m22, m31 (x), m32 (y) — all public
// Constructor:
pdf_extract::Transform::row_major(m11, m12, m21, m22, m31, m32)

// Compute rendered font size (no euclid dependency needed):
fn rendered_size(trm: &pdf_extract::Transform, font_size: f64) -> f64 {
    let vx = (trm.m11 + trm.m21) * font_size;
    let vy = (trm.m12 + trm.m22) * font_size;
    let product = vx * vy;
    if product > 0.0 { product.sqrt() } else { font_size.abs() }
}
// With identity+translation (scale=1.0): rendered_size == font_size
```

## Two-Pass Architecture

**Pass 1 — `FontSizeHistogram`**: count chars by quantized font size → modal size = body size.
Quantize to nearest 0.5pt: `key = (round(size × 2) / 2 × 10) as u16`.

**Pass 2 — `HeadingAwareOutput`**: processes all pages via `output_doc_page` loop (not
`output_doc`); emit text with y-change line detection; classify lines as H1 (≥ `body × 1.6`),
H2 (≥ `body × 1.3`), or body. Produces `Vec<PdfSection>`. The incremental loop preserves
heading state across page boundaries and allows per-page error attribution.

**Fallback**: if `distinct_sizes ≤ 1` (uniform font), use `PageTextAccumulator` (an `OutputDev`
implementation) via `output_doc_page` loop and chunk by page with `["Page N"]` hierarchy.
(Note: an earlier version used `extract_text_from_mem_by_pages`; the current implementation
uses the `OutputDev` interface throughout for consistency.)

## Error Conditions

| Input | Behaviour |
|---|---|
| Empty `&[u8]` | `Document::load_mem` returns `Err` |
| No `%PDF-` header | `Document::load_mem` returns `Err` |
| Valid PDF, scanned (no text) | Succeeds with zero sections/chunks |
| Valid PDF with mixed fonts | Full two-pass heading detection |
| Valid PDF, uniform font | Fallback to per-page chunking |

## Chunk ID Discriminators

| Path | Chunk ID source format |
|---|---|
| Two-pass (section-based) | `{source_path}#section={N}#segment={M}` |
| Fallback (page-based) | `{source_path}#page={N}#segment={M}` |

Re-sync previously ingested PDFs: `graphtor sync --force`.

## Graph Links

PDF rendering does not preserve hyperlink targets or code block structure.
Always set `references: Vec::new()` and `code_snippets: Vec::new()`.

## Evidence

- PR #13 commit `3ed3460`: initial page-based implementation
- PR #18: two-pass OutputDev architecture — `src/parse/pdf.rs`
- `tests/parse_pdf_test.rs`, `tests/pipeline_pdf_test.rs`
