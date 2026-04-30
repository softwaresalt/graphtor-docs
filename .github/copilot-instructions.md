---
description: Shared development guidelines for the graphtor-docs (LocalDocRAG) project.
---
# graphtor-docs Development Guidelines

Last updated: 2026-04-29

## Issue Tracking

This project uses **Backlog.md** via the backlog MCP server for all task and project management.

**Getting started:** Read `backlog://workflow/overview` (MCP resource) or call `backlog.get_workflow_overview()` to understand the full workflow.

**Quick reference:**
- `backlog.list_tasks()` — Find available work (filter by status)
- `backlog.create_task()` — Create a new task
- `backlog.update_task()` — Update task status (e.g., mark done or blocked)
- Tasks are tracked in `backlog.md` and committed with git

## Technology Stack

| Layer | Technology | Justification |
|---|---|---|
| Language | Rust (stable) | Memory safety, single-binary distribution, zero runtime dependencies, native performance |
| Unified Store | CozoDB (`cozo` crate, sqlite backend) | Embedded graph + document DB with Datalog queries and property-graph traversal; HNSW vector search is planned (current search: keyword/text) |
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
- Replacing a stack component requires explicit justification and review.

## Core Principles

### I. Local-First Architecture

All components MUST run locally with zero cloud service dependencies.

- Embedded databases MUST be used for storage (CozoDB for unified graph + vector storage). No networked database servers.
- Embedding inference MUST use the in-process Candle framework running the all-MiniLM-L6-v2 model natively in Rust. No external model servers, no API calls to remote providers.
- Graph extraction MUST use deterministic parsing (pulldown-cmark AST) rather than LLM inference. No LLM is required for the ingestion pipeline.
- The MCP plugin server MUST bind to localhost only (STDIO transport).
- No documentation content or user queries leave the developer's machine.
- The entire system compiles to a single binary with zero runtime dependencies.

### II. Lightweight Footprint

Every dependency and resource choice MUST minimize disk, memory, and CPU consumption.

- The system MUST compile to a single, zero-dependency Rust binary.
- Use embedded/specialized databases over general-purpose servers.
- Use a dynamic `sources.yaml` registry so developers index only the documentation they need.
- Prefer efficient models: `all-MiniLM-L6-v2` (~80MB) for embeddings via the Candle framework.
- Graph relationships are extracted deterministically from Markdown structure — no LLM required.
- Every new crate dependency MUST justify its inclusion.
- For Git sources, use shallow clones (`--depth 1`) via the git2 crate to minimize disk usage.

### III. Data Pipeline Integrity

The ingestion pipeline MUST produce deterministic, reproducible, and verifiable results.

- Every chunk MUST carry a stable `chunk_id` (SHA-256 of content + source path) that serves as the correlation key across vector and graph stores.
- Markdown parsing uses pulldown-cmark's AST event stream to deterministically extract headings, links, and code blocks with 100% precision.
- Normalization steps MUST be idempotent — running them twice on the same input MUST produce identical output.
- Schema changes to CozoDB MUST be versioned and accompanied by a migration path or full rebuild procedure.
- Incremental sync MUST use git commit hashes (for Git sources) and file modification timestamps (for local sources) to identify changed files.

### IV. MCP-Native Interface

All capabilities MUST be exposed exclusively via the Model Context Protocol.

- Each MCP tool MUST have a clear, descriptive name and description that guides AI-agent tool selection.
- Tool responses MUST return structured markdown content suitable for direct LLM consumption.
- MCP is the single interface contract — there MUST NOT be alternative access paths that bypass it for production use.
- Tool parameter schemas MUST be validated; malformed requests MUST return actionable error messages.

### V. Automation & Reproducibility

All data acquisition, processing, and loading MUST be fully scriptable, idempotent, and re-runnable without manual intervention.

- The `sources.yaml` registry defines exactly what documentation the developer wants indexed.
- Clone operations MUST skip repositories that already exist locally.
- The full pipeline (acquire → parse → embed → load) MUST be executable as a single `sync` command.
- The system MUST support incremental updates: track last-processed commit or file mtime, compute diffs, and surgically re-ingest only what changed.
- Generated artifacts (database files, sync state) MUST be reproducible from source inputs alone.

