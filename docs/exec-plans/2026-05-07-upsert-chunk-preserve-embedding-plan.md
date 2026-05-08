# Implementation Plan: Fix upsert_chunk to Preserve Existing Embedding

**Feature:** 033-F  
**Date:** 2026-05-07  
**Source:** backlogit feature description + compound learning `cozodb-hnsw-api-patterns-2026-05-07.md`

---

## Problem Frame

`upsert_chunk` in `src/db/chunks.rs` (line 74) unconditionally sets `embedding = null` in
the `:put` Datalog script:

```datalog
?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
    <- [[$chunk_id, $source_id, $path, $title, $position, $char_offset, $headings, $content, null]]
:put doc_chunks { ... }
```

When an incremental sync runs without an embedding model (`model=None`), it calls
`upsert_chunk` to update chunk metadata. This **erases** any embedding previously
stored by `upsert_vector`. The HNSW index then loses the vector for that chunk until a
full re-embed pass runs.

The compound learning `cozodb-hnsw-api-patterns-2026-05-07.md` explicitly identifies this
as a known issue: "Direct `:put` with `embedding: null` in `upsert_chunk` will erase
stored embeddings; use join-put to preserve existing embeddings (tracked as 033-F)."

The fix uses a **join-put pattern** (already proven in `upsert_vector` in `vectors.rs`):
read the existing row to retrieve the current embedding, then write back with that
embedding preserved. For new chunks (no existing row), the embedding is `null`.

---

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| upsert_chunk must not erase existing non-null embeddings | Change Datalog script to join-put pattern with two disjunctive rules |
| New chunks still get `embedding = null` | Second rule handles case where no existing row is found |
| Existing tests continue passing | `upsert_chunk_is_idempotent` and round-trip tests remain green |
| New test proves embedding preservation | Add `upsert_chunk_preserves_existing_embedding` test |
| Doc comment updated | Remove "always sets embedding to null" statement |

---

## Implementation Units

### Unit 1: Test (Red Phase)

**Posture:** Test-first  
**Files:** `tests/db_chunks_test.rs`  
**Effort:** ~15 minutes

Add a new integration test:

```rust
#[test]
fn upsert_chunk_preserves_existing_embedding() {
    let s = store();
    let chunk = sample_chunk("embed-preserve", "docs/preserve.md", 0);
    upsert_chunk(&s, "src-001", &chunk).unwrap();

    // Store an embedding via upsert_vector
    let embedding = vec![0.1_f32; 384];
    upsert_vector(&s, "embed-preserve", &embedding).unwrap();

    // Re-upsert the chunk metadata (simulates model=None re-sync)
    upsert_chunk(&s, "src-001", &chunk).unwrap();

    // Embedding must still be present
    let retrieved = get_vector(&s, "embed-preserve").unwrap();
    assert!(retrieved.is_some(), "embedding should survive chunk re-upsert");
    let v = retrieved.unwrap();
    assert!((v[0] - 0.1).abs() < 1e-6);
}
```

Run `cargo test db_chunks_test::upsert_chunk_preserves_existing_embedding` — expect **FAIL**.

### Unit 2: Fix upsert_chunk (Green Phase)

**Posture:** Implementation  
**Files:** `src/db/chunks.rs`  
**Effort:** ~30 minutes

Replace the current single-rule Datalog script (lines 72–76) with a two-rule
join-put pattern:

**Rule 1 (preserve existing embedding):** Joins with the existing `doc_chunks` row,
reads the stored `embedding`, filters for `!is_null(embedding)`, and writes back
with the existing embedding preserved alongside the new metadata.

**Rule 2 (new chunk or null embedding):** Handles the case where no existing row
exists or the existing embedding is null. Uses `null` for the embedding.

The CozoDB Datalog approach:

```datalog
# Rule 1: existing chunk with non-null embedding → preserve it
?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
    := chunk_id = $chunk_id,
       source_id = $source_id,
       path = $path,
       title = $title,
       position = $position,
       char_offset = $char_offset,
       headings = $headings,
       content = $content,
       *doc_chunks{ chunk_id: $chunk_id, embedding },
       !is_null(embedding)

# Rule 2: no existing embedding → use null
?[chunk_id, source_id, path, title, position, char_offset, headings, content, embedding]
    := chunk_id = $chunk_id,
       source_id = $source_id,
       path = $path,
       title = $title,
       position = $position,
       char_offset = $char_offset,
       headings = $headings,
       content = $content,
       embedding = null,
       not *doc_chunks{ chunk_id: $chunk_id, embedding: e }, !is_null(e)

:put doc_chunks { chunk_id => source_id, path, title, position,
                  char_offset, headings, content, embedding }
```

**Alternative (simpler two-step Rust approach):** If the disjunctive Datalog negation
proves unreliable with CozoDB's null semantics, fall back to:

1. Query the existing embedding: `get_vector(store, chunk_id)?`
2. If `Some(emb)` → pass the embedding as a parameter instead of `null`
3. If `None` → pass `null` as currently done

This is more explicit, easier to test, and has known-good CozoDB behaviour at the
cost of one extra query per upsert. Given that `upsert_chunk` is already I/O-bound
(one `:put` per call), the extra read is negligible.

