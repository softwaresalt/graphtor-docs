---
title: "Large PDF Ingestion Performance Optimization"
source: "docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md"
feature_id: "023-F"
date: 2026-05-03
---

## Problem Frame

The current PDF ingestion pipeline (`src/parse/pdf.rs`) works correctly for
small documents but scales poorly on large PDFs:

- A 6.3 MB PDF takes **91.7 seconds** (release mode, `--no-embed`).
- A 104.7 MB PDF fails to complete parsing within 9+ minutes.

Root causes identified by the spike:

1. **Double parse on fallback**: The uniform-font fallback path calls
   `extract_text_from_mem_by_pages(bytes)` which internally calls
   `Document::load_mem` again while the first `Document` is still alive.
2. **Full-document histogram scan**: Pass 1 scans _every_ page with `output_doc()`
   just to determine the body font size — overkill for large documents.
3. **Warning spam**: `pdf_extract` emits thousands of duplicate WARN-level
   glyph messages (19,606 in the benchmark) that compete for stderr I/O and
   contribute to wall-clock time.
4. **No large-file bypass**: There is no heuristic to skip the expensive
   two-pass flow for very large files where page-oriented chunking is acceptable.
5. **No backend abstraction**: The `pdf_extract` API is called directly throughout
   `parse_pdf_document()` — any future library migration requires rewriting the
   entire entry point.

All optimization targets are in `src/parse/pdf.rs`; the pipeline dispatch in
`src/pipeline/mod.rs` passes bytes and receives `ParsedDocument` unchanged.

## Requirements Trace

| Spike Recommendation | Implementation Action |
|---|---|
| 1. Eliminate fallback reparse | Unit 1: Page-accumulator OutputDev |
| 2. Large-file heuristic | Unit 3: Size-gate before two-pass flow |
| 3. Suppress glyph warnings | Unit 2: Tracing env-filter for pdf_extract |
| 4. Histogram sampling (first N pages) | Unit 4: Sampled histogram via output_doc_page |
| 5. PdfBackend trait boundary | Unit 5: Trait abstraction in src/parse/pdf.rs |

## Implementation Units

### Unit 1: Eliminate Fallback Reparse

**Goal**: Replace `extract_text_from_mem_by_pages(bytes)` with a lightweight
`PageTextAccumulator` OutputDev that runs on the already-loaded `Document`.

**Files affected**:
- `src/parse/pdf.rs` — add `PageTextAccumulator` struct, remove `bytes` dependency
  from fallback branch

**Changes**:
1. Add a `PageTextAccumulator` implementing `OutputDev` that collects per-page
   text strings (one `String` per page boundary via `begin_page`/`end_page`).
2. Replace the fallback branch at line 535 with
   `output_doc(&doc, &mut page_accumulator)?` using the already-loaded `doc`.
3. Feed the accumulated `Vec<String>` to the existing `chunk_pdf_pages()` and
   `extract_title_from_pages()`.

**Tests**:
- Existing `parse_pdf_test.rs` error-case tests continue to pass unchanged.
- Add a new test: `fallback_path_produces_same_chunks_as_before` using a synthetic
  uniform-font PDF (single font size, multiple pages).
- Add a test: `fallback_does_not_double_load` by asserting the function signature
  no longer needs the raw `bytes` for the fallback path (compile-time guarantee:
  remove `bytes` from the fallback closure scope).

**Execution posture**: Test-first. Write characterization test capturing current
fallback output, then refactor, confirm output unchanged.

**Dependency**: None (first unit).

---

### Unit 2: Suppress Repeated Glyph Warnings

**Goal**: Prevent `pdf_extract` from flooding stderr with thousands of identical
WARN messages about unknown glyph names.

**Files affected**:
- `src/main.rs` — adjust tracing subscriber initialization to filter `pdf_extract`
  module at `error` level (suppress WARN)
- `src/parse/pdf.rs` — no changes needed

**Changes**:
1. In the `tracing-subscriber` `EnvFilter` initialization (or layer configuration),
   add a directive: `pdf_extract=error` as a default filter alongside the existing
   base level.
2. This suppresses WARN and INFO from the `pdf_extract` crate while preserving
   ERROR-level messages for genuine failures.
3. Document in the module doc comment that `pdf_extract` glyph warnings are
   suppressed by default and can be re-enabled via `RUST_LOG=pdf_extract=warn`.

