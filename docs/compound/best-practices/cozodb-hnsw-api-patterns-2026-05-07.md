---
title: "CozoDB HNSW index: create, drop, upsert, and tilde-query patterns"
description: "CozoDB HNSW indexes require ::hnsw drop (not ::remove), a join-put upsert, tilde-query syntax, and a full row-export migration — none of which follow the base-relation conventions."
problem_type: "api_mismatch"
category: "best-practices"
component: "src/db/vectors.rs"
root_cause: "CozoDB HNSW index operations use a distinct command namespace (::hnsw) and query syntax (tilde-query) that differ from base-relation operations; schema migration requires full row export because CozoDB has no ALTER TABLE."
resolution_type: "code_fix"
severity: "high"
message: "index drop failed / embedding not stored / tilde-query syntax error"
file_path: "src/db/vectors.rs"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/38"
tags:
  - cozodb
  - hnsw
  - vector-search
  - datalog
  - schema-migration
---

## Problem

Implementing native HNSW vector search in CozoDB exposes several non-obvious
API distinctions that differ from base-relation operations:

- Index removal used `::remove` (base-relation command) instead of `::hnsw drop`
- Embedding upserts required a join-put pattern — direct `:put` with `null` erases stored embeddings
- Tilde-query (`~relation:index{...}`) is distinct syntax not used elsewhere in CozoDB
- Schema changes require full row export because CozoDB has no `ALTER TABLE`

## Root Cause

CozoDB HNSW operations live in a separate command namespace (`::hnsw`) and use
a distinct query syntax (tilde prefix) that is not documented alongside
base-relation operations. Key differences:

| Operation | Base relation | HNSW index |
|---|---|---|
| Drop | `::remove relation` | `::hnsw drop relation:index_name` |
| Check existence | `::relations` (shows all) | Filter for `:` in name |
| Upsert | `:put relation { ... }` | Join-put: read row, write back with `vec($emb)` |
| Query | `*relation{ ... }` | `~relation:index{ cols | query: q, k: $k }` |
| Read back | `DataValue::Str` / `DataValue::Num` | `DataValue::Vec(Vector::F32(arr))` |

Schema migration is especially subtle: CozoDB has no `ALTER TABLE`, so adding
a column requires exporting all rows, dropping the relation, recreating with
the new schema, and re-inserting.

## Resolution

### Index create and drop

```datalog
-- Create (idempotent: check ::relations first)
::hnsw create doc_chunks:embedding_idx {
    dim: 384, m: 16, dtype: F32,
    fields: [embedding], distance: Cosine, ef_construction: 200
}

-- Drop (use ::hnsw drop, NOT ::remove)
::hnsw drop doc_chunks:embedding_idx
```

Parameters: `dim` must match model output (all-MiniLM-L6-v2 = 384);
`m = 16` and `ef_construction = 200` are good production defaults.

### Nullable column declaration

```datalog
:create doc_chunks {
    chunk_id: String =>
    ...
    embedding: <F32; 384>?   -- nullable; null rows are skipped by HNSW automatically
}
```

### Embedding upsert (join-put pattern)

The chunk must exist before calling this. If the join returns zero rows, the
`:put` is a silent no-op — call `upsert_chunk` first.

```datalog
?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
    := *doc_chunks{ chunk_id: $chunk_id, source_id, path, title,
                    position, char_offset, headings, content },
       embedding = vec($emb)
:put doc_chunks { chunk_id => source_id, path, title, position,
                  char_offset, headings, content, embedding }
```

In Rust, pass `embedding` as `DataValue::List(Vec<DataValue>)` where each
element is `DataValue::Num(Num::Float(f64_value))`.

### Tilde-query (approximate nearest neighbour)

```datalog
?[chunk_id, source_id, path, title, heading_hierarchy, content, dist]
    := ~doc_chunks:embedding_idx{ chunk_id | query: q, k: $k, ef: 50 },
       *doc_chunks{ chunk_id, source_id, path, title, headings: heading_hierarchy, content }
```

`ef = 50` is a reasonable search-time expansion factor; `k` is passed as
`DataValue::Num(Num::Int(n))`. The `dist` column is lower for closer matches.

### Reading back stored embeddings

```rust
if let DataValue::Vec(cozo::Vector::F32(arr)) = &row[col_idx] {
    let floats: Vec<f32> = arr.to_vec();
}
```

### Schema migration (no ALTER TABLE)

1. Export all rows: `?[...] := *old_relation{...}` → `Vec<Vec<DataValue>>`
2. Drop old relations: `::remove old_relation`
3. Recreate with new schema (including the `embedding` column)
4. Re-insert exported rows via `:put` (idempotent)
5. Create the HNSW index

Metadata is preserved; embeddings are cleared (null) and must be regenerated
via a full pipeline re-sync after migration.

## Prevention

- Always use `::hnsw drop relation:index_name` to remove HNSW indexes — never `::remove`.
- Use `::relations` to check index existence before `::hnsw create`; filter for names containing `:`.
- The chunk row MUST exist before calling the embedding upsert — call `upsert_chunk` first.
- When adding a column to an existing relation, plan for the full export/drop/recreate cycle.
- Direct `:put` with `embedding: null` in `upsert_chunk` will erase stored embeddings. The fix
  (shipped in 033-F / PR #42) uses a two-step Rust approach: call `get_vector(store, chunk_id)`
  before the `:put`, and pass the returned embedding (or `null` if absent) as a `$embedding`
  parameter in the Datalog script. This is safer than disjunctive Datalog negation with nullable
  columns, which has edge-case risk in CozoDB.
- `distance: Cosine` is correct for L2-normalised embeddings — dot product equals cosine similarity.

## Citations

- `src/db/vectors.rs` — `upsert_vector` (join-put), `search_by_vector` (tilde-query)
- `src/db/chunks.rs` — `upsert_chunk` two-step embedding preservation fix (033-F / PR #42)
- `src/db/schema.rs` — `ensure_schema`, `migrate_to_v3`, `create_hnsw_index_if_missing`
- `docs/decisions/2026-05-07-cozodb-hnsw-feasibility-spike.md` — spike confirming API contract
- PR #38: `feat(db): migrate vector search to CozoDB native HNSW index (schema v3)`
- PR #42: `fix(db): preserve existing embedding in upsert_chunk (join-put)`
