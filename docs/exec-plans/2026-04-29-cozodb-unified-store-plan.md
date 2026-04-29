# Implementation Plan: 012-F — Unified Data Store (CozoDB)

**Feature:** 012-F  
**Status:** Not yet implemented  
**Date:** 2026-04-29  
**Supersedes:** 005-F (LanceDB), 006-F (Kùzu)

## Problem Frame

The ingestion pipeline needs persistent storage for both vector embeddings (ANN
search) and structural graph relationships (multi-hop traversal). The original
architecture split this across LanceDB (vectors) and Kùzu (graph), requiring
two separate embedded databases with independent schemas and a chunk_id
correlation key to bridge them.

CozoDB replaces both with a single embedded database that natively supports:
- **Relational data** via Datalog queries
- **Graph traversal** via fixed-point recursive Datalog
- **Vector search** via built-in HNSW indexing

This reduces dependencies, eliminates cross-database correlation complexity, and
aligns with the Lightweight Footprint principle (one engine instead of two).

**Crate dependency required:**
- `cozo` — CozoDB embedded engine with Rust API

**Key architectural change:** The `src/db/vector.rs` and `src/db/graph.rs`
pattern from the original design collapses into a single `src/db/` module with
CozoDB as the unified backend. All vector operations AND graph operations route
through the same database instance.

## Requirements Trace

| Requirement (from 005-F + 006-F) | Implementation Target |
|---|---|
| Database lifecycle (open/close/path) | `src/db/store.rs` — `DataStore::open()` |
| Schema definition (node/relation types) | `src/db/schema.rs` — Datalog DDL |
| Chunk storage with vectors | `src/db/chunks.rs` — `store_chunks()` |
| Vector ANN search | `src/db/search.rs` — `search_similar()` |
| Repo-scoped filtering | `src/db/search.rs` — filter param on search |
| Graph node CRUD (SourceRepo, Document, Chunk, CodeSnippet) | `src/db/nodes.rs` |
| Graph edge CRUD (BELONGS_TO, CONTAINS_CHUNK, HAS_CODE, REFERENCES) | `src/db/edges.rs` |
| Multi-hop traversal | `src/db/traverse.rs` — recursive Datalog |
| Upsert/idempotent loading | `src/db/chunks.rs` — put-if-absent by chunk_id |
| Cross-concern correlation via chunk_id | Native — single DB, single key space |

## Implementation Units

### Unit 1: Database Lifecycle & Connection

- **What:** Create `src/db/store.rs` with `DataStore` struct wrapping
  `cozo::DbInstance`. Implement `open(path)` for persistent storage and
  `open_memory()` for tests. Handle creation, reopening, and closing.
- **Files:** `src/db/store.rs`, `src/db/mod.rs`
- **Tests:** `tests/db_lifecycle_test.rs` — open, close, reopen preserves data;
  memory mode works for tests
- **Posture:** Test-first
- **2-hour estimate:** ~1 hour (CozoDB API is straightforward for basic lifecycle)

### Unit 2: Schema Definition (Datalog DDL)

- **What:** Create `src/db/schema.rs` defining stored relations:
  ```
  :create source_repos {id: String => url: String}
  :create documents {path: String => title: String, repo_id: String}
  :create chunks {chunk_id: String => content: String, heading: String,
                  position: Int, source_path: String, repo_id: String,
                  vector: <F32; 384>}
  :create code_snippets {id: String => chunk_id: String, language: String,
                         content: String}
  :create belongs_to {doc_path: String, repo_id: String}
  :create contains_chunk {doc_path: String, chunk_id: String}
  :create has_code {chunk_id: String, snippet_id: String}
  :create references {source_chunk_id: String, target_doc_path: String,
                      link_text: String}
  ```
  Implement `ensure_schema(db)` that creates relations if they don't exist.
- **Files:** `src/db/schema.rs`
- **Tests:** `tests/db_schema_test.rs` — schema creation is idempotent; calling
  twice doesn't error
