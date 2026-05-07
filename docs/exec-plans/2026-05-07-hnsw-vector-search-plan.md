---
title: "HNSW Vector Search Index Implementation Plan"
source: "docs/decisions/2026-05-07-cozodb-hnsw-feasibility-spike.md"
date: 2026-05-07
status: draft
---

# HNSW Vector Search Index — Implementation Plan

## Problem Frame

The current semantic search in `src/db/vectors.rs` uses brute-force O(n) cosine
similarity: `load_all_vectors()` pulls every embedding from `doc_vectors` as
JSON strings, deserializes each into `Vec<f32>`, computes dot-product similarity
in Rust, sorts all results, and returns top-k. This approach:

1. **Scales linearly** — unusable beyond ~100k chunks (target: 500k+)
2. **Wastes memory** — loads entire vector corpus into RAM per query
3. **Incurs JSON serde overhead** — embeddings stored as `String` not native `<F32; 384>`
4. **Requires a join** — `doc_vectors` is a separate relation from `doc_chunks`,
   requiring post-search metadata resolution (lines 165–173 of `vectors.rs`)

CozoDB 0.7.6 provides native HNSW indexing over `<F32; N>` columns with
automatic index maintenance. The spike confirmed the full API contract
(creation, query, distance metrics, parameter tuning).

## Requirements Trace

| Requirement (from spike) | Implementation Action |
|---|---|
| Native vector storage (no JSON strings) | Merge embedding into `doc_chunks` as `<F32; 384>` column |
| O(log n) approximate search | Create HNSW index with `::hnsw create` |
| Cosine distance metric | Configure `distance: Cosine` on the index |
| Automatic index maintenance on mutation | CozoDB handles this; no custom code needed |
| Migration from schema v2 → v3 | Version-aware `ensure_schema` with migration path |
| Backward compatibility | Graceful degradation when no embeddings are stored |
| Re-ingestion of existing embeddings | Full pipeline re-run rebuilds vectors natively |

## Implementation Units

### Unit 1: Schema v3 — Merge Embedding Column into `doc_chunks`

**Changes:**
- `src/db/schema.rs`: Add `embedding: <F32; 384>?` (nullable) column to `doc_chunks` DDL
- `src/db/schema.rs`: Bump `SCHEMA_VERSION` from 2 to 3
- `src/db/schema.rs`: Add migration logic: detect v2, create new relation, copy data, drop old `doc_vectors`
- `src/db/schema.rs`: Add `::hnsw create doc_chunks:embedding_idx { ... }` after relation creation

**Files affected:**
- `src/db/schema.rs`

**Tests:**
- Schema v3 creates successfully on fresh database
- Schema migration from v2 → v3 copies existing data
- HNSW index exists after schema creation (verify via `::relations`)

**Execution posture:** Migration-first — validate the schema DDL works before touching any other module.

**Estimated effort:** ~1.5 hours

---

### Unit 2: Replace `vectors.rs` with HNSW-Based Search

**Changes:**
- `src/db/vectors.rs`: Replace `upsert_vector` to store native `DataValue::Vec(Vector::F32(...))` in the embedding column of `doc_chunks`
- `src/db/vectors.rs`: Replace `search_by_vector` with a single HNSW Datalog query using `~doc_chunks:embedding_idx{...}`
- `src/db/vectors.rs`: Remove `load_all_vectors`, `cosine_similarity`, `l2_norm` (dead code)
- `src/db/vectors.rs`: Update `get_vector` to read from `doc_chunks.embedding`
- `src/db/vectors.rs`: Update `delete_vectors_by_chunk_ids` to null the embedding column

**Files affected:**
- `src/db/vectors.rs`

**Tests:**
- `search_by_vector` returns correct top-k results (same as current tests, new implementation)
- `upsert_vector` persists and is retrievable
- Search with empty database returns empty results
- HNSW query respects `limit` parameter

**Execution posture:** Test-first — adapt existing `db_vectors_test.rs` tests to the new API, confirm they fail (since schema changed), then implement.

**Estimated effort:** ~1.5 hours

---

### Unit 3: Update Pipeline and Sync to Use Unified Upsert

