---
title: "PDFium dual backend for large PDF ingestion"
source: "docs/decisions/2026-05-04-pdfium-dual-backend-deliberation.md"
spike: "docs/decisions/2026-05-04-pdfium-dual-backend-spike.md"
date: 2026-05-04
status: draft
---

## Problem Frame

`pdf-extract`'s underlying `lopdf` library eagerly parses all PDF objects in
the xref table via `Document::load_mem()`. For a 104 MB PDF this takes >20
minutes before our code even runs. The 023-F optimizations (shipped in 014-S)
improved extraction and chunking but cannot fix this architectural bottleneck.

`pdfium-render` wraps Google's PDFium engine with dynamic loading at runtime.
Document opening is lazy (xref index only), making it O(1) regardless of file
size. The DLL is an optional runtime enhancement — without it, the system
behaves identically to current code.

**Affected code path**: `src/parse/pdf.rs` → `parse_pdf_document()` →
`PdfExtractBackend::parse()` → `pdf_extract::Document::load_mem()`.

## Requirements Trace

| # | Requirement (from deliberation) | Implementation action |
|---|---|---|
| R1 | 104 MB Cosmos DB PDF completes ingestion | Unit 1: `PdfiumBackend` with lazy document opening |
| R2 | Small/medium PDFs continue using two-pass pipeline | Unit 3: routing logic in `parse_pdf_document()` |
| R3 | Binary compiles and runs without PDFium DLL | Unit 1: `PdfiumBindError::NotAvailable` fallback path |
| R4 | Graceful degradation when DLL absent | Unit 3: warn-level log, fall through to pdf-extract |
| R5 | Error distinction: binding vs extraction failure | Unit 1: `PdfiumBindError` enum |
| R6 | DLL discovery: env var → exe dir → system | Unit 2: `load_pdfium()` search chain |

## Implementation Units

### Unit 1: `PdfiumBackend` struct and text extraction

**What**: Implement `PdfiumBackend` with `try_parse()` and `PdfiumBindError`
enum in `src/parse/pdf.rs`.

**Files**: `src/parse/pdf.rs`

**Changes**:
- `PdfiumBindError` enum: `NotAvailable(String)`, `ExtractionFailed(String)`
  with `Display` impl.
- `PdfiumBackend` struct with `try_parse(bytes, source_path)`:
  - Calls `load_pdfium()` (Unit 2).
  - Opens document lazily via `pdfium.load_pdf_from_byte_slice(bytes, None)`.
  - Iterates pages with `document.pages().get(idx)`.
  - Extracts text per page via `page.text()?.all()`.
  - Delegates to existing `extract_title_from_pages()` and
    `chunk_pdf_pages()` for chunking.
  - Returns `ParsedDocument` with same structure as `PdfExtractBackend`.
- Structured tracing: log page count at document open, chunk count at
  completion, backend attribution.

**Tests**:
- `pdfium_backend_not_available_returns_error`: Verify `try_parse` returns
  `NotAvailable` when no DLL is present (expected in CI).
- `pdfium_bind_error_display_formatting`: Verify `Display` impl for both
  variants.

**Execution posture**: Test-first for `PdfiumBindError` display; the
`try_parse` test is a characterization test that confirms graceful failure
in CI (where no DLL is available).

**Estimated scope**: 1 file, 3 functions, 2 test scenarios. ✅ 2-hour rule.

### Unit 2: DLL discovery via `load_pdfium()`

**What**: Implement the three-tier DLL search in `PdfiumBackend::load_pdfium()`.

**Files**: `src/parse/pdf.rs`

**Changes**:
- `load_pdfium() -> Result<Pdfium, PdfiumBindError>`:
  1. `$GRAPHTOR_PDFIUM_PATH` env var (directory path).
  2. Executable's directory via `std::env::current_exe()`.
  3. System library search via `Pdfium::bind_to_system_library()`.
- Uses `Pdfium::pdfium_platform_library_name_at_path()` for cross-platform
  library file naming.
- Debug-level tracing for which search path succeeded.

**Tests**:
- `pdfium_load_fails_gracefully_without_dll`: Confirm `load_pdfium()` returns
  `NotAvailable` and does not panic when no DLL exists.
- (Integration test with DLL is manual/local only — not CI-runnable.)

**Execution posture**: Test-first for graceful failure.

**Estimated scope**: 1 file, 1 function, 1 test scenario. ✅ 2-hour rule.

### Unit 3: Routing logic in `parse_pdf_document()`

**What**: Modify the public `parse_pdf_document()` entry point to try
`PdfiumBackend` first for large PDFs, with graceful fallback.

**Files**: `src/parse/pdf.rs`

**Changes**:
- When `bytes.len() >= LARGE_PDF_THRESHOLD`:
  - Call `PdfiumBackend::try_parse()`.
  - On `Ok(doc)`: return immediately.
  - On `Err(NotAvailable)`: warn, fall through to `PdfExtractBackend`.
  - On `Err(ExtractionFailed)`: error-level log, fall through.
- When `bytes.len() < LARGE_PDF_THRESHOLD`: use `PdfExtractBackend` directly
  (no change).