**Recommendation:** Start with the disjunctive Datalog approach (single query, elegant).
If test failures reveal CozoDB null-binding quirks, fall back to the two-step approach.

### Unit 3: Update Documentation

**Posture:** Documentation  
**Files:** `src/db/chunks.rs` (module doc comment, lines 9–10)  
**Effort:** ~5 minutes

Update the module doc comment to reflect the new behaviour:

```rust
//! The `doc_chunks` relation includes an `embedding: <F32; 384>?` column
//! that is indexed by the `doc_chunks:embedding_idx` HNSW index for
//! semantic search. [`upsert_chunk`] preserves any existing non-null
//! embedding — call [`crate::db::vectors::upsert_vector`] to store or
//! update embeddings explicitly.
```

Also update the `upsert_chunk` function doc comment to describe the join-put
preservation behaviour.

---

## Dependency Graph

```
Unit 1 (test-red) → Unit 2 (implementation-green) → Unit 3 (docs)
```

Linear dependency — each unit builds on the previous. No parallelism needed given
the small scope.

---

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Disjunctive Datalog over two-step Rust | Single atomic write, no TOCTOU race, matches the "join-put pattern" idiom documented in the compound learning. Fallback available if CozoDB null semantics don't cooperate. |
| Test uses synthetic 384-dim vector | Matches existing pattern in `db_vectors_test.rs` — avoids loading the real ML model in tests. |
| Preserve existing embedding, never overwrite with null | The caller contract is clear: to erase an embedding, call `delete_vectors_by_chunk_ids`. `upsert_chunk` manages metadata only. |

---

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| CozoDB negation (`not *rel{...}`) with nullable columns may have edge cases | Fallback to two-step Rust approach is pre-planned. Test confirms correctness. |
| Performance regression from join-put (extra read per upsert) | Negligible: `upsert_chunk` is already one `:put` per call. Bulk sync is I/O-dominated by file reads. The join is O(1) on the primary key. |
| HNSW index may need re-index after in-place embedding preservation | CozoDB automatically updates the HNSW index on every `:put` — no manual re-index needed. |

---

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | **No** | No schema change. `upsert_chunk` signature unchanged. Internal Datalog script change only. |
| Security, auth, permission, or compliance-sensitive | **No** | Database write logic, no auth surface. |
| Migration, backfill, destructive data/config action | **No** | No migration. Existing null embeddings remain null. Only preserves non-null values. |
| External integration, operator checkpoint, or external dependency | **No** | Pure internal change. |
| High runtime, rollout, or rollback risk | **No** | Rollback is reverting one file. No data loss risk — fix only prevents data loss. |

**Requires plan hardening: no**

---

## Runtime Verification and Closure

| Unit | Runtime Surface Changed? | Verification | Closure |
|---|---|---|---|
| Unit 2 (fix) | No direct CLI/MCP surface change. Internal DB write behaviour. | `cargo test` green. Manual verification: run sync twice (with and without model) — confirm embedding persists. | None required — internal fix, no monitoring surface. |

---

## Summary

This is a focused, low-risk bugfix. One test file + one source file + doc comment update.
Total estimated effort: well within the 2-hour rule. No plan hardening needed.

---

## Plan Review

**Gate Decision: PASS**  
**Reviewed:** 2026-05-07  
**Hardening required:** No (all signals absent — correctly assessed)

### Persona Results

| Persona | Findings |
|---|---|
| Constitution Reviewer | 0 findings — plan complies with all five core principles |
| Rust Reviewer | 1 finding (P3) |
| Scope Boundary Auditor | 0 findings — tight scope, no creep |
| Learnings Researcher | 0 findings — plan cites and follows `cozodb-hnsw-api-patterns-2026-05-07.md` |
| Architecture Strategist | 1 finding (P3) |

### Findings

#### P3-001 (Rust Reviewer): Test imports need expansion

The new test in `db_chunks_test.rs` will require additional imports:
`use graphtor_core::db::vectors::{upsert_vector, get_vector};`

This is obvious from context but worth noting for the implementer.

**Action:** Advisory — implementer will naturally add these.

#### P3-002 (Architecture Strategist): Two-step Rust approach preferred

The disjunctive Datalog with negation (`not *doc_chunks{...}`) is elegant but
adds cognitive complexity. The two-step approach (read existing embedding via
`get_vector`, then include it in the `:put` params) is:
- Easier to debug
- Has known-good CozoDB null semantics (proven in `vectors.rs`)
- More explicit about the intent

The plan already identifies this as a fallback. Recommendation: **lead with the
two-step approach** as the primary implementation, keep disjunctive Datalog as
the aspirational alternative if performance profiling later warrants it.

**Action:** Advisory — implementer may choose either approach.

### Gate Rationale

- No P0 or P1 findings
- No P2 findings
- Two P3 advisories (imports, implementation preference) — non-blocking
- Plan hardening correctly assessed as not required
- Runtime verification adequate (test coverage + manual sync verification)
- Dependency graph is acyclic and linear
- All requirements from the feature description map to implementation units
- 2-hour rule satisfied: 3 files touched, ~50 minutes estimated effort

**Result: Proceed to harvest.**
