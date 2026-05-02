---
title: "Word document (.docx) ingestion crate selection and architecture"
type: spike
date: 2026-05-02
time_box: "2h"
conclusion: "proceed"
confidence: "medium"
linked_parent_work_item: "020-F"
promoted_to: ["plan"]
tags:
  - "docx-ingestion"
  - "parser-backend"
  - "crate-evaluation"
---

## Goal

Which Rust crate should graphtor-docs use for Word document (.docx) text
extraction, and how should DOCX parsing integrate into the existing pipeline
architecture?

## Success Criteria

- Evaluate candidate OOXML/docx parser crates for text extraction quality,
  heading-aware structure, API simplicity, and Rust version compatibility
- Determine the architecture integration pattern (new parser backend alongside
  Markdown and PDF)
- Produce a recommendation with enough detail to feed `impl-plan`

## Scope Constraints

- Read-only investigation — no code changes or prototype
- Must be compatible with `rust-version = "1.75"` (current project minimum)
- Must respect the single-binary, local-first architecture principles
- Must handle documents of any reasonable size (1–100+ MB)
- No external services or cloud APIs

## Investigation Approach

1. Audit the existing parse pipeline to identify extension points
2. Search for Rust OOXML/docx parsing crates
3. Evaluate each candidate on: API, extraction quality, weight, rust-version
4. Consider the manual zip+XML parsing fallback
5. Design the integration pattern into the existing pipeline
6. Assess chunking strategy for Word document structure

## Findings

### Existing Parse Pipeline Extension Points

The current pipeline dispatches by file extension in `process_batch()`:

```text
process_batch()
  ├─ .md  → read_to_string → parse_document()     → ParsedDocument
  ├─ .pdf → read (binary)  → parse_pdf_document()  → ParsedDocument
  └─ .docx → [NEW] read (binary) → parse_docx_document() → ParsedDocument
```

This is the same file-type dispatch pattern established by the PDF spike.
Adding a `.docx` arm requires:
- New `parse_docx_document()` function returning `ParsedDocument`
- New match arm in `process_batch()` for the `.docx` extension
- New module `src/parse/docx.rs`

### Crate Evaluation

| Crate | Version | Rust Version | Size | Read | Write | Heading Aware | Notes |
|-------|---------|-------------|------|------|-------|--------------|-------|
| `docx-rs` | 0.4 | 1.56+ ✅ | 320 KB | ✅ | ✅ | ✅ Paragraph styles | Read/write library. Parses `w:pStyle` for heading detection |
| `ooxmlsdk` | 0.2 | 1.70+ ✅ | 2.4 MB | ✅ | ✅ | ✅ Full OOXML | Generated from OOXML schemas. Comprehensive but heavy |
| `docx` | 0.2 | 1.60+ ✅ | 85 KB | ⚠️ Limited | ✅ | ❌ Write-focused | Primarily a document generator, not a reader |
| Manual `zip` + `quick-xml` | — | 1.56+ ✅ | ~200 KB | ✅ | ❌ | ✅ Custom | Full control. Parse `word/document.xml` directly |

### Detailed Assessment

#### `docx-rs` (0.4) — Recommended

- **API**: `docx_rs::read_docx(bytes)` → `Docx` struct with paragraphs, tables, styles
- **Structure**: Exposes `Paragraph` objects with `ParagraphStyle` (e.g., `Heading1`,
  `Heading2`) — essential for heading-aware chunking
- **Text extraction**: Walk paragraphs → runs → text nodes. Clean pattern
- **Tables**: Accessible as `Table` → `Row` → `Cell` → paragraphs
- **Images**: References are available but binary content needs separate extraction
  from the zip archive. Out of scope for v1 text extraction
- **Weight**: 320 KB — reasonable. Depends on `zip` and `xml-rs`
- **License**: MIT
- **Limitation**: API is verbose — extracting text requires walking the paragraph
  tree manually. No convenience `extract_text()` method

#### Manual `zip` + `quick-xml` — Fallback

A .docx file is a ZIP archive. The main content is in `word/document.xml`:

```xml
<w:body>
  <w:p>
    <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
    <w:r><w:t>Chapter Title</w:t></w:r>
  </w:p>
  <w:p>
    <w:r><w:t>Paragraph text here.</w:t></w:r>
  </w:p>
</w:body>
```

Using `zip` (already a transitive dep) + `quick-xml` (~150 KB):
- Extract `word/document.xml` from the zip
- Parse XML, walk `w:p` elements
- Check `w:pStyle` for heading levels
- Concatenate `w:t` text runs per paragraph

This gives full control and minimal deps, but requires ~150-200 lines of
XML walking code and testing against edge cases (nested runs, hyperlinks,
bookmarks, footnotes, tracked changes).

#### `ooxmlsdk` — Too Heavy

At 2.4 MB, this is a full OOXML SDK generated from Microsoft's schema files.
Comprehensive but violates the Lightweight Footprint principle for what is
essentially text extraction from one XML file.

### Architecture Design

#### `parse_docx_document()` Implementation Shape

