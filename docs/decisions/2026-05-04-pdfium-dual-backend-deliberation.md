---
title: "PDFium dual backend for large PDF ingestion"
description: "Decision to add pdfium-render as an optional runtime backend for PDFs ≥20 MiB, revisiting the prior spike's rejection with new evidence."
topic: "Large PDF ingestion backend selection"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/decisions/2026-05-04-pdfium-dual-backend-spike.md"
  - "docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md"
tags:
  - "pdf-ingestion"
  - "performance"
  - "pdfium"
  - "dual-backend"
  - "architecture"
---

## Problem Frame

The pdf-extract optimizations shipped in 023-F (014-S) are insufficient for
PDFs ≥100 MB. Diagnostic benchmarking isolated the bottleneck to
`lopdf::Document::load_mem()`, which eagerly parses all xref entries serially
— taking >20 minutes for a 104 MB PDF before our code even runs. This is an
architectural limitation of lopdf, not an algorithmic issue we can optimize
away.

The prior spike (2026-05-03) rejected `pdfium-render` because it "requires
Pdfium runtime or static-link complexity." New evidence shows this rejection
was based on incomplete information: `pdfium-render` uses dynamic loading
(`dlopen`/`LoadLibrary`) at runtime, compiles cleanly with
`#![forbid(unsafe_code)]`, and the PDFium DLL is an optional runtime
enhancement — without it, the system behaves identically to current code.

**Constraints:**

* Must preserve the single-binary compilation property.
* Must preserve graceful degradation — no new mandatory runtime dependencies.
* Must not regress small/medium PDF quality (heading-aware extraction).
* Must handle 100+ MB PDFs within a reasonable time bound.

**Success criteria:**

* 104 MB Cosmos DB PDF completes ingestion (currently DNF).
* Small/medium PDFs continue using the existing two-pass pipeline.
* Binary compiles and runs without PDFium DLL installed.

## Research Findings

### Benchmark evidence (post-023-F, release build)

| PDF | Size | Pages | Time | Status |
|---|---|---|---|---|
| AzureFabric.ebook.pdf | 4.1 MB | 46 | 0.93 s | ✅ |
| DMVs PDF | 6 MB | 326 | 5.19 s | ✅ |
| azure-cosmos-db.pdf | 104 MB | ~1100 | >25 min | ❌ DNF |

### Bottleneck isolation

`pdf_diag.rs` diagnostic binary confirmed:

* File read: 0.2 s
* `load_mem()`: >20 min (100% of time)
* Text extraction: never reached

### lopdf rayon feature — insufficient

Cargo feature unification verified (single lopdf 0.38.0 with rayon). Cosmos DB
PDF still DNF >10 min. Rayon does not parallelize the serial xref parsing.

### pdfium-render technical assessment

* Compiles with `#![forbid(unsafe_code)]` — FFI unsafe internal to crate.
* Uses `dlopen`/`LoadLibrary` for dynamic loading — no compile-time native
  dependency.
* Document open is lazy (xref index only) — O(1) regardless of file size.
* Per-page text extraction via `page.text()?.all()`.
* Font size metadata available via `unscaled_font_size()` for future heading
  extraction.
* Pre-built binaries from `bblanchon/pdfium-binaries` (~7 MB DLL).

### Prior learnings consulted

* `docs/decisions/2026-05-03-large-pdf-ingestion-strategy-spike.md`
* `docs/decisions/2026-05-03-streaming-pdf-heading-aware-spike.md`
* `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md`
* `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md`
* `docs/compound/best-practices/pdf-heading-detection-heuristics-2026-05-03.md`

## Options Evaluated

### Option A: pdfium-render dual backend (recommended)

Add `pdfium-render` as an optional runtime backend. PDFs ≥20 MiB try pdfium
first; fall back to pdf-extract if the DLL is absent.

* **Pros**: Eliminates DNF for 100+ MB PDFs. Preserves single-binary
  compilation. Graceful degradation — works without DLL. Lazy document
  opening (instant for any size). Proven engine (Chromium/Android).
* **Cons**: +9 transitive compile-time crates. +7 MB optional DLL. Text
  extraction quality not yet benchmarked against pdf-extract. New DLL
  discovery code path to maintain.
* **Effort**: Medium (backend struct, routing logic, DLL discovery, tests).
* **Fit**: High — directly addresses the measured bottleneck without
  compromising existing architecture.

### Option B: Accept the limitation

Document that PDFs >50 MB are not supported. Zero code change.

