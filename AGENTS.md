# AGENTS.md — Runtime AI-Agent Development Guidance

Last updated: 2026-03-18

This file provides runtime guidance for AI agents (GitHub Copilot, Cursor,
or any MCP-compatible agent) working in the graphtor-docs (LocalDocRAG)
codebase. It complements the development guidelines at
`.github/copilot-instructions.md`.

## Authoritative Hierarchy

When documents conflict, follow this precedence order:

1. **Development Guidelines** (`.github/copilot-instructions.md`) —
   core principles, technology stack, code style, quality gates,
   terminal policies
2. **This file** (`AGENTS.md`) — runtime agent behavior and conventions

## Before You Start

1. Read `.github/copilot-instructions.md` to understand the five core
   principles (Local-First, Lightweight Footprint, Data Pipeline Integrity,
   MCP-Native Interface, Automation & Reproducibility), quality gates,
   code style, and terminal command policies.
2. Check `paths/ms-docs-grouped.txt` if the task involves adding or
   reorganizing documentation sources.

## Project Context

LocalDocRAG is a documentation cross-reference system that:

- **Acquires** documentation from Git repositories (shallow-cloned), local
  directories, and web URLs configured in `sources.yaml`
- **Parses** markdown content deterministically via pulldown-cmark AST
  (frontmatter stripping, heading-based chunking, link extraction)
- **Chunks** documents into units with stable SHA-256 correlation keys
- **Embeds** chunks using `all-MiniLM-L6-v2` via Candle (in-process,
  384-dim vectors, pure Rust ML inference)
- **Loads** chunks into CozoDB (embedded, sqlite backend)
- **Serves** queries via MCP tools bound to localhost (STDIO transport)

All processing is local. No data leaves the developer's machine.
The entire system compiles to a single Rust binary with zero runtime
dependencies.

## Development Workflow Requirements

### Issue Tracking

This project uses **Backlog.md** via the backlog MCP server for all task and project management.

**Getting started:** Read `backlog://workflow/overview` (MCP resource) or call `backlog.get_workflow_overview()` to understand the full workflow.

**Quick reference:**

- `backlog.list_tasks()` — Find available work (filter by status)
- `backlog.create_task()` — Create a new task
- `backlog.update_task()` — Update task status (e.g., mark done or blocked)
- Tasks are tracked in `backlog.md` and committed with git

### TDD Is Non-Negotiable

Every new function or module MUST follow red-green-refactor:

1. Write the test in `tests/` or as a `#[cfg(test)]` module
2. Run `cargo test` and confirm the test **fails**
3. Implement the production code
4. Run `cargo test` and confirm the test **passes**
5. Refactor if needed, re-run tests

Never write production code before the corresponding test exists and has
been observed to fail.

### Quality Gate Sequence

Run gates in this exact order after every meaningful change:

```text
1. cargo check
2. cargo clippy --all-targets -- -D warnings -D clippy::pedantic
3. cargo fmt --all -- --check
4. cargo test
```

Do not proceed to subsequent gates until the current gate passes. Do not
skip gates.

### Idempotency Requirement

Every pipeline stage MUST be idempotent. Running the same
operation twice on the same input MUST produce identical output. Use:

- Existence checks before creating files or directories
- Upserts for database writes
- Skip-if-exists patterns for clone operations
- Deterministic chunk IDs (SHA-256 of content + source path, not random)

## Code Conventions for Agents

### File Creation Checklist

When creating a new `.rs` module, include:

1. `//!` module-level doc comment explaining the module's purpose
2. `///` doc comments on all public items (structs, functions, traits)
3. `pub(crate)` as default visibility unless public API is required
4. Error handling via `thiserror` domain error types from `src/error/`

### Domain Errors

All errors flow through the typed hierarchy defined in `src/error/`:

```text
ConfigError         — configuration and environment issues
DatabaseError       — LanceDB or Kùzu operation failures
PipelineError       — pipeline stage execution failures
ExtractionError     — parsing or extraction failures
SchemaError         — data schema validation failures
PathViolationError  — path traversal or security violations
ChunkIntegrityError — chunk ID or content integrity failures
```

Use `thiserror` derive macros. Map external errors via `From` impls or
explicit `.map_err()`. Never use `unwrap()` or `expect()` in library code.

### Database Access Rules

- **Vector (LanceDB)**: All operations MUST go through `src/db/vector.rs`.
  No raw LanceDB calls in pipeline stages, MCP tools, or tests.
- **Graph (Kùzu)**: All operations MUST go through `src/db/graph.rs`.
  No raw Cypher queries outside this module.
- **Test databases**: Use temporary directories — never write to
  production database paths in tests.

### Path Security

Before reading or writing any file path:

```rust
let resolved = std::fs::canonicalize(&path)?;
if !resolved.starts_with(&allowed_root) {
    return Err(PathViolationError::new(resolved));
}
```

