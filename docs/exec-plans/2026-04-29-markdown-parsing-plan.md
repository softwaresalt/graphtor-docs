# Implementation Plan: 003-F — Markdown Parsing & Chunking

**Feature:** 003-F  
**Status:** Implementation complete — this plan validates coverage  
**Date:** 2026-04-29

## Problem Frame

The ingestion pipeline requires deterministic, AST-based markdown parsing that
splits documents into semantic chunks at heading boundaries, extracts hyperlink
references as graph edges, isolates fenced code blocks, strips YAML frontmatter,
and generates stable SHA-256 chunk identifiers for cross-database correlation.

All parsing runs in-process via `pulldown-cmark` with zero LLM dependency and
100% precision on explicit structural elements.

## Requirements Trace

| Requirement | Implementation |
|---|---|
| pulldown-cmark AST event stream | `src/parse/ast.rs` — `parse_ast()` |
| Heading-based splitting (H1/H2/H3) | `src/parse/chunker.rs` — `chunk()` |
| Link/edge extraction | `src/parse/links.rs` — `extract()` |
| Code block isolation | `src/parse/code.rs` — `extract()` |
| YAML frontmatter stripping | `src/parse/frontmatter.rs` — `strip()` |
| Stable chunk_id (SHA-256) | `src/chunk/id.rs` — `generate_chunk_id()` |
| Pipeline entry point | `src/parse/mod.rs` — `parse_document()` |

## Implementation Units

### Unit 1: AST Walker (COMPLETE)

- **Files:** `src/parse/ast.rs`
- **Tests:** `tests/parse_ast_test.rs`
- **Verification:** Heading, link, code block, paragraph events extracted correctly

### Unit 2: Heading Chunker (COMPLETE)

- **Files:** `src/parse/chunker.rs`
- **Tests:** `tests/parse_chunker_test.rs`
- **Verification:** H1/H2/H3 boundaries produce separate chunks; H4+ stays inline

### Unit 3: Link Extraction (COMPLETE)

- **Files:** `src/parse/links.rs`
- **Tests:** `tests/parse_links_test.rs`
- **Verification:** References carry source_chunk_id, target_path, link_text, anchor

### Unit 4: Code Block Extraction (COMPLETE)

- **Files:** `src/parse/code.rs`
- **Tests:** `tests/parse_code_test.rs`
- **Verification:** Language tag, content, parent chunk_id captured

### Unit 5: Frontmatter Stripping (COMPLETE)

- **Files:** `src/parse/frontmatter.rs`
- **Tests:** `tests/parse_frontmatter_test.rs`
- **Verification:** YAML delimiters detected, metadata extracted, body returned clean

### Unit 6: Chunk ID Generation (COMPLETE)

- **Files:** `src/chunk/id.rs`
- **Tests:** Unit tests in `src/chunk/id.rs`
- **Verification:** SHA-256 of content + "\0" + source_path; deterministic and stable

## Dependency Graph

```text
Unit 1 (AST) → Unit 2 (Chunker) → Units 3, 4 (Link/Code extraction)
Unit 5 (Frontmatter) → Unit 1 (strips before AST walk)
Unit 6 (Chunk ID) → Unit 2 (ID assigned per chunk)
```

## Decisions and Rationale

1. **H1/H2/H3 as split boundaries** — balances granularity (too deep = too many
   tiny chunks) with semantic coherence (H4+ stays within parent context).
2. **SHA-256 over UUID** — deterministic IDs enable idempotent re-ingestion.
3. **Normalized content in chunks** — reconstructed from AST rather than raw
   slicing, ensuring consistent formatting for embeddings.

## Risks and Caveats

- **Risk:** Edge cases in malformed markdown (unclosed fences, nested headings).
  **Mitigation:** pulldown-cmark is spec-compliant and handles these gracefully.
- **Risk:** Large documents produce many chunks.
  **Mitigation:** Future pagination/batching in the embed stage.

## Plan Hardening Signals

- public API, schema, or contract change: **No** — internal module, stable types
- security, auth, permission, or compliance-sensitive behavior: **No**
- migration, backfill, destructive data/config action: **No**
- external integration, operator checkpoint, or external dependency: **No**
- high runtime, rollout, or rollback risk: **No**

**Requires plan hardening: no**

## Runtime Verification and Closure

- **Runtime surface:** None (library module, no CLI/API exposure yet)
- **Verification:** `cargo test` passes all parse_* integration tests
- **Closure:** Feature considered absorbed when all 6 test files pass green

---

## Plan Review

**Gate decision: PASS**  
**Date:** 2026-04-29  
**Plan hardening required:** No (all signals absent)  
**Plan hardening present:** N/A

### Reviewer Findings

#### Constitution Reviewer — 0 findings

All five core principles satisfied:
- Local-first: pulldown-cmark in-process, no network
- Lightweight footprint: single crate dependency
- Data pipeline integrity: deterministic AST parsing, stable SHA-256 chunk_ids
- MCP-native: module outputs feed into future MCP tools
- Automation: idempotent — same input always produces same output

#### Rust Reviewer — 0 findings

- Error handling via `GraphtorError::Parse` variant ✅
- All public types derive `Serialize`/`Deserialize` ✅
- Module structure follows `src/{module}/mod.rs` convention ✅
- `pub(crate)` default visibility ✅
- Tests exist for all 6 submodules ✅

#### Scope Boundary Auditor — 0 findings

- All units stay within markdown parsing concern
- No scope creep into embedding, storage, or MCP
- Width isolation maintained (code only, no config/docs changes)

#### Learnings Researcher — 0 findings

- No compound learnings exist yet (empty library)
- No contradictions with prior decisions

#### Architecture Strategist — 0 findings

- Clean module decomposition (ast → chunker → links/code extraction)
- Types module provides clear inter-module contracts
- Dependency graph is acyclic and well-sequenced

### Summary

This is a retroactive validation plan for already-implemented and tested code.
All personas confirm the implementation aligns with project conventions. No
action required — proceed to harvest confirmation.
