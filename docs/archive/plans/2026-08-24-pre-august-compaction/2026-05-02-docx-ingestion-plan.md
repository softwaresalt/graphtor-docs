---
title: "Word Document (.docx) Ingestion"
type: impl-plan
date: 2026-05-02
source: docs/decisions/2026-05-02-docx-ingestion-crate-spike.md
linked_feature: "020-F"
requires_hardening: false
---

## Problem Frame

graphtor-docs currently parses Markdown (`.md`) and PDF (`.pdf`) documents.
Feature 020-F adds a third parser backend for Word documents (`.docx`) so the
pipeline can ingest OOXML content. The integration pattern is identical to PDF:
a new `parse_docx_document()` function returns `ParsedDocument`, and a new
match arm in `process_batch()` dispatches `.docx` files to it.

The spike (`docs/decisions/2026-05-02-docx-ingestion-crate-spike.md`) confirmed
feasibility with `docx-rs` (OOXML parser with heading-style awareness), with a
manual `zip` + `quick-xml` fallback if `docx-rs` proves inadequate.

## Requirements Trace

| Requirement | Implementation Unit |
|---|---|
| Parse .docx files into pipeline | Unit 1: DOCX parser module |
| Heading-aware chunking from paragraph styles | Unit 1: DOCX parser module |
| Table text extraction | Unit 1: DOCX parser module |
| List detection and formatting | Unit 1: DOCX parser module |
| File-type dispatch in pipeline | Unit 2: Pipeline wiring |
| Large file handling | Unit 1: DOCX parser module (size guard) |

## Implementation Units

### Unit 1: DOCX Parser Module — `src/parse/docx.rs`

**What**: New module that reads `.docx` bytes, walks the OOXML paragraph tree
to extract text with heading-level awareness, chunks at heading boundaries,
and returns `ParsedDocument`.

**Files**:
- `src/parse/docx.rs` — New module (~200 lines):
  - `parse_docx_document(bytes: &[u8], source_path: &str) -> Result<ParsedDocument>`
  - Internal: `extract_sections(docx: &Docx) -> Vec<DocxSection>`,
    `chunk_docx_sections(sections: &[DocxSection], path: &str) -> Result<Vec<Chunk>>`,
    `extract_title(sections: &[DocxSection], fallback: &str) -> String`,
    `flatten_table(table: &Table) -> String`
- `src/parse/mod.rs` — Add `pub mod docx;` and export `parse_docx_document` (2 lines)
- `Cargo.toml` — Add `docx-rs = "0.4"` dependency (1 line)

**Tests**: Integration test file `tests/parse_docx_test.rs`:
- `parse_docx_empty_document` — empty .docx produces zero chunks
- `parse_docx_single_paragraph` — single paragraph produces one chunk
- `parse_docx_heading_structure` — H1/H2 headings produce correct hierarchy
- `parse_docx_table_extraction` — table text appears in chunk content
- `parse_docx_invalid_bytes` — garbage bytes produce `GraphtorError::Parse`

**Posture**: Test-first. Build a minimal `.docx` test fixture programmatically
using `docx-rs` write API (same crate, no additional deps for test fixtures).

**Compound learnings to apply**:
- **Chunk ID uniqueness** (`pdf-chunk-id-uniqueness-pattern`): DOCX documents
  may have repeated boilerplate headers/footers. Include section index in
  `chunk_id_source` discriminator.
- **Path normalization** (`windows-path-normalization`): `source_path` should
  use forward slashes when stored as chunk key.
- **pdf-extract API pattern** (`pdf-extract-api-usage-pattern`): Follow the same
  error mapping pattern (`map_err` to `GraphtorError::Parse`), title extraction
  heuristic, and empty `references`/`code_snippets` convention.

### Unit 2: Pipeline Wiring — `.docx` dispatch arm

**What**: Add `.docx` match arm to `process_batch()` file-type dispatch so
`.docx` files are routed to `parse_docx_document()`.

**Files**:
- `src/pipeline/mod.rs` — Add match arm for `"docx"` extension (~5 lines)
- `src/parse/mod.rs` — Ensure `parse_docx_document` is re-exported (done in Unit 1)

**Tests**: Integration test `tests/pipeline_docx_test.rs`:
- `docx_flows_through_pipeline` — end-to-end: .docx file → chunks in CozoDB

**Posture**: Test-first. Depends on Unit 1.

## Dependency Graph