- **Posture:** Test-first
- **2-hour estimate:** ~1 hour

### Unit 3: Chunk Storage & Upsert

- **What:** Create `src/db/chunks.rs` implementing:
  - `store_chunk(db, chunk, embedding)` — upsert a single chunk with its vector
  - `store_chunks_batch(db, chunks_with_embeddings)` — batch upsert
  - Uses `:put` operation for idempotent insert/update by chunk_id
- **Files:** `src/db/chunks.rs`
- **Tests:** `tests/db_chunks_test.rs` — insert, re-insert (idempotent), verify
  content and vector stored correctly
- **Posture:** Test-first
- **2-hour estimate:** ~1.5 hours

### Unit 4: Vector Search (HNSW)

- **What:** Create `src/db/search.rs` implementing:
  - `search_similar(db, query_vector, top_k, repo_filter)` — ANN search
  - Create HNSW index on the `chunks` relation's `vector` column
  - Return results with chunk_id, content, heading, distance score
- **Files:** `src/db/search.rs`
- **Tests:** `tests/db_search_test.rs` — insert known vectors, search with a
  query vector, verify correct results returned in distance order; test
  repo_filter narrows results
- **Posture:** Test-first
- **2-hour estimate:** ~1.5 hours

### Unit 5: Node CRUD (Graph Entities)

- **What:** Create `src/db/nodes.rs` implementing typed insert/query for:
  - `insert_source_repo(db, id, url)`
  - `insert_document(db, path, title, repo_id)`
  - `get_document(db, path)` / `list_documents(db, repo_id)`
  - `delete_document(db, path)` — cascades to edges
- **Files:** `src/db/nodes.rs`
- **Tests:** `tests/db_nodes_test.rs` — CRUD operations, cascade delete
- **Posture:** Test-first
- **2-hour estimate:** ~1.5 hours

### Unit 6: Edge CRUD (Relationships)

- **What:** Create `src/db/edges.rs` implementing:
  - `insert_belongs_to(db, doc_path, repo_id)`
  - `insert_contains_chunk(db, doc_path, chunk_id)`
  - `insert_has_code(db, chunk_id, snippet_id)`
  - `insert_references(db, source_chunk_id, target_doc_path, link_text)`
  - Batch variants for pipeline efficiency
- **Files:** `src/db/edges.rs`
- **Tests:** `tests/db_edges_test.rs` — insert, query, duplicate handling
- **Posture:** Test-first
- **2-hour estimate:** ~1.5 hours

### Unit 7: Graph Traversal (Recursive Datalog)

- **What:** Create `src/db/traverse.rs` implementing:
  - `get_linked_documents(db, chunk_id)` — follow REFERENCES edges
  - `get_document_chunks(db, doc_path)` — follow CONTAINS_CHUNK
  - `traverse_from_chunk(db, chunk_id, hops)` — multi-hop expansion
  - Uses CozoDB's fixed-point recursion for multi-hop queries
- **Files:** `src/db/traverse.rs`
- **Tests:** `tests/db_traverse_test.rs` — build a small graph, verify 1-hop
  and 2-hop traversal returns expected nodes
- **Posture:** Test-first
- **2-hour estimate:** ~2 hours

## Dependency Graph

```text
Unit 1 (Lifecycle) → Unit 2 (Schema) → Unit 3 (Chunk Storage)
                                      → Unit 5 (Node CRUD)
                                      → Unit 6 (Edge CRUD)
Unit 3 → Unit 4 (Vector Search)
Unit 5 + Unit 6 → Unit 7 (Traversal)
```

Linear critical path: 1 → 2 → 3 → 4 (vector search end-to-end)
Parallel path: 2 → 5 → 6 → 7 (graph operations)

## Decisions and Rationale

1. **CozoDB over LanceDB + Kùzu** — single embedded database handles vectors,
   graphs, AND relations. Reduces dependency count from 2 to 1, eliminates
   cross-DB correlation complexity, simplifies the binary (one less C++ binding).
