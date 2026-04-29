# Specification Analysis Report: Source Registry & Acquisition

**Branch**: `003-source-management` | **Date**: 2026-03-14
**Artifacts analyzed**: spec.md, plan.md, tasks.md, data-model.md, research.md, contracts/acquire-api.md, SCENARIOS.md

## Findings

| ID | Category | Severity | Location(s) | Summary | Recommendation |
|----|----------|----------|-------------|---------|----------------|
| C1 | Underspecification | MEDIUM | spec.md FR-003 | "skip cloning if a local directory already exists for that source ID" is ambiguous — does it check for directory name only, or require `.git` subdir? SCENARIOS.md S016 addresses this edge case but the FR itself is imprecise. | Amend FR-003 to: "skip cloning if a local directory with a `.git` subdirectory already exists for that source ID" |
| C2 | Coverage Gap | MEDIUM | spec.md edge cases | Edge case "disk runs out of space during Git clone" has no corresponding FR or task. Clone operation may leave a partial directory. | Add handling note: partial clone directories should be cleaned up on failure to avoid false skip-if-exists on retry |
| C3 | Underspecification | LOW | spec.md FR-011 | "basic format correctness" for URL validation is vague. Research.md clarifies (scheme+host for HTTPS, git@host:path for SSH), but the FR should be self-contained. | Amend FR-011 to explicitly state: "HTTPS URLs must contain scheme and host; SSH URLs must match `git@<host>:<path>` format" |
| C4 | Coverage Gap | LOW | spec.md edge cases | Edge case "permissions change between validation and scanning" has no specific task. | Covered by general error handling in `scan_local_source()` (walkdir reports permission errors). No task change needed. |
| C5 | Inconsistency | LOW | tasks.md T016 | T016 adds `From<git2::Error>` to `src/error/types.rs`, but the research.md (R4) decided to use `Pipeline` variant with manual mapping, not a From impl. A From impl would lose the source_id context. | Change T016 to: "Add helper function `git_error_to_pipeline(e: git2::Error, source_id: &str) -> GraphtorError` in `src/acquire/git.rs`" |
| C6 | Coverage Gap | LOW | spec.md edge cases | Edge case "two different sources have the same source ID" — this is already handled by FG-001 validation (duplicate ID rejection in config parsing). | No change needed. Document in spec that this is handled upstream by FG-001. |

## Coverage Summary Table

| Requirement | Has Task? | Task IDs | Notes |
|-------------|-----------|----------|-------|
| FR-001 (shallow clone) | ✅ | T015 | |
| FR-002 (organize by source ID) | ✅ | T015 | |
| FR-003 (skip existing) | ✅ | T012, T015 | See finding C1 |
| FR-004 (single branch) | ✅ | T015 | |
| FR-005 (recursive scan) | ✅ | T023 | |
| FR-006 (include patterns) | ✅ | T009, T027 | |
| FR-007 (exclude patterns) | ✅ | T009, T027 | |
| FR-008 (include before exclude) | ✅ | T008, T009 | |
| FR-009 (no include = all) | ✅ | T009 | |
| FR-010 (no exclude = none) | ✅ | T009 | |
| FR-011 (URL validation) | ✅ | T040, T041 | See finding C3 |
| FR-012 (path existence) | ✅ | T040 | |
| FR-013 (glob syntax) | ✅ | T040 | |
| FR-014 (collect all errors) | ✅ | T039, T040 | |
| FR-015 (fault isolation) | ✅ | T013, T033 | |
| FR-016 (summary) | ✅ | T034 | |
| FR-017 (path security) | ✅ | T032, T040 | |
| FR-018 (logging) | ✅ | T017, T024, T034 | |
| FR-019 (dry-run) | ✅ | T044 | |
| FR-020 (non-existent branch) | ✅ | T013, T015 | |
| FR-021 (auto-create data root) | ✅ | T032 | |

## Constitution Alignment Issues

No CRITICAL constitution violations found.

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First | ✅ Aligned | All operations local. git2 in-process. No cloud dependencies. |
| II. Lightweight Footprint | ✅ Aligned | git2 is in Constitution tech stack. walkdir is minimal. |
| III. Data Pipeline Integrity | ✅ Aligned | Idempotent acquisition. Deterministic file ordering. Shallow clones. |
| IV. MCP-Native Interface | N/A | Acquisition is internal pipeline, not MCP tool. |
| V. Automation & Reproducibility | ✅ Aligned | Single-command acquisition. Skip-if-exists. Dry-run support. |
| TDD Workflow | ✅ Aligned | Tasks follow test-first pattern per constitution. |

## Unmapped Tasks

None. All tasks map to at least one FR, user story, or infrastructure need.

## Metrics

- **Total Requirements**: 21 (FR-001 through FR-021)
- **Total Tasks**: 50
- **Coverage**: 100% (21/21 requirements have ≥1 task)
- **Ambiguity Count**: 2 (C1, C3)
- **Duplication Count**: 0
- **Critical Issues**: 0
- **High Issues**: 0
- **Medium Issues**: 2
- **Low Issues**: 4

## Next Actions

1. **Proceed to implementation** — No CRITICAL or HIGH issues. The 2 MEDIUM findings (C1, C2) are clarification-level improvements that can be addressed before or during Phase 3 implementation.
2. **Recommended pre-implementation fixes**:
   - Amend FR-003 to specify `.git` subdirectory check (C1)
   - Add cleanup-on-failure note for partial Git clones (C2)
   - Adjust T016 from `From<git2::Error>` impl to helper function (C5)
3. **No architecture or scope changes needed**.
