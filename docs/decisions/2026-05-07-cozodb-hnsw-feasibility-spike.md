---
title: "CozoDB HNSW Vector Index API Feasibility"
type: spike
date: 2026-05-07
time_box: "2h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: "stash:7DF2B95F"
promoted_to: ["plan"]
tags:
  - vector-search
  - cozodb
  - hnsw
  - performance
---

## Goal

How does CozoDB 0.7.6 expose HNSW vector indexing, and what is the concrete
migration path from the current brute-force `doc_vectors` JSON-string approach
to native HNSW search?

## Success Criteria

- Exact Datalog syntax for creating an HNSW index over a Vec column
- Exact Datalog syntax for querying the HNSW index (nearest-neighbor search)
- How native vectors are stored (column type, DataValue variant)
- Tunable parameters (ef_construction, M, distance metric)
- Migration path from current schema to HNSW-backed schema

## Scope Constraints

- Read-only investigation — no production changes
- CozoDB 0.7.6 (the version in our Cargo.lock) only

## Investigation Approach

1. Locate HNSW implementation in CozoDB 0.7.6 source (cargo registry)
2. Read the schema parser for Vec column type syntax
3. Read the system operations parser for HNSW index creation syntax
4. Read the query parser for HNSW search syntax
5. Confirm with integration tests in the CozoDB source

## Findings

### What Was Discovered

**CozoDB 0.7.6 has full, mature HNSW support built in.** The implementation
lives in `src/runtime/hnsw.rs` (42KB) and is exercised by integration tests.
No additional crate features or dependencies are needed beyond what we already
have (`storage-sqlite`, `rayon`).

#### 1. Native Vector Column Type

CozoDB supports a first-class `Vec` column type with compile-time dimension:

```datalog
:create doc_chunks_v2 {
    chunk_id: String
    =>
    source_id: String,
    path: String,
    title: String?,
    position: Int,
    char_offset: Int,
    headings: String,
    content: String,
    embedding: <F32; 384>
}
```

The `<F32; 384>` syntax declares a fixed-size 384-dimensional F32 vector.
Internally this maps to `DataValue::Vec(Vector::F32(Array1<f32>))` — native
ndarray storage, no JSON serialization.

#### 2. HNSW Index Creation

```datalog
::hnsw create doc_chunks_v2:embedding_idx {
    dim: 384,
    m: 16,
    dtype: F32,
    fields: [embedding],
    distance: Cosine,
    ef_construction: 200
}
```

Required parameters:
- `dim` — vector dimensionality (must match column)
- `m` (or `m_neighbours`) — max connections per layer (typical: 16-64)
- `dtype` — `F32` or `F64`
- `fields` — which Vec column(s) to index
- `distance` — `L2`, `Cosine`, or `IP` (inner product)
- `ef_construction` — construction-time search width (typical: 100-400)

Optional parameters:
- `filter` — Datalog expression for conditional indexing
- `extend_candidates` — HNSW extension flag (default: false)
- `keep_pruned_connections` — retain pruned edges (default: false)

#### 3. HNSW Query Syntax

```datalog
?[chunk_id, source_id, path, headings, content, dist] :=
    ~doc_chunks_v2:embedding_idx{
        chunk_id, source_id, path, headings, content
        | query: q, k: 10, ef: 50, bind_distance: dist
    },
    q = vec($query_vec)
```

The `~relation:index` syntax (tilde prefix) triggers an HNSW search. Parameters
after `|` are search-time controls:
- `query` — the query vector (bound to a variable containing `vec([...])`)
- `k` — number of nearest neighbors to return (required)
- `ef` — search-time beam width (required, typically `ef >= k`)
- `bind_distance` — output variable for the distance score
- `bind_field` — output variable for which indexed field matched
- `bind_field_idx` — output variable for the field index
- `bind_vector` — output variable for the stored vector itself
- `radius` — optional distance threshold for range queries
- `filter` — runtime filter expression

#### 4. Vector Data Input from Rust

Vectors are passed as `DataValue::Vec(Vector::F32(Array1::from(vec)))`:

```rust
use cozo::DataValue;
use ndarray::Array1;

// From our embedding model output (Vec<f32>):
let embedding: Vec<f32> = embed_text(model, text)?;
let dv = DataValue::Vec(cozo::Vector::F32(Array1::from(embedding)));
```

For parameterized queries, vectors can also be passed via the `vec([...])` 
built-in function in Datalog, which accepts a JSON array of floats.

#### 5. Distance Metrics

