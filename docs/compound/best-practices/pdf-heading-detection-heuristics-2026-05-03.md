---
title: "PDF Heading Detection via Font-Size Histogram and OutputDev"
---

## Context

PR #18 — `feat/streaming-pdf-heading-aware` (013-S). Replaces page-based PDF chunking with
section-based chunking using `pdf-extract 0.10`'s `OutputDev` trait.

## Core Heuristic: Two-Pass Font-Size Ratio Threshold

```
H1_RATIO = 1.6   // rendered_size >= body_size × 1.6 → H1
H2_RATIO = 1.3   // rendered_size >= body_size × 1.3 → H2
```

The thresholds work because PDF typographic conventions reliably distinguish body text (10–11pt)
from section headings (13–14pt, ~1.3×) and chapter headings (16–18pt, ~1.6×).

## Pass 1: Font-Size Histogram

Quantize all rendered character sizes to the nearest 0.5pt and count characters per bucket.
The mode (most-frequent bucket) is the body font size.

```rust
fn quantize(font_size: f64) -> u16 {
    let quantized = (font_size * 2.0).round() / 2.0;
    let key = (quantized * 10.0).round();
    key.clamp(0.0, f64::from(u16::MAX)) as u16
}
fn body_font_size(&self) -> f64 {
    self.counts.iter()
        .max_by_key(|(_, &count)| count)
        .map_or(10.0, |(&key, _)| f64::from(key) / 10.0)
}
```

Quantizing to 0.5pt buckets groups glyphs at 9.8pt and 10.2pt into the same bucket, handling
minor float variation in the PDF renderer.

## Pass 2: HeadingAwareOutput Line Detection

Line boundaries are detected by y-coordinate change:

```rust
if !self.last_y.is_nan() && (raw_y - self.last_y).abs() > size * 0.5 {
    self.flush_line();
}
```

Threshold `size × 0.5` distinguishes new lines (full line-height ~1.2em) from sub-pixel drift
within the same line. Values below 0.3× produce false positives; above 0.8× miss close lines.

## Fallback: Uniform Font Documents

When `distinct_sizes ≤ 1` (all text in one size bucket), headings cannot be detected. Fall back
to `extract_text_from_mem_by_pages` with `["Page N"]` hierarchy.

```rust
let distinct_sizes = histogram.counts.len();
if distinct_sizes <= 1 {
    // fallback: per-page chunking
}
```

This covers: books with no visual heading differentiation, scanned-then-OCR'd PDFs, and
developer-generated PDFs with uniform monospace fonts.

## Test Construction Helper

With a pure-scaling matrix (scale=1.0, x, y), `rendered_size(trm, font_size) == font_size`
exactly. Use this in tests to set known rendered sizes without importing euclid:

```rust
fn make_trm(scale: f64, x: f64, y: f64) -> pdf_extract::Transform {
    pdf_extract::Transform::row_major(scale, 0.0, 0.0, scale, x, y)
}
```

## Float Comparison in Tests

`clippy::float_cmp` fires on `assert_eq!(float, literal)`. For font-size comparisons where the
values are exact (produced by the quantize formula on round numbers), add:

```rust
#[allow(clippy::float_cmp)]
fn test_histogram_returns_10pt() { /* ... */ }
```

Do not use this exemption in production code.

## Trait Method Visibility in Tests

`OutputDev` methods are not in scope by default in a `#[cfg(test)]` module. Add:

```rust
use pdf_extract::OutputDev as _;
```

This imports the trait for method resolution without polluting the test namespace.

## Evidence

- `src/parse/pdf.rs` — `FontSizeHistogram`, `HeadingAwareOutput`, `sections_to_chunks`
- `docs/decisions/2026-05-03-streaming-pdf-heading-aware-spike.md`
- `docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-03-streaming-pdf-heading-aware-plan.md`
