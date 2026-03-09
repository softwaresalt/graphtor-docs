# Tasks: Documentation Ingestion Pipeline

**Input**: Design documents from `/specs/001-doc-ingestion-pipeline/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/cli-contract.md

**Tests**: Tests are included — each user story has integration tests to verify independent functionality.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization, dependency management, and basic structure

- [ ] T001 Create project directory structure per plan.md layout under src/ and tests/
- [ ] T002 Create pyproject.toml with Python 3.11+ requirement and dependencies: lancedb, kuzu, pyarrow, ollama, pydantic, click (CLI)
- [ ] T003 [P] Create src/__init__.py, src/common/__init__.py, src/acquire/__init__.py, src/normalize/__init__.py, src/chunk/__init__.py, src/extract/__init__.py, src/embed/__init__.py, src/load/__init__.py
- [ ] T004 [P] Create tests/conftest.py with shared pytest fixtures directory structure and sample data path constants

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core utilities that ALL user stories depend on — MUST complete before any story phase

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T005 [P] Implement deterministic chunk_id generation (SHA-256 of normalized text + source path) in src/common/chunk_id.py
- [ ] T006 [P] Implement structured logging utilities (stdout for progress, stderr for errors) in src/common/logging.py with `[STAGE]` prefix format
- [ ] T007 [P] Implement pipeline configuration dataclass with defaults (ollama_url, embed_model, extract_model, output_dir) in src/common/config.py
- [ ] T008 Create CLI entry point with click command group and shared options in src/cli.py
- [ ] T009 [P] Create sample test fixtures: sample markdown files with YAML frontmatter, UI extensions, heading structure in tests/fixtures/

**Checkpoint**: Foundation ready — user story implementation can now begin

---

## Phase 3: User Story 1 — Acquire Documentation Repositories (Priority: P1) 🎯 MVP

**Goal**: Clone targeted documentation repositories from the manifest, organized by thematic group, with skip-if-exists logic

**Independent Test**: Run `acquire --manifest paths/ms-docs-grouped.txt --groups 1` and verify repos appear in group folder

### Tests for User Story 1

- [ ] T010 [P] [US1] Unit test for manifest parser (group detection, URL extraction, comment skipping) in tests/unit/test_manifest.py
- [ ] T011 [P] [US1] Integration test for acquire stage (clone small test repo, verify skip-on-rerun) in tests/integration/test_acquire_pipeline.py

### Implementation for User Story 1

- [ ] T012 [P] [US1] Implement manifest parser to read ms-docs-grouped.txt, extract groups with numbers/titles/URLs, skip commented lines in src/acquire/manifest.py
- [ ] T013 [US1] Implement repository cloner with shallow clone (--depth 1), skip-if-exists, group folder organization, and error handling in src/acquire/cloner.py
- [ ] T013a [US1] Implement locale filter to exclude non-English (non-en-us) content directories after acquisition in src/acquire/cloner.py (FR-004)
- [ ] T014 [US1] Wire `acquire` subcommand in src/cli.py with --manifest, --groups, --output-dir options per CLI contract
- [ ] T015 [US1] Add progress logging for acquire stage: repo count, skip notifications, clone status per src/common/logging.py patterns

**Checkpoint**: Acquire stage fully functional — repos can be cloned from manifest

---

## Phase 4: User Story 2 — Normalize Raw Documentation (Priority: P1)

**Goal**: Strip YAML frontmatter (keep title/date/description), remove UI extensions, produce clean markdown idempotently

**Independent Test**: Run `normalize --source <acquired-dir>` and verify output files have no frontmatter or UI tags; re-run and verify identical output

### Tests for User Story 2

- [ ] T016 [P] [US2] Unit test for frontmatter stripping (keep title/date/description, strip rest, handle malformed YAML) in tests/unit/test_frontmatter.py
- [ ] T017 [P] [US2] Unit test for UI tag removal (:::zone, [!NOTE], [!TIP], [!WARNING], [!IMPORTANT], [!CAUTION], moniker ranges, tab groups) in tests/unit/test_ui_tags.py
- [ ] T018 [P] [US2] Integration test for normalization pipeline (process directory, verify idempotency, skip non-md files) in tests/integration/test_normalize_pipeline.py

### Implementation for User Story 2

- [ ] T019 [P] [US2] Implement YAML frontmatter parser and stripper (retain only title, ms.date, description fields) in src/normalize/frontmatter.py
- [ ] T020 [P] [US2] Implement UI tag/extension remover (regex-based stripping of DocFX extensions) in src/normalize/ui_tags.py
- [ ] T021 [US2] Implement normalization orchestrator: traverse directory, filter .md files, apply frontmatter + UI tag passes, write output in src/normalize/processor.py
- [ ] T022 [US2] Wire `normalize` subcommand in src/cli.py with --source, --output-dir options per CLI contract
- [ ] T023 [US2] Add progress logging for normalize stage: file count, skip notifications, error summaries

**Checkpoint**: Normalize stage fully functional — raw docs produce clean markdown idempotently

---

## Phase 5: User Story 3 — Chunk Documents into Retrievable Segments (Priority: P2)

**Goal**: Split normalized markdown by H2/H3 headings into self-contained chunks with stable chunk_id and provenance metadata

**Independent Test**: Run `chunk --source <normalized-dir>` and verify output JSON files contain chunks with chunk_id, text, document_title, source_url, parent_headers

### Tests for User Story 3

- [ ] T024 [P] [US3] Unit test for markdown chunker (H2/H3 splitting, no-heading fallback, metadata propagation, chunk_id stability) in tests/unit/test_chunker.py
- [ ] T025 [P] [US3] Unit test for chunk_id stability (same content → same ID, changed content → new ID) in tests/unit/test_chunk_id.py

### Implementation for User Story 3

- [ ] T026 [US3] Implement heading-based markdown chunker: split on H2/H3 boundaries, include H4+ in parent chunk, propagate parent_headers, assign heading_level in src/chunk/markdown_chunker.py
- [ ] T027 [US3] Integrate chunk_id generation (from src/common/chunk_id.py) into chunker output with document_title and source_url metadata
- [ ] T028 [US3] Wire `chunk` subcommand in src/cli.py with --source, --output-dir options per CLI contract; output JSON files per document
- [ ] T029 [US3] Add progress logging for chunk stage: documents processed, chunks generated, empty-section warnings

**Checkpoint**: Chunk stage fully functional — normalized docs become retrievable segments with stable IDs

---

## Phase 6: User Story 4 — Extract Knowledge Graph Entities (Priority: P2)

**Goal**: Send chunks to local LLM, extract structured entities (Service, SDK_Class, Concept, CodeSnippet) and relationships (CONTAINS, REQUIRES_CONFIG, HAS_EXAMPLE) with retry on malformed output

**Independent Test**: Run `extract --chunks <chunks-dir>` and verify output JSON files contain valid nodes and edges with chunk_id references

### Tests for User Story 4

- [ ] T030 [P] [US4] Unit test for extraction JSON schema validation (valid/invalid LLM responses, Pydantic model enforcement) in tests/unit/test_schema.py
- [ ] T031 [P] [US4] Unit test for extractor retry logic (malformed output → retry, persistent failure → log and skip) in tests/unit/test_extractor.py
- [ ] T032 [P] [US4] Integration test for extraction pipeline (mock Ollama responses, verify entity/edge output) in tests/integration/test_extract_pipeline.py

### Implementation for User Story 4

- [ ] T033 [P] [US4] Define Pydantic models for extraction output: GraphNode (name, node_type, description, language, chunk_id), GraphEdge (source, target, relationship) in src/extract/schema.py
- [ ] T034 [P] [US4] Create LLM prompt templates for entity extraction with strict JSON schema instructions in src/extract/prompt.py
- [ ] T035 [US4] Implement extraction orchestrator: iterate chunks, call Ollama, validate response against Pydantic schema, retry on failure (max 2 retries), log failures in src/extract/extractor.py
- [ ] T036 [US4] Wire `extract` subcommand in src/cli.py with --chunks, --output-dir, --ollama-url, --model options per CLI contract
- [ ] T037 [US4] Add progress logging for extract stage: chunks processed, entities found, retry count, failure count

**Checkpoint**: Extract stage fully functional — chunks produce structured knowledge graph data

---

## Phase 7: User Story 5 — Load Data into Embedded Databases (Priority: P3)

**Goal**: Compute embeddings, load embedded chunks into LanceDB and graph entities into Kùzu with upsert behavior and cross-DB chunk_id correlation

**Independent Test**: Run `embed` then `load`, then `verify --correlation` to confirm both databases populated and linked

### Tests for User Story 5

- [ ] T038 [P] [US5] Integration test for LanceDB loading (create table, insert chunks, verify upsert, query by chunk_id) in tests/integration/test_load_pipeline.py
- [ ] T039 [P] [US5] Integration test for Kùzu loading (create schema, insert nodes/edges, verify chunk_id properties, query relationships) in tests/integration/test_load_pipeline.py

### Implementation for User Story 5

- [ ] T040 [P] [US5] Implement vector embedding via Ollama (nomic-embed-text, 768 dims, batch processing) in src/embed/embedder.py
- [ ] T041 [P] [US5] Implement LanceDB loader: create/open table with PyArrow schema, upsert embedded chunks by chunk_id in src/load/lance_loader.py
- [ ] T042 [P] [US5] Implement Kùzu loader: create node/rel tables (Cypher DDL from data-model.md), insert/upsert nodes and edges with chunk_id in src/load/kuzu_loader.py
- [ ] T043 [US5] Wire `embed` subcommand in src/cli.py with --chunks, --output-dir, --ollama-url, --model options
- [ ] T044 [US5] Wire `load` subcommand in src/cli.py with --embeddings, --entities, --lance-dir, --kuzu-dir options
- [ ] T045 [US5] Wire `verify` subcommand in src/cli.py with --db, --correlation, --lance-dir, --kuzu-dir options
- [ ] T046 [US5] Add progress logging for embed/load stages: items processed, DB stats, correlation check results

**Checkpoint**: Both databases populated and queryable; cross-DB correlation verified

---

## Phase 8: User Story 6 — Run the Full Pipeline End-to-End (Priority: P3)

**Goal**: Orchestrate all stages (acquire → normalize → chunk → extract → embed → load) as a single `run` command with progress reporting and graceful error handling

**Independent Test**: Run `python -m src.cli run --manifest paths/ms-docs-grouped.txt --groups 1` on a clean system and verify both databases populated

### Tests for User Story 6

- [ ] T047 [US6] Integration test for full pipeline (mock Ollama, run end-to-end on test fixtures, verify DB state) in tests/integration/test_full_pipeline.py

### Implementation for User Story 6

- [ ] T048 [US6] Implement `run` command in src/cli.py that orchestrates acquire → normalize → chunk → extract → embed → load in sequence with shared config
- [ ] T049 [US6] Implement exit code logic: 0 (all success), 1 (partial failures), 2 (stage failure) per CLI contract
- [ ] T050 [US6] Add stage-transition logging: `[PIPELINE] Stage N/6 complete: {stage_name} — {items_processed} items`
- [ ] T051 [US6] Implement graceful error handling: catch per-item failures, accumulate error log, continue pipeline unless stage completely fails

**Checkpoint**: Full pipeline executable as single command — the complete developer workflow

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Quality improvements across all stages

- [ ] T052 [P] Add CLI --verbose flag for detailed output across all subcommands in src/cli.py
- [ ] T053 [P] Create README section or update quickstart.md with final CLI usage examples
- [ ] T054 Run full pipeline against 2+ real repository groups and validate SCENARIOS.md success criteria (SC-001 through SC-009)
- [ ] T055 [P] Add type hints and docstrings to all public functions across src/
- [ ] T056 Code cleanup: remove debug prints, ensure consistent error message formatting, validate all edge cases from SCENARIOS.md

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 completion — BLOCKS all user stories
- **US1 Acquire (Phase 3)**: Depends on Phase 2
- **US2 Normalize (Phase 4)**: Depends on Phase 2 (can run in parallel with US1 if test fixtures used)
- **US3 Chunk (Phase 5)**: Depends on Phase 2 (can run in parallel with US1/US2 if test fixtures used)
- **US4 Extract (Phase 6)**: Depends on Phase 2 (can run in parallel with US1-US3 if test fixtures used)
- **US5 Load (Phase 7)**: Depends on Phase 2 (can run in parallel with US1-US4 if test fixtures used)
- **US6 End-to-End (Phase 8)**: Depends on ALL user stories (Phases 3-7) being complete
- **Polish (Phase 9)**: Depends on Phase 8

### User Story Dependencies

- **US1 (Acquire)**: Independent — no dependency on other stories
- **US2 (Normalize)**: Independent — can use test fixtures without acquire output
- **US3 (Chunk)**: Independent — can use test fixtures without normalize output
- **US4 (Extract)**: Independent — can use test fixtures without chunk output
- **US5 (Load)**: Independent — can use test fixtures without extract/embed output
- **US6 (E2E)**: Depends on US1 + US2 + US3 + US4 + US5

### Within Each User Story

- Tests written first (TDD) → should fail before implementation
- Models/utilities before orchestration logic
- CLI wiring after core logic
- Progress logging last

### Parallel Opportunities

**Phase 2** (all foundational tasks are independent files):
- T005, T006, T007, T009 can run in parallel

**Each user story** (tests can run in parallel within a story):
- US2: T016, T017, T018 in parallel; then T019, T020 in parallel
- US4: T030, T031, T032 in parallel; then T033, T034 in parallel
- US5: T038, T039 in parallel; then T040, T041, T042 in parallel

**Cross-story** (with test fixtures, all stories can develop in parallel):
- US1, US2, US3, US4, US5 can all develop simultaneously against fixture data

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: US1 — Acquire
4. Complete Phase 4: US2 — Normalize
5. **STOP and VALIDATE**: Acquire repos, normalize them, verify clean output
6. This delivers the data acquisition layer — valuable even without the rest

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 (Acquire) → Test independently → Repos cloned
3. Add US2 (Normalize) → Test independently → Clean markdown
4. Add US3 (Chunk) → Test independently → Retrievable segments
5. Add US4 (Extract) → Test independently → Knowledge graph data
6. Add US5 (Load) → Test independently → Both databases populated
7. Add US6 (E2E) → Full pipeline operational
8. Polish → Production-ready

---

## Notes

- Total tasks: 57 (T001–T056, including T013a)
- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable against test fixtures
- SCENARIOS.md (56 scenarios) maps to test tasks for validation
- Commit after each task or logical group
