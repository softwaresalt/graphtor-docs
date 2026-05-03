---
title: "Streaming PDF parser with heading-aware chunking"
type: impl-plan
date: 2026-05-03
source: "docs/decisions/2026-05-03-streaming-pdf-heading-aware-spike.md"
feature_id: "022-F"
requires_hardening: false
---

## Problem Frame

The current `parse_pdf_document()` in `src/parse/pdf.rs` calls
`pdf_extract::extract_text_from_mem()` which:

1. Loads the entire PDF into memory
2. Processes ALL pages into a single `String`
3. Joins pages with `\x0c` form-feed delimiters
4. Discards all font-size metadata

The subsequent `chunk_pdf_text()` splits on `\x0c` to produce page-aligned
chunks with `heading_hierarchy: ["Page N"]` — no actual heading detection.

For large PDFs (109 MB Cosmos DB ebook), debug-mode extraction takes 45+
minutes and provides no semantic structure. The spike
(`2026-05-03-streaming-pdf-heading-aware-spike.md`) confirmed that
`pdf-extract` 0.10 already exposes:

- `extract_text_from_mem_by_pages()` — per-page text extraction
- `OutputDev` trait with `output_character(font_size)` — per-character
  font-size metadata
- `output_doc_page()` — single-page processing

This plan implements the spike recommendation in two phases: Phase 1
(per-page batch extraction) and Phase 2 (custom `OutputDev` with
heading-aware chunking).

## Requirements Trace

| Spike Requirement | Implementation Action |
|---|---|
| Per-page text extraction | Unit 1: Switch to `extract_text_from_mem_by_pages()` |
| Font-size histogram for body detection | Unit 2: Lightweight `FontSizeHistogram` `OutputDev` |
| Heading detection via font-size | Unit 3: `HeadingAwareOutput` `OutputDev` |
| Two-pass integration | Unit 4: Wire histogram → heading-aware in `parse_pdf_document` |
| Tests and documentation | Unit 5: Tests, compound learning, doc updates |

## Implementation Units

### Unit 1: Switch to Per-Page Extraction

**Posture**: Test-first

**What changes:**
Replace `extract_text_from_mem()` with `extract_text_from_mem_by_pages()` in
`parse_pdf_document()`. Each page's text arrives as a separate `String`,
eliminating the `\x0c` split/join round-trip. Chunk processing changes from
split-on-formfeed to iterate-over-pages.

**Files affected:**
- `src/parse/pdf.rs` — rewrite `parse_pdf_document()` and `chunk_pdf_text()`

**Tests:**
1. Existing unit tests in `src/parse/pdf.rs` must continue to pass (behavior
   should be identical for the simple cases)
2. New test: verify per-page extraction produces same chunk count as
   form-feed splitting for multi-page input
3. Integration tests in `tests/parse_pdf_test.rs` and
   `tests/pipeline_pdf_test.rs` must pass unchanged

**Execution notes:**
- `chunk_pdf_text()` currently takes `&str` (single text blob). Change
  signature to accept `&[String]` (per-page text). Rename to
  `chunk_pdf_pages()` for clarity.
- Preserve the existing `split_long_text()` logic for pages exceeding
  `MAX_CHUNK_CHARS`.
- Chunk ID generation stays the same: `{source_path}#page={N}#segment={M}`.
- `heading_hierarchy` stays `["Page N"]` in this unit (heading detection
  comes in Unit 3).

**Estimate:** <2 hours

---

### Unit 2: Font-Size Histogram OutputDev

**Posture**: Test-first

**What changes:**
Implement a lightweight `FontSizeHistogram` struct that implements
`pdf_extract::OutputDev`. This first-pass scanner counts characters per
quantized font size (rounded to nearest 0.5pt) without accumulating text.
Provides `body_font_size()` returning the mode (most frequent size).

**Files affected:**
- `src/parse/pdf.rs` — add `FontSizeHistogram` struct and `OutputDev` impl

**Tests:**
1. Unit test: histogram with mixed font sizes returns correct mode
2. Unit test: histogram with single font size returns that size
3. Unit test: empty histogram returns a reasonable default (e.g., 10.0)
4. Unit test: quantization groups sizes within 0.5pt

**Execution notes:**
- `OutputDev` trait is public in `pdf_extract`. We implement it in our crate.
- Only `output_character()` needs a real implementation (track font size).
  All other trait methods (`begin_page`, `end_page`, `begin_word`,
  `end_word`, `end_line`) are no-ops.
