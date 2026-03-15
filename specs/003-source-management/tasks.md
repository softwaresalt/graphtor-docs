# Tasks: Source Registry & Acquisition

**Input**: Design documents from `/specs/003-source-management/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/acquire-api.md

**Tests**: TDD approach — tests are written first for each phase per Constitution §Development Workflow.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add new dependencies and create the acquire module skeleton

- [x] T001 Add `git2 = "0.19"` and `walkdir = "2"` to `[dependencies]` in Cargo.toml
- [x] T002 Create module skeleton `src/acquire/mod.rs` with submodule declarations (git, local, filter, plan, result) and public re-exports
- [x] T003 [P] Create `src/acquire/result.rs` with all result types: SourceAction, SourceType, PlannedSource, AcquisitionPlan, AcquiredSource, FilteredFileSet, SourceOutcome, AcquisitionResult, ValidationError, ValidationReport
- [x] T004 [P] Register `pub mod acquire;` in `src/lib.rs` and add re-exports for public types
- [x] T005 Verify `cargo check` passes with the new module skeleton (all submodules can be empty stubs with `todo!()` or empty structs)

**Checkpoint**: Module skeleton compiles. No functionality yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement shared types and utilities that all user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T006 Write unit tests for all result types in `src/acquire/result.rs` — verify struct construction, enum variant matching, and Display/Debug formatting
- [x] T007 Implement all result types in `src/acquire/result.rs` per data-model.md (SourceAction, SourceType, PlannedSource, AcquisitionPlan, AcquiredSource, FilteredFileSet, SourceOutcome, AcquisitionResult, ValidationError, ValidationReport)
- [x] T008 Write unit tests for `filter_files()` in `src/acquire/filter.rs` covering scenarios S026–S034 (include/exclude patterns, defaults, precedence, edge cases)
- [x] T009 Implement `filter_files()` in `src/acquire/filter.rs` — compile include/exclude GlobSets, apply include-then-exclude logic, handle empty pattern defaults (FR-006 through FR-010)
- [x] T010 Verify `cargo test acquire` passes for all foundational tests

**Checkpoint**: Foundation ready — result types and glob filtering functional. User story implementation can begin.

---

## Phase 3: User Story 1 — Acquire Git Documentation Repositories (Priority: P1) 🎯 MVP

**Goal**: Clone Git repositories using shallow fetch (depth=1), organize by source ID, skip existing clones.

**Independent Test**: Provide sources.yaml with Git entries, verify repos cloned to correct dirs with shallow depth; re-run skips existing clones; one failure doesn't block others.

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T011 [P] [US1] Write test for `clone_git_source()` happy path in `tests/acquire_git_test.rs` — create a local bare repo with `git2::Repository::init_bare()`, configure as Git source, verify clone to target dir (scenario S008)
- [x] T012 [P] [US1] Write test for skip-if-exists in `tests/acquire_git_test.rs` — pre-create target dir with `.git`, verify clone is skipped (scenario S010)
- [x] T013 [P] [US1] Write test for non-existent branch error in `tests/acquire_git_test.rs` — clone with invalid branch, verify Pipeline error returned (scenario S012)
- [x] T014 [P] [US1] Write test for unreachable URL error in `tests/acquire_git_test.rs` — use invalid URL, verify Pipeline error returned with source ID (scenario S011)

### Implementation for User Story 1

- [x] T015 [US1] Implement `clone_git_source()` in `src/acquire/git.rs` — use `git2::build::RepoBuilder` with `FetchOptions` for depth=1 and single-branch fetch; handle skip-if-exists; convert git2 errors to GraphtorError::Pipeline (FR-001, FR-002, FR-003, FR-004)
- [x] T016 [US1] Add helper function `git_error_to_pipeline(e: git2::Error, source_id: &str) -> GraphtorError` in `src/acquire/git.rs` — map git2 errors to Pipeline variant with stage="acquire" and source ID context
- [x] T017 [US1] Add tracing instrumentation to `clone_git_source()` — INFO on start/complete, WARN on skip, ERROR on failure (FR-018)
- [x] T018 [US1] Run `cargo test acquire_git` and verify all US1 tests pass

**Checkpoint**: Git clone acquisition works independently. Can clone repos, skip existing, handle errors.

---

## Phase 4: User Story 2 — Index Local Documentation Directories (Priority: P1)

**Goal**: Recursively scan local directories, discover all files, sort deterministically.

**Independent Test**: Create temp directory with files at various depths, verify all discovered; re-scan produces same results.

### Tests for User Story 2

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T019 [P] [US2] Write test for `scan_local_source()` happy path in `tests/acquire_local_test.rs` — create temp dir with nested .md files, verify all discovered (scenario S017)
- [x] T020 [P] [US2] Write test for deterministic sort order in `tests/acquire_local_test.rs` — scan twice, verify identical ordering (scenario S019)
- [x] T021 [P] [US2] Write test for non-existent directory error in `tests/acquire_local_test.rs` — scan missing dir, verify Pipeline error (scenario S020)
- [x] T022 [P] [US2] Write test for path security violation in `tests/acquire_local_test.rs` — source path outside allowed root, verify PathViolation error (scenario S021)

### Implementation for User Story 2

- [x] T023 [US2] Implement `scan_local_source()` in `src/acquire/local.rs` — use `walkdir::WalkDir` with `follow_links(false)`, collect regular files, sort paths, validate against allowed root (FR-005, FR-017)
- [x] T024 [US2] Add tracing instrumentation to `scan_local_source()` — INFO on scan start/complete with file count, DEBUG per-file (FR-018)
- [x] T025 [US2] Run `cargo test acquire_local` and verify all US2 tests pass

**Checkpoint**: Local scanning works independently. Can discover files recursively, handle errors, enforce path security.

---

## Phase 5: User Story 3 — Filter Files Using Include/Exclude Patterns (Priority: P2)

**Goal**: Apply include/exclude glob patterns to acquired file lists per FR-006 through FR-010.

**Independent Test**: Provide known file paths and pattern combos, verify filtered output matches expectations.

> Tests for this story are already in Phase 2 (T008/T009 — foundational filter tests). This phase adds integration with acquisition results.

### Implementation for User Story 3

- [x] T026 [US3] Write integration test in `tests/acquire_filter_test.rs` — end-to-end: scan a local dir, then filter with include/exclude patterns, verify final file set (scenarios S026–S034)
- [x] T027 [US3] Integrate `filter_files()` into the acquisition pipeline — after `clone_git_source()` and `scan_local_source()`, apply filtering to produce FilteredFileSet (FR-006, FR-007, FR-008)
- [x] T028 [US3] Add WARN log when filtering results in empty file set (scenario S032)
- [x] T029 [US3] Run `cargo test acquire_filter` and verify all US3 tests pass

**Checkpoint**: Full acquire → scan → filter pipeline works for both Git and local sources.

---

## Phase 6: User Story 4 — Re-run Acquisition Safely (Priority: P2)

**Goal**: Idempotent acquisition — skip existing Git clones, re-scan local dirs, handle added sources.

**Independent Test**: Run acquisition twice on same config, verify second run skips clones and produces identical file list.

### Tests for User Story 4

- [x] T030 [P] [US4] Write integration test for idempotent Git re-run in `tests/acquire_plan_test.rs` — acquire twice, verify second run all SkipGit (scenario S048)
- [x] T031 [P] [US4] Write integration test for local re-scan in `tests/acquire_plan_test.rs` — acquire twice, verify local sources re-scanned (scenario S049)

### Implementation for User Story 4

- [x] T032 [US4] Implement `plan()` in `src/acquire/plan.rs` — resolve each source to PlannedSource with action (CloneGit/SkipGit/ScanLocal), auto-create data root via `create_dir_all`, validate path security (FR-003, FR-017, FR-021)
- [x] T033 [US4] Implement `execute()` in `src/acquire/mod.rs` — iterate PlannedSource list, dispatch to clone/scan/skip, apply filtering, collect SourceOutcome results, produce AcquisitionResult with aggregate counts (FR-015, FR-016)
- [x] T034 [US4] Add summary logging at end of `execute()` — INFO with total sources, succeeded, skipped, failed, total files (FR-016, FR-018)
- [x] T035 [US4] Run `cargo test acquire_plan` and verify all US4 tests pass

**Checkpoint**: Full acquisition pipeline is idempotent and produces summary reports.

---

## Phase 7: User Story 5 — Validate Source Registry Before Processing (Priority: P2)

**Goal**: Upfront validation of all sources, collecting ALL errors in a single pass.

**Independent Test**: Provide sources.yaml with various invalid entries, verify all errors reported together.

### Tests for User Story 5

- [x] T036 [P] [US5] Write test for `validate_sources()` with all-valid config in `tests/acquire_plan_test.rs` (scenario S035)
- [x] T037 [P] [US5] Write test for invalid URL detection in `tests/acquire_plan_test.rs` (scenario S036)
- [x] T038 [P] [US5] Write test for non-existent local path detection in `tests/acquire_plan_test.rs` (scenario S038)
- [x] T039 [P] [US5] Write test for multiple errors collected in single pass in `tests/acquire_plan_test.rs` (scenario S040)

### Implementation for User Story 5

- [x] T040 [US5] Implement `validate_sources()` in `src/acquire/plan.rs` — check URL format (HTTPS scheme or SSH format), local path existence, glob syntax, path security; collect all errors into ValidationReport (FR-011 through FR-014, FR-017)
- [x] T041 [US5] Add HTTPS URL validation helper (check scheme and host presence) and SSH URL validation helper (check `git@host:path` format) in `src/acquire/plan.rs`
- [x] T042 [US5] Run `cargo test` for all validation tests and verify they pass

**Checkpoint**: Source validation works end-to-end. All errors collected in single pass.

---

## Phase 8: Dry-Run Mode (Priority: P2)

**Goal**: Report planned actions without performing filesystem or network operations (FR-019).

- [x] T043 [P] Write test for dry-run mode in `tests/acquire_plan_test.rs` — verify no filesystem changes when dry_run=true (scenario S046)
- [x] T044 Implement dry-run support in `execute()` — add `dry_run: bool` parameter; when true, skip clone/scan but still plan and validate; produce AcquisitionResult with planned actions only (FR-019)
- [x] T045 Run `cargo test` for dry-run tests and verify they pass

**Checkpoint**: Dry-run mode functional. Validates and reports without side effects.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final integration, documentation, and quality pass

- [ ] T046 [P] Run full `cargo test` suite — all existing tests (config, error, path, logging, chunk) plus all new acquire tests must pass
- [ ] T047 [P] Run `cargo clippy -- -D warnings` and fix any lints in new code
- [ ] T048 Verify all public functions in `src/acquire/` have doc comments with `# Errors` sections per existing code style
- [ ] T049 Run quickstart.md validation — `cargo check`, `cargo test acquire`
- [ ] T050 Update `src/lib.rs` module-level doc comment to include `acquire` module description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **US1 Git Acquisition (Phase 3)**: Depends on Phase 2
- **US2 Local Scanning (Phase 4)**: Depends on Phase 2 — can run in parallel with Phase 3
- **US3 Glob Filtering (Phase 5)**: Depends on Phases 3 + 4 (needs acquired files to filter)
- **US4 Idempotent Acquisition (Phase 6)**: Depends on Phases 3 + 4 + 5 (orchestrates full pipeline)
- **US5 Validation (Phase 7)**: Depends on Phase 2 only — can start after foundational
- **Dry-Run (Phase 8)**: Depends on Phase 6 (extends execute with dry-run flag)
- **Polish (Phase 9)**: Depends on all previous phases

