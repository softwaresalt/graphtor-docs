<!--
  SYNC IMPACT REPORT
  ==================
  Version change: N/A → 1.0.0 (initial ratification)
  Modified principles: N/A (initial)
  Added sections:
    - Core Principles (5): Local-First Architecture,
      Lightweight Footprint, Data Pipeline Integrity,
      MCP-Native Interface, Automation & Reproducibility
    - Technology Stack Constraints
    - Development Workflow
    - Governance
  Removed sections: N/A (initial)
  Templates requiring updates:
    - .specify/templates/plan-template.md ✅ no changes needed
    - .specify/templates/spec-template.md ✅ no changes needed
    - .specify/templates/tasks-template.md ✅ no changes needed
  Follow-up TODOs: None
-->

# LocalDocRAG Constitution

## Core Principles

### I. Local-First Architecture

All components MUST run locally with zero cloud service dependencies.

- Embedded databases MUST be used for storage (LanceDB for vectors,
  Kùzu for property graphs). No networked database servers.
- LLM inference (embedding and extraction) MUST use locally-hosted
  models via Ollama. No API calls to remote LLM providers.
- The MCP plugin server MUST bind to localhost only.
- No documentation content or user queries leave the developer's
  machine.

**Rationale**: Guarantees privacy, eliminates network latency,
and enables fully offline operation.

### II. Lightweight Footprint

Every dependency and resource choice MUST minimize disk, memory,
and CPU consumption.

- Use embedded/specialized databases over general-purpose servers.
- Target specific documentation repositories via shallow clones
  (`--depth 1`) rather than full history or web scraping.
- Prefer efficient models: `nomic-embed-text` (8k context) for
  embeddings, `phi-4` or `llama-3.2` for graph extraction.
- Every new dependency MUST justify its inclusion — if the standard
  library or an existing dependency can accomplish the task, use it.
- Filter localized repositories at acquisition time to prevent
  duplicate content bloat.

**Rationale**: The Microsoft Docs corpus is massive; the system
MUST remain practical on a single developer workstation.

### III. Data Pipeline Integrity

The ingestion pipeline MUST produce deterministic, reproducible,
and verifiable results.

- Every chunk MUST carry a stable `chunk_id` (UUID) that serves as
  the correlation key across vector and graph stores.
- Normalization steps (YAML frontmatter stripping, UI-tag removal,
  locale filtering) MUST be idempotent — running them twice on the
  same input MUST produce identical output.
- Graph extraction prompts MUST enforce strict JSON schema output.
  Freeform or unstructured LLM responses MUST be rejected and
  retried.
- Schema changes to LanceDB or Kùzu MUST be versioned and
  accompanied by a migration path or full rebuild procedure.

**Rationale**: Corrupt or inconsistent data silently degrades
retrieval quality and is extremely difficult to diagnose after
the fact.

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

- Clone operations MUST skip repositories that already exist
  locally.
- Group organization MUST be deterministic from the source
  manifest (`ms-docs-grouped.txt`).
- The full pipeline (acquire → normalize → chunk → extract → load)
  MUST be executable as a single command or ordered script
  sequence.
- Generated artifacts (batch scripts, database files) MUST be
  reproducible from source inputs alone.

**Rationale**: A pipeline that requires manual steps will drift,
break silently, and resist updates when Microsoft publishes new
documentation.

## Technology Stack Constraints

| Layer | Technology | Justification |
|-------|-----------|---------------|
| Language | Python 3.11+ | MCP SDK, LanceDB, and Kùzu all have first-class Python bindings |
| Vector Store | LanceDB (embedded) | Zero-server columnar vector DB with PyArrow integration |
| Graph Store | Kùzu (embedded) | High-performance embedded property-graph engine with Cypher support |
| Embeddings | `nomic-embed-text` via Ollama | 8k context window, efficient for code and documentation |
| Extraction LLM | `phi-4` or `llama-3.2` via Ollama | Small-footprint models sufficient for structured JSON extraction |
| Plugin Server | Python MCP SDK | Official protocol implementation for tool exposure |
| Scripting | Python + Windows Batch | Batch scripts for clone orchestration; Python for all processing |

- Adding a new dependency MUST be justified against this table.
- Replacing a stack component is a MAJOR constitution amendment.

## Development Workflow

- **Scripts first**: All automation lives under `.scripts/`. New
  pipeline stages MUST be implemented as standalone Python scripts
  with CLI entry points before any integration work.
- **Idempotent by default**: Every script MUST be safe to re-run.
  Use existence checks, upserts, and skip-if-exists patterns.
- **Group-based organization**: Documentation repositories are
  organized into thematic groups defined in
  `paths/ms-docs-grouped.txt`. New repos MUST be assigned to an
  existing group or a new group MUST be proposed and documented.
- **Shallow clones only**: Repository cloning MUST use `--depth 1`
  to minimize disk usage. Full history is never required.
- **Structured logging**: Pipeline scripts MUST emit progress to
  stdout and errors to stderr. Silent failures are prohibited.

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

**Version**: 1.0.0 | **Ratified**: 2026-03-09 | **Last Amended**: 2026-03-09
