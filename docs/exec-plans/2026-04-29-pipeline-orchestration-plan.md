# Implementation Plan: 007-F — Ingestion Pipeline Orchestration

**Feature:** 007-F  
**Status:** Ready for implementation  
**Date:** 2026-04-29  
**Dependencies:** 002-F (Acquisition) ✓, 003-F (Parsing) ✓, 012-F (CozoDB) ✓

## Problem Frame

The codebase has four fully implemented, independently tested modules:

- `src/acquire/` — clones Git repos and scans local dirs, returning `AcquisitionResult` with per-source `FilteredFileSet` (file path lists)
- `src/parse/` — parses markdown into `ParsedDocument` (chunks, references, code snippets)
- `src/embed/` — produces 384-dim vectors via `all-MiniLM-L6-v2` (Candle in-process)
- `src/db/` — CozoDB unified store with `upsert_chunk`, `upsert_edge`, `upsert_code_snippet`, `upsert_source`

**No orchestrator exists.** There is no code that ties these stages together into an end-to-end pipeline. The `main.rs` is a placeholder. This feature creates `src/pipeline/` to compose the stages into a reliable, idempotent, error-resilient ingestion flow.

### Data Flow

```text
SourceConfig
  └─ acquire::execute() → AcquisitionResult { Vec<SourceOutcome> }
       └─ per SourceOutcome::Success(FilteredFileSet { files: Vec<PathBuf> })
            └─ std::fs::read_to_string(file) → raw markdown
                 └─ parse::parse_document(content, path) → ParsedDocument
                      └─ embed::embed_batch(model, chunk_contents) → Vec<Vec<f32>>
                           └─ db::upsert_chunk + db::upsert_edge + db::upsert_code_snippet
```

### Key Design Decisions Already Made

- Sequential stage ordering (R-07 from research doc): no workflow engine
- Content-hash chunk IDs for idempotent upserts (R-01)
- Heading-based chunking (R-02)
- Continue-on-failure semantics per file (project guidelines: pipeline stages are resilient)

## Requirements Trace

| Task | Requirement | Implementation Target |
|---|---|---|
| 007.001-T | Stage sequencing: acquire → parse → embed → load | `src/pipeline/mod.rs` — `run()` orchestrator |
| 007.002-T | Batch processing with parallelism | `src/pipeline/mod.rs` — configurable batch size, optional rayon |
| 007.003-T | Structured progress reporting | `src/pipeline/progress.rs` — stage-level tracing spans |
| 007.004-T | Per-item error resilience | `src/pipeline/mod.rs` — per-file `Result` collection, error summary |
| 007.005-T | Idempotent execution | Upsert patterns in db layer (already implemented); skip-if-exists in acquire (already implemented) |
| 007.006-T | Upgrade cozo/git2 audit deps | `Cargo.toml` dependency bump |

## Implementation Units

### Unit 1: Pipeline Module Scaffold and Stage Sequencing (007.001-T)

**What:** Create `src/pipeline/mod.rs` with a `run()` function that orchestrates the four stages in order. Define a `PipelineConfig` struct for runtime parameters (data root, batch size, parallelism). Define a `PipelineResult` struct summarizing outcomes.

**Files:**
- `src/pipeline/mod.rs` (new)
- `src/lib.rs` (add `pub mod pipeline;` and re-exports)

**Tests:** `tests/pipeline_sequencing_test.rs`
- Given a `SourceConfig` with one local source pointing at a temp dir containing 3 markdown files, `pipeline::run()` produces a `PipelineResult` with 3 documents processed
- Database contains the expected chunks, edges, and code snippets after run
- Running the pipeline twice on identical input produces identical database state (idempotency)

**Posture:** Test-first  
**Scope:** 2 files new, 1 file modified, 3 test scenarios

### Unit 2: Per-File Error Resilience (007.004-T)

**What:** Wrap each file's parse/embed/load cycle in error handling. Failures on one file do not abort the batch. Accumulate per-file errors into `PipelineResult::errors: Vec<FileError>`.

**Files:**
- `src/pipeline/mod.rs` (extend `run()` inner loop)

**Tests:** `tests/pipeline_resilience_test.rs`
- Given a source with 3 files where one contains invalid UTF-8 or triggers a parse error, the pipeline processes the other 2 successfully and reports 1 error in `PipelineResult`
- The error entry contains the file path and error message

