<!--
  SYNC IMPACT REPORT
  ==================
  Version change: 1.0.0 → 2.0.0 (MAJOR: Rust-native rewrite)
  Modified principles:
    - I. Local-First Architecture: Ollama → Candle in-process embeddings
    - II. Lightweight Footprint: Python → single Rust binary, Ollama models → Candle all-MiniLM-L6-v2
    - III. Data Pipeline Integrity: LLM graph extraction → deterministic pulldown-cmark parsing
    - V. Automation & Reproducibility: sources.yaml dynamic registry, git2 incremental sync
  Added sections:
    - None (existing sections updated)
  Removed sections:
    - None (existing sections updated)
  Templates requiring updates:
    - .specify/templates/plan-template.md ✅ no changes needed (generic)
    - .specify/templates/spec-template.md ✅ no changes needed (generic)
    - .specify/templates/tasks-template.md ✅ no changes needed (generic)
  Follow-up TODOs:
    - Update .github/copilot-instructions.md for Rust conventions
    - Update AGENTS.md for Rust development workflow
-->

# LocalDocRAG Constitution

## Core Principles

### I. Local-First Architecture

All components MUST run locally with zero cloud service dependencies.

- Embedded databases MUST be used for storage (LanceDB for vectors,
  Kùzu for property graphs). No networked database servers.
- Embedding inference MUST use the in-process Candle framework running
  the all-MiniLM-L6-v2 model natively in Rust. No external model
  servers, no API calls to remote providers.
- Graph extraction MUST use deterministic parsing (pulldown-cmark AST)
  rather than LLM inference. No LLM is required for the ingestion
  pipeline.
- The MCP plugin server MUST bind to localhost only (STDIO transport).
- No documentation content or user queries leave the developer's
  machine.
- The entire system compiles to a single binary with zero runtime
  dependencies.

**Rationale**: Guarantees privacy, eliminates network latency, removes
environment friction (no Python, pip, or ML framework installation),
and enables fully offline operation on any developer workstation.

### II. Lightweight Footprint

Every dependency and resource choice MUST minimize disk, memory,
and CPU consumption.

- The system MUST compile to a single, zero-dependency Rust binary.
  Developers in C#, Go, Java, or Node.js workspaces can install and
  run the plugin without configuring interpreters or package managers.
- Use embedded/specialized databases over general-purpose servers.
- Use a dynamic `sources.yaml` registry so developers index only the
  documentation they need, not the entire corpus.
- Prefer efficient models: `all-MiniLM-L6-v2` (~80MB) for embeddings
  via the Candle framework. No heavyweight ML runtimes (PyTorch, etc.).
- Graph relationships are extracted deterministically from Markdown
  structure (headings, links, code blocks) — no LLM required.
- Every new crate dependency MUST justify its inclusion — if the
  standard library or an existing dependency can accomplish the task,
  use it.
- For Git sources, use shallow clones (`--depth 1`) via the git2 crate
  to minimize disk usage.

**Rationale**: The documentation corpus can be massive; the system
MUST remain practical on a single developer workstation with standard
hardware. A single binary eliminates the "Python virtual environment"
problem entirely.

### III. Data Pipeline Integrity

The ingestion pipeline MUST produce deterministic, reproducible,
and verifiable results.

- Every chunk MUST carry a stable `chunk_id` (SHA-256 of content +
  source path) that serves as the correlation key across vector and
  graph stores.
- Markdown parsing uses pulldown-cmark's AST event stream to
  deterministically extract headings, links, and code blocks with
  100% precision — no probabilistic LLM extraction.
- Normalization steps (YAML frontmatter stripping, heading-based
  chunking) MUST be idempotent — running them twice on the same
  input MUST produce identical output.
- Schema changes to LanceDB or Kùzu MUST be versioned and
  accompanied by a migration path or full rebuild procedure.
- Incremental sync MUST use git commit hashes (for Git sources) and
  file modification timestamps (for local sources) to identify
  changed files and perform surgical re-ingestion.

**Rationale**: Deterministic parsing eliminates the unpredictability
of LLM-based extraction. Corrupt or inconsistent data silently
degrades retrieval quality and is extremely difficult to diagnose.

### IV. MCP-Native Interface

All capabilities MUST be exposed exclusively via the Model Context
Protocol.

- Each MCP tool MUST have a clear, descriptive name and description
  that guides AI-agent tool selection (e.g.,
  `search_ms_docs_semantic`, `explore_ms_architecture`).
- Tool responses MUST return structured markdown content suitable
  for direct LLM consumption.
- MCP is the single interface contract — there MUST NOT be
  alternative access paths that bypass it for production use.
- Tool parameter schemas MUST be validated; malformed requests MUST
  return actionable error messages.