## Project Structure

```text
.github/
  agents/                 # Custom agent definitions (.agent.md files)
  copilot-instructions.md # THIS FILE — shared development guidelines
  instructions/           # Additional instruction files for agents
  prompts/                # Prompt files for agent workflows
  skills/                 # Skill definitions for agent capabilities
.engram/
  templates/              # Agent-harness prompt templates
.scripts/
  clone_ms_docs_repos.py  # Fetch GitHub org repo clone URLs
  comment_out_non_docs.py # Filter non-documentation repos from manifest
  generate_clone_scripts.py # Generate per-group batch clone scripts
  clone-groups/           # Generated .bat scripts (one per doc group)
paths/
  ms-docs.txt             # Raw list of all MicrosoftDocs repo clone URLs
  ms-docs-grouped.txt     # Grouped manifest (38 thematic groups, ~703 repos)
src/                      # Main application source (Rust)
  lib.rs                  # Library crate root (graphtor-core)
  main.rs                 # Binary entry point (graphtor-docs)
  pipeline/               # Ingestion pipeline stages
  mcp/                    # MCP server and tool definitions
  db/                     # Database access layer
    store.rs              # CozoDB DataStore: open, schema, lifecycle
    schema.rs             # Datalog DDL and schema versioning
    chunks.rs             # Chunk upsert operations
    nodes.rs              # Repo/document node CRUD
    edges.rs              # Graph edge insertion
    traverse.rs           # Multi-hop graph traversal
    search.rs             # Text/keyword search (HNSW vector search: planned)
  embed/                  # Candle embedding model
  error/                  # Domain error types
tests/
  integration/            # End-to-end pipeline and database tests
logs/                     # Transient output files (gitignored)
docs/
  adrs/                   # Architectural Decision Records
AGENTS.md                 # Runtime AI-agent development guidance
Cargo.toml                # Workspace manifest and dependencies
README.md                 # Project overview
```

### Entry Points

| Entry Point | Path | Description |
|---|---|---|
| Binary CLI | `src/main.rs` | `graphtor-docs` binary with `clap` subcommands |
| Library Crate | `src/lib.rs` | `graphtor-core` shared types and pipeline logic |
| MCP Server | `src/mcp/` | LocalDocRAG MCP plugin server (STDIO, localhost only) |
| Clone Scripts | `.scripts/clone-groups/*.bat` | Per-group shallow clone orchestration |

## Quality Gates

Every code change must pass these gates in order. Do not skip any gate.

### Gate 1 — Compilation

```powershell
cargo check
```

All code must compile cleanly. Run after every meaningful edit.

### Gate 2 — Lint Compliance

```powershell
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
```

Zero warnings or errors allowed. Fix violations before proceeding.

### Gate 3 — Formatting

```powershell
cargo fmt --all -- --check
```

If violations exist, run `cargo fmt --all` and re-check.

### Gate 4 — Tests

```powershell
cargo test
```

All unit and integration tests must pass. If output may be truncated, redirect:

```powershell
cargo test 2>&1 | Out-File logs\test-results.txt
```

### Gate 5 — TDD/BDD Discipline

When adding new functionality:
1. Write the test first
2. Run it and **confirm it fails** (red)
3. Implement the production code
4. Run the test and confirm it passes (green)

Never write production code before the corresponding test exists and has been observed to fail.

## Code Style and Conventions

### Crate-Level Attributes

* `#![forbid(unsafe_code)]` — no unsafe anywhere (both `src/main.rs` and `ctl/main.rs`)
* `[workspace.lints.rust]`: `unsafe_code = "deny"`, `missing_docs = "warn"`
* `[workspace.lints.clippy]`: `pedantic = "deny"`, `unwrap_used = "deny"`, `expect_used = "deny"`

### Error Handling

* Define domain errors using `thiserror` in `src/error/` module
* Error variants: `ConfigError`, `DatabaseError`, `PipelineError`, `ExtractionError`, `SchemaError`, `PathViolationError`, `ChunkIntegrityError`
* Map external errors via `From` impls or explicit `.map_err()`
* Never use `unwrap()` or `expect()` in library code
* Error messages are lowercase and do not end with a period
* Use `anyhow` for ad-hoc error propagation in binary entry points only

