# Implementation Plan: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-rust-foundation/spec.md`

## Summary

Build the foundational Rust library crate that all other feature groups depend on. This includes: Cargo workspace scaffolding with a library crate (`graphtor-core`) and binary target (`graphtor-docs`), a categorized error type hierarchy using `thiserror`, configuration types for parsing `sources.yaml` via `serde_yaml`, structured logging via `tracing`, path security validation utilities, and deterministic SHA-256-based chunk ID generation via the `sha2` crate.

## Technical Context

**Language/Version**: Rust (stable, 1.75+)
**Primary Dependencies**: thiserror, serde, serde_yaml, sha2, tracing, tracing-subscriber, globset
> **Note**: `serde_json` is intentionally deferred to FG-008 (Incremental Sync) where `.sync_state.json` is first required.
**Storage**: N/A (foundation layer — no database access)
**Testing**: cargo test (built-in Rust test framework)
**Target Platform**: Windows 10/11, Linux, macOS (cross-platform single binary)
**Project Type**: Library crate (graphtor-core) + binary target (graphtor-docs)
**Performance Goals**: Configuration parsing < 10ms, chunk ID generation < 1μs per chunk
**Constraints**: Zero runtime dependencies beyond the compiled binary. No network access.
**Scale/Scope**: Foundation for 10 feature groups. Expected to define ~15 public types used by all downstream crates.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First Architecture | ✅ PASS | Pure Rust library, no network calls, no external services. |
| II. Lightweight Footprint | ✅ PASS | Minimal crate dependencies (thiserror, serde, sha2, tracing). Each justified in tech stack table. |
| III. Data Pipeline Integrity | ✅ PASS | Deterministic SHA-256 chunk_id generation. Configuration validation at parse time. |
| IV. MCP-Native Interface | ⬜ N/A | MCP plugin is not part of this feature group. |
| V. Automation & Reproducibility | ✅ PASS | sources.yaml defines reproducible configuration. All operations deterministic. |

**Gate result**: PASS — no violations. Proceeding to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/002-rust-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
Cargo.toml                    # Workspace root — defines members
src/
├── lib.rs                    # Library crate root — re-exports public API
├── config/
│   ├── mod.rs                # Configuration module root
│   ├── source.rs             # SourceConfig, GitSource, LocalSource types
│   └── validation.rs         # Config validation (duplicates, glob syntax)
├── error/
│   ├── mod.rs                # Error module root
│   └── types.rs              # GraphtorError enum with thiserror derives
├── chunk/
│   ├── mod.rs                # Chunk module root
│   └── id.rs                 # Deterministic chunk_id generation (SHA-256)
├── logging/
│   ├── mod.rs                # Logging module root
│   └── init.rs               # tracing-subscriber initialization
├── path/
│   ├── mod.rs                # Path module root
│   └── security.rs           # Path validation against allowed roots
└── main.rs                   # Binary entry point (placeholder for FG-010)

tests/
├── config_test.rs            # Configuration parsing and validation tests
├── error_test.rs             # Error type construction and display tests
├── chunk_id_test.rs          # Chunk ID determinism and uniqueness tests
├── path_security_test.rs     # Path validation boundary tests
└── logging_test.rs           # Log initialization and level filtering tests
```

**Structure Decision**: Single Cargo workspace with library crate. The `src/` directory uses module-per-concern organization. The binary target (`main.rs`) is a thin wrapper that will be populated by FG-010 (CLI). Tests live in a top-level `tests/` directory for integration tests, with unit tests inline in each module.

## Complexity Tracking

> No constitution violations — this section is intentionally empty.
