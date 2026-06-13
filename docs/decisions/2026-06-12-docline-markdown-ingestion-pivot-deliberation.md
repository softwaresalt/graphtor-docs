---
title: "Docline Markdown Ingestion Pivot"
description: "Pivot graphtor-docs from raw multi-format acquisition to docline-emitted standardized Markdown ingestion."
topic: "docline markdown ingestion pivot"
depth: "deep"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-06-12-docline-markdown-ingestion-pivot-plan.md"
tags:
  - "docline"
  - "markdown"
  - "ingestion"
  - "contract"
  - "mcp"
source_material:
  - "stash@{0}^3:docs/design-docs/graphtor-docs-ingestion-contract.md"
  - "stash@{0}^3:docs/design-docs/schema-export-workflow.md"
  - "stash@{0}^3:docs/design-docs/base-frontmatter-v1.schema.json"
  - "stash@{0}^3:schemas/docline/base-frontmatter-v1.schema.json"
---

## Problem Frame

The operator wants `graphtor-docs` to stop behaving like a raw document
acquisition-and-format-conversion system and instead become:

1. an ingestion engine for **docline-emitted standardized Markdown**
2. a consumer of a **known frontmatter contract/schema**
3. an MCP server for **agentic coding workspaces**

The preserved source of truth is the git stash at `stash@{0}`. The relevant
artifacts live in the untracked parent tree `stash@{0}^3` and must be used for
planning **without** dropping or applying the stash:

* `docs/design-docs/graphtor-docs-ingestion-contract.md`
* `docs/design-docs/schema-export-workflow.md`
* `docs/design-docs/base-frontmatter-v1.schema.json`
* `schemas/docline/base-frontmatter-v1.schema.json`

### Who cares

* The operator maintaining `graphtor-docs` as a local-first MCP server
* Ship, which must execute a high-blast-radius runtime pivot safely
* Agents relying on stable CLI/MCP/workspace behavior

### Constraints

* The new contract is the docline v1 frontmatter surface: required
  `title`, `source`, `ingested_at`, `doc_type`, plus optional
  `description`, `content_sha256`, `source_path`, `chunk_strategy`,
  `schema_version`, and `docline`
* `source_path` must remain POSIX-normalized and workspace-relative
* Validation must fail closed on malformed frontmatter or unsupported major
  schema versions
* The pivot must remove raw Git/URL/PDF/DOCX/HTML ingestion behavior rather than
  leaving half-alive legacy paths
* The scope stays focused on ingestion/model behavior, not search-ranking or DB
  schema redesign

### Success criteria

* `graphtor-docs` ingests only standardized Markdown documents with validated
  docline frontmatter
* Raw Git/URL acquisition and PDF/DOCX parsing are removed from the supported
  runtime model
* Missing or malformed contract fields fail with actionable diagnostics
* Workspace commands no longer silently fall back to broad raw markdown
  auto-discovery when an explicit standardized source registry is absent
* CLI/MCP/docs all describe the same ingestion contract

### Scope boundaries

* IN: frontmatter contract validation, source-config narrowing, runtime
  de-scoping of Git/URL/PDF/DOCX/HTML paths, sync-state invalidation, MCP
  workspace messaging parity
* OUT: DB schema redesign, ranking/search changes, new remote-fetching modes,
  unrelated backlog cleanup

## Research Findings

### Current code surfaces

Repository inspection confirms the current runtime still reflects the older
acquisition-centric architecture:

* `src/config/source.rs` still models `Source::{Git, Local, Url}` and defaults
  formats to `md,pdf,docx`
* `src/config/validation.rs` still accepts `md,pdf,docx,markdown` and applies
  Git/URL-specific rules
* `src/acquire/` still contains clone and crawl flows
* `src/parse/mod.rs` still exports and dispatches markdown, PDF, and DOCX
  parsers
* `src/parse/frontmatter.rs` only extracts `title` and `description` and
  tolerates malformed YAML via `unwrap_or_default()`
* `src/db/chunks.rs` currently stores chunk `title` as `null`
* `src/sync/state.rs` tracks mtimes/commits only and has no parser/contract
  epoch to invalidate pre-pivot state
* `src/main.rs` still synthesizes a workspace auto-discovery source when no
  registry exists