### User Story Dependencies

- **US1 (P1)**: After Phase 2 — no story dependencies
- **US2 (P1)**: After Phase 2 — no story dependencies, can run in parallel with US1
- **US3 (P2)**: After US1 + US2 (needs both Git and local file lists)
- **US4 (P2)**: After US1 + US2 + US3 (orchestrates full pipeline)
- **US5 (P2)**: After Phase 2 — independent of other stories (validation only)

### Parallel Opportunities

- T003/T004 (Phase 1): Different files, can run in parallel
- T006/T008 (Phase 2): Tests for different modules, can run in parallel
- T011/T012/T013/T014 (Phase 3 tests): Independent test files
- T019/T020/T021/T022 (Phase 4 tests): Independent test files
- T030/T031 (Phase 6 tests): Independent tests
- T036/T037/T038/T039 (Phase 7 tests): Independent tests
- US1 (Phase 3) and US2 (Phase 4): Can run in parallel after Phase 2
- US5 (Phase 7): Can run in parallel with US3/US4

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T005)
2. Complete Phase 2: Foundational (T006–T010)
3. Complete Phase 3: US1 Git Acquisition (T011–T018)
4. **STOP and VALIDATE**: `cargo test acquire_git` passes, Git cloning works
5. This alone delivers the core value: cloning documentation repositories

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 (Git) + US2 (Local) in parallel → Both source types work
3. Add US3 (Filtering) → Files are properly filtered
4. Add US4 (Orchestration) → Full acquisition pipeline with idempotency
5. Add US5 (Validation) → Upfront error reporting
6. Add Dry-Run → Safe preview mode
7. Polish → Production-quality code

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- All tests use `tempfile::tempdir()` for filesystem isolation — no network access in tests
- Git tests create local bare repos with `git2::Repository::init_bare()` to avoid network
- Total tasks: 50
- P1 stories: 2 (US1 Git, US2 Local) — 18 tasks
- P2 stories: 3 (US3 Filter, US4 Idempotent, US5 Validate) + Dry-Run — 22 tasks
- Setup + Foundational: 10 tasks