2. **Datalog over SQL/Cypher** — CozoDB's native query language. More expressive
   for recursive graph traversal than SQL; more composable than Cypher for
   complex multi-source queries.
3. **`:put` for upserts** — CozoDB's native idempotent write operation. Inserts
   if key absent, updates if present. Perfect for incremental re-ingestion.
4. **HNSW over IVF** — CozoDB's built-in vector index type. Good recall/speed
   tradeoff for the expected dataset sizes (thousands to low millions of chunks).
5. **Single `DataStore` abstraction** — all database access routes through one
   typed Rust struct. No raw Datalog queries leak outside `src/db/`.

## Risks and Caveats

- **Risk:** CozoDB's Rust API maturity (younger ecosystem than LanceDB/Kùzu).
  **Mitigation:** Pin version; wrap behind trait interface for future swap.
- **Risk:** HNSW index performance at scale (>1M vectors).
  **Mitigation:** Acceptable for target use case (dev laptop, <100K chunks
  typical). Monitor and tune `ef_construction`/`M` params.
- **Risk:** Datalog learning curve for contributors.
  **Mitigation:** All queries encapsulated in `src/db/` modules; callers use
  typed Rust functions, never raw Datalog.
- **Risk:** Binary size increase from CozoDB.
  **Mitigation:** Still smaller than LanceDB + Kùzu combined. Single dependency
  vs. two.

## Plan Hardening Signals

- public API, schema, or contract change: **Yes** — defines the persistence
  schema that all downstream features depend on
- security, auth, permission, or compliance-sensitive behavior: **No**
- migration, backfill, destructive data/config action: **Yes** — schema creation
  is destructive if run on existing incompatible DB; need migration path
- external integration, operator checkpoint, or external dependency: **No** —
  fully embedded, no network
- high runtime, rollout, or rollback risk: **No**

**Requires plan hardening: yes**

The schema definition is a foundational contract. Changes after initial release
require migration procedures. The plan-harden step should define:
- Schema versioning strategy (version field in a metadata relation)
- Migration path for schema changes (drop-and-rebuild acceptable for v1)
- Rollback procedure (delete DB directory and re-run ingestion)

## Runtime Verification and Closure

- **Runtime surface:** Database files on disk (persistent state)
- **Verification:** Integration test performs full cycle: open DB → create schema
  → store chunks with vectors → search → traverse → verify results
- **Closure:** Feature absorbed when:
  1. All 7 unit test files pass
  2. Round-trip integration test (store → search → traverse) succeeds
  3. Idempotent re-ingestion produces identical query results
  4. Schema documentation written to `docs/design-docs/`

---

## Plan Hardening

**Hardening required: yes**

### Risk Triggers

| Signal | Present | Justification |
|---|---|---|
| Public schema/contract change | ✅ | Defines the stored-relation schema that every downstream feature (MCP tools, sync pipeline, CLI) depends on |
| Migration/destructive action | ✅ | `ensure_schema()` creates relations; running on an incompatible existing DB would fail or corrupt |
| Security/auth/compliance | ❌ | Fully local, no auth surface |
| External integration | ❌ | Embedded, no network |
| High rollback risk | ⚠️ | Schema changes after data is loaded require full re-ingestion |

### Protected Invariants

1. **Schema stability** — once the stored-relation definitions ship, any change
   to column types, key structure, or relation names is a breaking change
   requiring migration.
2. **chunk_id correlation** — the `chunk_id` field in the `chunks` relation is
   the foreign key used by edges, code_snippets, and vector search results. Its
   format (SHA-256 hex) must never change without a full re-index.
3. **Idempotent writes** — `:put` operations must be truly idempotent: storing
   the same chunk twice must not create duplicates or change vectors.
4. **Database file isolation** — the DB path must be validated against workspace
   root (path security) before opening.

### Proposed Actions (Elevated Risk)

