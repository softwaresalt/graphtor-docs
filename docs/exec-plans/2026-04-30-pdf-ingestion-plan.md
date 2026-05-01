---
title: "PDF Ingestion Pipeline — Implementation Plan"
type: plan
date: 2026-04-30
source: "docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md"
feature: "PDF Document Ingestion"
---

## Problem Frame

graphtor-docs currently only parses Markdown files via pulldown-cmark. The
pipeline reads files as UTF-8 strings (`read_to_string`) and routes all
content through `parse::parse_document()`. To ingest PDF documentation
(Microsoft Learn offline PDFs, user-supplied technical docs), we need:

1. A PDF text extraction backend that produces plain text from PDF bytes
2. A chunking strategy that creates `Chunk` records from extracted text
3. A file-type dispatcher in the pipeline that routes `.pdf` vs `.md` files
4. Integration tests proving end-to-end PDF → CozoDB ingestion

The spike at `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md`
recommends `pdf-extract` 0.10 for v1 (lightweight, no MSRV conflict).

## Requirements Trace

| Requirement | Source | Implementation Unit |
|---|---|---|
| Extract text from PDF files of any size | Stash `8474839B` | U1: PDF parser module |
| Produce `ParsedDocument` from PDF content | Spike architecture | U1: PDF parser module |
| Page-based chunking with size limits | Spike chunking strategy | U1: PDF parser module |
| Route `.pdf` files through PDF parser | Spike dispatcher pattern | U2: Pipeline dispatch |
| Handle binary file reads alongside text reads | Spike architecture | U2: Pipeline dispatch |
| Add `pdf-extract` dependency | Spike recommendation | U3: Dependency + error |
| Verify PDF → chunks → CozoDB round-trip | Success criteria | U4: Integration tests |
| Update stale doc comment in `src/chunk/mod.rs` | Stash `FF99F3D3` | U3: Dependency + error (bundled) |

## Implementation Units

### U1: PDF Parser Module

**What**: Create `src/parse/pdf.rs` with `parse_pdf_document()` that extracts
text from PDF bytes and returns a `ParsedDocument`.

**Files affected**:
- `src/parse/pdf.rs` (new) — PDF text extraction + page-based chunking
- `src/parse/mod.rs` — add `pub mod pdf;` and re-export

**Implementation details**:
- `parse_pdf_document(bytes: &[u8], source_path: &str) -> Result<ParsedDocument, GraphtorError>`
- Use `pdf_extract::extract_text_from_mem(bytes)` for text extraction
- Split extracted text at form-feed characters (`\x0c`) for page boundaries
- Within pages, split at double-newline paragraph boundaries when content
  exceeds 2000 chars (configurable constant)
- Generate `Chunk` records with `generate_chunk_id()` for stable SHA-256 IDs
- Title extraction: first non-empty line, or filename stem as fallback
- `heading_hierarchy`: page number as synthetic heading (e.g., `["Page 1"]`)
- `frontmatter`: always `None` for PDFs
- `references` and `code_snippets`: empty vecs (PDF text extraction doesn't
  preserve hyperlink or code block structure)

**Tests** (test-first):
1. Empty PDF bytes → returns error, not panic
2. Single-page PDF → single chunk with correct chunk_id and content
3. Multi-page PDF → multiple chunks split at page boundaries
4. Large page content → splits at paragraph boundaries within page
5. Title extraction from first line

**Execution posture**: Test-first. Write tests in `tests/parse_pdf_test.rs`
with embedded minimal PDF bytes (a valid 1-page PDF is ~200 bytes).

**Estimated scope**: 1 file new, 1 file modified, ~120 lines production,
~150 lines test. Under 2-hour rule.

### U2: Pipeline File-Type Dispatch

**What**: Modify `process_batch()` in `src/pipeline/mod.rs` to dispatch
files by extension — `.md` uses `parse_document()`, `.pdf` uses
`parse_pdf_document()`, others are skipped with a warning.

**Files affected**:
- `src/pipeline/mod.rs` — modify `process_batch()` function

**Implementation details**:
- Extract file extension via `file.extension().and_then(|e| e.to_str())`
- Match on extension:
  - `"md"` | `"markdown"` → existing `read_to_string` + `parse_document` path
  - `"pdf"` → `std::fs::read(file)` (binary) + `parse_pdf_document`
  - `_` → `debug!` log skip, continue to next file
- The read operation changes from `read_to_string` to `read` for PDF files
  because PDF is binary. This is the only change to the existing flow.
- All downstream code (embed, load) is unchanged — it operates on `ParsedDocument`.

**Tests** (characterization-first):
1. Existing pipeline tests must continue to pass (`.md` path unchanged)
2. New test: `.pdf` file in batch → routed to PDF parser → chunks loaded
3. New test: unknown extension (`.txt`) → skipped with no error
4. New test: mixed batch (`.md` + `.pdf`) → both processed correctly

**Execution posture**: Characterization-first (verify existing tests pass,
then add new dispatch tests).

**Estimated scope**: 1 file modified, ~20 lines changed, ~80 lines test.
Under 2-hour rule.

