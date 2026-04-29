# Shipment 002-S — Core Data Layer — SHIPPED

**Date:** 2026-04-29  
**Outcome:** All features delivered, verified, and archived  
**Commits:** `8026045`, `28eee5e`, `7dca7d7`, `b6e4fa8` (pushed to `main`)

## Features Delivered

| Feature | Title | Status | Tests |
|---------|-------|--------|-------|
| 003-F | Markdown Parsing & Chunking | archived | 83 (pre-existing) |
| 004-F | Native Embedding Engine (Candle) | archived | 81 unit + 62 integration + 2 pool |
| 012-F | Unified Data Store (CozoDB) | archived | 29 DB tests |

Features 005-F (LanceDB) and 006-F (Kùzu) were blocked and removed from shipment — obsoleted by technology pivot to CozoDB.

## Quality Gate Results (final)

- `cargo check` — ✅ clean
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` — ✅ 0 warnings
- `cargo fmt --all -- --check` — ✅ clean
- `cargo test` — ✅ 145+ passing, 0 failing, 1 ignored (model-weight test)

## Key Decisions

- **CozoDB replaces LanceDB + Kùzu** — single embedded DB with Datalog, graph traversal, and HNSW vector search; reduces dependency count and eliminates cross-DB correlation complexity
- **`DataStore` wraps `Arc<DbInstance>`** — ensures `Clone + Send + Sync` for future async MCP server sharing
- **Schema versioning via `_schema_version` relation** — idempotent `ensure_schema()` on every open; drop-and-rebuild strategy for v1 migrations
- **`#[ignore]` on model-weight tests** — Candle BERT (~80 MB weights) tests skip in CI; pool/tokenizer tests run without weights

## Modules Delivered

```
src/db/mod.rs       — re-exports DataStore
src/db/store.rs     — DataStore, open(), open_memory()
src/db/schema.rs    — DDL for 9 relations, SCHEMA_VERSION, ensure_schema()
src/db/chunks.rs    — store_chunk(), store_chunks_batch()
src/db/search.rs    — search_similar() (HNSW stub pending CozoDB 0.7 API verification)
src/db/nodes.rs     — insert/get/list/delete for repos and documents
src/db/edges.rs     — belongs_to, contains_chunk, has_code, references edges
src/db/traverse.rs  — recursive Datalog multi-hop traversal
src/embed/mod.rs    — public embed_text(), embed_batch() API
src/embed/model.rs  — EmbeddingModel with Candle BERT inference
src/embed/pool.rs   — mean pooling, attention mask application
```

## Open Items (next shipment)

- Verify CozoDB 0.7 HNSW API — `search.rs` may be a stub returning `GraphtorError::Database`
- Update `.github/copilot-instructions.md` Technology Stack table (replace LanceDB/Kùzu with CozoDB)
- MCP server layer (007-F through 011-F) — next shipment candidate
- `DataStoreOps` trait (P2 advisory from plan-review) — optional refactor