| Metric | CozoDB Name | Formula | Our Use Case |
|--------|-------------|---------|--------------|
| Cosine | `Cosine` | `1 - dot(a,b) / (‖a‖·‖b‖)` | ✅ Best for normalized embeddings |
| L2 | `L2` | `‖a-b‖²` | Alternative (euclidean) |
| Inner Product | `IP` | `1 - dot(a,b)` | For pre-normalized vectors |

Since `all-MiniLM-L6-v2` produces L2-normalized vectors, both `Cosine` and
`IP` would work identically. `Cosine` is the most semantically clear choice.

#### 6. Index Maintenance

HNSW indexes are **automatically maintained** on insert/update/delete to the
base relation. When you `:put` or `:rm` from the base relation, the HNSW index
is updated incrementally. No manual rebuild is required for ongoing operations.

The index can be dropped with:
```datalog
::hnsw drop doc_chunks_v2:embedding_idx
```

### What Was Tried and Failed

Nothing failed — the API is well-documented in CozoDB's source and tests.
The one subtlety discovered: the current `doc_vectors` relation stores
embeddings as JSON strings in a `String` column, which is incompatible with
the HNSW index (it requires a native `<F32; N>` column). This means we
cannot add an HNSW index to the existing relation — we need a schema migration.

### Remaining Unknowns

1. **Performance at scale** — the CozoDB test uses tiny vectors (dim=2). Our
   384-dim vectors at 50k+ scale haven't been benchmarked with this specific
   CozoDB HNSW implementation. However, HNSW is a well-understood algorithm
   and the implementation uses standard priority-queue + ndarray dot products.

2. **Concurrent access** — CozoDB with sqlite backend handles concurrent reads
   but serializes writes. Under our single-writer (sync pipeline) architecture
   this is fine, but worth noting for future multi-writer scenarios.

3. **Index size on disk** — HNSW indexes add O(n·M) edge storage overhead.
   For 100k chunks with M=16, this is ~1.6M edges. Likely modest but
   unmeasured for our specific configuration.

## Recommendation

**Conclusion**: proceed  
**Confidence**: high

CozoDB 0.7.6 provides a complete, production-ready HNSW implementation that
exactly meets our needs. The API is clean, the index is automatically maintained
on mutations, and the query syntax integrates naturally with Datalog joins.

The migration strategy is:

1. **Merge the embedding column into `doc_chunks`** — eliminate the separate
   `doc_vectors` relation. Store `embedding: <F32; 384>` directly alongside
   chunk metadata. This eliminates the current join between `doc_vectors` and
   `doc_chunks` during search.

2. **Create an HNSW index on the embedding column** — one `::hnsw create`
   command after schema creation.

3. **Replace `search_by_vector`** — the current O(n) Rust loop becomes a
   single Datalog query using `~doc_chunks:embedding_idx{...}`.

4. **Re-ingest all embeddings** — since the column type changes from JSON
   string to native Vec, all embeddings must be re-computed (or converted)
   during the migration. This is a one-time cost.

### Recommended HNSW Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `dim` | 384 | all-MiniLM-L6-v2 output dimension |
| `dtype` | F32 | Model produces f32; matches existing precision |
| `distance` | Cosine | Semantically correct for normalized embeddings |
| `m` | 16 | Standard default; good recall/speed tradeoff |
| `ef_construction` | 200 | Higher = better index quality, one-time cost |
| `ef` (query-time) | 50 | Default for k≤20; adjustable per query |

## Next Steps

1. Promote to implementation planning via `impl-plan`
2. Design the schema v3 migration (doc_vectors merged into doc_chunks)
3. Plan the `search_by_vector` replacement
4. Plan the re-ingestion pass
5. Plan backward-compatibility (handle databases without HNSW gracefully)

## References

- `D:\.cargo\registry\src\...\cozo-0.7.6\src\runtime\hnsw.rs` — HNSW implementation (42KB)
- `D:\.cargo\registry\src\...\cozo-0.7.6\src\parse\sys.rs:514-623` — index creation parser
- `D:\.cargo\registry\src\...\cozo-0.7.6\src\data\program.rs:1334-1555` — HNSW query normalization
- `D:\.cargo\registry\src\...\cozo-0.7.6\src\runtime\tests.rs:700-810` — integration tests
- `D:\.cargo\registry\src\...\cozo-0.7.6\src\parse\schema.rs:141-153` — Vec column type parser
- `D:\.cargo\registry\src\...\cozo-0.7.6\src\data\value.rs:206-213` — Vector enum (F32/F64)
- `src/db/vectors.rs` — current brute-force implementation
- `src/db/schema.rs` — current schema v2
- `Cargo.lock:597` — cozo 0.7.6 confirmed