- Font size quantization: `(font_size * 2.0).round() / 2.0` — maps
  9.8pt and 10.2pt both to 10.0pt.
- Uses `HashMap<u16, usize>` where key is `(quantized * 10) as u16` to
  avoid float keys.

**Estimate:** <1.5 hours

---

### Unit 3: HeadingAwareOutput OutputDev

**Posture**: Test-first

**What changes:**
Implement `HeadingAwareOutput` struct implementing `OutputDev`. This is the
core heading-detection engine. Given a known `body_font_size` (from Unit 2),
it:

1. Accumulates text character-by-character in `output_character()`
2. Tracks the dominant font size per line
3. Detects heading boundaries when a line's font size exceeds the body
   size by configurable thresholds:
   - H1: ≥1.6× body size
   - H2: ≥1.3× body size
   - Body: <1.3× body size
4. At heading boundaries: flushes the current section as chunks, starts
   a new section with the heading text
5. At `end_page()`: respects page boundaries as potential section breaks

**Output**: A `Vec<PdfSection>` where each section has:
- `heading: Option<String>` — the heading text (None for intro sections)
- `heading_level: u8` — 1, 2, or 0 (body)
- `content: String` — section body text
- `page_start: usize` — first page of this section

**Files affected:**
- `src/parse/pdf.rs` — add `HeadingAwareOutput`, `PdfSection`, and
  supporting types

**Tests:**
1. Section with large font followed by small font produces heading + body
2. Multiple heading levels detected correctly (H1 vs H2)
3. Section without heading (document intro) produces section with
   `heading: None`
4. Long section split at `MAX_CHUNK_CHARS` boundary
5. Empty pages produce no spurious sections

**Execution notes:**
- Line detection heuristic: track y-coordinate changes between characters.
  When y changes significantly (or `end_line()` is called), finalize the
  current line and check its font size against thresholds.
- Word spacing: `begin_word()` / `end_word()` insert space characters.
- The `Transform` matrix parameter in `output_character()` provides the
  rendered position. Compute `transformed_font_size` using the geometric
  mean formula from `PlainTextOutput`:
  ```rust
  let v = trm.transform_vector(vec2(font_size, font_size));
  let rendered_size = (v.x * v.y).sqrt();
  ```

**Estimate:** <2 hours

---

### Unit 4: Two-Pass Integration

**Posture**: Test-first

**What changes:**
Wire Units 2 and 3 into `parse_pdf_document()`:

1. Load the PDF document via `lopdf::Document::load_mem()` (re-exported
   by `pdf_extract` as `pdf_extract::Document`)
2. **Pass 1**: Run `FontSizeHistogram` over all pages via
   `output_doc_page()` to determine `body_font_size`
3. **Pass 2**: Run `HeadingAwareOutput` over all pages with the known
   body font size to produce `Vec<PdfSection>`
4. Convert sections to `Vec<Chunk>` with proper `heading_hierarchy`
5. Extract title from the first heading or first meaningful line

**Files affected:**
- `src/parse/pdf.rs` — rewrite `parse_pdf_document()` to use two-pass
  approach, replace `chunk_pdf_text()` / `chunk_pdf_pages()` with
  section-to-chunk conversion

**Tests:**
1. Integration test: parse a multi-page PDF with known heading structure,
   verify chunk heading hierarchies
2. Fallback test: when no heading-sized text is detected (all same font
   size), fall back to page-based chunking (Unit 1 behavior)
3. Existing integration tests in `tests/parse_pdf_test.rs` must pass

**Execution notes:**
- When `FontSizeHistogram` detects only one font size (all text same
  size), heading detection is impossible. Fall back to page-based
  chunking with `["Page N"]` hierarchy — identical to Unit 1 output.
- Section-to-chunk conversion:
  - Short sections (<`MAX_CHUNK_CHARS`): one chunk per section
  - Long sections: split at paragraph boundaries via existing
    `split_long_text()`
  - `heading_hierarchy`: `["H1 text"]` or `["H1 text", "H2 text"]`
    depending on nesting
- `chunk_id_source`: `{source_path}#section={section_idx}#segment={seg_idx}`
  (changes from page-based to section-based discriminator)

**Estimate:** <2 hours

---

### Unit 5: Tests, Learnings, Documentation

**Posture**: Documentation