```rust
// src/parse/docx.rs
pub fn parse_docx_document(
    bytes: &[u8],
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    let docx = docx_rs::read_docx(bytes)
        .map_err(|e| GraphtorError::Parse { ... })?;

    // Walk paragraphs, detect headings via pStyle, extract text
    let sections = extract_sections(&docx);

    // Chunk at heading boundaries (similar to Markdown H2/H3 chunking)
    let chunks = chunk_docx_sections(&sections, source_path)?;

    Ok(ParsedDocument {
        path: source_path.to_string(),
        title: sections.first().and_then(|s| s.heading.clone()),
        frontmatter: None,
        chunks,
        references: vec![],     // hyperlinks could be extracted in v2
        code_snippets: vec![],  // not applicable for Word docs
    })
}
```

#### Chunking Strategy

Word documents have rich heading structure that maps well to our existing
heading-based chunking:

1. **Heading detection**: Check `w:pStyle` values:
   - `Heading1` → H1, `Heading2` → H2, etc.
   - `Title` → H1 equivalent
   - Unknown styles → body text
2. **Section boundaries**: Split at H2/H3 (same as Markdown chunking)
3. **Table handling**: Flatten tables to pipe-delimited text within the
   current section's chunk
4. **List handling**: Detect `w:numPr` (numbered list) and `w:ilvl`
   (indent level), format as Markdown lists
5. **Size guard**: If a section exceeds `MAX_CHUNK_CHARS` (2000), split at
   paragraph boundaries

#### Pipeline Changes Required

| File | Change | Scope |
|------|--------|-------|
| `src/parse/docx.rs` | New module: DOCX text extraction + heading-aware chunking | ~200 lines |
| `src/parse/mod.rs` | Add `pub mod docx;` and export `parse_docx_document` | 2 lines |
| `src/pipeline/mod.rs` | Add `.docx` match arm in `process_batch()` file-type dispatch | ~5 lines |
| `Cargo.toml` | Add `docx-rs = "0.4"` dependency | 1 line |
| `tests/parse_docx_test.rs` | Integration test with a small embedded .docx | New file |

### Dependency Impact Assessment

| Crate | Size | New? | Justification |
|-------|------|------|---------------|
| `docx-rs` | 320 KB | Yes | OOXML parsing for .docx text extraction. No lighter alternative with heading awareness |

Total new dependency weight: 320 KB. Acceptable — `docx-rs` depends on `zip`
(already transitive) and `xml-rs` (small XML parser).

**Alternative**: Manual `zip` + `quick-xml` adds ~200 KB of dependencies
but ~150-200 lines of custom XML walking code. The `docx-rs` approach is
preferred because it handles edge cases (complex runs, nested elements,
character formatting) that manual parsing would need to duplicate.

### What Was Tried and Failed

N/A — this was a research spike, not a prototype.

### Remaining Unknowns

1. **Extraction fidelity with `docx-rs`**: The crate's read API is less
   documented than its write API. Need to verify that heading styles,
   tables, and lists are reliably accessible through the parsed `Docx` struct.
   A small prototype would confirm this.

2. **Complex formatting**: Tracked changes (`w:ins`, `w:del`), comments,
   footnotes, and endnotes are not handled by the proposed design. Acceptable
   for v1 — these are metadata, not primary content.

3. **Embedded images**: Word docs often contain diagrams and screenshots.
   The v1 design extracts text only. Image extraction (storing binary blobs
   or generating alt-text references) is a v2 concern.

4. **Large file performance**: For 100+ MB documents, loading the entire
   ZIP into memory may be problematic. `docx-rs` appears to load the full
   document tree. Streaming is not supported. May need a file-size guard
   or the manual `zip` approach for very large files.

5. **`.doc` (legacy) format**: The older binary `.doc` format is not
   addressable with OOXML parsers. Out of scope — only `.docx` is supported.

## Recommendation

**Conclusion**: Proceed
**Confidence**: Medium

The DOCX ingestion is feasible with `docx-rs` as the primary crate. The
integration follows the exact same file-type dispatch pattern as PDF:

- Add `parse_docx_document()` in `src/parse/docx.rs`
- Add `.docx` dispatch arm in `process_batch()`
- Return `ParsedDocument` for seamless downstream pipeline reuse

Confidence is **medium** (not high) because:
- `docx-rs`'s read API needs prototype validation for heading detection
- Table and list extraction fidelity is uncertain without hands-on testing
- The crate's maintenance trajectory is unclear (last release 2024)

**Mitigation**: If `docx-rs` proves inadequate during implementation, the
manual `zip` + `quick-xml` fallback is straightforward and gives full control.
The `parse_docx_document()` function signature and return type remain the same
regardless of which approach backs it.

## Next Steps

1. Promote to `impl-plan` for detailed implementation planning
2. During implementation, build a small prototype first to validate `docx-rs`
   heading detection before committing to the full module
3. Ship alongside 019-F (Web Crawler) in shipment 011-S — both extend the
   pipeline's input surface

## References

- `src/parse/mod.rs` — Parse pipeline dispatcher
- `src/parse/pdf.rs` — PDF parser backend (analogous pattern)
- `src/pipeline/mod.rs` — File-type dispatch in `process_batch()`
- `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md` — Prior spike
  with the same integration pattern
- `Cargo.toml` — Current dependency manifest