**Tests**:
- Integration test: run a PDF parse, assert no WARN lines appear in captured
  tracing output (use `tracing-subscriber` test utilities or a custom Layer).
- Verify existing tests still pass (no behavioral change to parsing logic).

**Execution posture**: Test-first. Capture current WARN output count, apply filter,
confirm count drops to zero.

**Dependency**: None (independent of Unit 1).

---

### Unit 3: Large-File Heuristic

**Goal**: Skip the expensive two-pass heading-aware flow for PDFs above a
configurable size threshold, using single-pass page-oriented extraction instead.

**Files affected**:
- `src/parse/pdf.rs` — add size check at top of `parse_pdf_document()`

**Changes**:
1. Add a module-level constant: `const LARGE_PDF_THRESHOLD: usize = 20_000_000;`
   (20 MB).
2. At the top of `parse_pdf_document()`, before `Document::load_mem()`, check
   `bytes.len()`. If `>= LARGE_PDF_THRESHOLD`, use the `PageTextAccumulator`
   from Unit 1 directly (single pass, page-oriented chunking) and skip the
   histogram entirely.
3. Log an info-level message when the heuristic triggers:
   `tracing::info!(size_bytes = bytes.len(), threshold = LARGE_PDF_THRESHOLD, "large PDF detected; using page-oriented chunking")`.

**Tests**:
- Unit test: verify that a byte slice of length `LARGE_PDF_THRESHOLD` triggers
  page-oriented mode (mock or minimal PDF).
- Unit test: verify that a byte slice just below threshold still enters two-pass
  mode.
- Integration test with the 6.3 MB benchmark PDF (below threshold → still uses
  heading-aware path, no regression).

**Execution posture**: Test-first. The large-file test needs a PDF fixture ≥20 MB
or a mock; use a small synthetic PDF with artificially inflated byte length for
the unit test, plus the real 6.3 MB fixture for characterization.

**Dependency**: Requires Unit 1 (`PageTextAccumulator`).

---

### Unit 4: Histogram Sampling (First N Pages)

**Goal**: For PDFs below the large-file threshold but with many pages, sample only
the first N pages for the font-size histogram instead of scanning the entire
document.

**Files affected**:
- `src/parse/pdf.rs` — replace `output_doc(&doc, &mut histogram)` with a loop
  over `output_doc_page()` for the first N pages

**Changes**:
1. Add constant: `const HISTOGRAM_SAMPLE_PAGES: u32 = 30;`.
2. Replace the full-document histogram pass with:
   ```rust
   let page_count = doc.get_pages().len() as u32;
   let sample_end = page_count.min(HISTOGRAM_SAMPLE_PAGES);
   for page_num in 1..=sample_end {
       pdf_extract::output_doc_page(&doc, &mut histogram, page_num)?;
   }
   ```
3. The `HeadingAwareOutput` pass (Pass 2) still scans all pages with
   `output_doc()` to ensure no content is lost — only the histogram is sampled.
4. Document the trade-off: sampling assumes the first 30 pages are representative
   of the document's typography. This is safe for technical documentation PDFs
   (which use consistent typography throughout) but could misdetect body size in
   documents that change font mid-way.

**Tests**:
- Unit test: verify histogram result from first 30 pages matches full-document
  histogram for the 6.3 MB benchmark PDF (regression guard).
- Unit test: verify `output_doc_page` is called at most `HISTOGRAM_SAMPLE_PAGES`
  times (use a counting wrapper or page-count assertion in the histogram).
- Edge case: PDF with fewer than 30 pages still works correctly.

**Execution posture**: Test-first. Characterization test captures current histogram
result, then refactoring to sampling must produce the same body_font_size.

**Dependency**: None (independent, but logically after Unit 1).

---

### Unit 5: PdfBackend Trait Boundary

**Goal**: Introduce a `trait PdfBackend` so the parsing entry point depends on an
abstraction rather than directly on `pdf_extract` API calls.

**Files affected**:
- `src/parse/pdf.rs` — define `PdfBackend` trait, implement for `PdfExtractBackend`
- `src/parse/mod.rs` — re-export trait if needed for testing