This applies to all user-supplied paths, file paths from manifests, and
paths constructed from repository names or document titles.

### Serialization Rules

All data structures that cross module boundaries MUST derive
`serde::Serialize` and `serde::Deserialize`. This includes:

- Chunk metadata passed between pipeline stages
- MCP tool request/response schemas
- Database record representations
- Configuration objects (`sources.yaml`, `.sync_state.json`)

## Pipeline Stage Conventions

Each pipeline stage in `src/pipeline/` follows this pattern:

```rust
//! Brief description of what this stage does.

use crate::error::PipelineError;
use tracing::{info, error};

/// Run the pipeline stage on the given input.
pub(crate) fn run(config: &StageConfig) -> Result<(), PipelineError> {
    info!(source = %config.source, "starting stage");
    // ...
    Ok(())
}
```

- Stage order: acquire → parse → embed → load
- Each stage reads from the previous stage's output
- Stages MUST be idempotent — safe to re-run on the same input
- Use `clap` subcommands for CLI invocation
- Progress output via `tracing::info!`; errors via `tracing::error!`

## MCP Tool Conventions

When implementing MCP tools under `src/mcp/tools/`:

1. **One tool per module** — each tool is a separate `.rs` module
2. **Descriptive names** — `search_local_docs`, `search_semantic`,
   `get_chunk_by_id`, not `search`, `query`, `get`
3. **Clear descriptions** — the tool description MUST be sufficient for an
   AI agent to decide when to use it without reading source code
4. **Typed parameters** — all tool parameters validated via typed Rust
   structs with serde
5. **Markdown responses** — tool output is structured markdown, suitable for
   direct LLM consumption
6. **Localhost only** — the MCP server MUST bind to localhost (STDIO transport)
7. **rmcp macros** — use `#[tool]`/`#[tool_router]` macros from `rmcp` 1.5

## Dependency Management

- All dependencies are listed in `Cargo.toml`
- Adding a new crate dependency MUST be justified against the Technology
  Stack table and Lightweight Footprint principle
- Prefer `std` solutions over new crates
- Replacing a stack component requires explicit justification and review

## Logging Standards

- Use the `tracing` crate — no `println!` in production code
- Configure `tracing-subscriber` in entry points only (`src/main.rs`)
- Library modules use `tracing::info!`, `tracing::debug!`, etc.
- Log levels:
  - `DEBUG` — per-document or per-chunk processing details
  - `INFO` — pipeline milestones (stage start/complete, counts)
  - `WARN` — recoverable issues (skipped files, retry attempts)
  - `ERROR` — failures requiring attention
- Include context in structured fields:
  `tracing::info!(count, source_path, "chunked documents")`

## Terminal Command Policy

See `.github/copilot-instructions.md` for the full terminal command
execution policy. Key rules:

- **One command per terminal call** — never chain with `;`, `&&`, `||`
- **Output redirection to `logs/`** — never to workspace root or `target/`
- **Inspect results between commands** — check exit codes before proceeding
- **Use `pwsh`** — never `powershell` or `powershell.exe`

## Git Commit Conventions

- Commit messages use imperative mood: "Add chunk pipeline stage"
- Prefix with scope when relevant: `pipeline: add normalize stage`
- Keep the first line under 72 characters
- Reference task IDs when implementing features: `task-001: add acquire stage`

## What NOT to Do

- Do NOT add cloud service dependencies (violates Local-First)
- Do NOT use networked database servers (use embedded LanceDB/Kùzu only)
- Do NOT call remote embedding/LLM APIs (use in-process Candle only)
- Do NOT write raw SQL/Cypher outside `src/db/` modules
- Do NOT skip quality gates or reorder them
- Do NOT use `println!` for logging in production code
- Do NOT use `unwrap()` or `expect()` in library code
- Do NOT construct file paths from unvalidated input
- Do NOT use `unsafe` code (enforced by `#![forbid(unsafe_code)]`)
- Do NOT write tests that depend on production database paths
- Do NOT add crate dependencies without justification
- Do NOT shell out to system `git` — use the `git2` crate

<!-- BACKLOG.MD MCP GUIDELINES START -->

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_workflow_overview()` tool to load the tool-oriented overview (it lists the matching guide tools).

- **First time working here?** Read the overview resource IMMEDIATELY to learn the workflow
- **Already familiar?** You should have the overview cached ("## Backlog.md Overview (MCP)")
- **When to read it**: BEFORE creating tasks, or when you're unsure whether to track work

These guides cover:
- Decision framework for when to create tasks
- Search-first workflow to avoid duplicates
- Links to detailed guides for task creation, execution, and finalization
- MCP tools reference

You MUST read the overview resource to understand the complete workflow. The information is NOT summarized here.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD MCP GUIDELINES END -->