```text
Unit 1 (DOCX Parser Module)
  └─ Unit 2 (Pipeline Wiring) — needs parse_docx_document()
```

Linear dependency chain. No parallelism possible.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Use `docx-rs` over manual XML parsing | `docx-rs` handles OOXML edge cases (complex runs, character formatting, nested elements) that manual `zip` + `quick-xml` would need to duplicate. 320 KB is acceptable. |
| Heading detection via `w:pStyle` | Standard OOXML convention. `Heading1`–`Heading9` and `Title` styles map directly to heading levels. |
| Empty `references` and `code_snippets` | Word documents do not have deterministic link/code structure like Markdown. Hyperlinks could be extracted in v2 but are not reliable for graph edges. |
| Flatten tables to pipe-delimited text | Preserves table content in a searchable format without requiring table-aware chunking. Simple and effective for v1. |
| Include section index in chunk ID | Prevents duplicate chunk IDs from repeated boilerplate (compound learning). |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| `docx-rs` read API may not expose heading styles | Fallback to manual `zip` + `quick-xml`. `parse_docx_document` signature stays the same. |
| Large files (100+ MB) may OOM | Add a file-size warning log at 50 MB. `docx-rs` loads full document tree in memory. |
| Complex formatting (tracked changes, comments) | Ignore for v1. These are metadata, not primary content. |
| `.doc` (legacy binary format) not supported | Document limitation clearly. Only `.docx` (OOXML) is in scope. |
| `docx-rs` maintenance (last release 2024) | The crate wraps stable XML+ZIP. Even without updates, OOXML format is stable. |

## Plan Hardening Signals

- Public API, schema, or contract change: **No** — internal parser, no public API change
- Security, auth, permission, or compliance-sensitive behavior: **No**
- Migration, backfill, destructive data/config action: **No**
- External integration, operator checkpoint, or external dependency: **No**
- High runtime, rollout, or rollback risk: **No** — additive feature, no existing behavior changed

**Requires plan hardening: no** — purely additive parser backend following the
established PDF pattern. No schema changes, no migrations, no external integrations.
Can be removed by reverting the Cargo.toml + module additions.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification |
|---|---|---|
| Unit 1 | None (library module) | Unit + integration tests pass |
| Unit 2 | CLI (`sync` subcommand) | `sources.yaml` with `include: ["**/*.docx"]` processes .docx files into CozoDB |

**Closure**: No monitoring or rollback needed — additive parser feature with no
production deployment. Verification is via integration tests and manual
`cargo run -- sync` with a test `sources.yaml` containing `.docx` files.

## Plan Review

**Gate Decision: PASS**
**Date**: 2026-05-02
**Reviewers**: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor, Learnings Researcher

### Hardening Assessment

No hardening signals present. Purely additive parser backend following the
established PDF integration pattern. No schema changes, no external integrations,
no migrations.

**Hardening requirement satisfied**: Yes (no hardening needed).

### Findings

| # | Severity | Persona | Finding | Recommendation |
|---|---|---|---|---|
| 1 | P3 | Constitution | `docx-rs` should be checked for `unsafe` usage before final dependency commit | Run `cargo geiger` or audit crate source during implementation |
| 2 | P2 | Rust | The spike notes medium confidence in `docx-rs` read API for heading style detection. The plan should include an explicit "prototype validation" step before committing to the full module | Add a characterization test in Unit 1 that verifies `w:pStyle` heading detection works before writing the full chunker. If it fails, pivot to `zip` + `quick-xml` fallback |
| 3 | P3 | Scope | Risk of `docx-rs` introducing `xml-rs` as a transitive dep alongside existing `quick-xml` (if used elsewhere) — potential dep duplication | Verify during implementation; not blocking |
| 4 | P3 | Learnings | The `pdf-extract-api-usage-pattern` learning references title extraction via first non-empty line. For DOCX, the `Title` paragraph style is more reliable than positional heuristic | Prefer `w:pStyle = "Title"` detection over first-line heuristic when available, with first-line as fallback |

### Summary

One P2 finding (prototype validation step for `docx-rs` heading detection) and
three P3 advisories. No P0 or P1 issues. The P2 finding does not block harvest —
it recommends an implementation-time verification step that is already implied by
the "medium confidence" assessment in the spike. The plan follows established PDF
patterns, correctly references all relevant compound learnings, and has clean
unit boundaries. Proceeds to harvest.