* tests still target Git/URL acquisition and PDF pipeline behavior

### Relevant prior learnings

The learnings retrieval step returned **confidence: medium** and surfaced the
following carry-forward constraints:

* keep validation and runtime acceptance lists identical
* normalize `source_path` to forward slashes before storage, hashing, and
  source matching
* do not infer source identity from `source_id` strings or path prefixes
* keep CLI/MCP/documentation contracts aligned from one implementation source
* remove retired legacy branches completely instead of hiding them behind stale
  config affordances

## Options Evaluated

### Option A: Compatibility wrapper around the legacy pipeline

Accept docline frontmatter, but keep Git/URL acquisition and PDF/DOCX parsing in
place behind config/runtime branches.

* **Pros**: smaller immediate delta; less migration pressure
* **Cons**: preserves two ingestion architectures; higher long-term drift risk;
  legacy branches remain test and dependency burden
* **Effort**: medium
* **Fit**: low

### Option B: Full docline-contract pivot

Make standardized Markdown with validated docline frontmatter the only supported
ingestion contract. Keep local workspace ingestion, but only through explicit
standardized-markdown source registries. Remove Git/URL acquisition and
PDF/DOCX parsing from the runtime model.

* **Pros**: one ingestion architecture; aligns with operator direction; removes
  stale dependency surfaces; creates a clean contract for agent workspaces
* **Cons**: breaking runtime/config behavior; requires sync-state and migration
  hardening; larger blast radius
* **Effort**: high
* **Fit**: high

### Option C: Dual-mode bridge release

Support both raw acquisition and docline-standardized ingestion for a transition
period.

* **Pros**: gentler migration
* **Cons**: duplicates validation, config, docs, and tests; invites long-lived
  ambiguity over which path is canonical
* **Effort**: high
* **Fit**: medium-low

## Trade-off Comparison

| Criterion | Option A | Option B | Option C |
|---|---|---|---|
| Architectural clarity | Low | High | Low |
| Runtime blast radius | Medium | High | High |
| Contract stability | Low | High | Medium |
| Ongoing maintenance cost | High | Low | High |
| Alignment with operator request | Low | High | Medium |

## Decision

**Chosen: Option B — Full docline-contract pivot**

The operator already made the product-direction choice: docline owns document
standardization, while `graphtor-docs` becomes a standardized-markdown ingest
engine plus MCP server. The planning question is therefore not whether to keep
the legacy modes, but how to remove them safely.

### Decision details

1. The docline v1 frontmatter contract becomes the single accepted ingest
   surface
2. A tracked schema snapshot must be promoted from the preserved stash before
   implementation so the contract is Git-visible and reviewable
3. `source_id` remains an internal registry identity, while validated
   frontmatter supplies document metadata and normalized `source_path`
4. Workspace auto-discovery of arbitrary markdown must be removed or gated; the
   new model requires an explicit standardized registry
5. Sync state must gain an explicit contract/parser epoch so existing workspaces
   do not silently reuse stale pre-pivot state

## Rejected Alternatives

* **Option A** was rejected because it keeps the exact split-brain ingestion
  model the operator wants to retire
* **Option C** was rejected because a dual-mode bridge would multiply parity,
  migration, and documentation burden without a stated product need

## Unresolved Questions

These are non-blocking and carry recommended answers into the plan:

* **Should the local source registry keep a `formats` field?**  
  Recommendation: keep the field only as a markdown-only affordance
  (`md`/`markdown` canonicalized together) rather than widening the breaking
  change to registry shape and runtime behavior at once.
* **Should MCP/JSON status surfaces expose a typed “no standardized registry”
  state?**  
  Recommendation: yes, when needed for parity, but without adding new tools or
  widening the MCP surface beyond diagnostics/status.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Legacy workspaces silently keep stale incremental state | Add explicit contract/parser epoch invalidation in sync state |
| Contract validation lands in the wrong layer and couples generic markdown parsing to business rules | Use a dedicated docline/ingest-contract module rather than making `frontmatter.rs` the sole owner |
| Removal of legacy modes leaves stale help/docs/tests/dependencies behind | Ship scope removal atomically across code, tests, dependencies, and docs |
| Source identity semantics drift between `source_id`, `source`, and `source_path` | Make the mapping explicit in the implementation plan and keep each field semantically distinct |
