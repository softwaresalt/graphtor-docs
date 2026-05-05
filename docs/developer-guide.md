---
title: Developer Guide
description: "Build instructions, quality gates, module map, coding conventions, and contribution workflow for graphtor-docs"
---

This guide covers everything needed to build graphtor-docs from source, run
the quality gates, understand the codebase layout, and extend the system.

## Prerequisites

### Required

| Requirement | Version | Notes |
|---|---|---|
| Rust toolchain | 1.75+ (stable) | Install via [rustup](https://rustup.rs/) |
| Git | any recent | For cloning and the `git2` system library |

On Linux/macOS, the `git2` crate links against `libgit2`. Install system
dependencies:

```sh
# Ubuntu / Debian
sudo apt-get install libgit2-dev pkg-config

# macOS (Homebrew)
brew install libgit2
```

On Windows, `git2` uses bundled static libgit2 — no extra steps.

### Optional: PDFium DLL

For large PDF files (≥ 20 MiB), graphtor-docs routes through PDFium instead
of the pure-Rust `pdf-extract` backend. Without PDFium, large PDFs still
parse via the fallback backend (slower, lower quality).

To enable PDFium:
1. Download the pre-built PDFium shared library from
   [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
2. Place it in the same directory as the graphtor-docs binary, **or** set
   `GRAPHTOR_PDFIUM_PATH` to its full path

PDFium is never required; omitting it only affects large-PDF throughput.

## Building from Source

```sh
git clone https://github.com/softwaresalt/graphtor-docs.git
cd graphtor-docs

# Debug build (faster compile, slower runtime)
cargo build

# Release build (recommended for production use)
cargo build --release
```

The binary is at `target/release/graphtor-docs` (or `target/debug/graphtor-docs`).

## Quality Gates

Run all gates in this order before committing or opening a PR:

### Gate 1 — Compilation

```sh
cargo check
```

All code must compile cleanly. Run after every meaningful edit.

### Gate 2 — Lint compliance

```sh
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
```

Zero warnings or errors allowed. Fix all violations before proceeding.

### Gate 3 — Formatting

```sh
cargo fmt --all -- --check
```

If violations exist, auto-fix with `cargo fmt --all`, then re-check.

### Gate 4 — Tests

```sh
cargo test
```

All unit and integration tests must pass. To capture full output:

```sh
cargo test 2>&1 | tee logs/test-results.txt
```

## Workspace Layout

```text
src/
  main.rs                 ← binary entry point; 8 CLI subcommands
  lib.rs                  ← library crate root (graphtor-core)
  cli/
    mod.rs                ← clap Cli + Command enums; SyncArgs, ServeArgs, etc.
  pipeline/
    mod.rs                ← acquire → parse → embed → load orchestration
  acquire/                ← source acquisition (git clone, local scan, URL crawl)
  parse/                  ← pulldown-cmark, pdf-extract, docx parsers
  embed/
    model.rs              ← EmbeddingModel wrapping Candle all-MiniLM-L6-v2
  db/
    store.rs              ← DataStore: open, schema lifecycle
    schema.rs             ← CozoDB DDL; ensure_schema (idempotent)
    chunks.rs             ← chunk upsert operations
    nodes.rs              ← source node CRUD
    edges.rs              ← graph edge insertion
    traverse.rs           ← multi-hop BFS traversal
    search.rs             ← text search + semantic search
    vectors.rs            ← cosine similarity over doc_vectors
  sync/
    state.rs              ← SyncState / SourceSyncState structs
    git_diff.rs           ← git-based change detection
    mtime_diff.rs         ← mtime-based change detection
    reingest.rs           ← delete old chunks; re-run pipeline on changed files
  config/
    source.rs             ← SourceConfig, GitSource, LocalSource, UrlSource
    validation.rs         ← duplicate-ID, required-field, glob-syntax checks
  mcp/
    server.rs             ← DocServer; #[tool_router] impl with 7 tools
    format.rs             ← Markdown formatting for tool responses
  workspace/
    paths.rs              ← workspace directory layout (.graphtor/ subdirs)
    doctor.rs             ← health checks
    install.rs            ← install routine
    upgrade.rs            ← upgrade routine
    init.rs               ← sources.yaml template generation
  error/                  ← thiserror domain error types
  path.rs                 ← validate_path — path traversal guard
tests/
  integration/            ← end-to-end pipeline and database tests
docs/                     ← user-facing and developer-facing documentation
logs/                     ← transient output files (gitignored)
```

## Code Conventions

### Error handling

Domain errors use `thiserror` in `src/error/`. Never use `unwrap()` or
`expect()` in library code. Map external errors via `From` impls or
`.map_err()`. Use `anyhow` only in `src/main.rs` for binary-level error
propagation.

### Logging

Use the `tracing` crate — no `println!` in production code. Configure
`tracing-subscriber` in `src/main.rs` only. Log levels:

| Level | When to use |
|---|---|
| `DEBUG` | Per-document or per-chunk processing details |
| `INFO` | Pipeline milestones (stage start/complete, counts) |
| `WARN` | Recoverable issues (skipped files, retry attempts) |
| `ERROR` | Failures requiring attention |

### Imports

Group in this order: `std` → external crates → `crate::...`. Prefer explicit
imports; avoid glob imports. Default visibility is `pub(crate)`.

### Database access

All CozoDB operations go through `src/db/` — no raw Datalog queries outside
this module. Test databases use `tempfile::TempDir` — never write to
production paths in tests.

### Path security

All file paths must be validated against the workspace root before use:

```rust
let resolved = std::fs::canonicalize(&path)?;
if !resolved.starts_with(&allowed_root) {
    return Err(GraphtorError::PathViolation { path: resolved });
}
```

`src/path.rs` provides `validate_path()` for this purpose.

## Contribution Workflow

1. Fork and clone the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Write a failing test first (TDD is enforced)
4. Implement the production code
5. Run all four quality gates
6. Open a pull request targeting `main`

Direct pushes to `main` are blocked by branch protection.

## Extending the Pipeline

### Adding a new parser

1. Add a module under `src/parse/` implementing
   `parse(path: &Path) -> Result<ParsedDocument, GraphtorError>`
2. Register the new extension in `src/pipeline/mod.rs`
   (in the `dispatch_parse` function or equivalent)
3. Write integration tests under `tests/integration/`

### Adding a new MCP tool

1. Add a parameter struct in `src/mcp/server.rs` deriving
   `Deserialize` + `rmcp::schemars::JsonSchema`
2. Add the `#[tool(description = "...")]` method to the `#[tool_router]`
   `impl DocServer` block
3. Add a formatter function in `src/mcp/format.rs` returning Markdown
4. Write a contract test verifying the parameter schema

### Adding a new source type

1. Add a variant to `Source` enum in `src/config/source.rs`
2. Implement the acquire step in `src/acquire/`
3. Wire the new variant into the pipeline dispatch in
   `src/pipeline/mod.rs` and `src/sync/reingest.rs`
4. Update the sync-state logic in `src/sync/` if the new source
   has a diff signal
5. Update `docs/configuration.md` with the new source type schema
