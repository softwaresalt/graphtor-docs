---
applyTo: "**"
---

# Agent Engram Constitution

## Core Principles

### I. Safety-First Rust

All production code MUST be written in Rust (stable toolchain,
edition 2024, `rust-version = "1.85"`). `unsafe` code is forbidden
at the workspace level (`#![forbid(unsafe_code)]`). Clippy pedantic
lints MUST pass with zero warnings. `unwrap()` and `expect()` are
denied in library code; all fallible operations MUST use the
`Result`/`EngramError` pattern defined in `src/errors/mod.rs`.

**Rationale**: The daemon manages an embedded database, file-system
writes for dehydration, and long-lived SSE connections on behalf of
AI coding assistants. Memory safety and explicit error handling are
non-negotiable to prevent data corruption, silent failures, or
state loss during unattended operation.

### II. MCP Protocol Fidelity

The server MUST implement the Model Context Protocol via the
`mcp-sdk` 0.0.3 crate (JSON-RPC 2.0). All MCP tools MUST be
unconditionally visible to every connected agent regardless of
configuration. Tools called in inapplicable contexts (e.g.,
workspace-scoped tools before `set_workspace`) MUST return a
descriptive error rather than being hidden. Transport is SSE
(GET `/sse`) with JSON-RPC dispatch (POST `/mcp`).

**Rationale**: Consistent tool surface ensures agents can discover
capabilities without conditional logic. Protocol compliance
guarantees interoperability with any MCP-compatible client (Claude
Code, GitHub Copilot CLI, Cursor, VS Code).

### III. Test-First Development (NON-NEGOTIABLE)

Every feature MUST have tests written before implementation code.
The test directory structure (`tests/contract/`, `tests/integration/`,
`tests/unit/`) MUST be maintained. Contract tests validate MCP tool
input/output schemas and error codes. Integration tests validate
cross-module interactions (hydration, dehydration, search,
concurrency). Unit tests validate isolated logic. Property-based
tests with `proptest` validate serialization round-trips. All tests
MUST pass via `cargo test` before any code is merged.

**Rationale**: The daemon operates unattended for extended periods.
Regressions in task management, hydration/dehydration cycles, graph
traversal, or semantic search can silently corrupt workspace state.
Test-first discipline catches failures before they reach production.

### IV. Workspace Isolation & Security Boundaries

All file-system operations MUST resolve within the configured
workspace root. Path traversal attempts MUST be rejected. Each
workspace MUST map to a unique SurrealDB database via deterministic
SHA-256 hash of the canonical workspace path. Database queries MUST
execute solely within the active workspace's database context. The
daemon MUST bind exclusively to `127.0.0.1`; no external network
exposure is permitted. No secrets or credentials MUST appear in
`.engram/` files (which may be committed to Git).

**Rationale**: The daemon serves multiple workspaces simultaneously
and persists state to Git-committable files. Without strict
isolation, a misbehaving agent could access or corrupt another
workspace's data, leak internal paths, or expose sensitive
information through serialized state.

### V. Structured Observability

All significant operations MUST emit structured tracing spans to
stderr via `tracing-subscriber`. Span coverage MUST include: MCP
tool call execution, workspace lifecycle events (bind, hydrate,
flush), database operations, SSE connection management, and
embedding/search operations. Log output MUST support both
human-readable and JSON formats via `tracing-subscriber` features.
No external metrics endpoint or telemetry collector is required
for v1.

**Rationale**: The daemon runs as a background service for hours or
days. When something goes wrong during unattended operation,
structured traces are the primary diagnostic tool. Without them,
debugging hydration failures, stale-file detection, or concurrency
issues would require reproducing the exact scenario.

### VI. Single-Binary Simplicity

The project MUST produce a single binary (`engram`). Dependencies
MUST be managed via `Cargo.toml` workspace dependencies. New
dependencies MUST be justified by a concrete requirement — do not
add libraries speculatively. Prefer the standard library over
external crates when the standard library solution is adequate.
SurrealDB embedded (surrealkv) is the sole persistence layer; do
not introduce additional databases or caches. Optional capabilities
(e.g., embeddings via `fastembed`) MUST use Cargo feature flags.

**Rationale**: Operational simplicity is critical for a tool that
developers install on personal workstations. Every additional
dependency increases build time, attack surface, and maintenance
burden. The single-binary model ensures deployment is a single
file copy.

### VII. CLI Workspace Containment (NON-NEGOTIABLE)