**Changes:**
- `src/pipeline/mod.rs`: Change the Load phase to pass embeddings directly with chunk upsert (or immediately after as a field update on `doc_chunks`)
- `src/db/chunks.rs`: Add an optional `embedding` parameter to `upsert_chunk` or add a companion `upsert_chunk_embedding` that updates the embedding column in `doc_chunks`
- `src/sync/reingest.rs`: Fix the current no-op embedding behavior — actually persist embeddings during reingest

**Files affected:**
- `src/pipeline/mod.rs`
- `src/db/chunks.rs`
- `src/sync/reingest.rs`

**Tests:**
- Pipeline integration test: after sync, chunks have embeddings
- Reingest persists vectors (not a no-op anymore)
- Embedding failure doesn't prevent chunk storage (nullable column)

**Execution posture:** Test-first — write a pipeline integration test that asserts embeddings are stored, watch it fail, implement.

**Estimated effort:** ~1.5 hours

---

### Unit 4: Update MCP Server Search Tools

**Changes:**
- `src/mcp/server.rs`: `search_semantic` tool — update to use new `search_similar` (API unchanged for callers, internal implementation changed)
- `src/mcp/server.rs`: `research_topic` tool — same; the search result type is unchanged
- `src/db/search.rs`: `search_similar` signature unchanged, but internal delegation goes to the new HNSW-backed `search_by_vector`

**Files affected:**
- `src/db/search.rs` (minimal — just ensure delegation still works)
- `src/mcp/server.rs` (verify no compile errors from changed internals)

**Tests:**
- Existing MCP tool tests continue to pass
- `search_similar` returns results ranked by relevance

**Execution posture:** Characterization-first — run existing tests to confirm they pass through the new implementation without changes to tool signatures.

**Estimated effort:** ~45 minutes

---

### Unit 5: Cleanup and Documentation

**Changes:**
- Remove the old `doc_vectors` relation from documentation
- Update `AGENTS.md` db module descriptions (mention HNSW)
- Update `src/db/vectors.rs` module doc comments to reflect HNSW
- Update `src/db/schema.rs` schema version table
- Remove dead code (brute-force helpers) if not already removed in Unit 2

**Files affected:**
- `AGENTS.md`
- `src/db/vectors.rs` (doc comments)
- `src/db/schema.rs` (doc comments)
- `docs/cli-reference/graphtor-docs.md` (if status output mentions schema version)

**Tests:**
- `cargo clippy` passes with no dead code warnings
- `cargo doc` generates without warnings

**Execution posture:** Documentation-first — update docs, verify build/lint clean.

**Estimated effort:** ~30 minutes

## Dependency Graph

```
Unit 1 (Schema v3)
  ├── Unit 2 (HNSW Search) — depends on schema existing
  │     └── Unit 4 (MCP Tools) — depends on search API working
  └── Unit 3 (Pipeline Upsert) — depends on schema existing
Unit 5 (Cleanup) — depends on all others
```