- Update module doc comment to describe the dual-backend architecture.

**Tests**:
- `parse_pdf_document_small_file_uses_pdf_extract`: Confirm files below
  threshold go through `PdfExtractBackend` (existing tests already cover
  this — verify they still pass).
- `parse_pdf_document_large_file_falls_back_without_pdfium`: Create a
  synthetic byte slice ≥ `LARGE_PDF_THRESHOLD`, confirm it attempts pdfium
  (logs the warning), then falls through to pdf-extract.

**Execution posture**: Test-first for fallback behavior.

**Estimated scope**: 1 file, 1 function, 2 test scenarios. ✅ 2-hour rule.

### Unit 4: Cargo dependency and clippy compliance

**What**: Add `pdfium-render` to `Cargo.toml` and ensure all quality gates
pass.

**Files**: `Cargo.toml`, `src/parse/pdf.rs`

**Changes**:
- `pdfium-render = { version = "0.9", default-features = false, features = ["thread_safe", "pdfium_latest"] }`
- Fix all clippy pedantic warnings (doc_markdown backticks, cast_sign_loss,
  unused imports).
- Verify `cargo fmt --all -- --check`.
- Verify `cargo test` — all existing tests pass, new tests pass.

**Tests**: All existing + new tests from Units 1-3.

**Execution posture**: Verification-first (dependency already added on branch).

**Estimated scope**: 2 files, 0 new functions, 0 new test scenarios.
✅ 2-hour rule.

## Dependency Graph

```text
Unit 4 (Cargo dep) → Unit 2 (DLL discovery) → Unit 1 (backend struct)
                                              ↘ Unit 3 (routing logic)
```

Unit 4 must be verified first (dependency compiles). Unit 2 provides the
binding that Unit 1 calls. Unit 3 wires Unit 1 into the public API. Units 1
and 3 can proceed in parallel after Unit 2.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Dynamic loading, not static linking | Preserves single-binary compilation. DLL is optional runtime enhancement. |
| `PdfiumBindError` local enum, not `GraphtorError` variant | Binding errors are internal dispatch detail, not public API errors. The caller maps them to appropriate `GraphtorError` or fallback behavior. |
| Page-based chunking only (no heading-aware for pdfium path) | Matches the existing large-file fast path behavior. Heading-aware extraction via `unscaled_font_size()` is a follow-up enhancement. |
| Reuse existing `chunk_pdf_pages()` and `extract_title_from_pages()` | Ensures output format is identical to pdf-extract fallback. No new chunking logic needed. |
| `default-features = false` for pdfium-render | Excludes `image` crate dependency (not needed for text extraction). Reduces transitive dependency count. |

## Risks and Caveats

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Text quality differs between pdfium and pdf-extract | Low | Medium | Both extract rendered text. Pdfium is Chromium's engine — quality is high. Manual spot-check during testing. |
| `pdfium-render` API changes in future versions | Low | Low | Version pinned to `0.9`. Update as needed. |
| Clippy pedantic on pdfium-render types (cast_sign_loss on page_count) | High | Low | Use `usize::try_from()` with `.unwrap_or(usize::MAX)` fallback. |
| CI tests can't exercise pdfium path (no DLL) | High | Medium | Tests verify graceful failure (NotAvailable). Integration testing is manual/local. |

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | No | `parse_pdf_document()` signature unchanged. Output format identical. |
| Security, auth, permission-sensitive behavior | No | No auth or permissions involved. DLL is loaded from controlled paths. |
| Migration, backfill, destructive action | No | No data migration. Existing chunks unchanged. |
| External integration or external dependency | Yes (partial) | Optional runtime DLL. But system works without it — graceful degradation. |
| High runtime, rollout, or rollback risk | No | Feature is additive. Rollback = remove pdfium-render dep and revert routing. |

**Requires plan hardening: no**

The external dependency is optional and the system degrades gracefully without
it. No public API changes, no data migration, no security surface.

## Runtime Verification and Closure

### Changed runtime surfaces

- **CLI `sync` command**: The parse phase now attempts pdfium for large PDFs.
  No CLI flag changes. Behavior is transparent to the user.

### Verification

- With DLL present: `sync --full --no-embed` on the 104 MB PDF completes
  within the SLO target (<30s for parse+chunk).
- Without DLL present: `sync --full --no-embed` on the 104 MB PDF logs a
  pdfium-unavailable warning and falls through to pdf-extract (which will be
  slow but functionally correct for smaller files at the threshold boundary).
- Existing small/medium PDF tests continue to pass unchanged.

### Operational closure

- Document the `GRAPHTOR_PDFIUM_PATH` environment variable in README.
- Add a note about PDFium DLL acquisition to the developer setup guide.
- No monitoring or rollback trigger needed — this is a local-only tool.

## Plan Review

**Date**: 2026-05-04
**Reviewers**: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor,
Learnings Researcher, Architecture Strategist (GPT-5.4 cross-model)
**Gate Decision**: **ADVISORY**
**Plan hardening required**: No (external dependency is optional, graceful degradation)

### Gate Rationale

