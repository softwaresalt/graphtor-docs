---
title: "Streaming PDF parser with heading-aware chunk accumulation"
type: spike
date: 2026-05-03
time_box: "4h"
conclusion: "pivot"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["plan"]
tags:
  - "pdf-ingestion"
  - "parser-architecture"
  - "performance"
  - "streaming"
---

## Goal

Can we replace the current whole-file PDF parsing (`pdf_extract::extract_text_from_mem`)
with a page-streaming architecture that detects heading boundaries via font-size
metadata and emits semantically meaningful chunks — solving both the performance
problem (109 MB PDF takes 45+ minutes in debug mode) and the chunk quality
problem (current chunks are page-aligned, not semantically aligned)?

## Success Criteria

- Determine whether `pdf-extract` or `lopdf` expose per-character font-size metadata
  for heading detection
- Identify a concrete architecture that bounds memory to O(pages) rather than O(document)
  for the text accumulation phase
- Quantify the performance improvement from page-by-page extraction vs whole-document
- Produce a recommendation with enough detail to feed `impl-plan`

## Scope Constraints

- Read-only investigation — no production code changes
- Must use crates already in the dependency tree (`pdf-extract` 0.10, which wraps `lopdf`)
  or justify a new dependency
- Must fit the existing `ParsedDocument` / `Chunk` schema (no schema changes)

## Investigation Approach

1. Audit the current `parse_pdf_document()` implementation and its performance bottleneck
2. Read the `pdf-extract` source code to find page-level and font-level APIs
3. Evaluate the `OutputDev` trait as a hook for font-size-based heading detection
4. Design the streaming architecture
5. Compare approaches and recommend

## Findings

### What Was Discovered

#### 1. Current Implementation Analysis

The current `parse_pdf_document()` in `src/parse/pdf.rs` calls:

```rust
let text = pdf_extract::extract_text_from_mem(bytes)?;
```

This does THREE expensive things at once:

1. **Loads the full PDF** into memory via `lopdf::Document::load_mem(buffer)`
2. **Processes ALL pages** sequentially through the `Processor` and `PlainTextOutput`
3. **Accumulates all text** into a single `String`

The page-based chunking (`chunk_pdf_text`) then splits on `\x0c` form-feed
characters, which is lossy — all font-size information is discarded.

For the 109 MB Azure Cosmos DB ebook, the debug-mode processing spent ~45
minutes in the PDF text extraction phase alone (steps 1-2 above), with memory
growing steadily as text accumulated.

#### 2. `pdf-extract` Already Has Per-Page Extraction

The `pdf-extract` crate (0.10) exposes a public API that was NOT used:

```rust
// Public — extracts text per page, returns Vec<String>
pub fn extract_text_from_mem_by_pages(buffer: &[u8]) -> Result<Vec<String>, OutputError>

// Internal — extracts a single page
fn extract_text_by_page(doc: &Document, page_num: u32) -> Result<String, OutputError>
```

`extract_text_from_mem_by_pages()` still loads the full `Document` object, but
produces per-page strings without the `\x0c` join/split round-trip. This is a
quick win but does not solve the memory or heading-detection problems.

#### 3. The `OutputDev` Trait Exposes Font Size Per Character

The `OutputDev` trait is the key discovery:

```rust
pub trait OutputDev {
    fn begin_page(&mut self, page_num: u32, media_box: &MediaBox, ...) -> Result<(), OutputError>;
    fn end_page(&mut self) -> Result<(), OutputError>;
    fn output_character(&mut self, trm: &Transform, width: f64, spacing: f64,
                        font_size: f64, char: &str) -> Result<(), OutputError>;
    fn begin_word(&mut self) -> Result<(), OutputError>;
    fn end_word(&mut self) -> Result<(), OutputError>;
    fn end_line(&mut self) -> Result<(), OutputError>;
}
```

**`output_character()` receives `font_size`** — the raw font size from the PDF
`Tf` operator. Combined with the transformation matrix `trm`, the *rendered*
font size is computed as:

```rust
let transformed_font_size_vec = trm.transform_vector(vec2(font_size, font_size));
let transformed_font_size = (transformed_font_size_vec.x * transformed_font_size_vec.y).sqrt();
```

This is exactly the information needed for heading detection: a line rendered
at 18pt while body text is 10pt is very likely a heading.

