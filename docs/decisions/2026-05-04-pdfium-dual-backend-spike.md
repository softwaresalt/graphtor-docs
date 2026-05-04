---
title: "PDFium dual backend for large PDF ingestion"
description: "Revisiting pdfium-render as an optional runtime backend for PDFs ≥20 MiB, with evidence from post-023-F benchmarking that shows pdf-extract's lopdf bottleneck is architectural and cannot be optimized away."
type: spike
date: 2026-05-04
time_box: "4h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["plan"]
deliberation_id: "001-DL"
prior_spike: "docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md"
tags:
  - "pdf-ingestion"
  - "performance"
  - "pdfium"
  - "dual-backend"
  - "local-first"
---

## Goal

Should graphtor-docs add `pdfium-render` as an optional dual backend for PDFs
≥20 MiB, given that the pdf-extract optimizations shipped in 023-F did not
resolve the architectural `lopdf::load_mem` bottleneck for very large files?

## Success Criteria

* Demonstrate that the 023-F optimizations are insufficient for the 104 MB
  Cosmos DB PDF (quantified evidence, not speculation).
* Evaluate whether pdfium-render's "reject for now" verdict from the prior
  spike should be revisited.
* Assess the single-binary and zero-runtime-dependency impact.
* Produce a recommendation with trade-off analysis.

## Scope Constraints

* Read-only investigation for production code.
* Benchmarking uses existing diagnostic binaries on the feature branch.
* Respect the current repo architecture and quality constraints.
* The PDFium native library is an *optional* runtime enhancement, not a
  mandatory compile-time dependency.

## Prior Work

This spike is a direct follow-up to
`docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md`, which:

* Recommended optimizing the current `pdf-extract` path (shipped as 023-F/014-S).
* Rejected `pdfium-render` with the rationale: "Requires Pdfium runtime or
  static-link complexity."
* Noted `lopdf`'s eager document load as a remaining limitation.

The 023-F optimizations (PageTextAccumulator, histogram sampling, logging
suppression, large-file heuristic) were all shipped and merged. This spike
investigates whether they were sufficient.

## Investigation Approach

1. Benchmark 023-F optimized code against all three test PDFs.
2. Isolate the remaining bottleneck using `pdf_diag.rs` diagnostic binary.
3. Test `lopdf` rayon feature as an intermediate fix.
4. Re-evaluate pdfium-render against updated project constraints.
5. Assess single-binary impact of pdfium-render dependency.

## Findings

### What Was Discovered

#### 023-F optimizations help small/medium PDFs but not the pathological case

Post-023-F benchmarks with `--release` builds (strip=symbols, lto=thin,
codegen-units=1):

| PDF | Size | Pages | Total Time | Chunks |
|---|---|---|---|---|
| AzureFabric.ebook.pdf | 4.1 MB | 46 | 0.93 s | 45 |
| performance-tuning-with-dmvs.pdf | 6 MB | 326 | 5.19 s | 411 |
| azure-cosmos-db.pdf | 104 MB | ~1100 | **DNF (>25 min)** | — |

The small and medium PDFs are well within acceptable SLOs. The 104 MB PDF
never completes.

#### The bottleneck is architectural, not algorithmic

Using `pdf_diag.rs` to isolate timing:

```text
Cosmos DB PDF (104 MB):
  File read:      0.2 s
  load_mem():   >20 min  ← ALL time is here
  Extraction:     — (never reached)
```

`lopdf::Document::load_mem()` eagerly parses ALL objects in the xref table:

* A 104 MB PDF has 50,000–100,000+ xref entries.
* Each entry may reference a compressed object stream requiring flate2
  decompression.
* Parsing is serial even with rayon enabled (verified — see below).
* No amount of optimization to *our* code can fix this — the bottleneck is
  inside `lopdf` before our code even runs.

#### `lopdf` rayon feature does not help

Cargo feature unification was verified: single `lopdf 0.38.0` instance with
rayon enabled. The Cosmos DB PDF still DNF after 10+ minutes. Rayon helps
with document *rendering* parallelism, not with the serial xref parsing that
dominates load time.

#### pdfium-render's "reject" rationale no longer holds

The prior spike rejected pdfium-render because it "requires Pdfium runtime
or static-link complexity." Re-evaluating with new information:

1. **No compile-time native dependency.** `pdfium-render` uses dynamic
   loading (`dlopen`/`LoadLibrary`) at runtime. The Rust crate compiles
   cleanly with `#![forbid(unsafe_code)]` — FFI unsafe is internal to the
   crate.

2. **Optional runtime enhancement, not a requirement.** When the PDFium DLL
   is absent, the system falls back to pdf-extract. The single-binary
   property is preserved — the DLL is a performance accelerator.

3. **Lazy document opening.** PDFium only reads the xref index at open time
   and decompresses page content on demand. This makes it O(1) for document
   open regardless of file size — the exact opposite of lopdf's O(N) eager
   parsing.

