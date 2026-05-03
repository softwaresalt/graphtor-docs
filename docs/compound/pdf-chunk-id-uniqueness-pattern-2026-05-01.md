# Compound Learning: PDF Chunk IDs Must Include Page and Segment Index

**Category:** Data Pipeline Integrity  
**Discovered:** 2026-05-01  
**Context:** PR #13 — PDF document ingestion pipeline (007-S)

## Problem

When chunking PDF text by page, using only `generate_chunk_id(segment, source_path)` produces
**duplicate chunk IDs** within a single document. PDFs commonly repeat headers, footers, page
numbers, or boilerplate text across pages. When two segments have identical text, their SHA-256
hash is identical, and CozoDB `:put` silently overwrites the earlier chunk, causing silent data
loss and corrupted document ordering.

## Solution

Include a stable per-chunk discriminator — page index and segment index — in the chunk ID source
string to guarantee uniqueness within a document regardless of repeated content:

```rust
for (segment_idx, segment) in segments.into_iter().enumerate() {
    let chunk_id_source = format!(
        "{source_path}#page={}#segment={segment_idx}",
        page_idx + 1
    );
    let chunk_id = generate_chunk_id(&segment, &chunk_id_source)?;
    // ...
}
```

The `chunk_id_source` is only used as a uniqueness discriminator for the SHA-256 hash; the stored
`Chunk.source_path` remains the plain `source_path` (without the `#page=…` suffix) so downstream
path matching is unaffected.

## Chunk ID Discriminator Formats

| Source format | Chunk ID source format | Path |
|---|---|---|
| PDF section-based (heading-aware) | `{source_path}#section={N}#segment={M}` | two-pass main path |
| PDF page-based (uniform-font fallback) | `{source_path}#page={N}#segment={M}` | fallback path |
| HTML, Markdown, etc. | format depends on parser | — |

The `chunk_id_source` is only used as a uniqueness discriminator for the SHA-256 hash; the stored
`Chunk.source_path` remains the plain `source_path` so downstream path matching is unaffected.

### Breaking Change: Re-sync Required

PDFs ingested with the old `#page=` discriminator must be re-synced: `graphtor sync --force`.
The new section-based IDs are not backward-compatible with page-based IDs.

## Evidence

- PR #13 commit `b6ca29b`: `fix(pipeline): address copilot review — path normalization and chunk ID uniqueness`
- `src/parse/pdf.rs` lines 82–92 — enumerate pattern with `chunk_id_source`
- Copilot review comment (resolved): `PRRC_kwDORiB5E869FpZs`

## Generalisation

Apply the same discriminator pattern for any source format that may produce repeated segment text:
- HTML (nav bars, sidebars, footers)
- EPUB/DOCX with boilerplate headers
- Any paginated format processed in a loop
