---
title: "Large PDF ingestion strategy under local-first constraints"
description: "Evidence-backed evaluation of large PDF ingestion approaches for graphtor-docs, including benchmarks, dependency trade-offs, and a consensus recommendation."
type: spike
date: 2026-05-03
time_box: "4h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["queue"]
tags:
  - "pdf-ingestion"
  - "performance"
  - "parser-architecture"
  - "benchmarking"
  - "local-first"
---

## Goal

Which approach should graphtor-docs use to ingest very large PDFs efficiently,
while preserving the project's local-first, Rust-first, single-binary, and
zero-runtime-dependency constraints?

## Success Criteria

* Compare multiple viable approaches for large-PDF ingestion.
* Ground the comparison in current code, realistic benchmarks, and upstream
  tool documentation.
* Produce a recommendation that Stage can use later for planning and shipment
  work.

## Scope Constraints

* Read-only investigation for production code.
* Respect the current repo architecture and quality constraints.
* Favor approaches that preserve local execution, a single Rust binary, and no
  external runtime dependencies.

## Investigation Approach

1. Review prior PDF spikes, compound learnings, and backlog follow-up items.
2. Inspect the current parser and pipeline code paths.
3. Benchmark the current pipeline on isolated single-PDF configs.
4. Compare alternative extraction stacks against project constraints.
5. Run independent model reviews and synthesize a consensus recommendation.

## Findings

### What Was Discovered

#### Current root causes are concrete, not speculative

The current path loads and processes PDFs in a way that scales poorly for large
files:

* `src/pipeline/mod.rs` reads PDFs with `std::fs::read(file)` before handing
  the full buffer to `parse_pdf_document()`.
* `src/parse/pdf.rs` then calls `pdf_extract::Document::load_mem(bytes)`,
  which eagerly loads the document from the in-memory byte buffer.
* The heading-aware path performs two full `pdf_extract::output_doc()` walks:
  one for `FontSizeHistogram`, one for `HeadingAwareOutput`.
* The uniform-font fallback reparses the same bytes again through
  `extract_text_from_mem_by_pages(bytes)`, which is exactly the issue already
  captured in backlog task `024-T`.

This means the current implementation pays for full-byte loading, at least two
full document walks, and sometimes a second parse of the same file.

#### Warning spam is a real performance multiplier

The isolated 6.3 MB benchmark emitted **19,606** `WARN pdf_extract` lines for
repeated unknown glyph names from the same font family. The most common warning
patterns repeated 642 times each. This log volume is large enough to be part of
the performance problem, not just a cosmetic annoyance.

#### The current parser is already too slow on modest PDFs

Using an isolated single-source config and `sync --full --no-embed`:

| File | Size | Result |
|---|---|---|
| `performance-tuning-with-dmvs.pdf` | 6.3 MB | Completed in `91,691 ms` pipeline time, produced 413 chunks |
| `azure-cosmos-db.pdf` | 104.7 MB | Did not finish parsing within more than 9 minutes; run was stopped manually |

The large-file run completed acquisition in `7 ms`, then remained in the parse
phase without reaching a `parse/embed/load stage complete` log line.

#### `pdf-extract` still exposes useful internal levers

Official `pdf-extract` docs and prior repo learnings confirm that the crate
already exposes:

* `Document::load_mem()`
* `output_doc()`
* `output_doc_page()`
* `extract_text_from_mem_by_pages()`
* `OutputDev`

That matters because we can improve the current backend without adding a new
crate:

* Reuse the already-loaded `Document` in the fallback path.
* Add a lightweight per-page accumulator `OutputDev`.
* Switch large files to a cheaper page-oriented mode.
* Sample only the first N pages for body-font detection instead of scanning the
  full document twice.

#### Conversion-first Markdown tooling is not a fit here

`markdown-it` is not a PDF converter. It is a Markdown parser. It does not
solve PDF ingestion by itself.

Marker and Docling do solve PDF-to-Markdown conversion, but their official
docs introduce constraints that are misaligned with this repo:

* **Marker** requires Python 3.10+, PyTorch, `pip install marker-pdf`,
  optional GPU acceleration, and has GPL/commercial licensing constraints.
* **Docling** requires Python packaging, targets Python 3.10+, and ships a
  larger ML-oriented document processing stack with Markdown export.

These tools may be strong in absolute terms, but they conflict with the
project's single-binary and zero-runtime-dependency goals.

#### Native runtime engines are technically strong but operationally weak

`pdfium-render` is a Rust wrapper around Pdfium and its official README makes
the runtime dependency explicit: you must supply or link Pdfium separately.
That violates the repo's zero-runtime-dependency rule in the default case and
substantially complicates packaging.

`pdftotext` is fast and page-selective, but it is an external CLI with a
system dependency, and it discards the heading-aware structure that the current
parser was added to recover.

#### `pdf_oxide` remains the only serious future pivot