#### Action 1: Schema Definition Freeze

- **ProposedAction:** Define stored relations with typed columns and HNSW index
- **ActionRisk:** HIGH — once data is persisted, schema changes require migration
  or full rebuild
- **Approval:** Not required (v1 — schema has not shipped yet)
- **Rollback:** Delete database directory and re-run full ingestion pipeline

#### Action 2: HNSW Index Creation

- **ProposedAction:** Create HNSW index on `chunks::vector` column
- **ActionRisk:** MEDIUM — index parameters (`ef_construction`, `M`) affect both
  recall quality and build time; changing later requires index rebuild
- **Approval:** Not required (tuning is non-destructive to data)
- **Rollback:** Drop and recreate index with different params (data preserved)

#### Action 3: Replace `src/db/vector.rs` and `src/db/graph.rs` Design

- **ProposedAction:** The original architecture defined separate vector and graph
  modules. This plan collapses them into a unified `src/db/` module.
- **ActionRisk:** LOW — no code exists yet for the original design; this is a
  forward declaration change only
- **Approval:** Not required (no existing code to break)
- **Rollback:** N/A — no prior implementation exists

### Schema Versioning Strategy

For v1 (pre-release):
- **Strategy:** Drop-and-rebuild. No migration path needed.
- **Mechanism:** Store a `_schema_version` metadata relation:
  ```
  :create _schema_version {key: String => version: Int, created_at: String}
  ```
- **On mismatch:** Log warning, delete DB directory, re-create from scratch.
- **Rationale:** Pre-release software with no user data. Full rebuild from
  source documents is always possible and takes minutes.

For v1+ (post-release):
- **Strategy:** Versioned migration scripts in `src/db/migrations/`
- **Implementation:** Deferred to a future feature (not in scope for 012-F)

### Reinforced Verification Detail