**What changes:**
1. Update compound learning `pdf-extract-api-usage-pattern-2026-05-01.md`
   with the new per-page and `OutputDev` usage patterns
2. Write new compound learning for heading detection heuristics
3. Update the doc comment on `parse_pdf_document()` to document the
   two-pass approach
4. Verify all existing tests pass (regression check)

**Files affected:**
- `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md` — update
- `docs/compound/` — new learning file for heading detection
- `src/parse/pdf.rs` — doc comment updates only

**Tests:**
- No new test code. Run full `cargo test` regression check.

**Estimate:** <1 hour

## Dependency Graph

```text
Unit 1 (per-page extraction)
    ↓
Unit 2 (font-size histogram)  ← independent of Unit 1, but logical progression
    ↓
Unit 3 (HeadingAwareOutput)   ← requires Unit 2 for body_font_size input
    ↓
Unit 4 (two-pass integration) ← requires Units 1, 2, 3
    ↓
Unit 5 (docs + learnings)     ← requires Unit 4 complete
```

Units 1 and 2 could be developed in parallel (no code dependency), but
Unit 2 logically follows Unit 1 as the foundation for heading detection.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Use `pdf_extract::OutputDev` instead of raw `lopdf` | Spike confirmed `OutputDev` already provides font_size per character. Using it avoids reimplementing text layout (word detection, line breaks, coordinate transforms) that `PlainTextOutput` already handles. |
| Two-pass approach over adaptive single-pass | Cleaner design: first pass is cheap (no text accumulation), second pass has accurate body_font_size from the start. Avoids retroactive re-classification of buffered text. |
| Three heading tiers (H1 ≥1.6×, H2 ≥1.3×, body <1.3×) | Matches common PDF typography: chapter titles ~18pt, section headers ~14pt, body ~10-12pt. Configurable thresholds can be added later. |
| Keep page-based fallback | PDFs with uniform font size (e.g., plain text exports) have no heading structure to detect. Page-based chunking is the correct fallback, not an error. |
| Section-based chunk IDs instead of page-based | Sections can span pages. Using `section_idx + segment_idx` as the discriminator produces stable IDs aligned to semantic boundaries. This is a BREAKING CHANGE to existing chunk IDs for any previously ingested PDFs — see Risks. |
| All changes in `src/parse/pdf.rs` only | The `Chunk` and `ParsedDocument` types are unchanged. The pipeline dispatch in `pipeline/mod.rs` calls `parse_pdf_document()` and receives the same `ParsedDocument` — no changes needed upstream. |

## Risks and Caveats

| Risk | Severity | Mitigation |
|---|---|---|
| **Chunk ID instability**: Section-based IDs differ from page-based IDs for the same PDF. Previously ingested PDFs will produce duplicate chunks under new IDs. | Medium | Document in release notes. Users must re-sync (`graphtor sync --force`) to rebuild chunk store. This is acceptable for v0.x — no production deployments exist yet. |
| **Font-size heuristic false positives**: Some PDFs use large fonts for decorative elements (title pages, pull quotes) that are not headings. | Low | The fallback to page-based chunking when all font sizes are similar limits the blast radius. Heuristic thresholds can be tuned per-source in future work. |
| **`OutputDev` trait stability**: `pdf-extract` 0.10 is the latest release. The `OutputDev` trait is public API but may change in future versions. | Low | Pin `pdf-extract = "0.10"` in `Cargo.toml`. The trait has been stable across 0.7–0.10. |
| **Performance of two-pass**: Processing the document twice (histogram + extraction) adds overhead vs single-pass. | Low | First pass (histogram only) is much cheaper than text extraction — no string accumulation, no word/line detection. Expected overhead: <10% of total PDF processing time. |
| **`lopdf` re-export availability**: The spike found `pdf_extract` re-exports `lopdf` types (`pub use lopdf::*`). If this re-export is removed in a future version, we would need to add `lopdf` as a direct dependency. | Low | Currently works. If broken, adding `lopdf` explicitly is trivial. |

## Plan Hardening Signals

- **Public API, schema, or contract change**: No — `ParsedDocument` and `Chunk` types unchanged. Internal function signatures change but these are `pub(crate)` or private.
- **Security, auth, permission, or compliance-sensitive behavior**: No.
- **Migration, backfill, destructive data/config action, or irreversible step**: Chunk IDs change for PDFs → requires re-sync. Not irreversible (re-sync rebuilds).
- **External integration, operator checkpoint, or external dependency**: No new crate dependencies. Uses existing `pdf-extract` 0.10.
- **High runtime, rollout, or rollback risk**: No — rollback is revert the code change. Chunk store rebuild via `graphtor sync --force`.