### U3: Dependency, Error Handling, and Doc Fix

**What**: Add `pdf-extract` to `Cargo.toml`, map its errors into
`GraphtorError::Parse`, and fix the stale doc comment in `src/chunk/mod.rs`.

**Files affected**:
- `Cargo.toml` — add `pdf-extract = "0.10"` dependency
- `src/chunk/mod.rs` — fix stale doc comment (line 6: references LanceDB/Kùzu)

**Implementation details**:
- Add `pdf-extract = "0.10"` to `[dependencies]` section
- The `pdf-extract` crate returns `anyhow::Error` from `extract_text_from_mem`.
  Map it to `GraphtorError::Parse { message, path }` in the `parse_pdf_document`
  function using `.map_err()`.
- No new error variants needed — `GraphtorError::Parse` already covers parse
  failures with an optional path field.
- Fix `src/chunk/mod.rs` line 6: change "linking `LanceDB` vectors to Kùzu
  graph nodes" to "linking vector embeddings to graph nodes in `CozoDB`"

**Tests**: `cargo check` + `cargo clippy` confirm clean compilation.

**Execution posture**: Direct implementation (no test-first needed for
dependency addition and doc comment fix).

**Estimated scope**: 2 files modified, ~5 lines changed. Under 2-hour rule.

### U4: Integration Tests

**What**: End-to-end test proving PDF → parse → embed → load → query works.

**Files affected**:
- `tests/parse_pdf_test.rs` (new) — PDF-specific parse tests
- `tests/pipeline_pdf_test.rs` (new) — pipeline integration with PDF files

**Implementation details**:
- Create a minimal valid PDF programmatically (PDF 1.0 spec allows a ~250-byte
  single-page document with plain text). Embed as a `const MINIMAL_PDF: &[u8]`
  byte literal in the test file.
- Test scenarios:
  1. Parse a minimal PDF → verify `ParsedDocument` fields
  2. Parse a multi-page PDF → verify page-based chunk splitting
  3. Pipeline integration: write a `.pdf` file to a temp dir, run
     `process_batch()`, verify chunks appear in CozoDB
  4. Round-trip: PDF → load → `search::keyword_search()` finds content
  5. Error case: corrupted PDF bytes → graceful error, not panic

**Execution posture**: Test-first (tests written before the parser in U1,
but run and verified red before U1 implementation makes them green).

**Estimated scope**: 2 new files, ~200 lines test. Under 2-hour rule.

## Dependency Graph

```text
U3 (Cargo.toml + doc fix)
 └─► U1 (PDF parser module) ─── depends on pdf-extract being in Cargo.toml
      └─► U2 (Pipeline dispatch) ─── depends on parse_pdf_document existing
           └─► U4 (Integration tests) ─── depends on pipeline dispatch working
```

Execution order: U3 → U1 → U2 → U4

Note: U4's test files can be *written* (red) at any point, but they won't
pass until U1 and U2 are complete. In practice, write U4 tests alongside U1
for TDD compliance.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Use `pdf-extract` 0.10, not `pdf_oxide` | `pdf_oxide` requires Rust 1.88; project MSRV is 1.75. `pdf-extract` has no MSRV conflict and is adequate for v1 text extraction. |
| Page-based chunking, not structural | `pdf-extract` returns flat text with form-feed page delimiters. Heading-based chunking would require heuristic detection (unreliable). Page boundaries are deterministic. |
| 2000-char max chunk size | Matches the approximate size of markdown H2-level chunks. Prevents single-page PDFs from producing oversized chunks that degrade embedding quality. |
| No new error variant | `GraphtorError::Parse` already covers "failed to parse document at path" — reuse rather than create a redundant `PdfParse` variant. |
| Empty `references` and `code_snippets` for PDFs | PDF text extraction loses hyperlink and code block structure. These fields remain empty rather than attempting unreliable heuristic extraction. |
| Synthetic heading hierarchy (`["Page N"]`) | Preserves the heading_hierarchy contract for graph traversal. Page numbers provide navigational context even though they aren't true headings. |
| Bundle stale doc fix with dependency task | `FF99F3D3` is a 1-line fix in the same area. Bundling avoids a separate PR for trivial cleanup. |

## Risks and Caveats

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `pdf-extract` extraction quality on complex layouts | Medium | Medium | Accept v1 limitations; `pdf_oxide` migration path documented in spike for v2 |
| PDF with scanned images (no text layer) | Low | Low | `pdf-extract` returns empty string; parser returns a zero-chunk document; pipeline logs a debug warning |
| Large PDFs (100+ MB) consume significant memory | Low | Medium | Document as known limitation; defer streaming/mmap to v2 |
| `pdf-extract` transitive dependencies add build time | Low | Low | Measure build time delta; acceptable for a ~95 KB crate |
| Form-feed page delimiter assumption | Low | Medium | Some PDF producers may not insert `\x0c`; fall back to splitting by paragraph count if no form-feeds found |

## Plan Hardening Signals

- [x] **Public API change**: `parse_pdf_document` is `pub(crate)`, not public
  API. `ParsedDocument` type is unchanged. **No public API change.**