#### Environment Prechecks (Unit 1)
- Verify target DB path is writable before attempting open
- Validate path against workspace root (PathViolationError if outside)
- Check available disk space > 100 MB (warn, don't block)

#### Schema Idempotency Test (Unit 2)
- Run `ensure_schema()` twice in sequence — must not error
- Run `ensure_schema()` on DB that already has data — must preserve data
- Verify schema version matches expected constant

#### Vector Search Correctness (Unit 4)
- Insert 3 known vectors with known cosine distances
- Query with vector equidistant to two — verify tie-breaking is stable
- Query with `top_k=1` — verify only 1 result returned
- Query with repo_filter — verify excluded repos don't appear

#### Traversal Boundary Test (Unit 7)
- Build disconnected subgraphs — verify traversal doesn't cross boundaries
- Test max hops parameter — verify depth limit is respected
- Test with cyclic references — verify no infinite loops

### Reinforced Rollback Procedure

| Scenario | Rollback |
|---|---|
| Schema creation fails mid-way | Delete partially-created DB directory |
| Schema version mismatch on open | Delete DB, re-run ingestion from source |
| Vector index corrupt | Drop index, rebuild (data preserved) |
| Test DB leaks to production path | Path validation prevents this; test uses `tempdir` exclusively |

### Operational Closure Expectations

- **Monitoring signal:** Schema version logged at INFO on every DB open
- **Rollback trigger:** Schema version mismatch OR CozoDB internal error on open
- **Owner:** Pipeline maintainer (whoever merges 012-F)
- **Validation window:** Full integration test suite must pass; manual spot-check
  of vector search results on a real documentation corpus (≥100 chunks)

### Unresolved Decisions

1. **CozoDB storage engine choice** — CozoDB supports multiple backends (`mem`,
   `sqlite`, `rocksdb`). Plan assumes default (`sqlite` for persistence). May
   need operator input if performance characteristics differ significantly.
2. **HNSW parameters** — `ef_construction=200`, `M=50` are reasonable defaults.
   May need tuning after real-world corpus testing.

### Learnings and Instructions Consulted

- `.github/copilot-instructions.md` — Database Access Rules (all operations
  through `src/db/`; no raw queries outside module)
- `.github/copilot-instructions.md` — Path Security (validate all DB paths)
- `AGENTS.md` — Database Access Rules (test DBs use temp directories)
- `docs/research/architecture-blueprint.md` — original schema design (adapted)
- No prior compound learnings exist (first implementation)

---

## Plan Review

**Gate decision: ADVISORY**  
**Date:** 2026-04-29  
**Plan hardening required:** Yes  
**Plan hardening present:** Yes — substantive and complete

### Reviewer Findings

#### Constitution Reviewer — 0 findings

All five core principles satisfied:
- Local-first: CozoDB fully embedded, zero network ✅
- Lightweight footprint: one dependency replaces two (net reduction) ✅
- Data pipeline integrity: idempotent `:put`, stable chunk_ids, versioned schema ✅
- MCP-native: module outputs serve future MCP tools ✅
- Automation: schema creation idempotent, pipeline re-runnable ✅

Note: Plan correctly updates the architecture from the original blueprint (LanceDB
+ Kùzu → CozoDB). The `.github/copilot-instructions.md` Technology Stack table
will need updating as a follow-up chore (not in scope for 012-F).

#### Rust Reviewer — 1 finding (P2)

- **P2:** Plan does not specify thread-safety guarantees for `DataStore`. The MCP
  server runs on tokio with concurrent tool calls. CozoDB's `DbInstance` is
  internally `Send + Sync`, but the plan should explicitly state that `DataStore`
  will be `Arc<DataStore>` or implement `Clone` via `Arc` internally.
  *Recommendation: Add a note to Unit 1 specifying `DataStore` wraps
  `Arc<DbInstance>` for safe sharing across async tasks.*

#### Scope Boundary Auditor — 1 finding (P2)

- **P2:** Unit 7 (Graph Traversal) estimates ~2 hours and includes both simple
  1-hop queries AND recursive multi-hop with cycle detection. If cycle detection
  proves complex in Datalog, this unit may exceed the 2-hour boundary.
  *Recommendation: Split into Unit 7a (simple 1-hop/2-hop traversal) and Unit 7b
  (recursive multi-hop with cycle detection) if implementation reveals complexity.
  Acceptable to start as one unit and split during execution.*

#### Learnings Researcher — 0 findings

- No compound learnings exist (empty library)
- Architecture blueprint consulted and correctly adapted
- No contradictions with prior decisions

#### Architecture Strategist — 1 finding (P2)

- **P2:** The plan encapsulates all Datalog behind typed functions (Decision 5),
  which is correct. However, consider defining a `DataStore` trait (not just a
  struct) to enable test mocking without requiring a real CozoDB instance for
  unit tests of pipeline stages that depend on the store.
  *Recommendation: Define `trait DataStoreOps` with the public query methods,
  implement for the real `DataStore`, and use `mockall` or manual mock in pipeline
  stage tests. This can be done during implementation without changing the plan
  structure.*

### Hardening Assessment

Plan hardening is **present and sufficient**:
- ✅ Risk triggers identified with justification
- ✅ Protected invariants enumerated (schema stability, chunk_id, idempotency, path isolation)
- ✅ ProposedAction/ActionRisk entries for all elevated-risk actions
- ✅ Schema versioning strategy (drop-and-rebuild for v1, migration scripts deferred)
- ✅ Reinforced verification with specific test scenarios
- ✅ Rollback procedures for each failure mode
- ✅ Operational closure with monitoring, triggers, owner, and validation window

### Summary

Plan is architecturally sound with hardening complete. Three P2 findings noted:

1. Specify `Arc<DbInstance>` wrapping for thread-safe async access
2. Be prepared to split Unit 7 if cycle detection is complex
3. Consider `DataStoreOps` trait for test mockability

None block harvest. Implementer should address during execution. Proceed to
harvest with these advisories carried forward.