### Naming

* Module files: `src/{module}/mod.rs` pattern for directories
* Struct IDs: prefixed strings (`task:uuid`, `context:uuid`, `spec:uuid`)
* Status values: `snake_case` (`todo`, `in_progress`, `done`, `blocked`)
* Default visibility: `pub(crate)` unless the item needs to be public API

### Documentation

* All public items require `///` doc comments



* Module-level `//!` doc comments on every `mod.rs` or standalone module file

### Imports

* Group imports: `std` → external crates → crate-local (`use crate::...`)
* Prefer explicit imports over glob imports
* Use `pub(crate)` for internal visibility; `pub` only for public API items

### Database Access

* All CozoDB operations go through `src/db/` — no raw Datalog queries outside this module
* Sub-modules: `store.rs` (lifecycle), `schema.rs` (DDL), `chunks.rs`, `nodes.rs`, `edges.rs`, `traverse.rs`, `search.rs`
* All sub-modules expose typed functions, not raw query strings
* Test databases use temporary directories — never write to production paths
* Schema definitions are versioned; migrations or full-rebuild procedures accompany any schema change

### Path Security

* All file paths MUST be resolved and validated against the workspace or data root
* Use `std::fs::canonicalize()` and verify the path `starts_with(allowed_root)`
* Reject paths outside allowed directories — return `PathViolationError`
* Never construct file paths from user input without validation

### MCP Tools

* Each tool is a separate module under `src/mcp/tools/`
* Tool definitions use `#[tool]`/`#[tool_router]` macros from `rmcp` 1.5
* Tool names use `snake_case` with descriptive prefixes (`search_ms_docs_semantic`, `explore_concept_graph`)
* Tool descriptions MUST be clear enough for an AI agent to select the right tool
* All tool parameters validated via typed Rust structs with serde
* Tool responses return structured markdown suitable for LLM consumption
* The MCP server binds to `localhost` only (STDIO transport) — no external network exposure

### Pipeline Stages

* Each pipeline stage is a module under `src/pipeline/`
* Stages MUST be idempotent — safe to re-run on the same input
* Stage order: acquire → parse → embed → load
* Use `clap` subcommands for CLI invocation
* Progress output via `tracing::info!`; errors via `tracing::error!`

### Logging

* Use the `tracing` crate — no `println!` in production code
* Configure `tracing-subscriber` in entry points only (`src/main.rs`)
* Library modules use `tracing::info!`, `tracing::debug!`, etc.
* Log levels: `DEBUG` for pipeline step details, `INFO` for progress milestones, `WARN` for recoverable issues, `ERROR` for failures
* Include context in structured fields: `tracing::info!(count, source_path, "chunked documents")`

### Serialization

* All data structures that cross module boundaries MUST derive `serde::Serialize` and `serde::Deserialize`
* Use `serde_yaml` for `sources.yaml` configuration parsing
* Use `serde_json` for `.sync_state.json` and MCP JSON-RPC transport

## Architecture Reference

