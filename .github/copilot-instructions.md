---
description: Shared development guidelines for the graphtor-docs (LocalDocRAG) project.
---
# graphtor-docs Development Guidelines

Last updated: 2026-03-08

## Active Technologies

| Dependency | Version | Purpose |
|---|---|---|
| Python | 3.11+ | Language runtime — required by MCP SDK, LanceDB, and Kùzu bindings |
| `lancedb` | latest | Embedded columnar vector database (PyArrow integration) |
| `kuzu` | latest | Embedded property-graph engine (Cypher query support) |
| `ollama` | latest | Local LLM inference server for embeddings and extraction |
| `nomic-embed-text` | — | Embedding model (8k context) served via Ollama |
| `phi-4` / `llama-3.2` | — | Small-footprint extraction LLMs served via Ollama |
| `mcp` | latest | Python MCP SDK — tool registration, server lifecycle |
| `requests` | latest | HTTP client for GitHub API and external fetches |
| `ruff` | latest | Linting and formatting (replaces flake8, isort, black) |
| `mypy` | latest | Static type checking (strict mode) |
| `pytest` | latest | Test framework |
| `pytest-cov` | latest | Coverage reporting |
| `pydantic` | 2.x | Data validation, settings, and schema enforcement |
| `pyarrow` | latest | Columnar data interchange (LanceDB dependency) |
| `uuid` (stdlib) | — | Chunk and entity ID generation |
| `logging` (stdlib) | — | Structured logging (stdlib) |
| `pathlib` (stdlib) | — | Path manipulation and safety |
| `argparse` (stdlib) | — | CLI argument parsing for pipeline scripts |

## Project Structure

```text
.github/
  agents/                 # Custom agent definitions (.agent.md files)
  copilot-instructions.md # THIS FILE — shared development guidelines
  instructions/           # Additional instruction files for agents
  prompts/                # Prompt files for agent workflows
  skills/                 # Skill definitions for agent capabilities
.scripts/
  clone_ms_docs_repos.py  # Fetch GitHub org repo clone URLs
  comment_out_non_docs.py # Filter non-documentation repos from manifest
  generate_clone_scripts.py # Generate per-group batch clone scripts
  clone-groups/           # Generated .bat scripts (one per doc group)
.specify/
  memory/
    constitution.md       # Project constitution (authoritative principles)
  templates/              # Spec, plan, and task templates
  scripts/                # Specify workflow scripts
paths/
  ms-docs.txt             # Raw list of all MicrosoftDocs repo clone URLs
  ms-docs-grouped.txt     # Grouped manifest (38 thematic groups, ~703 repos)
specs/
  001-doc-ingestion-pipeline/ # Feature specifications
src/                      # (planned) Main application source
  pipeline/               # (planned) Ingestion pipeline stages
    acquire.py            # Repository cloning and update management
    normalize.py          # Markdown cleaning: frontmatter strip, UI-tag removal
    chunk.py              # Document chunking with stable chunk_id (UUID)
    extract.py            # LLM-powered graph extraction (JSON schema enforced)
    load.py               # Vector and graph database loading
  mcp/                    # (planned) MCP server and tool definitions
    server.py             # MCP server bootstrap and lifecycle
    tools/                # Individual MCP tool handlers
  models/                 # (planned) Pydantic domain models
  db/                     # (planned) Database access layer
    vector.py             # LanceDB operations
    graph.py              # Kùzu operations
tests/
  unit/                   # Isolated logic tests
  integration/            # End-to-end pipeline and database tests
  conftest.py             # Shared fixtures (temp dirs, test DBs)
logs/                     # Transient output files (gitignored)
AGENTS.md                 # Runtime AI-agent development guidance
README.md                 # Project overview
requirements.txt          # Python dependencies
```

### Entry Points

| Entry Point | Path | Description |
|---|---|---|
| Pipeline CLI | `src/pipeline/*.py` | Individual pipeline stage scripts with `argparse` CLIs |
| MCP Server | `src/mcp/server.py` | LocalDocRAG MCP plugin server (localhost only) |
| Clone Scripts | `.scripts/clone-groups/*.bat` | Per-group shallow clone orchestration |
| Repo Fetcher | `.scripts/clone_ms_docs_repos.py` | GitHub API repo URL discovery |

## Quality Gates

Every code change must pass these gates in order. Do not skip any gate.

### Gate 1 — Type Checking

```powershell
mypy src/ tests/ .scripts/ --strict
```

All code must pass strict type checking. Run after every meaningful edit. Use type annotations on all function signatures and module-level variables.

### Gate 2 — Lint Compliance

```powershell
ruff check src/ tests/ .scripts/
```

Zero warnings or errors allowed. Ruff configuration targets: pyflakes, pycodestyle, isort, pydocstyle, and pylint rules. Fix violations before proceeding.

### Gate 3 — Formatting

```powershell
ruff format --check src/ tests/ .scripts/
```

