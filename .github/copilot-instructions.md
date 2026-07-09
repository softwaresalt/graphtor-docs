# graphtor-docs Development Guidelines

Last updated: 2026-05-20

graphtor-docs is a documentation cross-reference system that acquires, parses, chunks, embeds, and indexes documentation from multiple sources (Git repos, local directories, web URLs), serving semantic search queries via MCP tools over STDIO transport.

## Technology Stack

| Layer           | Technology                | Notes                                 |
|-----------------|---------------------------|---------------------------------------|
| Language        | Rust edition 2021, rust-version 1.75 | Rust 2021 edition with MSRV 1.75          |
| Build           | cargo            | `cargo build`                   |
| Test            | cargo test           | `cargo test`                    |
| Lint            | clippy                | `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`                    |
| Format          | rustfmt             | `cargo fmt --all`                  |
| CI              | GitHub Actions           | fmt → clippy → test → audit pipeline                          |
| MCP SDK       | rmcp                      | STDIO transport, Rust-native                  |
| Async Runtime | tokio                     | Multi-threaded async I/O                      |
| CLI           | clap                      | Derive-based argument parsing                 |
| ML Inference  | candle                    | In-process all-MiniLM-L6-v2 embeddings        |
| Vector DB     | lancedb                   | Embedded vector similarity search              |
| Graph DB      | kuzu                      | Embedded property graph for doc relationships  |

## Project Structure

```text
src/                    # Application source
  acquire/              # Documentation source acquisition (git, local, web)
  chunk/                # Heading-based document chunking
  config/               # Configuration loading and validation
  error/                # Error type definitions (thiserror)
  logging/              # Structured logging setup
  path/                 # Path resolution and normalization
tests/                  # Integration tests
scripts/                # Build and automation scripts
docs/                   # Documentation root
  adrs/                 # Architecture Decision Records
  compound/             # Institutional learnings
  decisions/            # Decision outcomes
  exec-plans/           # Implementation plans
  memory/               # Session checkpoints
  closure/              # Release closure artifacts
  design-docs/          # Graduated design rationale
  product-specs/        # Product requirements
.github/                # Agent harness artifacts
  agents/               # Agent definitions
  skills/               # Skill workflows
  instructions/         # Scoped instruction files
  policies/             # Workflow policy registry
  prompts/              # Prompt templates
```

## Commands

```bash
cargo build              # Build
cargo test               # Run all tests
cargo clippy --all-targets -- -D warnings -D clippy::pedantic               # Lint
cargo fmt --all             # Format check
cargo check --all-targets       # Fast compilation check
cargo doc --no-deps             # Generate documentation
cargo audit                     # Security audit
```

## Code Style and Conventions

### Error Handling

* `thiserror` for library error types with structured error enums
* `anyhow` for binary crate and test error propagation
* Use `?` with `.context()` / `.with_context()` for error propagation
* No `.unwrap()` or `.expect()` in library code without a proof comment

### Naming

* `snake_case` for variables, functions, methods, modules, and crate names
* `PascalCase` for types, traits, and enum variants
* `SCREAMING_SNAKE_CASE` for constants and static variables
* Module filenames match module names (e.g., `acquire.rs` for `mod acquire`)

### Documentation

* `///` doc comments required on all public functions, types, traits, and modules
* `//!` module-level doc comments on every module file
* Include usage examples in doc comments for complex public APIs

### Testing

* TDD required: write tests first, verify they fail, then implement
* Test tiers in `tests` directory:
  * Unit tests: colocated `#[cfg(test)] mod tests` in source modules
  * Integration tests: `tests/` directory with per-module test files

## Search Strategy

Use available workspace search tools before falling back to file-based search
(grep, glob, view). Indexed search returns precise results with minimal token
cost. File-based tools read raw content into the context window, consuming
tokens proportional to file size.

**Search tool preference order:**