**Optimal execution order:** 1 → 2 → 3 → 4 → 5  
(Units 2 and 3 could be parallelized but serial is safer for a single agent.)

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Merge embedding INTO `doc_chunks` rather than keeping separate relation | Eliminates the join during search; HNSW query returns all metadata in one pass. CozoDB supports nullable columns, so chunks without embeddings just have `null`. |
| Use `<F32; 384>?` (nullable) | Graceful degradation: keyword search works for chunks without embeddings. Pipeline can run without model loaded. |
| `Cosine` distance, `m=16`, `ef_construction=200` | Standard HNSW parameters for 384-dim normalized embeddings. Cosine is semantically correct for L2-normalized all-MiniLM-L6-v2 output. M=16 balances recall/speed. ef=200 builds a high-quality index (one-time cost). |
| Query-time `ef=50` as default | Gives excellent recall for k≤20 (our typical search_k). Can be parameterized later if needed. |
| Full re-ingestion rather than in-place migration | Column type changes from `String` (in separate relation) to `<F32; 384>` (in same relation). No safe in-place conversion — simpler to rebuild. Idempotent pipeline makes this trivial. |
| Remove `doc_vectors` relation in migration | Dead weight after merge; keeping it creates confusion and potential for stale data. |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| HNSW recall is approximate (~95-99%) | Acceptable tradeoff for documentation search. Exact results rarely matter for semantic discovery. Can increase ef at query time if needed. |
| Schema migration deletes `doc_vectors` | Migration is destructive for existing databases. Mitigation: detect v2, warn, require re-sync after migration. The data is reproducible from source docs. |
| CozoDB HNSW with sqlite backend not benchmarked at 384-dim × 100k | Low risk — HNSW is algorithm-level efficient regardless of backend. The sqlite backend stores the index graph; query traversal is O(log n). |
| `ndarray` dependency pulled in by CozoDB for `Vector::F32` | Already a transitive dependency of cozo (confirmed in Cargo.lock). No new crate additions needed. |
| Concurrent write during search | CozoDB sqlite serializes writes. Single-writer pipeline architecture is safe. Search (read) is not blocked by writes. |

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | ✅ Yes | Schema v2 → v3 migration; `doc_vectors` relation removed |
| Security, auth, permission, or compliance-sensitive behavior | ❌ No | Internal storage change only |
| Migration, backfill, destructive data/config action, or irreversible step | ✅ Yes | Schema migration drops `doc_vectors`; requires re-ingestion |
| External integration, operator checkpoint, or external dependency | ❌ No | All local, no new crates |
| High runtime, rollout, or rollback risk | ⚠️ Partial | Rollback requires manual schema rebuild (drop & re-create from v2). Low blast radius since data is reproducible. |

**Requires plan hardening: yes** — schema migration is destructive and requires
re-ingestion. The hardening step should detail rollback procedure and operator
communication around the one-time re-sync requirement.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification | Closure Artifact |
|---|---|---|---|
| Unit 1 | Database schema | `status` command reports `schema_version: 3` | Schema version in status output |
| Unit 2 | `search_semantic` MCP tool | Semantic search returns ranked results | MCP tool response validation |
| Unit 3 | `sync` pipeline output | Sync logs show "embeddings stored" not "vectors not persisted" | Pipeline log review |
| Unit 4 | `research_topic` MCP tool | Research queries return BFS-enhanced results | MCP tool response validation |
| Unit 5 | Documentation | Docs reflect HNSW, no stale references | Doc review |

**Rollback trigger**: If the HNSW index causes query errors or timeouts on any
existing indexed corpus, rollback by: (1) drop the database file, (2) revert
schema.rs to v2, (3) re-run pipeline. Data is fully reproducible from sources.

## References

- Spike findings: `docs/decisions/2026-05-07-cozodb-hnsw-feasibility-spike.md`
- Current schema: `src/db/schema.rs` (v2)
- Current brute-force search: `src/db/vectors.rs` (lines 129-175)
- Pipeline embedding flow: `src/pipeline/mod.rs` (lines 424-453)
- CozoDB HNSW tests: `cozo-0.7.6/src/runtime/tests.rs` (lines 700-810)
- Existing vector tests: `tests/db_vectors_test.rs`

## Plan Review

**Gate Decision: ADVISORY**  
**Date:** 2026-05-07  
**Reviewers:** Constitution, Rust, Scope Boundary, Learnings Researcher, Architecture Strategist, Agent-Native Parity

### Summary

The plan is architecturally sound, well-scoped, and aligned with all five constitutional
principles. Two P2 findings and one P3 advisory were identified. No P0 or P1 issues block
harvest. Plan hardening signals are present but inline detail (rollback trigger, runtime
verification table, reproducible-data mitigation) provides equivalent coverage — formal
plan-harden step is optional.

### Findings

#### P2-1: Migration Test Contradicts Decision (Architecture Strategist)

**Location:** Unit 1 — Tests section ("Schema migration from v2 → v3 copies existing data")  
**Issue:** The Decisions section correctly states "Full re-ingestion rather than in-place
migration… No safe in-place conversion — simpler to rebuild." However, Unit 1's test
description says "copies existing data," which is ambiguous. The `doc_vectors` relation
stores embeddings as JSON `String`. These CANNOT be copied into a native `<F32; 384>`
column without parsing. The migration should:

1. Recreate `doc_chunks` with the new schema (adding `embedding` column)
2. Copy existing `doc_chunks` rows (metadata only — chunk_id, source_id, path, etc.)
3. Drop `doc_vectors` entirely (embeddings are lost)
4. Require a full `sync` to re-embed all chunks

**Recommendation:** Clarify Unit 1 test to: "Schema migration preserves doc_chunks metadata;
embeddings require re-ingestion via pipeline sync." Remove the ambiguous "copies existing data"
phrasing.

#### P2-2: HNSW Query Column-Position Alignment (Learnings Researcher)

**Location:** Unit 2 — HNSW query implementation  
**Prior learning:** `docs/compound/best-practices/cozo-query-columns-must-match-struct-2026-04-30.md`  
**Issue:** CozoDB result rows are positional — column order in the query head MUST match the
`row[idx]` decoding sequence in `row_to_result`. The HNSW tilde-query
`~doc_chunks:embedding_idx{chunk_id | query: q, k: 10, ef: 50, bind_distance: dist}` returns
columns in a specific order. The implementation must ensure the projected columns align exactly
with the `SearchResult` struct fields (chunk_id, source_id, path, heading_hierarchy, content).

**Recommendation:** During Unit 2 implementation, explicitly document the column order returned
by the HNSW query and verify positional alignment with `SearchResult`. Add a comment or test
asserting the column-to-field mapping.

#### P3-1: ndarray Direct Dependency (Rust Reviewer)

**Location:** Unit 2 — DataValue::Vec construction  
**Issue:** Constructing `DataValue::Vec(Vector::F32(Array1::from_vec(embedding)))` requires
`ndarray::Array1`. While ndarray is a transitive dependency of cozo, the project may need to
add it as a direct `[dependencies]` entry to import `Array1` in `vectors.rs`. Alternatively,
cozo may re-export the type.

**Recommendation:** Verify during implementation whether `cozo` re-exports `Vector`/`Array1`
or if `ndarray` needs explicit addition to `Cargo.toml`. If added, justify per Lightweight
Footprint (already transitive — making it explicit adds zero build cost).

### Plan Hardening Assessment

The plan declares `Requires plan hardening: yes` based on schema migration and destructive
drop of `doc_vectors`. However, the plan already contains equivalent hardening detail:

| Hardening aspect | Coverage in plan |
|---|---|
| Rollback procedure | ✅ 3-step: drop DB → revert schema.rs → re-run pipeline |
| Rollback trigger | ✅ "HNSW index causes query errors or timeouts" |
| Runtime verification | ✅ Per-unit verification table |
| Data loss risk mitigation | ✅ "Data is fully reproducible from sources" |
| Operator communication | ⚠️ Partial — plan should note that CLI `status` warns when re-sync is needed |

**Assessment:** Inline hardening detail is sufficient for the risk level. Formal `plan-harden`
invocation is optional. The single gap (operator communication after migration) is minor and
can be addressed during implementation.

### Persona Details

**Constitution Reviewer:** All five principles satisfied. Local-first (no external deps),
lightweight (no new crates), pipeline integrity (deterministic IDs preserved), MCP-native
(tools unchanged externally), automation (idempotent re-ingestion). No violations.

**Rust Reviewer:** Error handling follows existing patterns (DataStore::query returns
Result, propagated via `?`). Type signatures are sound. Nullable column avoids Option<>
complexity at the schema level. No `unwrap`/`expect` concerns. P3 on ndarray import.

**Scope Boundary Auditor:** All five units directly serve the HNSW migration. Reingest fix
(Unit 3) is necessary — current no-op would leave the new column permanently null. No YAGNI
or scope creep detected. ef_query parameterization deferred correctly.

**Learnings Researcher:** One directly relevant learning identified (P2-2). No contradictions
with existing compound library. The plan is consistent with established CozoDB patterns.

**Architecture Strategist:** Schema merge (embedding → doc_chunks) is correct — eliminates
join, simplifies query path. Nullable column provides graceful degradation. Dependency graph
is acyclic and correctly ordered. P2-1 on migration ambiguity.

**Agent-Native Parity Reviewer:** MCP tool external contracts unchanged. SearchResult struct
preserved. Agents see no breaking change — only performance improvement. No findings.
