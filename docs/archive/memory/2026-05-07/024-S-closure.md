---
type: session-memory
date: 2026-05-07
shipment: 024-S
feature: 033-F
pr: "https://github.com/softwaresalt/graphtor-docs/pull/42"
status: shipped
---

# Session Memory: 033-F — Fix upsert_chunk Embedding Preservation

## Outcome

Shipped shipment `024-S` (feature `033-F`) via PR #42, merged to `main` on 2026-05-07.

The bug: `upsert_chunk` in `src/db/chunks.rs` wrote `embedding = null` unconditionally in its
CozoDB `:put` statement. Any model=None re-sync or metadata-only re-upsert silently erased
previously stored HNSW embeddings.

The fix: two-step Rust approach — call `get_vector(store, chunk_id)` before the `:put` and pass
the result (or `null` if absent) as a `$embedding` parameter, replacing the hardcoded `null`.

## Changed Files

| File | Change |
|------|--------|
| `src/db/chunks.rs` | Added `get_vector` import; two-step embedding preservation in `upsert_chunk` |
| `tests/db_chunks_test.rs` | Added `upsert_chunk_preserves_existing_embedding` test; added `vectors::` imports |
| `docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-07-upsert-chunk-preserve-embedding-plan.md` | Implementation plan + plan-review results |
| `.backlogit/queue/033-F.md` | Feature moved to archived |
| `.backlogit/queue/033.001-T.md` | Red-phase task archived |
| `.backlogit/queue/033.002-T.md` | Green-phase task archived |
| `.backlogit/queue/024-S.md` | Shipment archived |
| `.backlogit/archive/033-F.md` | Archive copy |
| `.backlogit/archive/033.001-T.md` | Archive copy |
| `.backlogit/archive/033.002-T.md` | Archive copy |
| `.backlogit/archive/024-S.md` | Archive copy |
| `docs/compound/best-practices/cozodb-hnsw-api-patterns-2026-05-07.md` | Updated Prevention + Citations to reflect fix |

## Key Decisions

1. **Two-step Rust over disjunctive Datalog**: A Datalog `not *doc_chunks{...}` pattern with
   nullable columns has edge-case risk in CozoDB. Reading via `get_vector` first is explicit,
   proven-safe, and O(1) by primary key.

2. **Red → green TDD confirmed**: The failing test (`upsert_chunk_preserves_existing_embedding`)
   reproduced the bug before the fix, confirming the defect was real. After the fix, all 6
   db_chunks_test cases and 8 db_vectors_test cases passed.

## CI / Review

- CI: build pass on both commits (`0005775` and `22471ed`)
- Copilot review finding: missing YAML frontmatter on exec-plan doc — fixed in `22471ed`
- Review thread `PRRT_kwDORiB5E86AgkES` resolved via GraphQL after reply

## Remaining Work

- `013.008-T` (queue): still blocked on upstream dependencies; not part of this shipment

## Compound Learning Updated

`docs/compound/best-practices/cozodb-hnsw-api-patterns-2026-05-07.md` — Prevention section
updated to document the two-step Rust fix approach and cite PR #42.