The earlier PDF crate-selection spike identified `pdf_oxide` as the aspirational
next step, and the current upstream `Cargo.toml` still sets `rust-version =
"1.88"`. That keeps it out of scope for the current repo baseline, but it is
still the strongest future migration candidate because it is Rust-native and
positions itself around faster extraction and Markdown conversion.

### Multi-model review consensus

Independent reviews from Claude Sonnet 4.6 and GPT-family review both converged
on the same ranking:

1. Optimize the current `pdf-extract` backend now.
2. Keep a future `pdf_oxide` migration as the only serious alternative path.
3. Reject Marker, Docling, Poppler `pdftotext`, and `pdfium-render` for the
   current project constraints.

The Claude review added one strategic refinement: introduce a small backend
boundary such as `trait PdfBackend` so future library experiments remain low
risk.

### Option comparison

| Option | Strengths | Main blockers | Verdict |
|---|---|---|---|
| Optimize current `pdf-extract` path | No new runtime deps, low implementation risk, preserves current architecture | Still limited by eager document load | **Best current option** |
| Marker or Docling pre-conversion | Strong Markdown output and layout recovery | Python, ML stack, larger footprint, licensing and packaging burden | Reject |
| `pdfium-render` | Strong extraction engine, likely faster on complex PDFs | Requires Pdfium runtime or static-link complexity | Reject for now |
| Poppler `pdftotext` | Fast and mature | External CLI dependency, weaker structure preservation | Reject |
| Future `pdf_oxide` migration | Rust-native, promising future path | MSRV `1.88`, migration cost, needs dedicated validation | Keep as future spike/feature seed |

### What Was Tried and Failed

* A direct `parse_pdf_document()` timing run against `azure-cosmos-db.pdf`
  remained active for more than 15 minutes and was stopped manually.
* An isolated full-sync run for the same file, with embeddings disabled and the
  source narrowed to a single PDF, still failed to finish parsing within more
  than 9 minutes.
* The idea of using `markdown-it` as the conversion layer failed immediately,
  because `markdown-it` does not convert PDFs.

### Remaining Unknowns

* How much of the current wall-clock time is parser work versus repeated glyph
  warning overhead.
* What threshold should trigger a large-file downgrade from heading-aware mode
  to page-oriented mode.
* Whether sampling the first N pages for the histogram preserves heading
  quality across varied document layouts.
* Whether the repo will eventually accept an MSRV increase that would reopen
  `pdf_oxide` as a real implementation candidate.

## Recommendation

**Conclusion**: proceed
**Confidence**: high

Proceed with a **hybrid optimization of the current `pdf-extract` path**, not a
stack replacement.

Recommended order:

1. Eliminate the fallback reparse by replacing
   `extract_text_from_mem_by_pages(bytes)` with a page accumulator that runs on
   the already-loaded `Document`.
2. Add a large-file heuristic so very large PDFs skip the expensive two-pass
   heading-aware flow and use a cheaper page-oriented mode.
3. Deduplicate or suppress repeated `pdf_extract` glyph warnings so parsing does
   not spend time flooding stderr.
4. Consider histogram sampling over the first N pages instead of scanning the
   full document twice.
5. Introduce a small `PdfBackend` abstraction so a future `pdf_oxide`
   experiment is a bounded swap instead of a parser rewrite.

This recommendation preserves the repo's architectural principles, addresses
the measured bottlenecks directly, and creates a clean path for future
benchmark-driven migration work if the MSRV policy changes later.

## Next Steps

1. Stage a feature or chore around large-PDF optimization, using this spike as
   the planning seed.
2. Promote the `024-T` fallback reparse fix from "nice to have" to part of the
   core performance work.
3. Add a benchmark fixture or repeatable benchmark harness for at least one
   small and one large PDF.
4. Open a future follow-up spike if the repo is willing to revisit MSRV and
   evaluate `pdf_oxide` in a controlled branch.

## References

* `src/pipeline/mod.rs`
* `src/parse/pdf.rs`
* `.backlogit/queue/024-T.md`
* `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md`
* `docs/decisions/2026-05-03-streaming-pdf-heading-aware-spike.md`
* `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md`
* `docs/compound/best-practices/pdf-heading-detection-heuristics-2026-05-03.md`
* <https://docs.rs/pdf-extract/latest/pdf_extract/>
* <https://docs.rs/pdf-extract/latest/pdf_extract/fn.output_doc_page.html>
* <https://github.com/jrmuizel/pdf-extract/blob/master/README.md>
* <https://github.com/ajrcarey/pdfium-render/blob/master/README.md>
* <https://github.com/datalab-to/marker/blob/master/README.md>
* <https://github.com/docling-project/docling/blob/main/README.md>
* <https://www.mankier.com/1/pdftotext>
* <https://github.com/yfedoseev/pdf_oxide/blob/main/Cargo.toml>
