# Adversarial Analysis: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10
**Artifacts Reviewed**: spec.md, plan.md, tasks.md, SCENARIOS.md, data-model.md, contracts/library-api-contract.md

## Analysis Method

Single-reviewer cross-artifact consistency check covering:
- Requirement traceability (FR → Scenarios → Tasks)
- Data model consistency
- Contract alignment
- Edge case coverage
- Scope boundary validation

## Findings

### RC-01: Missing FR coverage for LocalSource `exclude` patterns [MEDIUM]

**Location**: spec.md FR-001, data-model.md LocalSource
**Issue**: `GitSource` supports both `include` and `exclude` patterns, but `LocalSource` only defines `include` in the data model and contracts. The spec FR-001 mentions include/exclude for Git sources but only "include patterns" for local sources. However, users may need to exclude patterns from local directories too.
**Recommendation**: Either explicitly add `exclude` to `LocalSource` in data-model.md and contracts, or add a clarifying note in spec.md that local sources intentionally omit exclude patterns (and explain why).

### RC-02: Scenario gap — no test for `exclude` pattern behavior [LOW]

**Location**: SCENARIOS.md
**Issue**: No scenario tests the behavior of `exclude` patterns specifically — e.g., what happens when a file matches both an `include` and an `exclude` pattern (exclude should win).
**Recommendation**: Add scenario S040 testing include/exclude precedence: when a file matches both include and exclude, exclude takes priority.

### RC-03: tasks.md missing US2 phase label [MEDIUM]

**Location**: tasks.md Phase 2
**Issue**: Phase 2 implements US2 (Error Diagnostics) but tasks T006-T012 do not carry the `[US2]` story label. The template requires story labels for user story phases. However, since error types are a foundational prerequisite (not strictly a user story phase), this is arguably correct — but the inconsistency with the spec's US2 designation should be noted.
**Recommendation**: Either add `[US2]` labels to T006-T012, or add a note in tasks.md explaining that US2 was promoted to the Foundational phase because all other stories depend on it.

### RC-04: Contract specifies `panic` behavior for chunk_id [HIGH]

**Location**: contracts/library-api-contract.md, chunk module
**Issue**: The contract states "Panics if content or source_path is empty (caught by validation before reaching this point)." Panicking in a library function is a Rust anti-pattern — callers cannot recover from panics gracefully. This contradicts the spec's emphasis on actionable error messages (FR-004).
**Recommendation**: Change `generate_chunk_id` to return `Result<String, GraphtorError>` instead of panicking. Return `GraphtorError::Parse` or a new validation variant for empty inputs.

### RC-05: Missing scenario for symlink-based path traversal [MEDIUM]

**Location**: SCENARIOS.md Path Security section
**Issue**: The spec mentions symlink resolution (FR-008), and the research document discusses `canonicalize()` resolving symlinks. However, no scenario explicitly tests symlink-based directory escape (e.g., a symlink inside the root that points outside it).
**Recommendation**: Add scenario S041: Given a symlink inside the allowed root that points to a directory outside the root, When path validation runs on a file accessed through the symlink, Then it is rejected as a PathViolation.

### RC-06: data-model.md GitSource missing `type` discriminator [LOW]

**Location**: data-model.md Source entity
**Issue**: The YAML sources.yaml format shown in research.md uses a `type: git` / `type: local` discriminator field. The data-model.md defines Source as an enum but doesn't explicitly call out how serde deserializes the type discriminator from YAML.
**Recommendation**: Add a note in data-model.md that the Source enum uses serde's tagged union deserialization (e.g., `#[serde(tag = "type")]`), or that the type field is the YAML discriminator.

### TF-01: tasks.md T028 duplicates T009 scope [LOW]

**Location**: tasks.md T028 and T009
**Issue**: T028 says "Implement input validation: reject empty content or path with descriptive error in src/chunk/id.rs" while T009 already establishes the error type system. The chunk_id validation in T028 is appropriate, but if RC-04 is addressed (returning Result instead of panicking), T028 should explicitly reference returning `GraphtorError` instead of "descriptive error."
**Recommendation**: Update T028 description to specify returning `Result<String, GraphtorError>` for empty inputs.

## Severity Summary

| Severity | Count | Finding IDs |
|----------|-------|-------------|
| CRITICAL | 0 | — |
| HIGH | 1 | RC-04 |
| MEDIUM | 3 | RC-01, RC-03, RC-05 |
| LOW | 3 | RC-02, RC-06, TF-01 |
| **Total** | **7** | |

## Recommended Actions

1. **RC-04 (HIGH)**: Fix contract to return `Result` instead of panicking — apply to contracts/library-api-contract.md and tasks.md T027/T028
2. **RC-01 (MEDIUM)**: Add `exclude` to LocalSource or document intentional omission
3. **RC-03 (MEDIUM)**: Add note to tasks.md explaining US2 → Foundational promotion
4. **RC-05 (MEDIUM)**: Add symlink traversal scenario to SCENARIOS.md
5. **RC-02, RC-06, TF-01 (LOW)**: Record as suggestions for spec refinement