| Concern | Approach |
|---|---|
| Unified storage | CozoDB (embedded, sqlite backend) — Datalog queries, property-graph traversal; HNSW vector search planned |
| Embeddings | `all-MiniLM-L6-v2` via Candle (in-process, 384-dim vectors) |
| Graph extraction | `pulldown-cmark` (deterministic AST-based Markdown parsing) |
| MCP interface | `rmcp` crate — async STDIO JSON-RPC, localhost-only |
## Remote Approval Workflow for Destructive File Operations
When the agent-intercom MCP server is running, agents may write files directly for creation and modification. The remote approval workflow is reserved for **destructive operations only** — file deletion, directory removal, or any operation that permanently removes content from the filesystem. This allows the operator to review and approve destructive changes via Slack before they execute.
Additionally, **do not write multiple files in a single proposal.** Each destructive operation must be proposed, reviewed, and approved separately to ensure clear audit trails and granular control.
For terminal commands, **never chain multiple commands together**. Each command must be submitted separately to the `evaluate_command` tool for proper policy evaluation and approval. If the terminal command is **not** already auto-approved for the current workspace or current working directory, it may be executed directly without approval, but still must not be chained with other commands unless those commands are effectively piping output.
### Required Call Sequence (Destructive Operations Only)
```text
1. auto_check       →  Can this destructive operation bypass approval?
2. check_clearance   →  Submit the proposal (blocks until operator responds)
3. check_diff        →  Execute the approved destructive operation
```
### Step 1 — `auto_check`
Call **before** every destructive file operation (deletion, directory removal) to check if the workspace policy allows the operation without human review.
| Parameter   | Type     | Required | Description |
|-------------|----------|----------|-------------|
| `tool_name` | `string` | yes      | Name of the destructive operation being executed |
| `context`   | `object` | no       | `{ "file_path": "...", "risk_level": "..." }` |
- If `auto_approved: true` → the agent may write the file directly (skip steps 2–3).
- If `auto_approved: false` → proceed to step 2.
### Step 2 — `check_clearance`
Submit the proposed destructive operation for operator review. This call **blocks** until the operator taps Accept/Reject in Slack or the timeout elapses.
| Parameter     | Type     | Required | Description |
|---------------|----------|----------|---------------------------------------------------------------------------------------|
| `title`       | `string` | yes      | Concise summary of the proposed change |
| `diff`        | `string` | yes      | Standard unified diff or full file content |
| `file_path`   | `string` | yes      | Target file path relative to workspace root |
| `description` | `string` | no       | Additional context about the change |
| `risk_level`  | `string` | no       | `low` (default), `high`, or `critical` |
| `snippets`    | `array`  | no       | Curated code excerpts for inline Slack review (see below) |
**`snippets` array** — each element has:
- `label` (string, required) — short human-readable title, e.g. `"handle() — main entry point"`
- `language` (string, optional) — markdown code-fence language, e.g. `"rust"`, `"toml"`
- `content` (string, required) — the code to display (server truncates at 2,600 chars)
When `snippets` is provided, the server posts them as a **threaded Slack message** using inline code blocks, which Slack always renders as readable text. This is the preferred approach for all `check_clearance` calls: curate the 1–4 most meaningful sections of the affected file (changed functions, modified public APIs, key callers) rather than relying on the server to upload the whole file. See the build-feature skill for full curation guidance.
Two key conventions apply to every snippet:
- **Function-boundary scoping**: each snippet must span one complete function or method — from its signature to its closing delimiter. Never include a partial function even if only one line changed.
- **Changed-line annotation**: Slack code blocks render all content as literal text (`**bold**` becomes asterisks). Annotate changed lines with inline comments instead: `// ← new`, `// ← modified`, `// ← deleted` (or `#`, `--`, `<!-- -->` for Python/SQL/HTML respectively).
**Response:** `{ "status": "approved" | "rejected" | "timeout", "request_id": "...", "reason": "..." }`
- `approved` → proceed to step 3 with the returned `request_id`.
- `rejected` → do **not** apply the change. Adapt or abandon based on the `reason`.
- `timeout` → treat as rejection. Do not retry automatically without operator guidance.

### Step 3 — `check_diff`

