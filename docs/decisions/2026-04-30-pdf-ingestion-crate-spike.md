---
title: "PDF ingestion crate selection and architecture"
type: spike
date: 2026-04-30
time_box: "2h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: null
promoted_to: ["plan"]
tags:
  - "pdf-ingestion"
  - "parser-backend"
  - "crate-evaluation"
---

## Goal

Which Rust PDF crate should graphtor-docs use for text extraction, and how
should PDF parsing integrate into the existing pipeline architecture?

## Success Criteria

- Evaluate at least 3 PDF crates for text extraction quality, API simplicity,
  dependency weight, and Rust version compatibility
- Determine the architecture integration pattern (new parser backend vs
  standalone stage)
- Produce a recommendation with enough detail to feed `impl-plan`

## Scope Constraints

- Read-only investigation — no code changes or prototype
- Must be compatible with `rust-version = "1.75"` (current project minimum)
- Must respect the single-binary, local-first architecture principles

## Investigation Approach

1. Audit the existing parse pipeline to identify extension points
2. Search crates.io for candidate PDF libraries
3. Evaluate each candidate on: API, weight, rust-version, features, license
4. Design the integration pattern into the existing pipeline
5. Recommend crate + architecture

## Findings

### Existing Parse Pipeline Extension Points

The current pipeline has a clean boundary:

```text
file (PathBuf) → read_to_string → parse_document(content, path) → ParsedDocument
```

Key observations:
- `process_batch()` in `pipeline/mod.rs` calls `std::fs::read_to_string(file)`
  — this assumes text files. PDF needs `std::fs::read(file)` (binary).
- `parse_document()` in `parse/mod.rs` expects markdown string input and returns
  `ParsedDocument { path, title, frontmatter, chunks, references, code_snippets }`.
- The `Chunk` type is format-agnostic: `chunk_id`, `content` (text), `heading_hierarchy`,
  `position`, `char_offset`, `source_path`.
- The `filter_files()` function already supports any glob pattern — `**/*.pdf`
  works out of the box.
- The `sources.yaml` include patterns just need `*.pdf` alongside `*.md`.

**Integration pattern**: Add a file-type dispatcher in `process_batch()` that
routes `.pdf` files to a new `parse_pdf()` function, which returns the same
`ParsedDocument` type. This maximizes code reuse — all downstream stages
(embed, load, search, traverse) work unchanged.

### Crate Evaluation

| Crate | Version | Rust Version | Download Size | License | Text Extract | Markdown Convert | Notes |
|-------|---------|-------------|---------------|---------|-------------|-----------------|-------|
| `pdf_oxide` | 0.3.40 | **1.88** ❌ | 2.1 MB | MIT/Apache-2.0 | ✅ Excellent | ✅ Built-in | Fastest (0.8ms mean), 100% pass on 3,830 PDFs, has OCR + table ML features. **BLOCKER: requires Rust 1.88, we're on 1.75** |
| `pdf-extract` | 0.10.0 | None specified | 95.9 KB | MIT | ✅ Good | ❌ Text only | Focused text extraction, simple API, lightweight. Built on top of `pdf` crate |
| `pdf` | 0.10.0 | None specified | 100.4 KB | MIT | ⚠️ Low-level | ❌ Manual | Full PDF DOM reader — gives you page objects, streams, fonts. Need to build text extraction on top |
| `lopdf` | 0.40.0 | 1.74 ✅ | 6.7 MB | MIT | ⚠️ Manipulation | ❌ Manual | PDF manipulation library (read/write/merge). Heavy. Not focused on text extraction |

### What Was Discovered

#### `pdf-extract` is the right v1 choice

- **API**: `pdf_extract::extract_text(&bytes)` returns `Result<String, Error>` — dead simple
- **Architecture**: Built on top of the `pdf` crate for PDF parsing, adds text layout analysis
- **Weight**: Tiny (95.9 KB source), reasonable transitive deps
- **Rust version**: No explicit MSRV, works with stable Rust
- **License**: MIT (compatible with project)
- **Limitation**: Returns flat text, not structured. We need to build page/section-aware chunking on top

#### `pdf_oxide` is the aspirational v2 choice

- Has built-in PDF-to-Markdown conversion (exactly what we need)
- 5× faster than `pdf-extract` with better accuracy
- But requires Rust 1.88 — our project's `rust-version = "1.75"`
- Bumping MSRV is a separate decision with broader impact (CI, developer toolchains)
- When/if we bump to 1.88+, migrating from `pdf-extract` to `pdf_oxide` is straightforward

### Architecture Design

#### File-Type Dispatcher Pattern

```text
process_batch()
  ├─ .md  → read_to_string → parse_document()     → ParsedDocument
  ├─ .pdf → read (binary)  → parse_pdf_document()  → ParsedDocument
  └─ other → skip with warning
```

#### `parse_pdf_document()` Implementation Shape