- [ ] **Security/auth/permission**: No security-sensitive behavior.
- [ ] **Migration/destructive action**: No schema changes. `doc_chunks` relation
  accepts PDF chunks with no DDL modification.
- [ ] **External integration**: `pdf-extract` is a pure Rust crate with no
  external service dependency. Aligns with local-first principle.
- [ ] **High runtime/rollback risk**: Additive feature. Existing `.md` pipeline
  is completely unchanged. Rollback = remove `pdf-extract` from Cargo.toml.

**Requires plan hardening: no**

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification | Closure |
|---|---|---|---|
| U1 | None (library module) | Unit tests in `tests/parse_pdf_test.rs` | N/A |
| U2 | CLI `sync` command | Manual: add a PDF file to a local source in `sources.yaml`, run `graphtor-docs sync`, verify chunks in DB | Confirm sync logs show PDF file processed |
| U3 | Build system | `cargo check` + `cargo clippy` pass | N/A |
| U4 | None (test suite) | `cargo test` passes all new tests | N/A |

Post-implementation: run `graphtor-docs sync` with a real Microsoft Learn PDF
to validate extraction quality on production-like content. This is manual
verification, not automated — document results in a follow-up comment on the
backlog item.

## Plan Review

**Gate decision: PASS**

Reviewed by: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor,
Learnings Researcher, Architecture Strategist, Agent-Native Parity Reviewer.

Plan hardening required: **no** — confirmed. No public API changes, no schema
changes, no security-sensitive behavior, no external integrations. Purely
additive feature with clean rollback (remove dependency).

### Findings

#### P2 — Test file overlap between U1 and U4

U1 specifies tests in `tests/parse_pdf_test.rs` and U4 also creates
`tests/parse_pdf_test.rs`. This creates a width isolation overlap — both
units modify the same file. **Recommendation**: U1 should own all parse-level
tests in `tests/parse_pdf_test.rs` (TDD: write test, implement, verify green).
U4 should contain only pipeline integration tests in `tests/pipeline_pdf_test.rs`.
Remove the parse test scenarios from U4's scope to avoid duplication.

#### P2 — Missing user configuration note for sources.yaml

The plan does not document that users with `include: ["**/*.md"]` in their
`sources.yaml` source entries will need to add `"**/*.pdf"` to see PDF files
ingested. The default (no include patterns) passes all files, but explicit
include lists will filter out PDFs. **Recommendation**: Add a note to U2 or
the Runtime Verification section: "Users must add `**/*.pdf` to their source
include patterns in `sources.yaml` if they have explicit include lists."

#### P3 — API name may differ from actual pdf-extract API

The spike and plan reference `pdf_extract::extract_text_from_mem(bytes)`. The
actual API name should be verified against `pdf-extract` 0.10 docs during U1
implementation. This is a minor detail resolved at coding time.

#### P3 — Candle embedding tests may be slow

Per compound learning `cargo-test-candle-ml-codegen-slow-2026-04-30.md`,
tests that exercise the embedding model are slow. U4 integration tests that
include the embed step should use `model: None` to skip embedding unless
specifically testing the embed path, consistent with existing pipeline tests.

#### P3 — Future extensibility via trait-based dispatch

As more formats are added (DOCX is in the stash), the `match` on file
extension in `process_batch()` will grow. A trait-based parser dispatch
(e.g., `trait DocumentParser { fn parse(&self, bytes: &[u8], path: &str) -> ... }`)
would be cleaner long-term. Not actionable now — a `match` for 2-3 formats is
adequate. Note for future planning.

### Constitution Compliance

| Principle | Status |
|---|---|
| Local-first | ✅ `pdf-extract` is a pure Rust crate, no cloud dependencies |
| Lightweight footprint | ✅ 95.9 KB source, justified against Technology Stack table in spike |
| Data pipeline integrity | ✅ Chunk IDs via `generate_chunk_id()` (SHA-256), idempotent parsing |
| MCP-native interface | ✅ No MCP changes needed — existing tools search all chunks |
| Automation & reproducibility | ✅ Deterministic pipeline, idempotent stages |
| `#![forbid(unsafe_code)]` | ✅ Our code remains safe; dependency internals are out of scope |
| `unwrap_used`/`expect_used` deny | ✅ Plan specifies `.map_err()` error mapping |
| Test-first discipline | ✅ U1 test-first, U2 characterization-first, U4 test-first |

### Learnings Cross-Reference

| Learning | Relevance | Status |
|---|---|---|
| `file-error-path-semantics-pathbuf` | PDF errors in `process_batch` use same `FileError.path` patterns | ✅ Consistent |
| `pipeline-source-metadata-lookup` | Not directly applicable (no new source type) | ✅ N/A |
| `cargo-test-candle-ml-codegen-slow` | U4 tests should skip embed step | P3 noted above |

### Conclusion

Plan is well-structured with clear decomposition, proper dependency ordering,
and sound architectural decisions. All P2 findings are addressable during
implementation without plan revision. Proceed to harvest.