Execute the approved destructive operation. Only call this after receiving `status: "approved"`.
| Parameter    | Type      | Required | Description |
|--------------|-----------|----------|-------------|
| `request_id` | `string`  | yes      | The `request_id` from the `check_clearance` response |
| `force`      | `boolean` | no       | `true` to overwrite even if the file changed since proposal |
**Response:** `{ "status": "applied", "files_written": [{ "path": "...", "bytes": N }] }`
If the server returns `patch_conflict` (file changed since proposal), the agent should re-read the file, regenerate the diff, and restart from step 2.
### Rules
1. **File creation and modification proceed directly** when the MCP server is reachable. No approval workflow is needed for non-destructive writes.
2. **Broadcast every file change.** After each non-destructive file write, call `broadcast` at `info` level with `[FILE] {action}: {file_path}` (where `action` is `created` or `modified`) and include the unified diff (for modifications) or full file content (for new files) in the message body. These broadcasts are non-blocking and keep the operator informed in real time.
3. **Destructive operations require approval.** File deletion, directory removal, or any operation that permanently removes content must go through the `auto_check` → `check_clearance` → `check_diff` workflow.
4. **One destructive operation per approval.** Submit each deletion or removal as a separate `check_clearance` call.
5. **Set `risk_level`** to `high` or `critical` for destructive operations targeting configuration files, security-sensitive modules (`diff/path_safety.rs`, `policy/`, `slack/events.rs`), or database schema (`persistence/schema.rs`).
6. **Do not retry rejected proposals** with the same content. Incorporate the operator's feedback first.
7. **Handle all response statuses.** Never assume approval — always branch on `approved`, `rejected`, and `timeout`.
## Destructive Terminal Command Approval (NON-NEGOTIABLE)
**All destructive terminal commands MUST go through agent-intercom operator approval regardless of whether the agent is running in `--allow-all`, `--yolo`, or any other permissive mode.** This rule has no exceptions and cannot be overridden by agent configuration, workspace policy, or auto-approve rules.
### Definition of Destructive Terminal Commands
A terminal command is considered **destructive** if it:
- Deletes files or directories (`rm`, `Remove-Item`, `del`, `rmdir`)
- Overwrites files without creating backups (`mv` to existing target, `Move-Item -Force`)
- Modifies system configuration (`reg`, `Set-ExecutionPolicy`, `chmod`, `chown`)
- Alters version control history (`git reset --hard`, `git push --force`, `git clean -fd`)
- Drops or truncates database content (`DROP TABLE`, `TRUNCATE`, `DELETE FROM` without `WHERE`)
- Installs or removes system-level packages (`npm install -g`, `cargo install`, `apt remove`)
- Executes arbitrary code from untrusted sources (`curl | sh`, `iex (irm ...)`)
### Required Workflow
1. **Detect**: Before executing any terminal command, evaluate whether it is destructive per the definition above.
2. **Route through agent-intercom**: If destructive, call `auto_check` with the full command string. If not auto-approved, call `check_clearance` with:
   - `title`: The command being proposed
   - `description`: Why the command is needed and what it will affect
   - `risk_level`: `high` for most destructive commands, `critical` for force-pushes, database drops, or system config changes
3. **Execute only after approval**: Only run the command after receiving `status: "approved"` from the operator.
4. **Never bypass**: Even if `--allow-all` or `--yolo` flags are active, destructive terminal commands MUST still go through this approval workflow. These flags only affect non-destructive operations.
### Rationale
Permissive agent modes (`--allow-all`, `--yolo`) exist to reduce friction for routine operations like file creation, modification, and safe build/test commands. They must NEVER extend to destructive terminal operations because:
- A single misrouted destructive command can irrecoverably corrupt repositories, delete production data, or break system configuration.
- Agents operating autonomously for extended periods may accumulate context drift that leads to incorrect destructive actions.
- The operator retains final authority over any operation that permanently removes or alters critical resources.
<!-- MANUAL ADDITIONS START -->
## Terminal Command Execution Policy

**Do NOT chain terminal commands.** Run each command as a separate, standalone invocation.

### Rules

1. **One command per terminal call.** NEVER, NEVER chain or combine commands with `;`, `&&`, `||`, or `|` unless it falls under an allowed exception below.
2. **No `cmd /c` wrappers.** Run commands directly in the shell rather than wrapping them in `cmd /c "..."`. If `cmd /c` is genuinely required (e.g., for environment isolation), it must contain a single command only.
3. **No exit-code echo suffixes.** Do not append `; echo "EXIT: $LASTEXITCODE"` or `&& echo "done"` to commands. The terminal tool already captures exit codes.
4. **Check results between commands.** After each command, inspect the output and exit code before deciding whether to run the next command. This is safer and produces better diagnostics.
5. **Always use `pwsh`, never `powershell`.** When invoking PowerShell explicitly (e.g., to run a `.ps1` script), use `pwsh` — the cross-platform PowerShell 7+ executable. Never use `powershell` or `powershell.exe`, which refers to the legacy Windows PowerShell 5.1 runtime.
6. **Always use relative paths for output redirection.** When redirecting command output to a file, use workspace-relative paths (e.g., `logs\results.txt`), never absolute paths (e.g., `d:\Source\...\logs\results.txt`). Absolute paths break auto-approve regex matching.
7. **Temporary output files go in `logs/`.** All temporary output files — compilation logs, test results, clippy output, diff captures, or any other ephemeral terminal output redirected to disk — must be written to the `logs/` folder, never to `target/` or the workspace root. The `logs/` folder is gitignored and designated for transient artifacts. Example: `cargo test 2>&1 | Out-File logs\test-results.txt`.

