---
type: session-memory
timestamp: 2026-05-01T12:20:00-07:00
agent: ship
shipment: 008-S
feature: 016-F
pr: "14"
outcome: shipped
branch: feat/vector-search
merge_commit: 5ac6835
---

## Session Summary

Shipped **016-F** (vector storage and semantic search) via **008-S** through PR #14.

## What Was Delivered

- `src/db/vectors.rs` — new module: `upsert_vector`, `search_by_vector`, `cosine_similarity`
- `src/db/schema.rs` — `doc_vectors { chunk_id => embedding }` relation, SCHEMA_VERSION 2
- `src/db/search.rs` — `search_by_text` now includes `source_id` in results
- `src/pipeline/mod.rs` — `compute_embeddings` returns `HashMap<String, Vec<f32>>`; vectors stored post-chunk-upsert to prevent orphans
- `src/mcp/server.rs` — `search_semantic` MCP tool with `invalid_params` error for missing model
- `tests/db_vectors_test.rs` — 8 TDD integration tests, all green

## Issues Encountered

### 1. Copilot Review (4 comments, all resolved)
- `scored.truncate(limit)` → scan-past-top-k for correct limit enforcement
- `ErrorData::internal_error` → `ErrorData::invalid_params` for missing model
- `compute_and_store_embeddings` → `compute_embeddings` returning `HashMap` (pipeline restructure)
- `cosine_similarity` dimension mismatch guard added

### 2. CI Clippy Failure (1 cycle)
- `clippy::useless_conversion` on `vecs.into_iter()` at `src/pipeline/mod.rs:462`
- Triggered on CI Rust 1.95+, missed by older local toolchain
- Fix: `vecs.into_iter().chunks(N)` → `vecs.chunks(N)`
- Compound learning written: `docs/compound/workflow-issues/clippy-useless-conversion-ci-rust-version-skew-2026-05-01.md`

## Decisions

- Brute-force O(n) cosine similarity chosen over HNSW — justified by Lightweight Footprint principle; HNSW planned for future iteration
- Embeddings stored as JSON string in CozoDB (no native float array type in current Cozo version)
- Schema version bump (1→2) is additive — no migration needed for existing DBs

## Next Steps

- Consider adding `rust-toolchain.toml` to pin toolchain and prevent future CI/local version skew
- HNSW vector indexing is the planned upgrade path (backlogged, not yet scheduled)
- `search_by_text` `source_id` field now available in `SearchResult` — downstream MCP tools may use it for filtering
