# Tasks: Rust Foundation & Core Types

**Input**: Design documents from `/specs/002-rust-foundation/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/library-api-contract.md

**Tests**: Tests are included — TDD is required by the project constitution. Write tests first, confirm they fail, then implement.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Cargo Workspace)

**Purpose**: Initialize the Rust project structure and dependency manifest

- [x] T001 Create Cargo.toml workspace with library crate (graphtor-core) and binary target (graphtor-docs) per plan.md structure
- [x] T002 [P] Create directory structure: src/config/, src/error/, src/chunk/, src/logging/, src/path/ with mod.rs files
- [x] T003 [P] Create src/lib.rs with module declarations and public re-exports
- [x] T004 [P] Create src/main.rs with placeholder binary entry point
- [x] T005 [P] Create tests/ directory for integration tests

---

## Phase 2: Foundational — Error Type Hierarchy (US2, promoted to foundational)

**Purpose**: Error types are used by every other module — MUST complete before user story phases

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

> **Note**: User Story 2 (Error Diagnostics) was promoted to the Foundational phase because all other user stories depend on the error type hierarchy. Tasks carry the [US2] label for traceability.

### Tests for Error Types

- [ ] T006 [P] [US2] Write unit tests for GraphtorError variant construction and Display output in src/error/types.rs (test module)
- [ ] T007 [P] [US2] Write unit tests for From conversions (std::io::Error → GraphtorError::Io, serde_yaml::Error → GraphtorError::Config) in src/error/types.rs (test module)
- [ ] T008 [US2] Write integration test verifying all 8 error categories produce distinct, human-readable messages with context in tests/error_test.rs

### Implementation for Error Types

- [ ] T009 [US2] Define GraphtorError enum with thiserror derives in src/error/types.rs: Config, Database, Pipeline, Parse, Embed, PathViolation, Sync, Io variants per data-model.md
- [ ] T010 [P] [US2] Implement From<std::io::Error> and From<serde_yaml::Error> conversions in src/error/types.rs
- [ ] T011 [US2] Implement Display format `[{category}] {message}: {context}` for all variants in src/error/types.rs
- [ ] T012 [US2] Export error types from src/error/mod.rs and re-export from src/lib.rs

**Checkpoint**: Error types available — all modules can now use `GraphtorError` for typed error handling

---

## Phase 3: User Story 1 — Configuration Parsing (Priority: P1) 🎯 MVP

**Goal**: Parse and validate sources.yaml with Git and local source definitions

**Independent Test**: Provide various valid/invalid YAML files and verify parsing succeeds or produces specific error messages

### Tests for User Story 1

- [ ] T013 [P] [US1] Write unit tests for SourceConfig deserialization from valid YAML (Git + local sources, all fields, defaults) in src/config/source.rs (test module)
- [ ] T014 [P] [US1] Write unit tests for config validation: duplicate IDs, empty fields, invalid glob patterns in src/config/validation.rs (test module)
- [ ] T015 [P] [US1] Write unit tests for edge cases: empty sources list, wrong YAML structure, non-existent file in src/config/source.rs (test module)
- [ ] T016 [US1] Write integration test for end-to-end config parsing from file in tests/config_test.rs

### Implementation for User Story 1

- [ ] T017 [P] [US1] Define SourceConfig, GitSource, LocalSource structs with serde derives in src/config/source.rs per data-model.md
- [ ] T018 [P] [US1] Implement default values (branch = "main") via serde default attributes in src/config/source.rs
- [ ] T019 [US1] Implement SourceConfig::parse(path) to read YAML file and deserialize in src/config/source.rs
- [ ] T020 [US1] Implement config validation: duplicate ID detection, source ID format check, glob pattern syntax check in src/config/validation.rs
- [ ] T021 [US1] Implement clear error mapping: serde_yaml errors → GraphtorError::Config with field/position context in src/config/source.rs
- [ ] T022 [US1] Export config types from src/config/mod.rs and re-export from src/lib.rs

**Checkpoint**: sources.yaml parsing and validation functional — can load developer's documentation registry

---

## Phase 4: User Story 3 — Chunk ID Generation (Priority: P2)

**Goal**: Deterministic SHA-256 chunk identifiers for cross-database correlation

**Independent Test**: Generate IDs for known inputs and verify determinism, uniqueness, and format

### Tests for User Story 3

- [ ] T023 [P] [US3] Write unit tests for chunk_id determinism: same input → same ID across calls in src/chunk/id.rs (test module)
- [ ] T024 [P] [US3] Write unit tests for chunk_id uniqueness: different content → different ID, different path → different ID in src/chunk/id.rs (test module)
- [ ] T025 [P] [US3] Write unit tests for chunk_id format: output matches `^[0-9a-f]{64}$` in src/chunk/id.rs (test module)
- [ ] T026 [US3] Write unit tests for edge cases: unicode content, very large content, empty input handling in src/chunk/id.rs (test module)

### Implementation for User Story 3

- [ ] T027 [US3] Implement generate_chunk_id(content, source_path) → Result<String, GraphtorError> with 64-char hex SHA-256 hash in src/chunk/id.rs per contracts/library-api-contract.md
- [ ] T028 [US3] Implement input validation: return GraphtorError::Parse for empty content or path in src/chunk/id.rs
- [ ] T029 [US3] Export chunk types from src/chunk/mod.rs and re-export from src/lib.rs

**Checkpoint**: Chunk ID generation functional — deterministic cross-database correlation key available

---

## Phase 5: User Story 4 — Structured Logging (Priority: P2)

**Goal**: Initialize tracing-based structured logging with configurable verbosity

**Independent Test**: Initialize logging at each verbosity level and verify correct filtering

### Tests for User Story 4

- [ ] T030 [P] [US4] Write unit tests for LogVerbosity enum → tracing level mapping in src/logging/init.rs (test module)
- [ ] T031 [US4] Write integration test for logging initialization and verbosity filtering in tests/logging_test.rs

### Implementation for User Story 4

- [ ] T032 [US4] Define LogVerbosity enum (Quiet, Normal, Verbose) in src/logging/init.rs
- [ ] T033 [US4] Implement init_logging(verbosity) to configure tracing-subscriber with level filter and stderr output in src/logging/init.rs
- [ ] T034 [US4] Handle double-initialization gracefully (return error, do not panic) in src/logging/init.rs
- [ ] T035 [US4] Export logging types from src/logging/mod.rs and re-export from src/lib.rs

**Checkpoint**: Logging infrastructure functional — all pipeline stages can emit structured diagnostics

---

## Phase 6: User Story 5 — Path Security (Priority: P2)

**Goal**: Validate file paths against allowed root to prevent directory traversal

**Independent Test**: Construct paths with various traversal techniques and verify boundary enforcement

### Tests for User Story 5

- [ ] T036 [P] [US5] Write unit tests for valid paths: relative within root, absolute within root in src/path/security.rs (test module)
- [ ] T037 [P] [US5] Write unit tests for rejected paths: .. traversal, absolute outside root in src/path/security.rs (test module)
- [ ] T038 [P] [US5] Write unit tests for edge cases: redundant separators, Windows-style paths in src/path/security.rs (test module)
- [ ] T039 [US5] Write integration test for path validation with real filesystem (temp dirs) in tests/path_security_test.rs

### Implementation for User Story 5

- [ ] T040 [US5] Implement validate_path(path, allowed_root) → Result<PathBuf, GraphtorError> in src/path/security.rs per contracts/library-api-contract.md
- [ ] T041 [US5] Implement canonicalization and starts_with boundary check in src/path/security.rs
- [ ] T042 [US5] Handle non-existent path case: canonicalize parent, validate constructed child in src/path/security.rs
- [ ] T043 [US5] Export path types from src/path/mod.rs and re-export from src/lib.rs

**Checkpoint**: Path security functional — all file operations can be validated against workspace boundaries

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Quality improvements across all modules

- [ ] T044 [P] Add rustdoc documentation comments to all public types and functions across src/
- [ ] T045 [P] Run clippy lints and fix all warnings: `cargo clippy -- -W clippy::all`
- [ ] T046 [P] Run rustfmt and ensure formatting compliance: `cargo fmt --check`
- [ ] T047 Run full test suite and verify all SCENARIOS.md P1 scenarios pass: `cargo test`
- [ ] T048 [P] Update quickstart.md with verified, runnable code examples
- [ ] T049 Validate against contracts/library-api-contract.md: all public APIs match contract

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **US1 Config (Phase 3)**: Depends on Phase 2 (uses GraphtorError for error reporting)
- **US3 Chunk ID (Phase 4)**: Depends on Phase 2 (uses GraphtorError for validation errors)
- **US4 Logging (Phase 5)**: Depends on Phase 2 (uses GraphtorError for init errors)
- **US5 Path Security (Phase 6)**: Depends on Phase 2 (uses GraphtorError::PathViolation)
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **US1 (Config)**: Depends on Foundational only — independent of US3, US4, US5
- **US3 (Chunk ID)**: Depends on Foundational only — independent of US1, US4, US5
- **US4 (Logging)**: Depends on Foundational only — independent of US1, US3, US5
- **US5 (Path Security)**: Depends on Foundational only — independent of US1, US3, US4

### Within Each User Story

- Tests written first (TDD) → must FAIL before implementation
- Struct definitions before logic
- Validation before export
- Module exports last

### Parallel Opportunities

**Phase 1** (all setup tasks are independent):
- T002, T003, T004, T005 can run in parallel

**Phase 2** (tests are independent):
- T006, T007 can run in parallel (different test targets)

**After Phase 2** (all user stories are independent):
- US1, US3, US4, US5 can all develop simultaneously
- Within US1: T013, T014, T015 in parallel; then T017, T018 in parallel
- Within US3: T023, T024, T025 in parallel
- Within US5: T036, T037, T038 in parallel

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (error types)
3. Complete Phase 3: US1 (config parsing)
4. **STOP and VALIDATE**: Parse a real sources.yaml, verify error messages
5. This delivers the configuration layer — the entry point to the system

### Incremental Delivery

1. Setup + Foundational → Error types available
2. Add US1 (Config) → Test independently → sources.yaml parseable
3. Add US3 (Chunk ID) → Test independently → Deterministic IDs
4. Add US4 (Logging) → Test independently → Structured diagnostics
5. Add US5 (Path Security) → Test independently → File access validation
6. Polish → Production-quality library crate

---

## Notes

- Total tasks: 49 (T001–T049)
- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- SCENARIOS.md (39 scenarios) maps to test tasks for validation
- Commit after each task or logical group
- All test tasks must be observed to FAIL before corresponding implementation