* **Pros**: Zero implementation effort. No new dependencies.
* **Cons**: Excludes real documentation (104 MB Azure Cosmos DB PDF).
  Undermines the project's value proposition for comprehensive doc indexing.
* **Effort**: Low (documentation only).
* **Fit**: Low — avoids the problem rather than solving it.

### Option C: Fork lopdf for lazy loading

Fork `lopdf` to add lazy xref parsing. Maximum control, no new runtime deps.

* **Pros**: No new runtime dependency. Fixes the root cause at the source.
* **Cons**: High maintenance burden. Uncertain timeline. Deep PDF spec
  expertise required. No upstream indication of lazy-loading support.
* **Effort**: Very high (PDF internals, ongoing maintenance).
* **Fit**: Medium — right direction but impractical timeline.

### Option D: Wait for pdf_oxide

Wait for `pdf_oxide` to mature and lower its MSRV (currently 1.88, repo at
1.85).

* **Pros**: Rust-native, promising architecture. Would eventually solve the
  problem without external runtime deps.
* **Cons**: Timeline unknown. MSRV gap. Does not solve the problem today.
* **Effort**: None now (wait), high later (migration).
* **Fit**: Low for current needs — good long-term direction.

## Trade-off Comparison

| Criterion | A: pdfium dual | B: Accept limit | C: Fork lopdf | D: Wait pdf_oxide |
|---|---|---|---|---|
| Solves 100 MB DNF | ✅ Yes | ❌ No | ✅ Yes | ⏳ Eventually |
| Implementation risk | Medium | None | Very high | None now |
| New runtime deps | Optional DLL | None | None | None |
| Single-binary preserved | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Maintenance burden | Low | None | Very high | Future |
| Time to solution | Days | Hours | Months | Unknown |
| Architecture fit | High | N/A | High | High |

## Decision

**Chosen: Option A — pdfium-render dual backend.**

**Rationale:**

1. The bottleneck is architectural in lopdf and cannot be optimized in our
   code. Options B, C, D all either accept the problem, require impractical
   effort, or defer indefinitely.

2. The prior spike's rejection rationale ("requires Pdfium runtime") is
   invalidated. pdfium-render uses dynamic loading — no compile-time native
   dependency. The DLL is an optional runtime enhancement. Without it,
   behavior is identical to current code.

3. The dual-backend pattern with graceful fallback is the right architecture:
   small PDFs get the higher-quality heading-aware pipeline, large PDFs get
   instant opening. Users without the DLL are not worse off.

4. The 9-crate compile-time cost is justified by eliminating a DNF scenario
   for an entire class of input files.

**Principle compatibility:**

* **Local-First**: ✅ PDFium runs entirely locally.
* **Lightweight Footprint**: ⚠️ +7 MB optional DLL. Justified by enabling
  100+ MB PDF ingestion that currently fails entirely.
* **Single Binary**: ✅ Binary compiles and runs without the DLL.
* **Zero Runtime Dependencies**: ⚠️ Optional dependency. System works
  without it. This is an enhancement, not a requirement.

## Rejected Alternatives

* **Option B** rejected because accepting a size limit undermines the
  project's purpose of comprehensive documentation indexing.
* **Option C** rejected because forking lopdf for lazy loading requires
  deep PDF spec expertise and carries ongoing maintenance burden with
  uncertain timeline.
* **Option D** rejected for immediate needs — pdf_oxide is the right
  long-term direction but does not solve today's problem. It remains a
  future spike/feature seed.

## Unresolved Questions

1. **Text extraction quality**: How does pdfium's `page.text()?.all()`
   compare to pdf-extract for the same document? Needs benchmark during
   implementation.
2. **DLL distribution**: Should the DLL be bundled in releases, downloaded
   on first use, or left to users? Deferred to operational packaging phase.
3. **SLO target**: What is the acceptable time bound for 100 MB PDF
   ingestion? Propose <30s based on pdfium's lazy architecture.
4. **Heading-aware extraction for large PDFs**: Can pdfium's
   `unscaled_font_size()` API enable heading detection for large PDFs too?
   Deferred to a follow-up enhancement.

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| PDFium DLL not available on target platform | Medium | Low | Graceful fallback to pdf-extract. Log warning. |
| Text quality regression vs pdf-extract | Low | Medium | Benchmark during implementation. Accept "good enough" for RAG. |
| pdfium-render crate abandoned | Low | Medium | PDFium itself is Google-maintained. Alternative wrappers exist. |
| DLL version mismatch | Low | Low | Use `pdfium_latest` feature. Document tested version. |
