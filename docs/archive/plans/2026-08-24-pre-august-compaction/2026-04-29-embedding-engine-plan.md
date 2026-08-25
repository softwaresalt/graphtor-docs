# Implementation Plan: 004-F — Native Embedding Engine

**Feature:** 004-F  
**Status:** Not yet implemented  
**Date:** 2026-04-29

## Problem Frame

The ingestion pipeline requires in-process generation of 384-dimensional dense
vectors from parsed text chunks. The embedding engine must run entirely locally
with zero network calls, using the `all-MiniLM-L6-v2` model (~80 MB) via the
Candle framework (`candle-core`, `candle-transformers`).

The module sits between the parse stage (which produces `Chunk` values) and the
store stage (which persists vectors alongside chunk metadata). It must support
both single-chunk and batch embedding with deterministic output for the same
input text.

**Crate dependencies required:**
- `candle-core` — tensor operations
- `candle-transformers` — BERT/MiniLM model implementation
- `candle-nn` — neural network primitives
- `tokenizers` — HuggingFace tokenizer (Rust-native)
- `hf-hub` — model weight downloading

## Requirements Trace

| Requirement | Implementation Target |
|---|---|
| Candle model loader | `src/embed/model.rs` — `EmbeddingModel::load()` |
| Tokenizer integration | `src/embed/tokenizer.rs` — `Tokenizer::encode()` |
| Single-chunk embedding | `src/embed/mod.rs` — `embed_text()` |
| Batch embedding | `src/embed/mod.rs` — `embed_batch()` |
| Model weight management | `src/embed/model.rs` — `hf-hub` download + cache |
| Mean pooling | `src/embed/pool.rs` — `mean_pool()` |

## Implementation Units

### Unit 1: Model Loader & Weight Management

- **What:** Create `src/embed/model.rs` with `EmbeddingModel` struct that loads
  `all-MiniLM-L6-v2` weights from local cache or downloads via `hf-hub`.
- **Files:** `src/embed/model.rs`, `src/embed/mod.rs` (module declaration)
- **Tests:** `tests/embed_model_test.rs` — verify model loads, produces correct
  output shape (384 dims)
- **Posture:** Test-first (verify dimensions and determinism)

### Unit 2: Tokenizer Integration

- **What:** Create `src/embed/tokenizer.rs` wrapping the `tokenizers` crate.
  Load the MiniLM tokenizer config, encode text to token IDs with attention
  masks, handle truncation at 512 tokens.
- **Files:** `src/embed/tokenizer.rs`
- **Tests:** `tests/embed_tokenizer_test.rs` — verify encoding produces expected
  token counts, handles edge cases (empty string, max-length input)
- **Posture:** Test-first

### Unit 3: Mean Pooling

- **What:** Create `src/embed/pool.rs` implementing mean pooling over token
  embeddings with attention mask weighting.
- **Files:** `src/embed/pool.rs`
- **Tests:** Unit tests in `src/embed/pool.rs` — verify pooling with known
  tensor inputs produces expected output
- **Posture:** Test-first

### Unit 4: Single-Text Embedding

- **What:** Wire model + tokenizer + pooling into `embed_text(model, text) ->
  Result<Vec<f32>, GraphtorError>` in `src/embed/mod.rs`.
- **Files:** `src/embed/mod.rs`
- **Tests:** `tests/embed_integration_test.rs` — embed a known string, verify
  384-dim output, verify determinism (same input → same output)
- **Posture:** Integration test after units 1-3

### Unit 5: Batch Embedding

- **What:** Add `embed_batch(model, texts) -> Result<Vec<Vec<f32>>,
  GraphtorError>` that processes multiple chunks efficiently, reusing the model
  and tokenizer across calls.
- **Files:** `src/embed/mod.rs`
- **Tests:** `tests/embed_integration_test.rs` — batch of 10 chunks produces 10
  vectors of 384 dims each
- **Posture:** Test-first (verify batch size matches input count)

## Dependency Graph

```text
Unit 1 (Model Loader) ─┐
Unit 2 (Tokenizer)     ├─→ Unit 4 (Single Embed) → Unit 5 (Batch Embed)
Unit 3 (Mean Pool)     ─┘
```

