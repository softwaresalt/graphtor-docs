---
title: "CozoDB HNSW API Patterns"
date: 2026-05-07
shipment: 023-S
pr: 38
tags: [cozodb, hnsw, vector-search, datalog]
---

# CozoDB HNSW API Patterns

Hard-won findings from implementing native HNSW vector search in CozoDB
(graphtor-docs shipment 023-S / PR #38). These patterns apply to any Rust
project embedding CozoDB with the `cozo` crate.

## Index Management

### Create an HNSW index

```datalog
::hnsw create doc_chunks:embedding_idx {
    dim: 384,
    m: 16,
    dtype: F32,
    fields: [embedding],
    distance: Cosine,
    ef_construction: 200
}
```

Key parameters:
- `dim` must match the model output exactly (all-MiniLM-L6-v2 → 384)
- `m = 16` is a reasonable default; higher improves recall at cost of RAM
- `ef_construction = 200` balances build quality vs. index construction time
- `distance: Cosine` is appropriate for L2-normalised embeddings (dot product = cosine)

### Drop an HNSW index

```datalog
::hnsw drop doc_chunks:embedding_idx
```

**Critical**: Use `::hnsw drop` — NOT `::remove`. `::remove` works on base
relations only. Using `::remove` on an HNSW index will fail silently or
raise a confusing error.

### Idempotent index creation

Check whether the index already exists before creating it:

```datalog
::relations
```

Returns a list of relation names including index names like
`doc_chunks:embedding_idx`. Filter this list in Rust before calling
`::hnsw create`.

## Column Declaration

Declare a nullable fixed-width vector column:

```datalog
:create doc_chunks {
    chunk_id: String =>
    ...
    embedding: <F32; 384>?   -- nullable; null chunks skipped by HNSW automatically
}
```

The `?` suffix makes the column nullable. CozoDB's HNSW implementation
skips null-embedding rows automatically — no special handling needed.

## Embedding Upsert (Join-Put Pattern)

HNSW indexes are maintained automatically on every `:put`. The join-put
pattern reads the existing row and writes it back with the new embedding:

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

**Important**: The chunk MUST exist in `doc_chunks` before calling this.
If the join returns zero rows (chunk missing), the `:put` is a no-op —
no error is raised, no embedding is stored.

## Tilde-Query (Approximate Nearest Neighbour)

```datalog
?[chunk_id, source_id, path, title, heading_hierarchy, content, dist]
    := ~doc_chunks:embedding_idx{ chunk_id | query: q, k: $k, ef: 50 },
       *doc_chunks{ chunk_id, source_id, path, title, headings: heading_hierarchy, content }
```

- `k` is the number of results; pass as `DataValue::Num(Num::Int(n))`
- `ef = 50` is a reasonable search-time expansion factor (higher → better recall, slower)
- The tilde-query returns a `dist` column (lower = closer for cosine distance)
- Results are automatically joined with the base relation in the same query

## Reading Back Stored Embeddings

Stored embeddings come back as `DataValue::Vec(cozo::Vector::F32(arr))`
where `arr: ndarray::Array1<f32>`. Pattern-match accordingly:

```rust
if let DataValue::Vec(cozo::Vector::F32(arr)) = &row[col_idx] {
    let floats: Vec<f32> = arr.to_vec();
}
```

## Schema Migration (v2 → v3)

CozoDB has no `ALTER TABLE`. Migration requires:

1. Export all rows from the old relation into a `Vec<Vec<DataValue>>`
2. Drop the old relation (`::remove doc_chunks`)
3. Drop the old `doc_vectors` relation if it exists
4. Recreate with the new schema including the `embedding` column
5. Re-insert the exported rows (metadata preserved; embeddings cleared)
6. Create the HNSW index

This is idempotent when re-run — row export + `:put` is idempotent.
Embeddings must be regenerated via a full pipeline re-sync after migration.

## Known Limitation: upsert_chunk Null Overwrite

`upsert_chunk` (direct `:put` with `embedding: null`) will erase a stored
embedding if called on a chunk that already has one. This affects the
`model=None` sync path. The fix is to use the join-put pattern inside
`upsert_chunk` to preserve existing embeddings — tracked as feature 033-F.

The normal sync path is safe: `upsert_chunk` is always followed immediately
by `upsert_vector`, which restores the embedding in the same pipeline step.