1. When the `agent-engram` capability pack is enabled and reachable: `unified_search`, `query_memory`, `map_code`, `list_symbols`, `impact_analysis`, `query_graph`
2. Otherwise use workspace-indexed tools (if available): semantic search, symbol lookup, call graphs
3. File-based fallback: grep, glob, view — only when indexed results are insufficient

## Durable Knowledge Layout

| Path | Purpose |
|---|---|
| `docs/compound/` | Reusable learnings and hard-won fixes |
| `docs/exec-plans/` | Implementation plans |
| `docs/decisions/` | Durable decisions and investigation outputs |
| `docs/memory/` | Session memory and checkpoints |
| `docs/closure/` | Review, runtime verification, and closure artifacts |
| `docs/design-docs/` | Graduated architecture and design rationale |
| `docs/product-specs/` | Product-oriented requirements |

## Session Memory Requirements

* Working agent sessions MUST persist output to `docs/memory/` before the session ends — do NOT rely on built-in AI assistant memory features, which write to their own managed locations.
* When the context window reaches approximately 65% capacity, checkpoint current work before continuing.
* For long sessions, save memory checkpoints after completing each phase or major task group.
* Content to capture: task IDs completed, files modified, decisions and rationale, failed approaches, open questions, and next steps.
* File convention: `docs/memory/{YYYY-MM-DD}/{descriptive-slug}-memory.md`
* After writing memory, invoke the **compact-context** skill to consolidate stale checkpoints and finalize decided-plans. This is a mandatory workflow step, not advisory.
* If context has grown from loading multiple skill definitions mid-session, consider invoking **compact-context** proactively before hitting hard thresholds.

## Foundational Protocols

| Protocol | Location | When |
|---|---|---|
| **Circuit Breaker** | `.github/instructions/circuit-breaker.instructions.md` | All retry loops and failure handling |
| **Concurrency Control** | `.github/instructions/concurrency.instructions.md` | Multi-agent or human+agent concurrent edits |
| **Skill Discovery** | `scripts/search.ps1` / `scripts/search.sh` | Finding capabilities by keyword (Primitive 6) |

## Optional Capability Packs

### agent-intercom

When the workspace enabled the `agent-intercom` capability pack:

* verify the intercom server / tool surface is reachable before depending on remote approval or operator steering
* call heartbeat / ping at session start and keep it alive during long-running work
* broadcast major workflow transitions so the operator can observe planning, build, review, verification, and closure progress
* route destructive terminal commands and destructive file operations through the intercom approval workflow
* use transmit / standby flows when blocked on operator clarification or when intentionally pausing for instructions
* if the intercom service is unreachable, warn that remote visibility is degraded and avoid pretending approval or operator awareness exists

### agent-engram

When the workspace enabled the `agent-engram` capability pack:

* verify the engram daemon / MCP surface is reachable before depending on indexed lookup
* prefer engram tools for conceptual search, symbol discovery, call-graph lookup, impact analysis, and workspace-memory retrieval
* verify the workspace binding state before relying on results; if the daemon auto-binds the workspace, prefer status checks over repeated rebinding
* use `sync_workspace` or the equivalent freshness operation when code changed outside the expected indexing flow
* if semantic search is unavailable or degraded, fall back to `list_symbols` + `map_code` + `impact_analysis` before resorting to broad file scans
* treat `.engram/` generated artifacts as tool-managed state rather than files to hand-edit casually

### backlogit

When the workspace enabled the `backlogit` capability pack:

* verify the backlogit MCP / CLI surface is reachable before depending on queue, dependency, memory, or traceability operations
* prefer backlogit query operations for targeted state lookup instead of reading many backlog markdown files into context
* use backlogit queue and dependency operations when available rather than inferring execution order from prose alone
* write concise memory summaries and checkpoints through backlogit operations at task and session boundaries when supported
* append significant task comments and associate commits with task IDs for execution traceability when those operations are available
* if backlog content was edited outside the normal mutation flow, refresh the backlogit index before relying on query results

### browser-verification

When the workspace enabled the `browser-verification` capability pack:

* verify the target server or preview environment is reachable before launching browser work
* choose headed vs headless mode intentionally and record the reason
* derive browser routes from changed pages, components, or affected user journeys
* treat OAuth, email, SMS, payments, CAPTCHAs, or other external flows as explicit human checkpoints
* carry browser findings into runtime verification and operational closure rather than leaving them as informal notes

### continuous-learning

When the workspace enabled the `continuous-learning` capability pack:

* store observation state under `.autoharness/continuous-learning/`
* keep hook capture optional and environment-specific; manual capture is still valid
* use `observe` to capture recurring workflow signals, `learn` to infer instincts, and `evolve` to promote mature patterns into `learned-*` artifacts
* do not harden a rule into a learned instruction or skill until it has enough corroborating observations to justify the promotion
* treat learned artifacts as explicit repository knowledge rather than invisible prompt-only behavior

### strict-safety

When the workspace enabled the `strict-safety` capability pack:

* follow `.github/instructions/strict-safety.instructions.md`
* express risky work as `ProposedAction` entries with `ActionRisk` and `ActionResult`
* require explicit approval before destructive actions and prefer approval for high-blast-radius actions
* keep risky action records visible in plan hardening, review, runtime verification, and operational closure

### release-observability

When the workspace enabled the `release-observability` capability pack:

* follow `.github/instructions/release-observability.instructions.md`
* produce monitoring plans with SLIs, dashboards, baselines, and alert thresholds before merge
* complete pre-deploy audit checklists for runtime, migration, or rollout-risk changes
* define explicit post-deploy observation windows with owner and duration
* declare rollback triggers with named metrics and thresholds
* carry all release-observability artifacts into operational closure

### adversarial-review

When the workspace enabled the `adversarial-review` capability pack:

* follow `.github/instructions/adversarial-review.instructions.md`
* escalate from standard review when 3+ P0/P1 findings appear or the work is security-sensitive
* dispatch parallel reviewer instances across different model tiers for cross-model diversity
* assemble consensus-weighted findings (HIGH / MEDIUM / LOW confidence)
* treat HIGH-confidence P0/P1 findings as gate-blocking
* feed remediation queue entries into backlog

### graphtor-docs

When the workspace enabled the `graphtor-docs` capability pack:

* prefer `search_local_docs` / `search_semantic` / `research_topic` over raw file scans or web search for documentation lookups covered by indexed sources
* verify server reachability via `get_status` before trusting query results
* fall back to file-based search only when the graphtor-docs server is unavailable or sources are unindexed
* treat `.graphtor/config/sources.yaml` as the authoritative source registry and `.graphtor/` generated artifacts as tool-managed state

## Remote Operator Integration

### agent-intercom

When `agent-intercom` is available:

* Call `ping` at the start of any multi-step session to confirm liveness.
* Broadcast progress at meaningful phase transitions — do not broadcast every trivial step.
* Route approval for destructive actions through the intercom approval workflow before executing.
* If intercom becomes unreachable mid-task, warn that operator visibility is degraded and continue only with safe, non-destructive work.

The `ping-loop.prompt.md` prompt is available in `.github/prompts/` for sustained heartbeat sessions when the pack is installed.

### agent-engram

When `agent-engram` is available:

* Verify workspace binding before relying on indexed results.
* If the workspace is not bound or indexed, run `sync_workspace` or the workspace's equivalent freshness operation before searching.
* Fall back to grep, glob, or direct file reads only when indexed results are unavailable or insufficient.

## Backlog Workflow Expectations

When a backlog tool is active in the workspace:

* prefer queue-aware and dependency-aware operations over prose-only sequencing when the tool surface supports them
* use comments, checkpoints, and commit-tracking operations when they add traceability
* refresh the backlog index or query cache after out-of-band edits before trusting query results
* avoid inventing parallel markdown trackers outside the configured backlog tool surface

Generated by autoharness | Template: copilot-instructions.md.tmpl