No P0 or P1 findings after deduplication. Seven P2 findings recorded as
awareness items. The plan's error handling approach (match on `PdfiumBindError`
variants in `parse_pdf_document()` with fallback) is sound and simpler than
the `From` impl alternative suggested by multiple reviewers. All requirements
(R1–R6) are explicitly mapped to implementation units. The spike and
deliberation provide strong institutional backing with no contradictions.

### Merged Findings

#### P2 — Moderate (record as backlog follow-up)

**P2-1: Error conversion path specificity**
*Sources: Constitution Reviewer, Rust Reviewer*
The plan specifies match arms (`NotAvailable` → warn + fallback,
`ExtractionFailed` → error + fallback) but multiple reviewers wanted an
explicit `From<PdfiumBindError> for GraphtorError` impl. The plan's match-based
approach is valid and simpler — no propagation to callers occurs. If both
backends fail, the final `PdfExtractBackend` error surfaces as `GraphtorError::Parse`.
**Action**: Verify during implementation that no `PdfiumBindError` leaks beyond
`parse_pdf_document()`.

**P2-2: Lifetime safety of `load_pdf_from_byte_slice`**
*Source: Rust Reviewer*
`pdfium-render`'s document may borrow the input byte slice. The plan's
`try_parse(bytes: &[u8], ...)` signature naturally constrains this — `bytes`
outlives the document because both live within the same function scope.
**Action**: Verify during implementation that `PdfDocument` does not escape
`try_parse()`.

**P2-3: Diagnostic message for missing DLL**
*Source: Constitution Reviewer*
Plan has R4 (graceful degradation with warn-level log) but could be more
specific about guiding users to download `PDFium`. The warn message should
include the expected search paths and a hint about `GRAPHTOR_PDFIUM_PATH`.
**Action**: Implement descriptive warn message in Unit 3 routing logic.

**P2-4: File size — consider module split**
*Source: Architecture Strategist (GPT-5.4)*
`pdf.rs` is ~1200 lines and adding a second backend increases complexity.
Splitting into `pdf/mod.rs`, `pdf/pdf_extract_backend.rs`,
`pdf/pdfium_backend.rs` would improve cohesion.
**Action**: Acceptable as-is for this shipment. Record as follow-up refactor
if the file exceeds ~1500 lines.

**P2-5: Chunk ID discriminator consistency**
*Source: Learnings Researcher*
Prior learning (`pdf-chunk-id-uniqueness-pattern-2026-05-01`) established
the `{source_path}#page={N}#segment={M}` discriminator format. The plan
reuses `chunk_pdf_pages()` which already implements this format — no
inconsistency, but verify during implementation.
**Action**: Spot-check chunk IDs from pdfium path match pdf-extract path
for the same content.

**P2-6: Observability around fallback path**
*Source: Architecture Strategist*
When pdfium fails and fallback succeeds, the backend attribution should be
clear in tracing. Include `backend = "pdfium"` or `backend = "pdf-extract"`
structured field in the completion log.
**Action**: Add `backend` field to the `tracing::info!` at parse completion.

**P2-7: Negative scope boundary for Unit 3**
*Source: Scope Boundary Auditor*
Unit 3 modifies only `parse_pdf_document()` in `src/parse/pdf.rs`. It does
NOT modify `PdfExtractBackend`, existing helpers, existing tests, or error
types. The plan states "1 file, 1 function" which implicitly bounds this.
**Action**: No plan change needed. Implementation review should verify scope.

#### P3 — Advisory

**P3-1**: Plan says `page_count` is `i32` (risk table) but `PdfPages::len()`
returns `u16`. The `u16 → usize` cast is always safe. Correct the risk
table during implementation. *(Rust Reviewer)*

**P3-2**: A shared `PdfBackend` trait is premature — only two backends,
simple centralized selection. Revisit if backend count grows.
*(Architecture Strategist, Scope Auditor)*

**P3-3**: Thread safety of `pdfium-render` with `thread_safe` feature is
adequate for the current single-threaded pipeline. Document if concurrent
PDF parsing is added later. *(Rust Reviewer)*

**P3-4**: `PdfiumBackend` should be a unit struct (matching `PdfExtractBackend`
pattern) with associated functions, not a struct with fields.
*(Rust Reviewer)*

**P3-5**: Reuse the existing `LARGE_PDF_THRESHOLD` constant for routing
(plan implies this but doesn't state it explicitly). *(Rust Reviewer)*

### Learnings Confirmation

The Learnings Researcher confirmed **no contradictions** with institutional
knowledge. The 2026-05-04 spike explicitly recommends this approach. The
prior 2026-05-03 spike rejection of pdfium-render is superseded by new
evidence (lopdf architectural limitation confirmed after 023-F shipped).

### Acknowledged Decisions

- Error handling via match arms (not `From` impl) — simpler, correct ✅
- Page-based chunking only for pdfium path — heading-aware is follow-up ✅
- Same file placement (not module split) — acceptable for now ✅
- No shared trait — premature abstraction avoided ✅

### Recommendation

**Proceed to harvest.** Address P2-3 (diagnostic message) and P2-6
(backend attribution in tracing) during implementation. All other P2
items are verification-during-implementation or backlog follow-ups.