**Rationale**: A consistent, well-described MCP interface enables
any compliant AI agent (GitHub Copilot, Cursor, etc.) to
leverage the knowledge base without custom integration.

### V. Automation & Reproducibility

All data acquisition, processing, and loading MUST be fully
scriptable, idempotent, and re-runnable without manual
intervention.

- The `sources.yaml` registry defines exactly what documentation
  the developer wants indexed. Adding a new source is a single
  config edit followed by `graphtor-docs sync`.
- Clone operations MUST skip repositories that already exist
  locally. Incremental sync MUST only re-process changed files.
- The full pipeline (acquire → parse → embed → load) MUST be
  executable as a single `sync` command.
- The system MUST support incremental updates: track last-processed
  commit (Git) or file mtime (local), compute diffs, and surgically
  re-ingest only what changed.
- Generated artifacts (database files, sync state) MUST be
  reproducible from source inputs alone.

**Rationale**: A pipeline that requires manual steps will drift,
break silently, and resist updates when documentation publishers
release new content.

## Technology Stack Constraints

| Layer | Technology | Justification |
|-------|-----------|---------------|
| Language | Rust (stable) | Memory safety, single-binary distribution, zero runtime dependencies, native performance |
| Vector Store | LanceDB (`lancedb` crate) | Rust-native columnar vector DB with zero-copy Arrow integration, disk-based ANN indexing |
| Graph Store | Kùzu (`kuzu` crate) | High-performance embedded property-graph engine with Cypher support, C++ core with Rust bindings |
| Embeddings | `all-MiniLM-L6-v2` via Candle (`candle-core`, `candle-transformers`) | ~80MB model, 384-dim vectors, pure Rust ML inference, no external runtime |
| Graph Extraction | `pulldown-cmark` | Deterministic AST-based Markdown parsing, 100% precision edge extraction from links/headings |
| Plugin Server | `rmcp` crate | Rust MCP SDK, async STDIO JSON-RPC transport via tokio |
| Git Operations | `git2` crate | Native Git bindings for cloning, diff detection, no shell-out to system git |
| Configuration | `serde_yaml` + `serde_json` | Type-safe YAML/JSON parsing for sources.yaml and .sync_state.json |
| CLI | `clap` crate | Derive-based CLI framework with subcommands |
| Async Runtime | `tokio` | Industry-standard async runtime for MCP server |
| Error Handling | `thiserror` + `anyhow` | Typed domain errors (thiserror) and ad-hoc error propagation (anyhow) |
| Logging | `tracing` + `tracing-subscriber` | Structured async-safe logging |

- Adding a new crate dependency MUST be justified against this table.
- Replacing a stack component is a MAJOR constitution amendment.

## Development Workflow

- **Cargo workspace**: The project is a single Cargo workspace with a
  library crate (`graphtor-core`) and a binary target (`graphtor-docs`).
  All shared types live in the library crate.
- **Test-first**: Every new function and module MUST have tests written
  before implementation (red-green-refactor TDD cycle).
- **Idempotent by default**: Every pipeline stage MUST be safe to re-run.
  Use existence checks, upserts, and skip-if-exists patterns.
- **sources.yaml registry**: Documentation sources are defined in a
  user-managed `sources.yaml` file. The system supports both Git
  repositories and local directories with include/exclude glob patterns.
- **Incremental sync**: The system tracks sync state per source
  (`.sync_state.json`) and only re-processes files that have changed
  since the last sync.
- **Structured logging**: Pipeline stages MUST emit progress via the
  `tracing` crate. Silent failures are prohibited.
- **Single binary distribution**: `cargo build --release` produces one
  executable with all dependencies statically linked. No runtime
  interpreters, package managers, or model servers required.

## Governance

This constitution is the authoritative source of architectural
and procedural decisions for the LocalDocRAG project.

- **Supremacy**: Where this constitution conflicts with other
  documentation, the constitution prevails.
- **Amendment procedure**: Proposed changes MUST be documented with
  rationale, reviewed, and versioned before adoption. All
  amendments MUST include a Sync Impact Report and update
  dependent templates where necessary.
- **Versioning policy**: The constitution follows semantic
  versioning (MAJOR.MINOR.PATCH). MAJOR for principle
  removals/redefinitions, MINOR for new principles or material
  expansions, PATCH for clarifications and wording fixes.
- **Compliance review**: All PRs and design documents MUST be
  evaluated against the active principles. Non-compliance MUST be
  justified in writing or the change MUST be rejected.
- **Guidance file**: Refer to `AGENTS.md` for runtime AI-agent
  development guidance that complements this constitution.

**Version**: 2.0.0 | **Ratified**: 2026-03-09 | **Last Amended**: 2026-03-10
