---
type: session-memory
timestamp: "2026-05-03T08:41:46.000-07:00"
agent: ship
shipment: 013-S
feature: 022-F
pr: "https://github.com/softwaresalt/graphtor-docs/pull/22"
merge_commit: ae62de5
---

## Session Summary

Executed shipment 013-S: streaming PDF heading-aware chunking (022-F).

## Outcome

**Merged** — PR #22 merged to main at ae62de5.

## What Was Implemented

Complete rewrite of `src/parse/pdf.rs` (~1,280 lines, +2166/-175 vs prior):

- **FontSizeHistogram** (`OutputDev`): Pass 1 builds character-weighted histogram of quantized font sizes (0.5pt buckets) to identify body font size
- **HeadingAwareOutput** (`OutputDev`): Pass 2 detects heading boundaries via font-size ratio thresholds (H1 ≥ 1.6×, H2 ≥ 1.3× body size) using y-coordinate line detection
- **sections_to_chunks**: Converts `PdfSection`s to `Chunk`s with correct H1/H2 hierarchy tracking
- **Fallback**: `distinct_sizes <= 1` triggers per-page chunking via `extract_text_from_mem_by_pages`
- **split_at_word_boundaries**: Word-boundary fallback for single oversized paragraphs; UTF-8 safe via `char_indices().nth()`

## Key Decisions

1. **Two-pass not one-pass**: Required for accurate body-size determination before heading detection
2. **Quantize to 0.5pt buckets**: Groups renderer float drift without losing meaningful size differences  
3. **char_indices().nth() for word splitting**: Byte-offset slicing panics on non-ASCII — caught by Copilot review
4. **chunk_id format**: `#section={N}#segment={M}` (section-based) or `#page={N}#segment={M}` (fallback) — breaking change requiring `graphtor sync --force`

## Review Summary

- Internal 4-persona review: fixed `split_long_text()` missing word-boundary fallback, module doc update, `quantize()` comment
- Copilot PR review: fixed UTF-8 safety bug; deferred re-parse optimization and real-PDF test to backlog

## Follow-up Backlog Items

- 023-T: Add real-PDF integration test with fixture
- 024-T: Optimize fallback to avoid re-parsing bytes via custom per-page OutputDev

## Files Changed

- `src/parse/pdf.rs` — complete rewrite
- `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md` — updated
- `docs/compound/pdf-chunk-id-uniqueness-pattern-2026-05-01.md` — updated
- `docs/compound/best-practices/pdf-heading-detection-heuristics-2026-05-03.md` — new
- `.backlogit/archive/022.001-T.md` through `022.005-T.md` — archived
- `.backlogit/archive/022-F.md` — archived
- `.backlogit/archive/013-S.md` — shipped
- `.backlogit/queue/023-T.md`, `024-T.md` — new follow-up tasks