#### 4. Custom `OutputDev` Is the Right Architecture

Rather than post-processing flat text, we can implement a custom `OutputDev`
that detects heading boundaries in real-time during extraction:

```rust
struct HeadingAwareOutput {
    // Font-size tracking for heading detection
    body_font_size: f64,         // dominant font size (body text)
    current_line_font_size: f64, // font size of current line being built
    heading_threshold: f64,      // ratio above body size to consider a heading (e.g., 1.3)

    // Accumulation buffers
    current_line: String,        // characters of the line being built
    current_section: String,     // accumulated section text since last heading
    current_heading: String,     // the heading text that started this section
    heading_hierarchy: Vec<String>,

    // Output
    chunks: Vec<Chunk>,          // completed chunks
    page_num: u32,
    position: usize,
    char_offset: usize,

    // Font-size histogram for body detection
    font_size_counts: HashMap<u16, usize>,  // quantized font-size → char count
}
```

The flow within `output_character()`:

1. Track `transformed_font_size` for each character
2. In `end_word()` / line-break detection: compare the line's dominant font size
   against `body_font_size`
3. If the line is significantly larger (>1.3× body size), treat it as a heading:
   - Flush the current section as a chunk
   - Start a new section with this line as the heading
4. In `end_page()`: emit a form-feed or section boundary marker

#### 5. The Document Object Must Still Be Loaded Fully

PDF is a random-access format — the cross-reference (xref) table at the end
of the file indexes all objects. `lopdf::Document::load_mem()` parses the xref
and builds the object table in memory. There is no way to avoid loading the
full document object.

However, the TEXT ACCUMULATION can be streaming:

| Phase | Current | Proposed |
|-------|---------|----------|
| PDF parse (xref + objects) | `load_mem(bytes)` — full doc in memory | Same — unavoidable |
| Text extraction | All pages → single `String` | Page-by-page via `output_doc_page()` |
| Text accumulation | Single `String` (O(total text)) | Section buffers, flushed per heading (O(section)) |
| Chunking | Post-hoc split on `\x0c` | Real-time emission from `OutputDev` |
| Heading detection | None (pages only) | Font-size heuristic in `output_character()` |

The `Document` object is ~proportional to PDF file size. For a 109 MB PDF,
this is significant. But the text accumulation buffer shrinks from "all pages'
text" to "one section's text" — a meaningful improvement.

#### 6. Performance: Release Mode Is the Immediate Fix

The debug-vs-release performance gap for PDF extraction is enormous. `pdf-extract`
and `lopdf` do heavy computation (decompression, font mapping, coordinate
transforms) that benefits greatly from LLVM optimizations.

**Expected improvement from release mode alone: 10-30×** (typical for
CPU-bound Rust code). This means the 45-minute debug extraction should
complete in 1.5–4.5 minutes in release mode.

The streaming architecture provides additional memory benefits and heading
detection, but release mode is the immediate performance fix.

#### 7. Two-Pass Approach for Body Font-Size Detection

A challenge with heading detection: you need to know what the "body" font size
is before you can detect headings. Two approaches:

**Option A: Two-pass**
1. First pass: build a font-size histogram (quick scan using a lightweight
   `OutputDev` that only tracks font sizes, no text)
2. Determine body font size as the mode (most frequent) of the histogram
3. Second pass: full extraction with heading detection using the body size

**Option B: Adaptive single-pass**
1. Start with a default assumption (e.g., 10-12pt is body)
2. Maintain a running histogram
3. After the first N characters (e.g., 1000), recalculate body size from the
   histogram and retroactively apply to buffered text

Option A is cleaner; Option B avoids double processing. For large PDFs, the
first pass is cheap (no text accumulation), so Option A is recommended.

### What Was Tried and Failed

N/A — this was a research spike. No prototype was built.

### Remaining Unknowns

1. **Font-size clustering quality**: Real PDFs use many font sizes (headings,
   subheadings, captions, footnotes, body). A simple threshold (>1.3× body)
   may over-detect headings. May need 2-3 tiers: H1 (>1.6×), H2 (>1.3×),
   body (<1.3×).

2. **Title pages and front matter**: Ebook-style PDFs often have title pages
   with very large text that is decorative, not a content heading. The first
   few pages may need special handling.

