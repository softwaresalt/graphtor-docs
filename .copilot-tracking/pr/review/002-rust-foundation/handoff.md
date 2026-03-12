<!-- markdownlint-disable-file -->
# PR Review Handoff: 002-rust-foundation

## PR Overview

FG-001 Rust Foundation & Core Types — establishes the Cargo workspace, error type hierarchy, `sources.yaml` configuration parsing, structured logging via `tracing`, path security enforcement, and deterministic SHA-256 chunk ID generation. All source is new (154 files, 19,140 insertions). The implementation is production-quality: `#[forbid(unsafe_code)]`, 68 tests across unit + integration + doc-test layers, correct SHA-256 null-byte separator to prevent hash ambiguity, and robust Windows UNC path handling with TOCTOU limitations documented inline.

* Branch: `002-rust-foundation`
* Base Branch: `main`
* Commit with review fixes: `9df3fcd`
* Total Files Changed: 154 (Rust source scope: 19 files)
* Total Review Items: 7 (6 fixed, 1 no-action)

---

## Quality Gate Results (post-fix)

| Gate | Result |
|------|--------|
| `cargo build` | ✅ Clean |
| `cargo test` | ✅ 68 passed, 0 failed |
| `cargo clippy -- -D warnings -W clippy::pedantic` | ✅ Clean |
| `cargo fmt --check` | ✅ Clean |

---

## PR Comments Ready for Submission

### File: `src/config/source.rs`

#### Comment 1 (Lines 34–37) — Redundant `map_err` bypasses `From` impl

* Category: Code Quality / Idiomatic Rust
* Severity: Medium
* **Status: Fixed in `9df3fcd`**

`GraphtorError` already implements `From<serde_yaml::Error>` in `error/types.rs`, which produces the identical `Config { message: e.to_string(), field: None }` variant. The explicit `map_err` created a maintenance split point: if the `From` impl were updated, this inline closure would silently diverge.

**Applied fix** — replaced with idiomatic `?` operator:
```rust
let config: Self = serde_yaml::from_str(&content)?;
```

#### Comment 2 (Lines 225–233) — Unit test too permissive for missing-file error

* Category: Test Correctness
* Severity: Medium
* **Status: Fixed in `9df3fcd`**

`parse_missing_file_returns_io_error` accepted either `[io]` or `[config]` as a valid outcome. A missing file is unambiguously an I/O condition. The permissive `||` meant a regression reclassifying this error as `Config` would silently pass. The integration test in `tests/config_test.rs` already asserted `[io]` only — the unit test was inconsistent with it.

**Applied fix:**
```rust
assert!(
    s.starts_with("[io]"),
    "missing file must produce an Io error, got: {s}"
);
```

---

### File: `src/config/mod.rs`

#### Comment 3 (Line 12) — `pub use validation::validate` leaks internal API

* Category: Design / API Hygiene
* Severity: Medium
* **Status: Fixed in `9df3fcd`**

Re-exporting the bare `validate` function as a public symbol invited callers to bypass the designed API (`SourceConfig::parse()` which validates automatically, or `SourceConfig::validate()` for explicit validation). Consumers calling `graphtor_core::config::validate(...)` directly couple to an internal implementation detail that should be free to evolve.

**Applied fix** — sealed the validation module:
```rust
pub(crate) mod validation;
// pub use validation::validate; ← removed
```

---

### File: `src/error/types.rs`

#### Comment 4 (Line 19) — `GraphtorError` missing `#[non_exhaustive]`

* Category: Design / Maintainability
* Severity: Low–Medium
* **Status: Fixed in `9df3fcd`**

As a library crate, `GraphtorError` will need additional variants as feature groups are implemented (e.g., variants for graph extraction, schema migration, incremental sync). Without `#[non_exhaustive]`, adding any variant is a semver-breaking change for downstream consumers matching exhaustively. Adding it now is a zero-cost guard.