**Changes**:
1. Define:
   ```rust
   pub(crate) trait PdfBackend {
       fn load(&self, bytes: &[u8]) -> Result<PdfDocument, GraphtorError>;
   }
   pub(crate) trait PdfDocument {
       fn page_count(&self) -> u32;
       fn scan_histogram(&self, max_pages: u32) -> Result<FontSizeHistogram, GraphtorError>;
       fn extract_headings(&self, body_font_size: f64) -> Result<Vec<PdfSection>, GraphtorError>;
       fn extract_pages(&self) -> Result<Vec<String>, GraphtorError>;
   }
   ```
2. Implement `PdfExtractBackend` (wrapping the current `pdf_extract` calls).
3. Refactor `parse_pdf_document()` to accept `&dyn PdfBackend` (or use a
   module-level default for the production path).
4. The public API signature remains unchanged — the trait is `pub(crate)` and
   the default backend is wired internally.

**Tests**:
- All existing PDF tests continue to pass unchanged (they call the public API).
- Add a mock backend test that verifies the parsing logic independently of
  `pdf_extract` (e.g., mock that returns known sections, verify chunking).
- Verify the trait is `pub(crate)` — no public API change.

**Execution posture**: Test-first. Existing tests serve as regression guards; new
mock-backend test verifies the abstraction layer.

**Dependency**: Logically last — all other units should be implemented first so
the trait boundary wraps the optimized code, not the pre-optimization code.

## Dependency Graph

```
Unit 1 (fallback reparse) ─┐
                            ├──→ Unit 3 (large-file heuristic)
Unit 2 (warning suppression)│
                            │
Unit 4 (histogram sampling) │
                            │
                            └──→ Unit 5 (PdfBackend trait)
```

Execution order: Units 1, 2, 4 are independent and can be done in any order.
Unit 3 depends on Unit 1 (reuses `PageTextAccumulator`). Unit 5 is last
(wraps the optimized implementation).

Recommended sequence: **1 → 2 → 4 → 3 → 5**.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Use `PageTextAccumulator` instead of word-level accumulation | Matches existing `chunk_pdf_pages()` contract (Vec<String> per page); minimal change surface |
| Filter at `pdf_extract=error` level, not per-message dedup | Simpler, zero runtime cost, covers all noisy messages; users can re-enable via RUST_LOG |
| 20 MB threshold for large-file heuristic | Spike showed 6.3 MB took 91s; 20 MB is ~3× that size, giving two-pass a chance on medium docs while protecting against 100+ MB pathology |
| Sample 30 pages for histogram | Most technical PDFs establish typography in first few pages; 30 is generous safety margin covering TOC + early chapters |
| Trait is `pub(crate)`, not `pub` | No external consumers need the abstraction; keeps public API surface unchanged |
| Unit 5 is last | Avoids premature abstraction; trait wraps the optimized, tested implementation |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| `PageTextAccumulator` produces slightly different text than `extract_text_from_mem_by_pages` | Characterization test compares output before/after; both use the same underlying glyph processing |
| Histogram sampling misidentifies body size for documents with late font changes | 30-page sample is generous; add a compound learning noting the limitation |
| 20 MB threshold may be too high or too low for some workloads | Make it a constant that's easy to tune; document as a knob in module comments |
| PdfBackend trait design may be over-engineered for current needs | Keep it minimal (4 methods); don't add generics or async until needed |
| Tracing filter may hide genuine pdf_extract errors | Filter is at `error` level (not `off`); genuine parse failures still bubble through Result types |

## Plan Hardening Signals

- **Public API, schema, or contract change**: No. `parse_pdf_document()` public
  signature is unchanged. Internal trait is `pub(crate)`.
- **Security, auth, permission, or compliance-sensitive behavior**: No.
- **Migration, backfill, destructive data/config action, or irreversible step**: No.
  Previously ingested PDFs may produce slightly different chunk IDs if fallback
  behavior changes, but `--force` re-sync handles this (documented in existing
  module comments).
- **External integration, operator checkpoint, or external dependency**: No.
- **High runtime, rollout, or rollback risk**: Low. Changes are internal to the
  PDF parsing module. Rollback = revert the branch.

**Requires plan hardening: no**

## Runtime Verification and Closure

### Changed Runtime Surface

The `graphtor-docs sync` CLI command (PDF parsing path).

### Verification Criteria

1. Run `graphtor-docs sync --full --no-embed` against the 6.3 MB benchmark PDF
   and verify:
   - Produces the same chunk count as before (413 chunks) or documents the
     difference if fallback behavior changes.
   - Completes in significantly less time (target: <30 seconds, down from 91s).
   - No WARN-level glyph spam in stderr output.
