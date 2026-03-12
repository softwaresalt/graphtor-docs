# ADR-0001: Single Cargo Package with Combined Library and Binary Targets

**Status**: Accepted  
**Date**: 2026-03-10  
**Phase**: Phase 1 — Setup (spec 002-rust-foundation, T001)

## Context

The Rust foundation needs to expose a library crate (`graphtor-core`) for downstream feature groups to depend on, and also ship a binary entry point (`graphtor-docs`) for the eventual CLI. Two structural options exist in Cargo:

1. **Separate workspace members** — two directories (`graphtor-core/` and `graphtor-docs/`), each with its own `Cargo.toml`. Clean boundary but more directory nesting and more `Cargo.toml` files to maintain.
2. **Single package with both lib and bin targets** — one `Cargo.toml` at the workspace root declaring `[lib]` + `[[bin]]`. Both targets share the same source tree.

## Decision

Use a single Cargo package at the repository root with both a `[lib]` target (`graphtor_core`) and a `[[bin]]` target (`graphtor-docs`) pointing to `src/main.rs`.

## Rationale

- The plan.md explicitly shows `src/lib.rs` and `src/main.rs` in the same directory — a single-package layout.
- The binary in Phase 1 is a placeholder only; the CLI feature group (FG-010) will populate it. Keeping it co-located avoids introducing a separate workspace member crate for placeholder code.
- Future feature groups that produce their own binaries or libraries can be added as workspace members later without restructuring existing code.

## Consequences

- **Positive**: Simpler directory structure for the foundational phase. One `Cargo.toml` to maintain.
- **Positive**: `cargo build` at the workspace root builds everything.
- **Negative**: If the library and binary grow large, separate crates become beneficial for compile-time isolation.
- **Risk**: If FG-010 requires a very different dependency set, the shared `Cargo.toml` will include those deps in the library too. Mitigated by using `[dev-dependencies]` for test-only deps.
