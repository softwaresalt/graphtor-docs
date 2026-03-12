<!-- markdownlint-disable-file -->
# PR Review Status: 002-rust-foundation

## Review Status

* Phase: 4 — Complete ✅
* Last Updated: 2026-03-12T06:10:00Z
* Summary: FG-001 Rust Foundation — 7 review items identified, 6 applied (RI-001–RI-005, RI-007), 1 closed as no-action (RI-006). All quality gates pass. Committed as `9df3fcd`.

## Branch and Metadata

* Normalized Branch: `002-rust-foundation`
* Source Branch: `002-rust-foundation`
* Base Branch: `main`
* Linked Work Items: FG-001 (backlog), spec `specs/002-rust-foundation/`
* `pr-ref-gen.sh` not present — reference built from `git diff --stat` and direct file analysis.

## Diff Mapping (Source & Test Files)

| File | Type | New Lines | Focus Areas |
|------|------|-----------|-------------|
| `Cargo.toml` | added | 1–30 | Metadata completeness, dep versions |
| `src/lib.rs` | added | 1–25 | Public API surface, re-exports |
| `src/main.rs` | added | 1–10 | Placeholder binary |
| `src/chunk/id.rs` | added | 1–188 | SHA-256 algorithm, error variants, tests |
| `src/chunk/mod.rs` | added | 1–10 | Module structure |
| `src/config/source.rs` | added | 1–235 | YAML parsing, `parse()` error handling, unit tests |
| `src/config/validation.rs` | added | 1–149 | Duplicate ID, glob validation |
| `src/config/mod.rs` | added | 1–12 | Public API exports (⚠️ leaky re-export) |
| `src/error/types.rs` | added | 1–324 | 8-variant enum, Display, From conversions |
| `src/error/mod.rs` | added | 1–9 | Module structure |
| `src/logging/init.rs` | added | 1–114 | `tracing` init, double-init guard |
| `src/logging/mod.rs` | added | 1–9 | Module structure |
| `src/path/security.rs` | added | 1–295 | Traversal prevention, Windows UNC, non-existent paths |
| `src/path/mod.rs` | added | 1–9 | Module structure |
| `tests/config_test.rs` | added | 1–125 | Integration: parse, validate, error types |
| `tests/error_test.rs` | added | 1–154 | All 8 variants, From conversions |
| `tests/logging_test.rs` | added | 1–63 | init, double-init, no-panic |
| `tests/path_security_test.rs` | added | 1–118 | Traversal, boundary, non-existent paths |

## Instruction Files Reviewed

* `.github/copilot-instructions.md`: Full applicability — Rust conventions, error handling, path security, public API hygiene, `#[forbid(unsafe_code)]`, test discipline.
* `AGENTS.md`: Domain exception hierarchy, public API rules, TDD requirements.
* `.specify/memory/constitution.md`: Local-first, Lightweight Footprint, Data Pipeline Integrity, technology stack constraints.

## Review Items

### ✅ Approved for PR Comment — Applied

#### RI-001: Unit test for missing-file too permissive ✅ Fixed

* File: `src/config/source.rs`, lines 231–233
* The `|| s.starts_with("[config]")` alternative was removed. Test now asserts `[io]` only, consistent with the integration test and semantic correctness.

#### RI-002: Redundant `map_err` bypasses `From` impl ✅ Fixed

* File: `src/config/source.rs`, lines 34–37
* Replaced explicit `map_err(|e| GraphtorError::Config { ... })` with `?`. The `From<serde_yaml::Error>` impl already produces the identical variant.

#### RI-003: `pub use validation::validate` leaks internal API ✅ Fixed

* File: `src/config/mod.rs`, line 12
* Changed `pub mod validation` → `pub(crate) mod validation`; removed `pub use validation::validate`. External callers must use `SourceConfig::parse()` or `SourceConfig::validate()`.

#### RI-004: `GraphtorError` missing `#[non_exhaustive]` ✅ Fixed

* File: `src/error/types.rs`, line 19
* Added `#[non_exhaustive]` attribute. Future variants can be added without a breaking semver change.

#### RI-005: Stale "ollama timeout" fixture string ✅ Fixed

* File: `tests/error_test.rs`, line 47
* Changed to `"embedding timeout"`. Ollama is not part of the Rust-native architecture.

#### RI-007: `Cargo.toml` missing metadata ✅ Fixed

* File: `Cargo.toml`, lines 4–11
* Added `authors`, `repository`, `keywords`, `categories`.

### 🔍 In Review

*(none — all items resolved)*

### ❌ Rejected / No Action

#### RI-006: `default_branch()` as `const fn` ❌ Not Applicable

* `serde`'s `#[serde(default = "fn")]` requires the function to return the same type as the field (`String`). `String` is heap-allocated and cannot be produced in a `const fn` on stable Rust. Current implementation is the correct and idiomatic serde pattern.

## Phase 2 Analysis Notes

### Overall Assessment

The implementation is solid: `#[forbid(unsafe_code)]`, well-structured modules, comprehensive test coverage (68 tests passing), clean pedantic clippy, correct SHA-256 algorithm with null-byte separator, robust Windows UNC path handling. The TOCTOU limitation in path security is well-documented inline. The foundation is production-quality with a handful of addressable issues.

### High-Risk Areas Identified

1. **Public API surface leak** (`config/mod.rs`): Exporting `validate` directly invites misuse and complicates future API evolution.
2. **Test correctness gap** (`config/source.rs`): The unit test accepting either error category masks potential regressions.
3. **`From` impl bypass** (`config/source.rs`): Minor but creates a maintenance split point.

### Coverage Notes

* All 5 modules have unit tests inline + matching integration tests — good layered coverage.
* Doc-tests present on `generate_chunk_id` and `validate_path` — good API contract anchoring.
* `main.rs` has 0 tests (placeholder only) — expected.

## Next Steps

* [x] Phase 1: Tracking directory, pr-reference.xml, document seeded
* [x] Phase 2: Files analyzed, findings categorized, review plan built
* [x] Phase 3: All 7 items resolved — 6 fixed, 1 no-action
* [x] Phase 4: Fixes committed (`9df3fcd`), handoff.md generated, all gates green