If violations exist, run `ruff format src/ tests/ .scripts/` and re-check. Line length: 100 characters.

### Gate 4 — Tests

```powershell
pytest tests/ -v
```

All unit and integration tests must pass. If output may be truncated, redirect:

```powershell
pytest tests/ -v 2>&1 | Out-File logs\test-results.txt
```

### Gate 5 — Coverage

```powershell
pytest tests/ --cov=src --cov-report=term-missing
```

New code must have test coverage. Coverage reports go to stdout or `logs/coverage.txt`.

### Gate 6 — TDD/BDD Discipline

When adding new functionality:
1. Write the test first
2. Run it and **confirm it fails** (red)
3. Implement the production code
4. Run the test and confirm it passes (green)

Never write production code before the corresponding test exists and has been observed to fail.

## Code Style and Conventions

### Type Annotations

* All function signatures MUST have type annotations (parameters and return types)
* Use `from __future__ import annotations` at the top of every module
* Use `typing` module types for complex generics (`dict[str, list[str]]`, `Optional[T]`, etc.)
* Prefer `X | None` over `Optional[X]` (Python 3.10+ union syntax)

### Error Handling

* Define domain exceptions in a central `src/errors.py` module
* Custom exception hierarchy rooted in a base `LocalDocRAGError`
* Exception variants: `ConfigError`, `DatabaseError`, `PipelineError`, `ExtractionError`, `SchemaError`, `PathViolationError`, `ChunkIntegrityError`
* Map external errors via explicit `try/except` with re-raise as domain exceptions
* Never use bare `except:` — always catch specific exception types
* Error messages are lowercase and do not end with a period
* Use `logging.exception()` for unexpected errors to capture tracebacks

### Naming

* Modules: `snake_case.py`
* Classes: `PascalCase`
* Functions and variables: `snake_case`
* Constants: `UPPER_SNAKE_CASE`
* Private members: single leading underscore `_internal_method()`
* Chunk IDs: UUID4 strings (`chunk_id = str(uuid.uuid4())`)
* Database table/collection names: `snake_case` (e.g., `doc_chunks`, `concept_nodes`)

### Documentation

* All public functions, classes, and modules require docstrings
* Use Google-style docstrings:
  ```python
  def process_chunk(content: str, source_path: str) -> ChunkResult:
      """Process a markdown chunk for vector and graph ingestion.

      Args:
          content: Raw markdown content of the chunk.
          source_path: Filesystem path to the source document.

      Returns:
          ChunkResult with embedding vector and extracted entities.

      Raises:
          ChunkIntegrityError: If the chunk fails validation.
      """
  ```
* Module-level docstrings on every `.py` file explaining its purpose

### Imports

* Sort with `ruff` (isort-compatible): stdlib → third-party → local
* Use absolute imports for cross-module references (`from src.models.chunk import Chunk`)
* Use `from __future__ import annotations` as the first import in every module

### Database Access

* All LanceDB operations go through `src/db/vector.py` — no raw LanceDB calls elsewhere
* All Kùzu operations go through `src/db/graph.py` — no raw Cypher queries elsewhere
* Both modules expose typed functions, not raw query strings
* Test databases use temporary directories (`tmp_path` fixture) — never write to production paths
* Schema definitions are versioned; migrations or full-rebuild procedures accompany any schema change

### Path Security

* All file paths MUST be resolved and validated against the workspace or data root
* Use `pathlib.Path.resolve()` and verify `path.is_relative_to(allowed_root)`
* Reject paths outside allowed directories — raise `PathViolationError`
* Never construct file paths from user input without validation

### MCP Tools

* Each tool is a separate module under `src/mcp/tools/`
* Tool functions are decorated with the MCP SDK's tool registration
* Tool names use `snake_case` with descriptive prefixes (`search_ms_docs_semantic`, `explore_concept_graph`)
* Tool descriptions MUST be clear enough for an AI agent to select the right tool
* All tool parameters validated via Pydantic models
* Tool responses return structured markdown suitable for LLM consumption
* The MCP server binds to `localhost` only — no external network exposure

### Pipeline Scripts

* Each pipeline stage is a standalone Python script with `argparse` CLI
* Scripts MUST be idempotent — safe to re-run on the same input
* Progress output goes to stdout; errors go to stderr via `logging`
* Use `if __name__ == "__main__":` guard on all scripts
* Each script MUST have a `--dry-run` flag where applicable

### Logging

* Use the `logging` stdlib module — no print statements in production code
* Configure logging in entry points only, not in library modules
* Log levels: `DEBUG` for pipeline step details, `INFO` for progress milestones, `WARNING` for recoverable issues, `ERROR` for failures
* Include context in log messages: `logger.info("chunked %d documents from %s", count, source_path)`

### Pydantic Models