### Allowed Exceptions

Output redirection is **not** command chaining — it is I/O plumbing that cannot execute destructive operations. The following patterns are permitted:

- **Shell redirection operators**: `>`, `>>`, `2>&1` (e.g., `cargo test > logs/results.txt 2>&1`)
- **Pipe to `Out-File` or `Set-Content`**: `cargo test 2>&1 | Out-File logs/results.txt` or `| Set-Content`
- **Pipe to `Out-String`**: `some-command | Out-String`

Use these when the terminal tool's ~60 KB output limit would truncate results (e.g., full `cargo test` compilation + test output).
### Why
Terminal auto-approve rules use regex pattern matching against the full command line. Chained commands create unpredictable command strings that cannot be reliably matched, forcing manual approval prompts that slow down the workflow. Single commands match cleanly and approve instantly.
### Correct Examples

```powershell
# Good: separate calls
cargo check
# (inspect output)
cargo clippy -- -D warnings
# (inspect output)
cargo test

# Good: output redirection to capture full results
cargo test 2>&1 | Out-File logs\test-results.txt
# Good: shell redirect when output may be truncated
cargo test > logs\test-results.txt 2>&1
```

### Incorrect Examples

```powershell
# Bad: chained with semicolons
cargo check; cargo clippy -- -D warnings; cargo test

# Bad: cmd /c wrapper with echo suffix
cmd /c "cargo test > logs\test-results.txt 2>&1"; echo "EXIT: $LASTEXITCODE"
# Bad: output redirect to target/ instead of logs/
cargo test 2>&1 | Out-File target\test-results.txt
# Bad: AND-chained
cargo fmt && cargo clippy && cargo test

# Bad: pipe to something other than Out-File/Set-Content/Out-String
cargo test | Select-String "FAILED" | Remove-Item foo.txt
```

### Full List of Auto-Approve Commands with RegEx

```json
"chat.tools.terminal.autoApprove": {
    ".engram/": true,
    "/^cargo (build|test|run|clippy|fmt|check|doc|update|install|search|publish|login|logout|new|init|add|upgrade|version|help|bench)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^& cargo (build|test|run|clippy|fmt|check|doc|update|install|search|publish|login|logout|new|init|add|upgrade|version|help|bench)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^cargo --(help|version|verbose|quiet|release|features)(\\s[^;|&`]*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^git (status|add|commit|diff|log|fetch|pull|push|checkout|branch|--version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^& git (status|add|commit|diff|log|fetch|pull|push|checkout|branch|--version)(\\s[^;|&`]*)?(\\s*(>|>>|2>&1|\\|\\s*(Out-File|Set-Content|Out-String))\\s*[^;|&`]*)*$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^(Out-File|Set-Content|Add-Content|Get-Content|Get-ChildItem|Copy-Item|Move-Item|New-Item|Test-Path)(\\s[^;|&`]*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^(echo|dir|mkdir|where\\.exe|vsWhere\\.exe|rustup|rustc|refreshenv)(\\s[^;|&`]*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "/^cmd /c \"cargo (test|check|clippy|fmt|build|doc|bench)(\\s[^;|&`]*)?\"(\\s*[;&|]+\\s*echo\\s.*)?$/": {
        "approve": true,
        "matchCommandLine": true
    },
    "New-Item": true,
    "Out-Null": true,
    "ForEach-Object": true
}
```
<!-- MANUAL ADDITIONS END -->
