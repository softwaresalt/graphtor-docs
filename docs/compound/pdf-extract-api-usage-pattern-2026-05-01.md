# Compound Learning: pdf-extract 0.10 API and Page Chunking Pattern

**Category:** Ingestion Pipeline  
**Discovered:** 2026-05-01  
**Context:** PR #13 — PDF document ingestion pipeline (007-S)

## Core API

```rust
// Cargo.toml
pdf-extract = "0.10"

// Usage
let text = pdf_extract::extract_text_from_mem(bytes)
    .map_err(|e| GraphtorError::Parse {
        message: format!("pdf text extraction failed: {e}"),
        path: Some(source_path.into()),
    })?;
```

`pdf-extract` wraps `lopdf` and returns `anyhow::Result<String>`. The result type requires
explicit conversion via `.map_err()` — it cannot be used with `?` directly in a function
returning a `thiserror`-typed `Result`.

## Page Delimiter

`pdf-extract` uses **form-feed (`\x0c`)** as the page delimiter in the extracted text. Split on
this character to process pages individually:

```rust
for (page_idx, page_text) in text.split('\x0c').enumerate() {
    let trimmed = page_text.trim();
    if trimmed.is_empty() { continue; } // empty pages / trailing FF
    // ...
}
```

Empty pages are common (trailing form-feed at end of document, blank pages). Always trim and skip.

## Error Conditions

| Input | Behaviour |
|---|---|
| Empty `&[u8]` | Returns `Err` — cannot parse zero bytes as PDF |
| Bytes without `%PDF-` header | Returns `Err` — not a valid PDF |
| Binary garbage | Returns `Err` |
| Valid PDF with no extractable text (scanned image PDF) | Returns `Ok("")` — empty string, no chunks produced |
| Valid PDF with text | Returns `Ok(text)` with `\x0c` page delimiters |

Image-only PDFs (scanned documents) succeed silently with empty text. Downstream code handles
this naturally: empty pages are skipped, so the result is a `ParsedDocument` with zero chunks.
If zero-chunk PDFs should be flagged, add an explicit check after `chunk_pdf_text`.

## Title Extraction

PDF metadata titles are not exposed by `pdf-extract`. Derive the title from the first
non-empty line of extracted text (heuristic):

```rust
fn extract_title(text: &str, fallback: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(120).collect())
        .unwrap_or_else(|| {
            Path::new(fallback)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(fallback)
                .to_string()
        })
}
```

## Graph Links

PDF rendering does not preserve hyperlink targets or code block structure. Set
`references: Vec::new()` and `code_snippets: Vec::new()` in `ParsedDocument` — these fields
are always empty for PDF sources.

## Evidence

- PR #13 commit `3ed3460`: `feat(pipeline): add PDF document ingestion pipeline`
- `src/parse/pdf.rs` — full implementation
- `tests/parse_pdf_test.rs`, `tests/pipeline_pdf_test.rs` — integration test coverage
