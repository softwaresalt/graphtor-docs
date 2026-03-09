# AGENTS.md — Runtime AI-Agent Development Guidance

Last updated: 2026-03-08

This file provides runtime guidance for AI agents (GitHub Copilot, Cursor,
or any MCP-compatible agent) working in the graphtor-docs (LocalDocRAG)
codebase. It complements the constitution at
`.specify/memory/constitution.md` and the development guidelines at
`.github/copilot-instructions.md`.

## Authoritative Hierarchy

When documents conflict, follow this precedence order:

1. **Constitution** (`.specify/memory/constitution.md`) — architectural
   principles and non-negotiable constraints
2. **Development Guidelines** (`.github/copilot-instructions.md`) — code
   style, quality gates, terminal policies
3. **This file** (`AGENTS.md`) — runtime agent behavior and conventions

## Before You Start

1. Read `.specify/memory/constitution.md` to understand the five core
   principles: Local-First, Lightweight Footprint, Data Pipeline Integrity,
   MCP-Native Interface, Automation & Reproducibility.
2. Read `.github/copilot-instructions.md` for quality gates, code style,
   and terminal command policies.
3. Check `specs/` for any active feature specifications relevant to the
   current task.
4. Check `paths/ms-docs-grouped.txt` if the task involves adding or
   reorganizing documentation sources.

## Project Context

LocalDocRAG is a documentation cross-reference system that:

- **Acquires** Microsoft Docs repositories via shallow clones organized
  into 38 thematic groups (~703 repos)
- **Normalizes** markdown content (frontmatter stripping, UI-tag removal,
  locale filtering)
- **Chunks** documents into units with stable UUID correlation keys
- **Extracts** graph relationships via local LLM (JSON schema enforced)
- **Loads** chunks into LanceDB (vector) and Kùzu (property graph)
- **Serves** queries via MCP tools bound to localhost

All processing is local. No data leaves the developer's machine.

## Development Workflow Requirements

### TDD Is Non-Negotiable

Every new function, class, or pipeline stage MUST follow red-green-refactor:

1. Write the test in `tests/unit/` or `tests/integration/`
2. Run `pytest` and confirm the test **fails**
3. Implement the production code
4. Run `pytest` and confirm the test **passes**
5. Refactor if needed, re-run tests

Never write production code before the corresponding test exists and has
been observed to fail.

### Quality Gate Sequence

Run gates in this exact order after every meaningful change:

```text
1. mypy src/ tests/ .scripts/ --strict
2. ruff check src/ tests/ .scripts/
3. ruff format --check src/ tests/ .scripts/
4. pytest tests/ -v
```

Do not proceed to subsequent gates until the current gate passes. Do not
skip gates.

### Idempotency Requirement

Every script and pipeline stage MUST be idempotent. Running the same
operation twice on the same input MUST produce identical output. Use:

- Existence checks before creating files or directories
- Upserts for database writes
- Skip-if-exists patterns for clone operations
- Deterministic chunk IDs (UUID from content hash, not random)

## Code Conventions for Agents

### File Creation Checklist

When creating a new `.py` file, include:

1. `from __future__ import annotations` as the first import
2. Module-level docstring explaining the file's purpose
3. Type annotations on all function signatures
4. Google-style docstrings on all public functions and classes
5. `if __name__ == "__main__":` guard for scripts

### Domain Exceptions

All errors flow through the hierarchy defined in `src/errors.py`:

```text
LocalDocRAGError (base)
  ├── ConfigError         — configuration and environment issues
  ├── DatabaseError       — LanceDB or Kùzu operation failures
  ├── PipelineError       — pipeline stage execution failures
  ├── ExtractionError     — LLM extraction failures or invalid JSON
  ├── SchemaError         — data schema validation failures
  ├── PathViolationError  — path traversal or security violations
  └── ChunkIntegrityError — chunk ID or content integrity failures
```

Never use bare `except:`. Always catch specific exception types and
re-raise as domain exceptions when crossing module boundaries.

