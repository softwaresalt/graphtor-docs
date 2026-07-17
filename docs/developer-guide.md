---
title: Developer Guide
description: "Build instructions, quality commands, module map, coding conventions, and extension workflow for graphtor-docs"
---

This guide covers how to build graphtor-docs from source, run the local quality
commands, understand the codebase layout, and extend the Rust MCP server.

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust toolchain | 1.75+ stable | Install via [rustup](https://rustup.rs/) |
| Git | any recent | Needed only to clone the repository |

graphtor-docs is a single Rust binary. The ingestion path is docline-emitted
Markdown only, backed by embedded CozoDB and in-process Candle embeddings.

### Embedding model

`sync`, `serve`, and `prewarm` resolve the
`sentence-transformers/all-MiniLM-L6-v2` model through the shared embedding
resolver. On first use, the model is downloaded to the Hugging Face cache. Set
`GRAPHTOR_EMBED_MODEL_DIR` to a local directory containing `config.json`,
`tokenizer.json`, and `model.safetensors` for offline operation.

## Building from Source

```sh
git clone https://github.com/softwaresalt/graphtor-docs.git
cd graphtor-docs

# Debug build
cargo build

# Release build
cargo build --release
```

The binary is at `target/debug/graphtor-docs` for debug builds and
`target/release/graphtor-docs` for release builds.

## Build and Quality Commands

Run the smallest command that covers your change while iterating:

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
cargo fmt --all
```

Before handing work off, run the quality gate sequence:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
cargo test --all-targets
```

Use `cargo fmt --all` to apply formatting, then re-run the format check.

## Workspace Layout

```text
src/
  main.rs                 <- binary entry point and CLI command dispatch
  lib.rs                  <- library crate root; forbids unsafe code
  cli/                    <- clap CLI definitions, JSON-RPC output, errors
  workspace/              <- install, init, doctor, upgrade, serve discovery
  acquire/                <- local directory scan and glob filtering only
  parse/                  <- Markdown parsing via pulldown-cmark
    frontmatter.rs        <- YAML frontmatter stripping
    ast.rs                <- pulldown-cmark event stream to AST nodes
    chunker.rs            <- H2/H3 heading-based chunking
    links.rs              <- Markdown link extraction
    code.rs               <- fenced code block extraction
  chunk/                  <- deterministic SHA-256 chunk IDs
  embed/                  <- Candle all-MiniLM-L6-v2 embeddings, 384 dimensions
  db/                     <- embedded CozoDB store, schema v4, graph and vectors
    schema.rs             <- schema creation and v4 docline migration gates
    chunks.rs             <- chunk CRUD
    nodes.rs              <- source records
    edges.rs              <- document link and code edges
    search.rs             <- text and semantic search entry points
    traverse.rs           <- BFS traversal over document links
    vectors.rs            <- exact brute-force cosine k-NN; no vector index
    urls.rs               <- canonical document link resolution
  config/                 <- local and read-only database source config
  error/                  <- thiserror GraphtorError hierarchy
  ingest_contract/        <- docline v1 frontmatter contract validation
  lock.rs                 <- advisory database and workspace locks
  logging/                <- tracing subscriber setup
  mcp/                    <- rmcp STDIO server and Markdown tool formatters
  path/                   <- containment checks and reparse-point detection
  pipeline/               <- acquire -> parse -> embed -> load orchestration
  query/                  <- shared query layer for MCP and CLI
  sync/                   <- mtime-based incremental re-ingest
tests/                    <- integration and regression tests
docs/                     <- user-facing and developer-facing documentation
```

## Code Conventions

### Rust baseline

Production code uses Rust 2021 with `rust-version = "1.75"`.
`#![forbid(unsafe_code)]` is set at crate roots. Clippy warnings, including
`clippy::pedantic`, are treated as errors.

### Error handling

Domain errors use `thiserror` in `src/error/`. Public library functions return
`Result<_, GraphtorError>`. Avoid `unwrap()` and `expect()` in library code
unless the invariant is proven next to the call. Use `anyhow` for binary-level
error context in `src/main.rs`.

### Logging

Use `tracing` for production logging. Configure subscribers through
`src/logging/` and binary startup code.

| Level | When to use |
|---|---|
| `DEBUG` | Per-document or per-chunk processing details |
| `INFO` | Pipeline milestones and count summaries |
| `WARN` | Recoverable degraded behavior |
| `ERROR` | Failures requiring operator attention |

### Imports

Group imports in this order: `std`, external crates, then `crate::...`. Prefer
explicit imports over glob imports. Keep visibility as narrow as possible.

### Database access

All CozoDB operations go through `src/db/`. Query behavior shared by MCP tools
and CLI query commands belongs in `src/query/`, not in individual command
handlers.

Semantic search uses exact brute-force cosine k-NN over embeddings stored in
`doc_chunks`. Do not document or add code paths that assume a maintained vector
index.

### Path security

Validate filesystem paths through `src/path/` before access:

```rust
let resolved = graphtor_core::path::validate_path(&candidate, &allowed_root)?;
```

Use `is_reparse_point()` for trust anchors such as `.graphtor/`, managed config
files, and upgrade destinations before reading or writing through them. Escapes
through `..`, symlinks, or Windows junctions fail closed as
`GraphtorError::PathViolation`.

### Ingestion contract

Runtime ingestion accepts docline-emitted Markdown files only. `parse_file`
validates the docline v1 frontmatter contract before chunking:

* `title`
* `source`
* `ingested_at`
* `doc_type`
* `source_path`

Files without valid contract frontmatter fail deterministically and do not
silently enter the index.

## Contribution Workflow

1. Fork and clone the repository
2. Create a feature branch
3. Write a failing test first
4. Implement the production change
5. Run the relevant local quality commands
6. Open a pull request targeting `main`

Direct pushes to `main` are blocked by branch protection.

## Extending the System

### Updating Markdown parsing

1. Change the focused parser module under `src/parse/`
2. Add or update tests for frontmatter, AST events, chunks, links, or code
   snippets
3. Keep `parse_file` docline-contract validation fail-closed
4. Confirm the pipeline still accepts only supported Markdown extensions

### Adding a new MCP tool

1. Add a parameter struct in `src/mcp/server.rs` deriving
   `Deserialize` and `rmcp::schemars::JsonSchema`
2. Add a `#[tool(description = "...")]` method to the `#[tool_router]`
   `impl DocServer` block
3. Put shared query behavior in `src/query/` when the CLI should use the same
   semantics
4. Add or update Markdown formatting in `src/mcp/format.rs`
5. Add tests for parameter validation and response content

### Changing source configuration

1. Update `src/config/source.rs` and `src/config/validation.rs`
2. Preserve the distinction between ingestible `type: local` sources and
   read-only `type: database` entries
3. Ensure read-only database entries never reach the sync or write path
4. Update user documentation that describes `sources.yaml`
5. Add tests for invalid fields, path containment, and multi-database routing