4. **Proven ecosystem.** PDFium is Google's PDF engine (Chromium, Android).
   Pre-built binaries are available from `bblanchon/pdfium-binaries` for all
   major platforms (~7 MB DLL).

5. **Text extraction quality.** `page.text()?.all()` provides clean
   per-page text extraction. Font size metadata is available via
   `unscaled_font_size()` for future heading-aware extraction.

#### Architecture: dual backend with graceful fallback

The proposed architecture:

```text
parse_pdf_document(bytes, path)
  │
  ├── bytes.len() >= 20 MiB?
  │     ├── Yes → PdfiumBackend::try_parse()
  │     │          ├── Ok(doc) → return doc
  │     │          ├── Err(NotAvailable) → warn, fall through
  │     │          └── Err(ExtractionFailed) → error, fall through
  │     └── No → fall through
  │
  └── PdfExtractBackend::parse()  (existing two-pass pipeline)
```

Key design decisions:

* **Error splitting**: `PdfiumBindError::NotAvailable` (expected fallback,
  warn-level) vs `PdfiumBindError::ExtractionFailed` (real bug, error-level).
* **DLL discovery**: `$GRAPHTOR_PDFIUM_PATH` → executable dir → system path.
* **Small PDFs use pdf-extract**: The two-pass heading-aware pipeline produces
  better section boundaries. pdfium is only needed where pdf-extract DNFs.
* **Backend attribution logging**: Which backend was selected and why.

#### Single-binary impact assessment

| Concern | Impact |
|---|---|
| Compile-time deps | +9 transitive crates, cargo check adds ~90s first build |
| Binary size | No change — pdfium-render is pure Rust wrapping dynamic calls |
| Runtime without DLL | Identical behavior to current code (pdf-extract only) |
| Runtime with DLL | +7 MB DLL alongside binary, 104 MB PDF expected <10s |
| `#![forbid(unsafe_code)]` | Compatible — unsafe is internal to pdfium-render |

### What Was Tried and Failed

* **lopdf rayon feature**: Enabled via Cargo feature unification. Cosmos DB
  PDF still DNF >10 min. Rayon does not parallelize the serial xref parsing.

* **Direct lopdf optimization**: Not feasible — `load_mem()` is an upstream
  function. We would need to fork lopdf or wait for upstream lazy-loading
  support (no indication this is planned).

### Remaining Unknowns

* Exact wall-clock time for pdfium-render on the 104 MB PDF (expected <10s
  based on PDFium's architecture, but not yet measured end-to-end).
* Text extraction quality comparison between pdfium and pdf-extract for the
  same document (pdfium uses a different text extraction engine).
* Whether heading-aware extraction can be implemented via pdfium's
  `unscaled_font_size()` API for large PDFs.
* Cross-platform DLL packaging story for Linux/macOS (same
  `bblanchon/pdfium-binaries` repo provides all platforms).

## Recommendation

**Conclusion**: proceed
**Confidence**: high

Add `pdfium-render` as an **optional dual backend** for PDFs ≥20 MiB:

1. The pdf-extract optimizations (023-F) are necessary and valuable for
   small/medium PDFs but architecturally insufficient for 100+ MB files.
2. The prior spike's "reject" rationale is invalidated by new evidence:
   pdfium-render is an optional runtime enhancement, not a mandatory
   dependency. The single-binary property is preserved.
3. The dual-backend architecture provides graceful degradation — users
   without the DLL get current behavior, users with it get instant large-PDF
   ingestion.
4. The 9-crate transitive dependency cost is justified by eliminating a DNF
   scenario for an entire class of input files.

This is not a replacement of pdf-extract. It is a surgical addition for the
specific case where pdf-extract's architectural limitation (eager `load_mem`)
makes ingestion impossible.

## Next Steps

1. Stage through deliberation to formalize the "optional runtime DLL"
   decision against local-first principles.
2. Create implementation plan for `PdfiumBackend` in `src/parse/pdf.rs`.
3. Run plan review before implementation.
4. Harvest into backlog tasks and ship via PR.
5. Add benchmark fixture comparing both backends on all three test PDFs.

## References

* `docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md` (prior spike)
* `docs/decisions/2026-05-03-streaming-pdf-heading-aware-spike.md`
* `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md`
* `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md`
* `docs/compound/best-practices/pdf-heading-detection-heuristics-2026-05-03.md`
* `src/parse/pdf.rs` — current parser with 023-F optimizations
* `src/bin/pdf_diag.rs` — diagnostic binary isolating `load_mem` bottleneck
* `src/bin/pdf_parse_timing.rs` — benchmarking binary
* <https://crates.io/crates/pdfium-render>
* <https://github.com/ajrcarey/pdfium-render>
* <https://github.com/bblanchon/pdfium-binaries>
* <https://pdfium.googlesource.com/pdfium/>