3. **Table of Contents**: Auto-generated TOC pages in PDFs list headings at
   body font size. These could confuse the section accumulator. May need
   a TOC-detection heuristic (sequential lines with trailing page numbers).

4. **Multi-column layouts**: Some PDFs use multi-column layouts. The
   `PlainTextOutput` already handles this via x-coordinate tracking, but
   heading detection across columns is untested.

5. **Performance of two-pass vs single-pass**: The first pass (histogram only)
   should be fast, but "fast" for a 109 MB PDF is still worth measuring.

## Recommendation

**Conclusion**: Pivot
**Confidence**: High

The original proposal assumed we need `lopdf` directly for font metadata.
**We do not.** The existing `pdf-extract` crate already exposes the `OutputDev`
trait with per-character font-size data. The solution is a custom `OutputDev`
implementation — no new crate dependency needed.

### Recommended Architecture

**Phase 1 — Immediate (release mode + per-page API):**
- Switch from `extract_text_from_mem()` to `extract_text_from_mem_by_pages()`
- Process pages in batches (e.g., 50 pages at a time)
- Flush chunks to CozoDB per batch rather than accumulating the full document
- Always build in release mode for PDF-heavy workloads (add `--release` note to docs)

**Phase 2 — Heading-aware chunking (custom `OutputDev`):**
- Implement `HeadingAwareOutput` that detects heading boundaries via font-size
- Two-pass approach: histogram pass → extraction pass with heading detection
- Section-based chunk emission: flush at heading boundaries rather than page boundaries
- Heading hierarchy: H1/H2/H3 tiers based on font-size relative to body

**Phase 3 — Optional enhancements:**
- TOC detection and filtering
- Multi-column layout handling
- Configurable font-size thresholds in `sources.yaml`

### Why Pivot Instead of Proceed

The original stash described "replace with lopdf for random page access." The
spike reveals that `pdf-extract` (which wraps `lopdf`) already provides the
needed APIs:

| Need | `lopdf` directly | `pdf-extract` |
|------|-----------------|---------------|
| Per-page text extraction | Manual: parse xref, decode streams | `extract_text_from_mem_by_pages()` |
| Font-size access | Manual: parse `Tf` operator from content streams | `OutputDev::output_character(font_size)` |
| Text layout (line/word) | Manual: coordinate transforms | `PlainTextOutput` handles this |

Using `pdf-extract`'s `OutputDev` trait gives us font-size data without adding
a dependency or reimplementing text layout. The pivot is: use the existing crate's
advanced API instead of dropping down to `lopdf`.

### Task Decomposition Estimate

1. **T1: Switch to per-page extraction** (~1.5h) — Replace `extract_text_from_mem`
   with `extract_text_from_mem_by_pages`, batch processing, per-batch flush
2. **T2: Font-size histogram pass** (~1h) — Lightweight `OutputDev` that counts
   font sizes, determine body size
3. **T3: HeadingAwareOutput implementation** (~2h) — Custom `OutputDev` with
   heading detection, section accumulation, chunk emission
4. **T4: Two-pass integration** (~1h) — Wire histogram → heading-aware extraction
   in `parse_pdf_document`
5. **T5: Tests and documentation** (~1.5h) — Unit tests for heading detection,
   integration test with real PDF, doc updates

Total: ~7h human-equivalent, decomposed into 5 tasks each under 2h.

## Next Steps

1. Promote to `impl-plan` for detailed implementation planning
2. Create feature in backlog covering the phased approach
3. Phase 1 (per-page + batch) can ship independently as a quick performance win
4. Phase 2 (heading-aware) builds on Phase 1

## References

- `src/parse/pdf.rs` — current implementation (lines 41-61: `parse_pdf_document()`)
- `pdf-extract` 0.10 source: `D:\.cargo\registry\src\...\pdf-extract-0.10.0\src\lib.rs`
  - Line 1876: `OutputDev` trait definition (font_size in `output_character`)
  - Line 2150: `PlainTextOutput` reference implementation
  - Line 2320: `extract_text_from_mem_by_pages()` public API
  - Line 2281: `extract_text_by_page()` internal per-page extraction
  - Line 2386: `output_doc_page()` per-page processing
- `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md` — prior spike (chose pdf-extract)
- `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md` — existing learnings
- `docs/compound/pdf-chunk-id-uniqueness-pattern-2026-05-01.md` — chunk ID pattern
- Stash entry: `17B07B96`