### Database Access Rules

- **Vector (LanceDB)**: All operations MUST go through `src/db/vector.py`.
  No raw LanceDB calls in pipeline stages, MCP tools, or tests.
- **Graph (Kùzu)**: All operations MUST go through `src/db/graph.py`.
  No raw Cypher queries outside this module.
- **Test databases**: Always use `tmp_path` (pytest fixture) for temporary
  database directories. Never read from or write to production database
  paths in tests.

### Path Security

Before reading or writing any file path:

```python
resolved = path.resolve()
if not resolved.is_relative_to(allowed_root):
    raise PathViolationError(f"path escapes allowed root: {resolved}")
```

This applies to all user-supplied paths, file paths from manifests, and
paths constructed from repository names or document titles.

### Pydantic Model Rules

All data structures that cross module boundaries MUST be Pydantic
`BaseModel` subclasses. This includes:

- Chunk metadata passed between pipeline stages
- MCP tool request/response schemas
- Database record representations
- Configuration objects

Use `.model_dump()` and `.model_validate()` — never raw dict access.

## Pipeline Stage Conventions

Each pipeline stage in `src/pipeline/` follows this pattern:

```python
"""Brief description of what this stage does."""

from __future__ import annotations

import argparse
import logging

logger = logging.getLogger(__name__)

def main(args: argparse.Namespace) -> None:
    """Entry point for the pipeline stage."""
    ...

def build_parser() -> argparse.ArgumentParser:
    """Build the CLI argument parser."""
    parser = argparse.ArgumentParser(description="...")
    parser.add_argument("--dry-run", action="store_true",
                        help="preview actions without executing")
    return parser

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    main(build_parser().parse_args())
```

- Stage order: acquire → normalize → chunk → extract → load
- Each stage reads from the previous stage's output
- `--dry-run` flag is required where the stage has side effects
- Progress goes to stdout via `logging.info()`; errors to stderr

## MCP Tool Conventions

When implementing MCP tools under `src/mcp/tools/`:

1. **One tool per module** — each tool lives in its own `.py` file
2. **Descriptive names** — `search_ms_docs_semantic`, `explore_concept_graph`,
   `get_document_chunk`, not `search`, `query`, `get`
3. **Clear descriptions** — the tool description MUST be sufficient for an
   AI agent to decide when to use it without reading source code
4. **Pydantic parameters** — all tool parameters validated via Pydantic models
5. **Markdown responses** — tool output is structured markdown, suitable for
   direct LLM consumption
6. **Localhost only** — the MCP server MUST bind to `127.0.0.1` or `localhost`

## Dependency Management

- All dependencies are listed in `requirements.txt`
- Adding a new dependency requires justification against the constitution's
  Lightweight Footprint principle and Technology Stack Constraints
- Prefer stdlib solutions over new packages
- Pin versions for production dependencies; use `>=` constraints for
  development tools

## Logging Standards

- Use `logging` stdlib — no `print()` in production code
- Configure logging only in entry points (`if __name__ == "__main__":`)
- Library modules use `logger = logging.getLogger(__name__)` only
- Log levels:
  - `DEBUG` — per-document or per-chunk processing details
  - `INFO` — pipeline milestones (stage start/complete, counts)
  - `WARNING` — recoverable issues (skipped files, retry attempts)
  - `ERROR` — failures requiring attention

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
- Reference spec numbers when implementing features: `spec-001: add acquire stage`

## What NOT to Do

- Do NOT add cloud service dependencies (violates Local-First)
- Do NOT use networked database servers (use embedded LanceDB/Kùzu only)
- Do NOT call remote LLM APIs (use local Ollama only)
- Do NOT write raw SQL/Cypher outside `src/db/` modules
- Do NOT skip quality gates or reorder them
- Do NOT use `print()` for logging in production code
- Do NOT construct file paths from unvalidated input
- Do NOT use bare `except:` without specifying exception types
- Do NOT write tests that depend on production database paths
- Do NOT add dependencies without justification