* All data structures that cross module boundaries MUST be Pydantic `BaseModel` subclasses
* Use `model_validator` for cross-field validation
* Use `Field(description="...")` for MCP-facing schemas
* Serialization: use `.model_dump()` and `.model_validate()` — never raw dict access on validated data

## Architecture Reference

| Concern | Approach |
|---|---|
| Vector storage | LanceDB (embedded) — columnar with PyArrow, zero-server |
| Graph storage | Kùzu (embedded) — property graph with Cypher queries |
| Embeddings | `nomic-embed-text` via Ollama (local, 8k context) |
| Graph extraction | `phi-4` or `llama-3.2` via Ollama (local, JSON schema enforced) |
| MCP interface | Python MCP SDK — tool registration, localhost-only server |
| Pipeline stages | Standalone Python scripts: acquire → normalize → chunk → extract → load |
| Configuration | Environment variables + CLI args (no config files yet) |
| Data manifest | `paths/ms-docs-grouped.txt` — 38 thematic groups, ~703 repos |
| Clone orchestration | Generated `.bat` scripts under `.scripts/clone-groups/` |
| Chunk correlation | UUID4 `chunk_id` as correlation key across vector and graph stores |
| Path security | `pathlib.Path.resolve()` + `is_relative_to()` validation |
| Testing | `pytest` — unit tests (isolated) + integration tests (real DBs in temp dirs) |
| Constitution | `.specify/memory/constitution.md` — authoritative architectural principles |
| Agent guidance | `AGENTS.md` — runtime AI-agent development conventions |

## Terminal Command Execution Policy

**Do NOT chain terminal commands.** Run each command as a separate, standalone invocation.

### Rules

1. **One command per terminal call.** NEVER chain or combine commands with `;`, `&&`, `||`, or `|` unless it falls under an allowed exception below.
2. **No exit-code echo suffixes.** Do not append `; echo "EXIT: $LASTEXITCODE"` or `&& echo "done"` to commands. The terminal tool already captures exit codes.
3. **Check results between commands.** After each command, inspect the output and exit code before deciding whether to run the next command.
4. **Always use `pwsh`, never `powershell`.** When invoking PowerShell explicitly, use `pwsh` — the cross-platform PowerShell 7+ executable.
5. **Always use relative paths for output redirection.** Use workspace-relative paths (e.g., `logs\results.txt`), never absolute paths.
6. **Temporary output files go in `logs/`.** All temporary output files — test results, lint output, type-check reports — must be written to the `logs/` folder. The `logs/` folder is gitignored and designated for transient artifacts.

### Allowed Exceptions

Output redirection is **not** command chaining — it is I/O plumbing. The following patterns are permitted:

- **Shell redirection operators**: `>`, `>>`, `2>&1` (e.g., `pytest tests/ > logs/results.txt 2>&1`)
- **Pipe to `Out-File` or `Set-Content`**: `pytest tests/ -v 2>&1 | Out-File logs/test-results.txt`
- **Pipe to `Out-String`**: `some-command | Out-String`

### Correct Examples

```powershell
# Good: separate calls
mypy src/ tests/ --strict
# (inspect output)
ruff check src/ tests/
# (inspect output)
pytest tests/ -v

# Good: output redirection to capture full results
pytest tests/ -v 2>&1 | Out-File logs\test-results.txt
```

### Incorrect Examples

```powershell
# Bad: chained with semicolons
mypy src/ --strict; ruff check src/; pytest tests/

# Bad: AND-chained
ruff format --check src/ && ruff check src/ && pytest tests/

# Bad: output redirect to wrong location
pytest tests/ -v 2>&1 | Out-File target\test-results.txt
```

### Full List of Auto-Approve Commands with RegEx

```json
"chat.tools.terminal.autoApprove": {
    ".specify/scripts/bash/": true,
    ".specify/scripts/powershell/": true,
    "/^python(3)?(\\.exe)?\\s+[^;|&`]+(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^& python(3)?(\\.exe)?\\s+[^;|&`]+(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^(pip|pip3)\\s+(install|uninstall|list|show|freeze|check|search|download|wheel|hash|config|debug|inspect)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^pytest(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^mypy(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^ruff\\s+(check|format|rule|linter|version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^ollama\\s+(list|show|pull|run|ps|serve|help|version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^git\\s+(status|add|commit|diff|log|fetch|pull|push|checkout|branch|--version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^& git\\s+(status|add|commit|diff|log|fetch|pull|push|checkout|branch|--version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^(Out-File|Set-Content|Add-Content|Get-Content|Get-ChildItem|Copy-Item|Move-Item|New-Item|Test-Path)(\\s[^;|&`]*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^(echo|dir|mkdir|where\\.exe)(\\s[^;|&`]*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "pip": true,
    "pip install": true,
    "pytest": true,
    "mypy": true,
    "ruff check": true,
    "ruff format": true,
    "git add": true,
    "git commit": true,
    "git push": true,
    "New-Item": true,
    "Out-Null": true,
    "ForEach-Object": true
}
```