```rust
// src/parse/pdf.rs
pub fn parse_pdf_document(
    bytes: &[u8],
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    // 1. Extract text via pdf-extract
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| GraphtorError::Extraction { ... })?;

    // 2. Split into pages or sections (page breaks → chunk boundaries)
    let chunks = chunk_pdf_text(&text, source_path)?;

    // 3. Return ParsedDocument with no frontmatter, no references, no code snippets
    Ok(ParsedDocument {
        path: source_path.to_string(),
        title: extract_title_from_first_line(&text),
        frontmatter: None,
        chunks,
        references: vec![],
        code_snippets: vec![],
    })
}
```

#### Chunking Strategy for PDF Text

Since `pdf-extract` returns flat text without heading structure:

1. **Page-based chunking**: Split at form-feed characters (`\x0c`) which
   `pdf-extract` uses as page delimiters
2. **Size-based splitting**: Within pages, split at paragraph boundaries
   when content exceeds a configurable chunk size (e.g., 2000 chars)
3. **Heading heuristic**: Lines that are ALL CAPS or match common patterns
   (e.g., `Chapter \d+`, `Section \d+`) can be promoted to heading_hierarchy
4. **Title extraction**: First non-empty line, or the filename stem

#### Pipeline Changes Required

| File | Change | Scope |
|------|--------|-------|
| `src/parse/pdf.rs` | New module: PDF text extraction + chunking | New file |
| `src/parse/mod.rs` | Add `pub mod pdf;` and export `parse_pdf_document` | 2 lines |
| `src/pipeline/mod.rs` | File-type dispatch in `process_batch()`: route `.pdf` to `parse_pdf_document()` | ~15 lines |
| `src/pipeline/mod.rs` | Change `read_to_string` to binary `read` for PDF files | ~5 lines |
| `src/error/types.rs` | Add `Extraction` variant if not present, or reuse existing | ~3 lines |
| `Cargo.toml` | Add `pdf-extract = "0.10"` dependency | 1 line |
| `tests/parse_pdf_test.rs` | Integration test with a small embedded PDF | New file |

#### Large File Handling

For PDFs of any size:
- `pdf-extract` loads the full PDF into memory (PDF files are not streamable
  by nature — random access to xref table is required)
- For typical documentation PDFs (1-50 MB), this is acceptable
- For very large PDFs (100+ MB), we should consider:
  - Processing pages in batches rather than extracting all text at once
  - Configurable page range in sources.yaml (e.g., `pages: "1-100"`)
  - Memory-mapped I/O via the `pdf` crate's `mmap` feature (future optimization)

### What Was Tried and Failed

N/A — this was a research spike, not a prototype.

### Remaining Unknowns

1. **Extraction quality on Microsoft Learn PDFs**: The downloadable PDFs from
   learn.microsoft.com use specific layouts and styling. How well does
   `pdf-extract` handle their text layout? Needs live testing.
2. **Table extraction**: `pdf-extract` does not extract tables as structured
   data. Tables will appear as jumbled text. This is acceptable for v1 but
   may need `pdf_oxide`'s ML-based table extraction for v2.
3. **Image/figure handling**: PDF images are ignored entirely. OCR is out of
   scope for v1.
4. **Non-Latin text**: `pdf-extract`'s CID font handling may struggle with
   CJK PDFs. Not a concern for Microsoft English docs.

## Recommendation

**Conclusion**: Proceed
**Confidence**: High

Use `pdf-extract` 0.10 as the PDF text extraction backend for v1. The
integration architecture is clean: add a file-type dispatcher in
`process_batch()` that routes `.pdf` files to a new `parse_pdf_document()`
function, returning the same `ParsedDocument` type used by the markdown
pipeline. All downstream stages (embed, load, search, traverse) work unchanged.

### Task decomposition estimate (for impl-plan):

1. **T1: PDF parser module** (~1h) — `src/parse/pdf.rs` with `parse_pdf_document()`,
   page-based chunking, title extraction
2. **T2: Pipeline file-type dispatch** (~1h) — modify `process_batch()` to route
   by extension, handle binary read for PDFs
3. **T3: Error variant + Cargo.toml** (~30m) — add `pdf-extract` dep, error handling
4. **T4: Integration tests** (~1h) — test with embedded small PDF, verify chunks,
   verify end-to-end pipeline
5. **T5: Documentation update** (~30m) — update README, AGENTS.md technology table

Total: ~4h human-equivalent effort, well within 2-hour-per-task agent limit
when decomposed into 5 tasks.

## Next Steps

Promote to `impl-plan` for detailed implementation planning, then harvest
into backlog tasks under a new feature.

## References

- `src/parse/mod.rs` — current markdown-only parse entry point
- `src/parse/types.rs` — `ParsedDocument`, `Chunk` types (format-agnostic)
- `src/pipeline/mod.rs` — `process_batch()` function (lines 274-403)
- `src/acquire/filter.rs` — glob filtering (already supports `**/*.pdf`)
- `pdf-extract` crate: https://crates.io/crates/pdf-extract
- `pdf_oxide` crate: https://crates.io/crates/pdf_oxide (v2 candidate)
- Stash entry: `8474839B`