**Applied fix:**
```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphtorError {
```

---

### File: `tests/error_test.rs`

#### Comment 5 (Line 47) — Stale Ollama reference in test fixture

* Category: Conventions / Documentation
* Severity: Low
* **Status: Fixed in `9df3fcd`**

The test fixture used `"ollama timeout"` as an embed error message. Ollama is not part of the Rust-native architecture — embeddings are generated in-process via Candle (`all-MiniLM-L6-v2`). Stale references create confusion for contributors reading the tests.

**Applied fix:**
```rust
message: "embedding timeout".to_string(),
```

---

### File: `Cargo.toml`

#### Comment 6 (Lines 4–11) — Missing library metadata fields

* Category: Documentation / Operations
* Severity: Low
* **Status: Fixed in `9df3fcd`**

Standard `[package]` metadata (`authors`, `repository`, `keywords`, `categories`) was absent. These fields improve discoverability if published and provide useful context in `cargo metadata` output consumed by tooling.

**Applied fix:**
```toml
authors = ["graphtor-docs contributors"]
repository = "https://github.com/graphtor/graphtor-docs"
keywords = ["documentation", "rag", "graph", "mcp", "search"]
categories = ["development-tools", "text-processing"]
```

---

## Closed — No Action

### RI-006: `default_branch()` as `const fn`

`serde`'s `#[serde(default = "fn_name")]` attribute requires the referenced function to return the same type as the struct field — in this case `String`. On stable Rust, `String` cannot be constructed in a `const fn` context. The current implementation (`fn default_branch() -> String { "main".to_string() }`) is the correct and idiomatic serde pattern. No change warranted.

---

## Review Summary by Category

| Category | Count |
|----------|-------|
| Code Quality (idiomatic Rust) | 2 (RI-001, RI-002) |
| Design / API Hygiene | 2 (RI-003, RI-004) |
| Conventions / Documentation | 2 (RI-005, RI-007) |
| Performance | 1 (RI-006 — closed, no action) |
| Security Issues | 0 |
| Reliability Issues | 0 |

---

## Instruction Compliance

* ✅ `.github/copilot-instructions.md`: All rules followed — `#[forbid(unsafe_code)]`, typed errors, path security, Google-style doc comments, idiomatic Rust.
* ✅ `AGENTS.md`: Error hierarchy matches 8-variant spec; public API boundaries respected after RI-003 fix; TDD discipline evident throughout.
* ✅ `.specify/memory/constitution.md`: Local-first, zero external dependencies, single binary target, embedded databases only, no cloud calls — all upheld.
* ⚠️ `.github/copilot-instructions.md` (minor, pre-existing): The file still references Python-era conventions (`from __future__ import annotations`, Pydantic, Ollama) that predate the Rust-native constitution amendment. These are agent instruction files, not source code — no action required on this PR, but the instruction file should be updated in a follow-up to reflect the current Rust architecture.

---

## Outstanding Risks & Follow-up Recommendations

1. **`copilot-instructions.md` staleness** — The instruction file references the old Python stack (Pydantic, Ollama, `from __future__ import annotations`). While harmless for this PR, future agents using it as guidance may introduce Python-oriented patterns. Recommend updating in a dedicated chore PR.

2. **`validate_globs` reports first error only** — Glob validation stops at the first invalid pattern. Users with multiple bad patterns must fix them one at a time. Consider accumulating all errors and returning them as a batch (informational; non-blocking for this PR).

3. **`Source::id()` visibility** — Currently `pub(crate)`. If downstream pipeline stages need to inspect source IDs without holding a `SourceConfig`, a `pub fn id(&self) -> &str` method would be appropriate. Worth considering when FG-007 (ingestion pipeline) is scoped.

4. **TOCTOU in `validate_path`** — Documented inline. Acceptable for batch ingestion pipelines; flagged for awareness if MCP server tools ever accept user-supplied paths in a high-security context.
