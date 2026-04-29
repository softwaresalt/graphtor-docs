# Implementation Plan: Source Registry & Acquisition

**Branch**: `003-source-management` | **Date**: 2026-03-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-source-management/spec.md`

## Summary

Implement the source acquisition layer that takes a parsed `SourceConfig` (from FG-001) and acquires documentation by cloning Git repositories (shallow, depth=1) via the `git2` crate and scanning local directories recursively. Acquired files are filtered through include/exclude glob patterns using the existing `globset` dependency. All operations are fault-isolated (one source failure doesn't block others), idempotent (skip-if-exists for Git clones), and observable (structured tracing at INFO/DEBUG levels).

## Technical Context

**Language/Version**: Rust stable (edition 2021, MSRV 1.75)
**Primary Dependencies**: `git2` (Git cloning), `globset` (already present — glob filtering), `walkdir` (recursive directory traversal)
**Storage**: Filesystem only — cloned repos stored under configurable data root directory
**Testing**: `cargo test` with `tempfile` (already in dev-dependencies) for filesystem isolation
**Target Platform**: Windows, macOS, Linux (developer workstations)
**Project Type**: Library crate (`graphtor-core`) + binary target (`graphtor-docs`)
**Performance Goals**: File enumeration and filtering of 1,000+ files in <5s (excluding network)
**Constraints**: Single-binary distribution, no runtime dependencies, local-first (no cloud)
**Scale/Scope**: Up to ~700 Git repositories, tens of thousands of files per repository

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First | ✅ Pass | All operations local. Git clones via `git2` (in-process, no shell-out). No cloud. |
| II. Lightweight Footprint | ✅ Pass | `git2` is essential for core functionality (Constitution tech stack). Shallow clones minimize disk. `walkdir` is tiny (~500 LOC). |
| III. Data Pipeline Integrity | ✅ Pass | Idempotent acquisition (skip-if-exists). Deterministic file enumeration (sorted). |
| IV. MCP-Native Interface | N/A | Acquisition is an internal pipeline stage, not an MCP tool. |
| V. Automation & Reproducibility | ✅ Pass | Single-command acquisition from `sources.yaml`. Skip-if-exists. Dry-run support. |

## Project Structure

### Documentation (this feature)

```text
specs/003-source-management/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── acquire-api.md   # Public API contract
└── tasks.md             # Phase 2 output (from /speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── acquire/             # NEW — source acquisition module
│   ├── mod.rs           # Module root, public re-exports
│   ├── git.rs           # Git cloning via git2 (shallow, single-branch)
│   ├── local.rs         # Local directory recursive scanning
│   ├── filter.rs        # Glob pattern filtering (include/exclude)
│   ├── plan.rs          # AcquisitionPlan resolution from SourceConfig
│   └── result.rs        # AcquisitionResult, AcquiredSource, FilteredFileSet types
├── config/              # EXISTING — SourceConfig, GitSource, LocalSource
├── error/               # EXISTING — GraphtorError (may add Acquire variant)
├── chunk/               # EXISTING — chunk ID generation
├── logging/             # EXISTING — tracing initialization
├── path/                # EXISTING — path security validation
├── lib.rs               # MODIFY — add `pub mod acquire;` and re-exports
└── main.rs              # EXISTING — unchanged

tests/
├── acquire_git_test.rs  # NEW — Git acquisition integration tests
├── acquire_local_test.rs # NEW — local scanning tests
├── acquire_filter_test.rs # NEW — glob filtering tests
├── acquire_plan_test.rs # NEW — acquisition plan tests
├── config_test.rs       # EXISTING
├── error_test.rs        # EXISTING
├── logging_test.rs      # EXISTING
└── path_security_test.rs # EXISTING
```

**Structure Decision**: The `acquire` module follows the established pattern (one module per concern) and lives inside the existing `graphtor-core` library crate. Types are defined in `result.rs` to avoid circular dependencies.

## Complexity Tracking

No constitution violations — no entries needed.