When GitHub Copilot operates in CLI mode, it MUST NOT create,
modify, or delete any file or directory outside the current
working directory tree. This applies to all tool invocations
including `create_file`, `replace_string_in_file`,
`multi_replace_string_in_file`, `run_in_terminal`, and any
operation that touches the filesystem. Paths that resolve above
or outside the cwd — whether via absolute paths, `..` traversal,
symlinks, or environment variable expansion — MUST be refused.
The only exception is reading files explicitly provided by the
user as context.

**Rationale**: CLI agents run with the operator's full filesystem
permissions and no interactive approval UI. A single misrouted
write can corrupt unrelated repositories, overwrite system
configuration, or destroy data in sibling directories. Strict
cwd containment is the last line of defense when no human is
watching.

### VIII. Git-Friendly Persistence

All workspace state MUST be serializable to human-readable,
Git-mergeable files in the `.engram/` directory. Markdown with
YAML frontmatter is the canonical format for task files.
Dehydration MUST use structured diff merge (via the `similar`
crate) to preserve user-added comments and formatting. No binary
files in `.engram/`. Writes MUST use atomic temp-file-then-rename
to prevent corruption. File formats MUST minimize merge conflicts
(sorted keys, stable ordering).

**Rationale**: Workspace state travels with the codebase in Git.
Human-readable files enable code review of agent-managed state,
conflict resolution during merges, and manual editing when needed.
Atomic writes prevent half-written state from corrupting the
workspace during crashes or concurrent access.

## Technical Constraints

- **Language**: Rust stable, edition 2024, `rust-version = "1.85"`
- **Async runtime**: Tokio 1 (full features)
- **MCP SDK**: `mcp-sdk` 0.0.3 (JSON-RPC 2.0 over SSE)
- **HTTP Transport**: Axum 0.7 with SSE at `/sse` and JSON-RPC
  at `/mcp`
- **Persistence**: SurrealDB 2 embedded (surrealkv backend),
  per-workspace namespace via SHA-256 path hash
- **Code Parsing**: `tree-sitter` 0.24 with `tree-sitter-rust`
  for code graph extraction
- **Embeddings**: `fastembed` 3 (optional, behind `embeddings`
  feature flag)
- **Diff/Merge**: `similar` 2 for structured diff merge during
  dehydration
- **Markdown**: `pulldown-cmark` 0.10 for parsing `.engram/`
  markdown files
- **Linting**: `.cargo/config.toml` sets
  `rustflags = ["-Dwarnings"]`; `cargo clippy` with pedantic
  and `-D warnings`
- **Build verification**: `cargo test` and `cargo clippy` MUST
  pass before merge
- **License**: MIT

## Development Workflow

1. **Feature specs first**: Every feature MUST have a specification
   in `specs/###-feature-name/spec.md` before implementation begins.
2. **Plan before code**: Implementation plans MUST be generated via
   the speckit workflow (`spec → plan → tasks`) and stored alongside
   the spec.
3. **Branch per feature**: Each feature MUST be developed on a
   dedicated branch matching the spec directory name
   (e.g., `001-core-mcp-daemon`).
4. **Contract-first design**: MCP tool schemas and data models MUST
   be defined in contract documents before implementation. Changes
   to contracts require updating corresponding contract tests.
5. **Commit discipline**: Each commit MUST represent a coherent,
   buildable change. Commit messages MUST follow conventional
   commits format (e.g., `feat:`, `fix:`, `docs:`, `test:`).
6. **No dead code**: Placeholder modules (e.g., `//! placeholder`)
   MUST be replaced with real implementations or removed before a
   feature is considered complete.

## Governance

This constitution supersedes all other development practices for
the Engram project. All code reviews and automated checks MUST
verify compliance with these principles.

- **Amendments**: Any change to this constitution MUST be documented
  with a version bump, rationale, and sync impact report. Principle
  removals or redefinitions require a MAJOR version bump. New
  principles or material expansions require MINOR. Clarifications
  and wording fixes require PATCH.
- **Compliance review**: Every implementation plan MUST include a
  "Constitution Check" section (per the plan template) that maps
  the proposed work against these principles and documents any
  justified violations in the Complexity Tracking table.
- **Conflict resolution**: When a principle conflicts with a
  practical implementation need, the conflict MUST be documented
  in the plan's Complexity Tracking table with the specific
  principle violated, the justification, and the simpler
  alternative that was rejected.

**Version**: 1.0.0 | **Ratified**: 2026-02-28 | **Last Amended**: 2026-02-28