**Posture:** Test-first  
**Scope:** 1 file modified, 2 test scenarios

### Unit 3: Structured Progress Reporting (007.003-T)

**What:** Add tracing spans around each stage transition and per-file processing. Emit structured `tracing::info!` events at stage boundaries with counts and elapsed time. Use `tracing::debug!` for per-file progress.

**Files:**
- `src/pipeline/mod.rs` (add span instrumentation)

**Tests:** `tests/pipeline_progress_test.rs`
- Given a tracing subscriber capturing events, running the pipeline emits INFO events matching `"stage complete"` with `stage`, `count`, and `elapsed_ms` fields
- DEBUG events are emitted per file

**Posture:** Test-first (tracing-test or custom subscriber)  
**Scope:** 1 file modified, 2 test scenarios

### Unit 4: Batch Processing with Optional Parallelism (007.002-T)

**What:** Process files in configurable batch sizes. When `PipelineConfig::parallel` is true, use rayon's `par_iter` for the embed step (CPU-bound). Parse and load remain sequential (I/O-bound with database writes).

**Files:**
- `src/pipeline/mod.rs` (batch loop refactor)
- `Cargo.toml` (rayon is already available via `cozo`'s `rayon` feature)

**Tests:** `tests/pipeline_batch_test.rs`
- With batch_size=2 and 5 files, all 5 files are processed across 3 batches
- With parallel=true, embedding results are identical to sequential (determinism check)

**Posture:** Test-first  
**Scope:** 1 file modified, 2 test scenarios

### Unit 5: Idempotent Execution Verification (007.005-T)

**What:** Integration test proving end-to-end idempotency. The pipeline already uses upsert patterns (db layer) and skip-if-exists (acquire layer). This unit adds an explicit integration test and ensures no duplicate records accumulate.

**Files:**
- `tests/pipeline_idempotent_test.rs` (new)

**Tests:**
- Run pipeline twice on same source. Assert chunk count equals document chunk count (not doubled).
- Modify one file's content, re-run. Assert only that file's chunks updated; other chunks unchanged.

**Posture:** Test-first  
**Scope:** 1 new test file, 2 test scenarios

### Unit 6: Dependency Audit Upgrade (007.006-T)

**What:** Bump `cozo` and `git2` crate versions in `Cargo.toml` to resolve RUSTSEC-2026-0041 (lz4_flex) and RUSTSEC-2026-0008 (git2 unsound Buf deref). Run `cargo audit` to confirm clean.

**Files:**
- `Cargo.toml` (version bumps)
- `Cargo.lock` (regenerated)

**Tests:**
- All existing tests pass after upgrade (regression check)
- `cargo audit` reports zero advisories

**Posture:** Migration-first (upgrade, then verify)  
**Scope:** 2 files modified, 0 new test scenarios (existing tests serve as regression)

## Dependency Graph

```text
Unit 1 (scaffold + sequencing)
  ├── Unit 2 (error resilience) — extends the inner loop from Unit 1
  ├── Unit 3 (progress reporting) — instruments the loop from Unit 1
  └── Unit 4 (batch parallelism) — refactors the loop from Unit 1

Unit 5 (idempotency test) — depends on Units 1 + 2 being functional

Unit 6 (dep upgrade) — independent, can be done first or in parallel
```

**Recommended execution order:**
1. Unit 6 (dep upgrade — unblocks clean audits, quick win)
2. Unit 1 (scaffold — everything else builds on this)
3. Unit 2 (error resilience — core reliability)
4. Unit 3 (progress reporting — observability)
5. Unit 4 (batch parallelism — performance)
6. Unit 5 (idempotency integration test — validation)

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Single `src/pipeline/mod.rs` rather than per-stage files | The orchestrator is thin glue code (~150-200 lines). Splitting into `acquire_stage.rs`, `parse_stage.rs` etc. would over-fragment what is essentially one function with four sequential calls. |
| Embed step is the parallelism target | Embedding is CPU-bound (model inference). Parse is fast. DB writes must be sequential (CozoDB SQLite backend is not concurrent-write safe). Acquire is I/O-bound but already handles its own parallelism. |
| `PipelineConfig` struct for all runtime params | Avoids function signatures with many parameters. Extensible for future config (e.g., incremental sync state, which is 008-F scope). |
| Error accumulation, not early-abort | Per project guidelines: "per-item error handling with continue-on-failure semantics." One bad file should not prevent ingestion of thousands of good files. |
| No embedding storage in this feature | CozoDB HNSW vector indexing is planned but not yet implemented. This feature generates embeddings and stores raw chunk content. Vector search integration is deferred to the HNSW implementation task. Embeddings can be stored as a JSON float array in a new column if needed for future retrieval, but the primary search path currently is text/keyword. |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| CozoDB SQLite WAL lock under parallel writes | Only the embed step uses rayon; db writes remain sequential. No concurrent write contention. |
| Large embedding model load time (~2-5s for first inference) | Load model once in `run()`, pass `&EmbeddingModel` through the pipeline. Do not reload per batch. |
| Memory pressure with many files | Batch processing limits in-flight data. Process `batch_size` files end-to-end before starting the next batch. |
| `cargo audit` dep upgrade may break API compatibility | Pin to compatible minor versions. Run full test suite after upgrade. If cozo 0.8+ has breaking changes, stay on latest 0.7.x patch. |
| Embedding dimension mismatch if model changes | The model is fixed (`all-MiniLM-L6-v2`, 384-dim). No runtime model selection in this feature. |

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | **No** | `pipeline::run()` is `pub(crate)`, no external API. DB schema unchanged. |
| Security, auth, permission, or compliance | **No** | All operations are local filesystem + embedded DB. No network, no auth. |
| Migration, backfill, destructive data/config | **No** | Pipeline creates new data via upserts. No destructive operations. |
| External integration, operator checkpoint | **No** | No external services. No operator approval points. |
| High runtime, rollout, or rollback risk | **No** | Feature adds a new module. Rollback = revert the commit. No data migration. |

**Requires plan hardening: no**

## Runtime Verification and Closure

### Changed Runtime Surfaces

The pipeline module does not expose a user-facing runtime surface in this feature (no CLI command yet — that's 010-F scope). It is a library-internal orchestrator invoked by tests.

### Verification Criteria

- All quality gates pass: `cargo check` → `cargo clippy` → `cargo fmt` → `cargo test`
- Integration test `pipeline_sequencing_test` exercises the full acquire → parse → embed → load flow against a temp directory with real markdown files
- `cargo audit` clean after dependency upgrade

### Operational Closure

No operational closure artifacts needed — this is an internal library module with no deployment, monitoring, or rollback concerns beyond standard version control.

## Plan Review

**Gate Decision: ADVISORY**  
**Date:** 2026-04-29  
**Reviewers:** Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor, Learnings Researcher

### Rationale

The plan is architecturally sound, correctly scoped, and aligned with all five constitutional principles. No P0 findings survived deduplication. Initial P1 findings were downgraded to P2 after cross-referencing with existing codebase safeguards (acquire module already validates paths, DataStore is Arc-wrapped, existing error patterns are established). The plan may proceed to harvest with awareness of the advisory findings below.

### Hardening Assessment

Plan hardening was evaluated and deemed **not required** — no high-risk signals detected (no external integrations, no data migrations, no irreversible operations).

### Merged Findings

#### P2 — Path validation for pipeline input files

**Source:** Constitution Reviewer, Scope Auditor  
**Units affected:** Unit 1, Unit 2  
**Description:** The plan does not explicitly specify path validation in the pipeline orchestrator. The constitution requires all file paths be resolved via `canonicalize()` and validated against an allowed root.  
**Mitigating context:** The acquire module already validates source directories and returns only files within configured roots. The pipeline receives paths from acquire's output, not from raw user input.  
**Recommendation:** Add a defensive `canonicalize()` + `starts_with(data_root)` check in `src/pipeline/mod.rs` as belt-and-suspenders. Include a test scenario for path traversal rejection.

#### P2 — Error accumulation mechanism needs specification

**Source:** Constitution Reviewer, Rust Reviewer  
**Units affected:** Unit 2  
**Description:** The plan states "continue-on-failure" with error accumulation but doesn't define the collection type or reporting interface in `PipelineResult`.  
**Recommendation:** Define `PipelineResult` with `errors_encountered: Vec<(String, GraphtorError)>`. Log each failure at WARN level with path context. Clarify that a file parse failure does not block sibling files.

#### P2 — Thread synchronization for rayon parallel embedding

**Source:** Constitution Reviewer, Rust Reviewer  
**Units affected:** Unit 4  
**Description:** Plan proposes rayon for embed step but doesn't specify the data sharing pattern.  
**Recommendation:** Use `rayon::scope()` with `Arc<DataStore>` clones. Collect embeddings per-file into thread-local vecs, join results sequentially before DB write. Document the pattern.

#### P2 — EmbeddingModel lifecycle and ownership

**Source:** Rust Reviewer  
**Units affected:** Unit 4  
**Description:** Plan says "load once" but doesn't specify timing (before/after acquire) or ownership (in config vs. local).  
**Recommendation:** Load eagerly in `run()` prelude before the main loop. Hold as `Arc<EmbeddingModel>` for rayon compatibility. Add doc comment noting the ~80MB download cost.

#### P2 — Idempotency test scenarios incomplete

**Source:** Constitution Reviewer  
**Units affected:** Unit 5  
**Description:** Plan tests re-run unchanged and re-run with modification, but doesn't verify deterministic chunk_id generation or unchanged-file skipping.  
**Recommendation:** Add a third scenario: verify SHA-256 chunk_ids are deterministic (same content + path → same ID). Future incremental sync (008-F) will add skip-unchanged; not needed here.

#### P2 — Unit 6 (dependency audit) scope belongs in separate chore

**Source:** Scope Auditor, Learnings Researcher  
**Units affected:** Unit 6  
**Description:** Dependency version bumps are maintenance work orthogonal to pipeline functionality. Including them in 007-F blurs scope boundaries.  
**Recommendation:** Consider extracting to a separate maintenance task. If retained, document that the upgrade is a prerequisite for CI gate compliance, not a feature requirement. Note: current CI uses `cargo-audit` with `--ignore` flags (audit.toml not auto-discovered per compound learning).

#### P2 — Ephemeral file handling (.gitignore coverage)

**Source:** Learnings Researcher  
**Description:** Pipeline operations may generate transient cache, state, or lock files. These must not conflict with git operations.  
**Recommendation:** Verify `.gitignore` covers any ephemeral artifacts the pipeline creates (temp chunks, intermediate state). Document the pattern.

#### P2 — Documentation synchronization

**Source:** Learnings Researcher  
**Description:** Architecture docs referencing pipeline capabilities must be updated in the same PR as implementation.  
**Recommendation:** Update `copilot-instructions.md` pipeline stage references in the same commit as the implementation. Use "(planned)" markers only for genuinely future features.

#### P3 — Tracing instrumentation coverage

**Source:** Rust Reviewer  
**Description:** Plan mentions tracing but doesn't enumerate instrumentation points.  
**Recommendation:** Emit structured logs at: pipeline start/end, per-stage completion with counts, per-file errors, and final summary.

#### P3 — Async vs sync function signature

**Source:** Rust Reviewer  
**Description:** If all operations (parse, embed, DB write) are synchronous, `async fn run()` adds overhead.  
**Recommendation:** Use sync `pub(crate) fn run()` unless async I/O is planned. Current CozoDB and Candle operations are all synchronous.

#### P3 — Module size ceiling

**Source:** Constitution Reviewer  
**Description:** Single `mod.rs` (~150-200 lines) is fine now but should be refactored if it exceeds 250 lines.  
**Recommendation:** Document the ceiling. If conditional stage execution or retry logic is added later, split into per-stage submodules.

#### P3 — Serialization of PipelineConfig/PipelineResult

**Source:** Rust Reviewer  
**Description:** If pipeline state is persisted (future), these types need serde derives.  
**Recommendation:** Omit serde for now (no persistence in 007-F scope). Add when 008-F introduces sync state.

### Gate Summary

| Severity | Count | Action |
|----------|-------|--------|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 8 | Noted for implementation awareness |
| P3 | 4 | Advisory only |

**Decision: ADVISORY — Proceed to harvest.** All P2 findings are implementation-level concerns addressable during coding without plan revision. No architectural or safety blockers identified.