2. Run against the 104.7 MB Cosmos DB PDF and verify:
   - Completes within a reasonable time (target: <120 seconds with `--no-embed`).
   - Triggers the large-file heuristic (info log message visible).
3. All existing tests pass: `cargo test`.

### Operational Closure

- No monitoring infrastructure needed (local-only tool).
- Rollback trigger: if chunk output quality degrades (heading detection stops
  working on previously-working PDFs), revert and investigate.
- Ownership: PDF parse module maintainer.
- Validation window: test against 2-3 real-world PDFs of varying size.

## References

- `docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md`
- `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md`
- `docs/compound/best-practices/pdf-heading-detection-heuristics-2026-05-03.md`
- `src/parse/pdf.rs` (current implementation)
- `src/pipeline/mod.rs` (dispatch point)
- `tests/parse_pdf_test.rs`, `tests/pipeline_pdf_test.rs`

## Plan Review

**Gate decision: PASS**

Reviewed 2026-05-03 by multi-persona subagents: Constitution Reviewer, Rust
Reviewer, Scope Boundary Auditor, Learnings Researcher, Architecture Strategist.

### Findings

#### P2 — Unit 2 requires EnvFilter migration (Rust Reviewer)

The plan states "adjust tracing subscriber initialization to filter `pdf_extract`
module at `error` level." However, the current `init_logging()` in
`src/logging/init.rs` uses `with_max_level(level)` which is a global filter and
does not support per-crate directives. The implementation must switch to
`tracing_subscriber::EnvFilter` with directives like `"info,pdf_extract=error"`.
This is a moderate infrastructure change to the logging module that should be
acknowledged in Unit 2's scope.

**Recommendation**: Unit 2 should explicitly note the `with_max_level` →
`EnvFilter` migration in `src/logging/init.rs`, update the `LogVerbosity` enum
mapping to produce `EnvFilter` strings, and preserve the existing `RUST_LOG`
environment variable override behavior that `EnvFilter` provides natively.

#### P3 — Unit 5 deferral option (Scope Boundary Auditor)

Unit 5 (PdfBackend trait) is a design refactoring that does not directly improve
performance. The spike recommended it as step 5 and the plan correctly sequences
it last. If the shipment is at risk of exceeding scope, Unit 5 can be deferred
to a follow-up without affecting the performance gains from Units 1–4.

**Recommendation**: Advisory. Keep in scope but mark as droppable if time
pressure arises.

#### P3 — PageTextAccumulator output fidelity (Learnings Researcher)

The compound learning `pdf-chunk-id-uniqueness-pattern-2026-05-01.md` documents
that chunk IDs use `{source_path}#page={N}#segment={M}` discriminators. The
`PageTextAccumulator` must emit page text in the same order and grouping as
`extract_text_from_mem_by_pages` to preserve chunk ID stability. The plan's
characterization test approach is the correct mitigation.

**Recommendation**: Advisory. The characterization test covers this risk.

#### P3 — Logging module test coverage (Rust Reviewer)

The existing `src/logging/init.rs` has unit tests for level mapping. After the
`EnvFilter` migration, those tests should be updated to verify per-crate
filtering behavior.

**Recommendation**: Advisory. Include updated logging tests in Unit 2.

### Constitution Check

| Principle | Status |
|---|---|
| Local-first | ✓ No external deps added |
| Lightweight footprint | ✓ Reduces work done, no new crates |
| Data pipeline integrity | ✓ Chunk IDs stable via characterization tests |
| MCP-native interface | N/A — no MCP changes |
| Automation & reproducibility | ✓ All changes idempotent |

### Prior Learnings Consulted

- `pdf-extract-api-usage-pattern-2026-05-01.md` — confirms `output_doc_page()` availability
- `pdf-heading-detection-heuristics-2026-05-03.md` — documents architecture to preserve
- `pdf-chunk-id-uniqueness-pattern-2026-05-01.md` — chunk ID discriminator pattern

### Hardening Assessment

Plan declares `Requires plan hardening: no`. Confirmed: no public API changes,
no security surfaces, no migrations, no external integrations, no irreversible
steps. The plan is eligible for direct harvest without hardening.

### Summary

0 P0, 0 P1, 1 P2, 3 P3. Gate: **PASS**. Proceed to harvest.
