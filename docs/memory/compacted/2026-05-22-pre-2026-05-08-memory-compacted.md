---
type: compacted-memory
agent: copilot-cli
timestamp: 2026-05-22T00:00:00Z
source_cutoff: 2026-05-08
archived_files: 25
---

# Compacted memory summary for pre-2026-05-08 shipments

## Scope

This summary consolidates stale memory artifacts created before 2026-05-08
and replaces verbose shipment-by-shipment notes with a durable overview.

## Outcomes

* Early shipments established the markdown parser, unified embedded database,
  acquisition pipeline, PDF ingestion, MCP server surface, CLI workspace
  distribution, and source-agnostic bootstrap.
* The codebase pivoted from a multi-store design toward CozoDB-backed storage
  with graph traversal and vector-search planning captured in follow-on work.
* The CLI and MCP layers were hardened with JSON-RPC output helpers, expanded
  tool manifests, and Windows-safe path normalization.

## Key decisions

* Standardized on CozoDB as the embedded persistence layer for document
  storage, traversal, and future vector indexing.
* Adopted `pdf-extract`-based ingestion with page and segment identifiers to
  keep PDF-derived chunk identities stable and unique.
* Normalized Windows paths to forward-slash form at ingestion and lookup
  boundaries to prevent chunk ID and MCP document mismatches.
* Kept embeddings computed in-process while deferring full HNSW persistence
  and related optimization work to later shipments.
* Reframed MCP tooling around source-agnostic local documentation search
  rather than product-specific naming.

## Main code and document surfaces touched

* `src/parse/markdown/`
* `src/parse/pdf.rs`
* `src/db/`
* `src/embed/`
* `src/pipeline/mod.rs`
* `src/mcp/server.rs`
* `src/cli/jsonrpc.rs`
* `start.ps1`
* `Cargo.toml`
* `AGENTS.md`
* `.github/copilot-instructions.md`
* `docs/compound/`
* `docs/closure/`
* `docs/exec-plans/`

## Durable learnings

* Stack-based markdown AST traversal preserved ordering better than earlier
  index-oriented parsing approaches.
* Frontmatter parsing needed strict byte-zero checks to avoid offset drift and
  parser panics.
* Composite chunk identifiers were required for repeated headings, empty code
  blocks, and PDF segment boundaries.
* `cargo fmt` verification on Windows needed extra care because local CRLF
  handling could mask CI differences.
* Rust closure ergonomics around `?` favored extracted helper functions for
  fallible sync and pipeline steps.
* Shell and Git automation needed explicit handling for stale locks, PR body
  escaping, and Copilot-review thread resolution workflows.

## Deferred or follow-up work

* HNSW and vector-search persistence verification remained follow-on work.
* Several cleanup tasks were deferred around batch result structure,
  path-typing, and clone minimization.
* Security and dependency follow-ups were tracked for upstream crate and action
  upgrades.
* Image-only PDF handling was left as graceful degradation with future warning
  improvements possible.

## Archived source artifacts

The following verbose memory files were archived under `docs/archive/memory/`
with their original relative paths preserved:

* `001-S-markdown-parser-shipped.md`
* `2026-04-29/002-S-shipped.md`
* `2026-04-29/003-s-pipeline-foundation-session.md`
* `2026-04-30/003-s-closure-session.md`
* `2026-04-30/003-s-ship-pipeline-refactors.md`
* `2026-04-30/005-S-cli-workspace-distribution.md`
* `2026-04-30/006-S-hardening-code-quality.md`
* `2026-05-01-ship-009s-mcp-tool-surface.md`
* `2026-05-01/007-s-pdf-ingestion-shipped.md`
* `2026-05-01/session-008-S-shipped-2026-05-01.md`
* `2026-05-02/pr-20-ureq-merge-closure.md`
* `2026-05-02/ship-011-s-complete.md`
* `2026-05-02/ship-012-S-closure.md`
* `2026-05-03/013-S-streaming-pdf-shipped.md`
* `2026-05-04/ship-015-s-closure.md`
* `2026-05-04/shipment-014-s-closure.md`
* `2026-05-05/018-S-closure.md`
* `2026-05-05/019-s-source-agnostic-bootstrap.md`
* `2026-05-05/pr28-pr31-review-followups.md`
* `2026-05-05/pr32-closure.md`
* `2026-05-05/ship-016-s-comprehensive-docs.md`
* `2026-05-06/020-s-zero-config-adoption.md`
* `2026-05-06/021-s-shipped-cli-jsonrpc.md`
* `2026-05-06/022-s-shipped-docs-alignment.md`
* `2026-05-07/024-S-closure.md`