Units 1, 2, 3 are independent and can be developed in parallel. Unit 4
integrates them. Unit 5 extends Unit 4.

## Decisions and Rationale

1. **Candle over ONNX Runtime** — pure Rust, no C++ runtime dependency, aligns
   with single-binary and zero-dependency principles.
2. **`hf-hub` for weight download** — standard Rust crate for HuggingFace model
   access, caches locally after first download.
3. **Mean pooling over CLS token** — MiniLM-L6-v2 is trained with mean pooling;
   using CLS would produce suboptimal embeddings.
4. **512-token truncation** — model max sequence length; longer chunks must be
   truncated (parsing stage keeps chunks reasonably sized via heading splits).
5. **Deterministic output** — same input text must always produce the same vector
   to support idempotent re-ingestion.

## Risks and Caveats

- **Risk:** First-run model download (~80 MB) requires internet.
  **Mitigation:** Clear error message; support pre-cached model path via config.
- **Risk:** Candle API instability (pre-1.0 crate).
  **Mitigation:** Pin to specific version; wrap behind our own trait interface.
- **Risk:** Memory usage during batch embedding.
  **Mitigation:** Process chunks in configurable batch sizes (default: 32).
- **Risk:** CPU-only inference may be slow for large corpora.
  **Mitigation:** Batch processing + future optional GPU feature flag.

## Plan Hardening Signals

- public API, schema, or contract change: **No** — internal module
- security, auth, permission, or compliance-sensitive behavior: **No**
- migration, backfill, destructive data/config action: **No**
- external integration, operator checkpoint, or external dependency: **Yes** —
  `hf-hub` downloads model weights from the internet on first run
- high runtime, rollout, or rollback risk: **No**

**Requires plan hardening: no**

The external dependency (model download) is a one-time bootstrap operation with
clear error handling. It does not affect production runtime after initial setup.

## Runtime Verification and Closure

- **Runtime surface:** None directly (library module consumed by pipeline)
- **Verification:** Integration test embeds known text and asserts 384-dim output
  with expected cosine similarity properties
- **Closure:** Feature absorbed when embed tests pass and batch processing
  handles 100+ chunks without memory issues

---

## Plan Review

**Gate decision: PASS**  
**Date:** 2026-04-29  
**Plan hardening required:** No (external dependency is one-time bootstrap only)  
**Plan hardening present:** N/A

### Reviewer Findings

#### Constitution Reviewer — 0 findings

All principles satisfied:
- Local-first: Candle runs in-process, zero network at runtime ✅
- Lightweight footprint: MiniLM ~80MB, pure Rust ML inference ✅
- Data pipeline integrity: deterministic embeddings (same input → same vector) ✅
- Automation: batch processing, no manual intervention ✅
- Note: `hf-hub` first-run download is acceptable bootstrap (documented in risks)

#### Rust Reviewer — 1 finding (P3)

- **P3 (advisory):** Plan does not specify whether `EmbeddingModel` should
  implement `Send + Sync`. The MCP server runs on tokio and may need to share
  the model across async tasks. Recommend documenting thread-safety expectation.
  *Action: Note for implementer; not blocking.*

#### Scope Boundary Auditor — 0 findings

- Width isolation maintained — pure embedding concern
- 2-hour rule satisfied for all 5 units
- No scope creep into storage, parsing, or MCP

#### Learnings Researcher — 0 findings

- No compound learnings exist
- Architecture blueprint references Candle with matching approach

#### Architecture Strategist — 1 finding (P3)

- **P3 (advisory):** Consider defining an `Embedder` trait to abstract the
  embedding model. Enables future model swaps (e.g., better small models) without
  changing callers. Optional for v1 — can be added when a second model is needed.
  *Action: Record as future enhancement, not blocking.*

### Summary

Plan is sound. Two P3 advisories noted for implementer awareness:
1. Document `Send + Sync` expectations for async context
2. Consider `Embedder` trait for future extensibility

Neither blocks harvest. Proceed.