**Requires plan hardening: no**

## Runtime Verification and Closure

### Affected Runtime Surfaces

1. **CLI `graphtor sync` command** — PDF parsing produces different chunks
   (section-based instead of page-based). Users see richer
   `heading_hierarchy` in search results.

2. **MCP tool `search_docs`** — search results for PDF-sourced chunks will
   include heading context instead of "Page N".

### Verification Approach

1. Build in release mode: `cargo build --release`
2. Ingest the Cosmos DB PDF (`tmp/azure-cosmos-db.pdf`) via
   `graphtor sync --config logs/cosmos-only.yaml`
3. Verify chunks have heading hierarchies beyond just "Page N"
4. Verify search results show section-level context
5. Measure time: release-mode ingestion should complete in <5 minutes
   (vs 45+ minutes in debug mode)

### Closure

- No monitoring needed (local-only tool)
- Rollback: revert to previous `parse_pdf_document()` + re-sync
- No ownership transfer needed

## Plan Review

**Gate decision: PASS**

Reviewed 2026-05-03 by four always-on personas. No P0 or P1 findings.
Plan is approved for harvest.

### Constitution Reviewer

No findings. Plan satisfies all five core principles:

- **Local-First**: No cloud dependencies added. All processing remains
  in-process via `pdf-extract`'s `OutputDev` trait.
- **Lightweight Footprint**: No new crate dependencies. Uses existing
  `pdf-extract` 0.10 public API.
- **Data Pipeline Integrity**: Chunk IDs use stable discriminators
  (`section_idx + segment_idx`). Chunk ID format change is documented
  as a known breaking change with mitigation (re-sync).
- **MCP-Native Interface**: No MCP tool changes. `ParsedDocument` and
  `Chunk` types are unchanged.
- **Automation & Reproducibility**: Two-pass approach is deterministic
  and idempotent. Same PDF → same chunks.

### Rust Reviewer

**P2 — Verify `OutputDev` trait method signatures match pdf-extract 0.10**

The plan references `output_character(&mut self, trm: &Transform, width: f64,
spacing: f64, font_size: f64, char: &str)` from the spike investigation. The
implementer should verify these signatures against the actual `pdf-extract`
0.10 API at build time. If the trait has default implementations for methods
like `begin_word()`/`end_word()`, only override the ones needed.

*Recommendation*: Record as implementation-time verification. Not blocking.

**P3 — Consider extracting `HeadingAwareOutput` to a submodule**

Unit 3 adds significant complexity to `src/parse/pdf.rs`. If the file grows
beyond ~400 lines, consider extracting the `OutputDev` implementations to
`src/parse/pdf_heading.rs` as a sibling module.

*Recommendation*: Advisory. Assess after implementation.

### Scope Boundary Auditor

No findings. All five units are well-scoped:

- Units 1-4 touch only `src/parse/pdf.rs` — single-file scope.
- Unit 5 is documentation-only — appropriate separation.
- No pipeline, database, MCP, or schema changes.
- Each unit satisfies the 2-hour rule and width isolation constraint.
- The dependency graph is acyclic with clear sequencing.

### Learnings Researcher

**P3 — Chunk ID uniqueness pattern applies to section-based IDs**

The compound learning `pdf-chunk-id-uniqueness-pattern-2026-05-01.md`
documents why page+segment indices are required in chunk ID discriminators.
The plan correctly preserves this pattern by switching to
`section_idx + segment_idx`. The implementer should ensure that sections
with identical heading text on different pages still produce unique chunk
IDs (the section index handles this, same as the page index did before).

*Recommendation*: Awareness only. Pattern is correctly applied.

**P3 — Path normalization is unaffected**

The compound learning `windows-path-normalization-for-chunk-ids-2026-05-01.md`
is relevant but requires no plan changes. Path normalization happens in
`pipeline/mod.rs` before calling `parse_pdf_document()`, so the PDF parser
always receives a forward-slash-normalized `source_path`.

### Summary

| Severity | Count | Action |
|---|---|---|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 1 | Record as implementation-time check |
| P3 | 3 | Advisory — no action required |

**Plan hardening required**: No (confirmed — no hardening signals present).

**Approved for harvest.**
