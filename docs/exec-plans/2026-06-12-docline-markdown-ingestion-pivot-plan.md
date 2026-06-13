---
title: "Docline Markdown Ingestion Pivot Plan"
status: draft
source_deliberation: "docs/decisions/2026-06-12-docline-markdown-ingestion-pivot-deliberation.md"
created: 2026-06-12
revised: 2026-06-12
review_attempt: 1
---

# Docline Markdown Ingestion Pivot Plan

## Objective

Pivot `graphtor-docs` from a raw multi-format acquisition pipeline to a
standardized-markdown ingestion engine by:

1. adopting the docline v1 frontmatter contract as the single accepted ingest
   surface
2. removing Git/URL/PDF/DOCX/HTML runtime paths from supported ingestion
3. requiring explicit standardized-markdown source registries instead of broad
   workspace auto-discovery
4. preserving MCP workspace usefulness through aligned diagnostics, status, and
   documentation

## Scope

### In scope

* Promotion of the stashed contract docs/schema into tracked repo paths
* Strict frontmatter validation for required fields, schema version, body hash,
  and normalized `source_path`
* Local-source-only config/runtime behavior for standardized markdown ingestion
* Removal of PDF/DOCX parser branches and Git/URL/HTML acquisition branches
* Sync-state invalidation for pre-pivot incremental state
* CLI/MCP/runtime guidance when standardized registries are missing or invalid
* End-to-end regression coverage for docline-markdown workflows

### Out of scope

* DB schema redesign beyond reusing existing title/path surfaces
* Search ranking, embedding-model behavior, or retrieval-quality work
* New remote-fetching modes or transitional dual-mode ingestion
* Unrelated backlog cleanup outside the pivot surfaces

## Constitution Check

| Principle | Applicability | How this plan complies |
|---|---|---|
| I. Safety-First Rust | Materially applicable | All changes remain Rust-only; no unsafe code; validation errors must be explicit rather than silently defaulted |
| II. Test-First Development | Materially applicable | Units 1, 2, and 9 create failing characterization coverage before the corresponding implementation/removal units |
| III. Workspace Isolation | Materially applicable | `source_path` remains workspace-relative; path-traversal and out-of-root cases are explicitly characterized |
| IV. CLI Workspace Containment | Materially applicable | No new out-of-tree file operations; explicit registries replace broad auto-discovery |
| V. Structured Observability | Materially applicable | Missing-registry, invalid-frontmatter, migrated-database, and contract-epoch behaviors must become operator-visible and parity-safe |
| VI. Single Responsibility | Materially applicable | Contract validation, identity mapping, source narrowing, legacy-branch retirement, migration handling, diagnostics, and docs are decomposed into distinct units |
| VII. Destructive Command Approval | Low applicability | No destructive shell commands are planned; any migrated-database reset behavior must remain code-mediated and runtime verification stays in repo-contained fixtures only |
| VIII. Safety Modes | Materially applicable | Plan uses investigate-first and freeze-scope with explicit protected invariants, rollback signals, and bounded validation environments |
| IX. Git-Friendly Persistence | Materially applicable | Contract artifacts are first promoted from stash into tracked files before implementation relies on them |
| X. Context Efficiency | Low applicability | No new agent tools are introduced; parity is achieved by aligning existing surfaces |
| XI. Merge Commit Preservation | Low applicability | Standard Ship/PR flow applies after staged work is claimed |

**Task granularity**: Each implementation unit stays within one primary skill
domain and is intended to fit a focused implementation session under the 2-hour
rule.

## Source of Truth

* Deliberation: `docs/decisions/2026-06-12-docline-markdown-ingestion-pivot-deliberation.md`
* Preserved git stash alias: `stash@{0}` → pinned commit `ba79092af64a4a4b16b63e76b094e6a4bbad4214`
* Stashed source tree alias: `stash@{0}^3` → pinned commit `2eba8c73284ae75ba2d11340f3b80ac71ec50fed`
* Contract docs in stash:
  * `docs/design-docs/graphtor-docs-ingestion-contract.md`
  * `docs/design-docs/schema-export-workflow.md`
  * `docs/design-docs/base-frontmatter-v1.schema.json`
  * `schemas/docline/base-frontmatter-v1.schema.json`

## Safety Mode

* **Investigate-first** for contract validation, metadata mapping, and
  sync-state invalidation
* **Freeze-scope** to:
  * `Cargo.toml`
  * `docs/design-docs/`
  * `schemas/docline/`
  * `src/config/`
  * `src/parse/`
  * `src/chunk/`
  * `src/pipeline/`
  * `src/acquire/`
  * `src/sync/`
  * `src/db/`
  * `src/bin/`
  * `src/cli/`
  * `src/mcp/`
  * `src/workspace/init.rs`
  * `src/workspace/install.rs`
  * `src/workspace/mcp_config.rs`
  * `src/main.rs`
  * targeted tests under `tests/`
  * targeted docs/reference/instruction files for the new ingestion model

Explicitly frozen out unless parity diagnostics prove necessity:

* DB schema redesign
* new MCP tools or manifest expansion
* search/ranking behavior
* unrelated workspace install/upgrade flows

## Architecture Constraints

1. **Dedicated contract boundary**: docline contract validation must live in a
   dedicated ingest-contract module, not only as tolerant YAML extraction in
   `src/parse/frontmatter.rs`
2. **Tracked schema source with installed-binary parity**: implementation must
   validate against a tracked v1 schema snapshot promoted into the repo, and
   the runtime must embed that schema at compile time (or use an equivalently
   drift-proof mechanism) so installed binaries behave identically to
   development builds without file-backed asset drift
3. **Canonical logical identity**:
   * `source_id` remains the internal source-registry identity
   * graphtor-docs treats contract `source_path` as a required logical document
     path even if the broader schema supplies a default
   * the unique persisted document identity is a namespaced tuple
     `{logical_source_namespace, source_path}`, with
     `logical_source_namespace` derived explicitly from registry `source_id`
   * on-disk path, namespaced logical identity, and contract `source` remain
     semantically distinct
4. **Path-owning surfaces stay aligned**: parse, pipeline, reingest, link
   resolution, traversal, and delete-by-path behavior must all use the same
   logical document identity model
5. **No implicit registry creation or fallback architecture**: missing registry
   config must neither synthesize a broad raw-workspace source nor write an
   empty stub during normal runtime commands
6. **Legacy-state invalidation and migrated-data cleanup**: incremental sync
   and pre-pivot databases must both detect the pivot and force explicit
   reprocessing/pruning rather than leaving stale legacy content searchable
7. **Atomic retirement**: removing a legacy capability also removes its tests,
   docs claims, and dependency surface

## Requirements Trace

| Requirement | Implementation action |
|---|---|
| Standardize on one markdown schema/frontmatter contract | Units 0, 1, 3, 4, 17–19 |
| Remove PDF/DOCX/HTML and raw repo acquisition paths | Units 6–10 |
| Remove parsing of GitHub-repo-sourced documentation | Units 9 and 10 retire Git/URL acquisition rather than ingesting raw remote docs |
| Act as ingestion engine for standardized markdown | Units 2, 4, 5A, 5B, 11–20 |
| Preserve agentic workspace/MCP usefulness | Units 14–19 |
| Keep blast radius hardened | Units 1, 2, 11–18 plus hardening/review sections |

## Relevant Current-State Evidence

* `src/config/source.rs` still exposes `Source::{Git, Local, Url}` and defaults
  to `md,pdf,docx`
* `src/config/validation.rs` still validates Git/URL rules and format aliases
* `src/parse/frontmatter.rs` silently tolerates malformed YAML via
  `unwrap_or_default()`
* `src/chunk/id.rs` still derives chunk identity from `content + source_path`
  without a source namespace
* `src/db/chunks.rs` persists chunk `title` as `null`
* `src/pipeline/mod.rs` still derives persisted document path from
  `allowed_root` / on-disk relative paths
* `src/sync/reingest.rs` still deletes and reloads by on-disk relative path
* `src/sync/state.rs` tracks only mtimes/commit hashes and cannot invalidate a
  contract/parser epoch change
* `src/config/mod.rs` still writes `sources: []` stubs during normal runtime
  loading when a database exists without config
* `src/main.rs` still auto-discovers workspace markdown when no registry exists
  and still downgrades some config failures to warnings in `serve` and `status`
* `src/workspace/install.rs` currently ships only the executable into installed
  workspaces

## Learnings to Carry Forward

* Validation and runtime acceptance lists must stay identical
* `source_path` normalization must happen before storage, hashing, and matching
* Do not infer semantic source identity from `source_id` strings
* Keep CLI/MCP/docs parity from one implementation source of truth
* Retire legacy branches completely rather than leaving dormant capability code

## Implementation Units

### Unit 0 — Promote docline contract artifacts from preserved stash

**Goal**

Make the docline contract Git-visible before code depends on it.

**Primary surfaces**

* `docs/design-docs/graphtor-docs-ingestion-contract.md`
* `docs/design-docs/schema-export-workflow.md`
* `docs/design-docs/base-frontmatter-v1.schema.json`
* `schemas/docline/base-frontmatter-v1.schema.json`

**Acceptance criteria**

* The four contract artifacts are copied from pinned commit
  `2eba8c73284ae75ba2d11340f3b80ac71ec50fed` into tracked repo paths without
  applying or dropping the stash
* The schema snapshot under `schemas/docline/` matches the design-doc snapshot
  byte-for-byte
* Follow-on code/tests reference the tracked files rather than direct stash
  lookups

**Posture**

* copy-first

### Unit 1 — Characterize docline frontmatter contract behavior

**Goal**

Add failing tests that define the accepted and rejected frontmatter contract.

**Primary surfaces**

* `tests/parse_frontmatter_test.rs`
* targeted new fixtures under `tests/fixtures/`

**Acceptance criteria**

* Required `title`, `source`, `ingested_at`, `doc_type`, and graphtor-docs'
  required non-empty `source_path` are covered
* Unsupported major `schema_version` and malformed frontmatter fail closed
* `source_path` must be relative and forward-slash normalized
* `content_sha256` mismatch against emitted body bytes is covered

**Posture**

* characterization-first

### Unit 2 — Characterize legacy-mode rejection, fail-closed registry behavior, and workspace boundaries

**Goal**

Define the rejection matrix for legacy source modes and missing standardized
registries before implementation begins.

**Primary surfaces**

* `tests/config_test.rs`
* `tests/acquire_git_test.rs`
* `tests/acquire_url_test.rs`
* targeted CLI/runtime tests for missing-registry behavior
* `tests/path_security_test.rs`

**Acceptance criteria**

* Git and URL source entries are covered as rejection cases
* Non-markdown formats (`pdf`, `docx`, `html`, others) are covered as rejection
  cases
* Missing source registry no longer auto-discovers arbitrary workspace markdown;
  it fails with explicit guidance
* Missing-registry runtime commands are covered as read-only behavior — no
  implicit stub creation or config mutation
* Workspace-boundary/path-traversal protections remain covered

**Posture**

* characterization-first

### Unit 3 — Implement dedicated docline contract validator with embedded schema source

**Goal**

Validate standardized-markdown documents against the promoted v1 contract using
an explicit ingest boundary.

**Primary surfaces**

* new ingest-contract module under `src/`
* `src/parse/frontmatter.rs`
* `src/parse/types.rs`
* compile-time embedded schema source for the validator

**Acceptance criteria**

* Malformed YAML no longer defaults silently; errors are explicit and actionable
* Contract validation checks required fields, supported major schema version,
  non-empty normalized `source_path`, and `content_sha256`
* Validation uses the tracked schema snapshot as the authoritative contract
  surface
* Installed-binary behavior matches development behavior without requiring a
  repo-relative schema file lookup at runtime
* Generic markdown parsing remains separate from contract/version policy

**Posture**

* test-first

### Unit 4 — Canonicalize logical document identity in parse, pipeline, and stored metadata

**Goal**

Define the canonical logical document key for the standardized-markdown path.

**Primary surfaces**

* `src/parse/mod.rs`
* `src/parse/types.rs`
* `src/chunk/`
* `src/pipeline/mod.rs`
* `src/db/chunks.rs`

**Acceptance criteria**

* Graphtor-docs treats contract `source_path` as the canonical logical path
  component and rejects empty or ambiguous logical identities
* Full sync and persisted chunk metadata use the same namespaced logical
  identity model `{logical_source_namespace, source_path}`
* Chunk/vector/edge correlation keys derive from the namespaced logical identity
  so same-path/same-content documents from different source namespaces do not
  alias in a shared database
* Stored titles and paths derive from the validated contract rather than the
  prior on-disk-relative defaults
* `source_id`, on-disk path, contract `source`, and namespaced logical identity
  remain semantically distinct

**Posture**

* test-first

### Unit 5A — Propagate logical identity through reingest and delete-by-path flows

**Goal**

Make incremental reingest and delete-by-path flows honor the canonical logical
identity model.

**Primary surfaces**

* `src/sync/reingest.rs`
* `src/sync/`
* delete-by-path surfaces under `src/db/`

**Acceptance criteria**

* Incremental reingest/delete paths use the namespaced logical identity instead
  of assuming on-disk-relative identity
* Source-path rename and delete cases do not orphan stale rows
* Reingest/delete operations remain aligned with the sync-state ownership model

**Posture**

* test-first

### Unit 5B — Propagate logical identity through links, traversal, and queries

**Goal**

Make read/query surfaces resolve documents in the same namespaced logical
namespace used by ingest and reingest.

**Primary surfaces**

* `src/parse/links.rs`
* traversal/query path-resolution surfaces under `src/db/` and related modules

**Acceptance criteria**

* Link extraction, edge persistence, traversal, and query resolution use the
  same namespaced logical namespace
* Fixtures where on-disk location intentionally differs from contract
  `source_path` resolve consistently

**Posture**

* test-first

### Unit 6 — Narrow source config to local standardized-markdown sources

**Goal**

Make the source registry reflect the single supported ingestion architecture.

**Primary surfaces**

* `src/config/source.rs`
* `src/config/validation.rs`

**Acceptance criteria**

* The config model no longer supports `Source::Git` or `Source::Url`
* Markdown acceptance is canonicalized consistently between validation and
  runtime (`md`/`markdown` together, or a single documented canonical form)
* Config diagnostics clearly describe the docline-markdown-only contract
* Existing workspace-boundary checks remain intact for local sources

**Posture**

* test-first

### Unit 7 — Remove PDF parser path and dependency surface

**Goal**

Eliminate PDF ingestion from the parse/runtime model.

**Primary surfaces**

* `src/parse/mod.rs`
* `src/parse/pdf.rs`
* relevant PDF-only targets under `src/bin/`
* `Cargo.toml`

**Acceptance criteria**

* PDF dispatch/export paths are removed from the parse module
* PDF-specific tests are removed or replaced where the contract now rejects the
  mode
* PDF-only binary targets no longer keep PDF parser code or dependencies alive
* PDF-only dependencies are removed if no longer referenced

**Posture**

* test-first

### Unit 8 — Remove DOCX parser path and dependency surface

**Goal**

Eliminate DOCX ingestion from the parse/runtime model.

**Primary surfaces**

* `src/parse/mod.rs`
* `src/parse/docx.rs`
* `Cargo.toml`

**Acceptance criteria**

* DOCX dispatch/export paths are removed from the parse module
* DOCX-specific tests are removed or replaced where the contract now rejects the
  mode
* DOCX-only dependencies are removed if no longer referenced

**Posture**

* test-first

### Unit 9 — Retire Git acquisition path and shared source abstractions

**Goal**

Remove raw repository acquisition from the supported runtime architecture.

**Primary surfaces**

* `src/acquire/`
* `Cargo.toml`
* affected shared source/action abstractions

**Acceptance criteria**

* Git planning/execution branches are removed from acquisition/runtime wiring
* Git-specific tests are removed or replaced with rejection coverage
* Git-only dependencies are removed if no longer referenced elsewhere
* Shared source/action abstractions no longer require Git-era variants to model
  the supported architecture

**Posture**

* test-first

### Unit 10 — Retire URL/HTML crawl path and shared source abstractions

**Goal**

Remove raw web crawl acquisition from the supported runtime architecture.

**Primary surfaces**

* `src/acquire/`
* `Cargo.toml`

**Acceptance criteria**

* URL/HTML crawl planning/execution branches are removed from acquisition/runtime
  wiring
* URL/crawl-specific tests are removed or replaced with rejection coverage
* Crawl-only dependencies are removed if no longer referenced elsewhere
* Remaining shared source/action abstractions no longer model retired URL/HTML
  ingestion behavior

**Posture**

* test-first

### Unit 11 — Add sync-state contract epoch invalidation

**Goal**

Prevent pre-pivot incremental state from silently preserving stale runtime
behavior.

**Primary surfaces**

* `src/sync/state.rs`
* `src/sync/mod.rs`
* targeted sync-state regression tests

**Acceptance criteria**

* Sync state records an explicit contract/parser epoch or equivalent invalidation
  marker
* Sync state persists enough prior on-disk-to-logical-path ownership (or an
  equivalent source-level reset strategy) to prevent `source_path` renames from
  orphaning stale rows
* Legacy state missing that marker forces reprocessing or a clear operator reset
  path
* Unchanged files under the new contract are not skipped purely because old
  mtimes/commit markers survived

**Posture**

* test-first

### Unit 12 — Add a DB-level pivot marker and migrated-database gate

**Goal**

Make pre-pivot databases fail deterministically into the migration path before
query surfaces expose stale content.

**Primary surfaces**

* `src/db/`
* `src/sync/`
* database-open/status surfaces that decide whether migration is required

**Acceptance criteria**

* Databases record an explicit pivot marker or equivalent version gate
* Pre-pivot databases cannot reach query surfaces without entering the migration
  path first
* The gate covers all pre-pivot data, including earlier local-markdown rows that
  predate contract validation

**Posture**

* test-first

### Unit 13 — Implement migrated-database prune/rebuild for pre-pivot data

**Goal**

Remove or rebuild stale pre-pivot content once the database gate detects the
pivot.

**Primary surfaces**

* `src/db/`
* `src/sync/`
* copied migrated-database fixtures under `tests/fixtures/`

**Acceptance criteria**

* Migration cleanup covers source records, chunks, edges, code snippets, and
  vectors associated with pre-pivot data
* Pre-pivot local-markdown rows are also refreshed under the new contract, not
  left searchable unchanged
* Migration behavior is verified only against repo-contained copied fixtures, not
  external/live operator databases

**Posture**

* test-first

### Unit 14 — Enforce shared registry validation and read-only missing-registry behavior

**Goal**

Replace permissive config loading with one shared, explicit registry-validation
contract.

**Primary surfaces**

* `src/config/mod.rs`
* `src/main.rs`

**Acceptance criteria**

* Runtime commands do not create empty source stubs or other config artifacts on
  missing-registry paths
* Explicit `--config` loads and default-path loads share the same validation path
* Invalid registries no longer downgrade to warnings that continue with stale or
  implicit databases

**Posture**

* test-first

### Unit 15 — Align structured diagnostics across CLI, JSON, MCP, and startup failures

**Goal**

Back all fail-closed registry and startup errors with one shared structured
diagnostics contract.

**Primary surfaces**

* `src/cli/mod.rs`
* `src/mcp/server.rs`
* `src/mcp/format.rs`
* `serve` startup failure surfaces tied to these modules

**Acceptance criteria**

* Human CLI, `--json`, surfaced MCP status/failure paths, and the `serve`
  startup failure path share one structured diagnostics/remediation payload
* The `serve` bootstrap failure path yields a stable machine-readable error or
  startup state usable by agents when the full MCP server cannot start
* Missing/invalid-registry and startup diagnostics use the same codes, fields,
  and remediation hints across those surfaces

**Posture**

* test-first

### Unit 16 — Align help, manifest, and generated editor MCP configs

**Goal**

Make help text and generated editor launch paths match the same explicit-registry
contract as direct CLI usage.

**Primary surfaces**

* `src/cli/mod.rs`
* `src/workspace/mcp_config.rs`
* help/manifest/status surfaces tied to these modules

**Acceptance criteria**

* `graphtor-docs --help` plus relevant subcommand help no longer advertise
  retired Git/URL/parser behavior
* Status/help/manifest wording matches the explicit-registry model
* Generated editor MCP configs pin the executable and registry-resolution
  contract explicitly (for example via cwd, `--config`, `--db-path`, or an
  equivalent wrapper) rather than relying on ambient defaults

**Posture**

* test-first

### Unit 17 — Update init/install scaffolding for the explicit-registry model

**Goal**

Make the supported workspace bootstrap path match the new standardized-markdown
contract.

**Primary surfaces**

* `src/workspace/init.rs`
* `src/workspace/install.rs`
* bootstrap/template surfaces owned by install/init flows

**Acceptance criteria**

* Only explicit bootstrap flows such as `init`/`install` create registry files
* Bootstrap output and templates describe local standardized-markdown/docline
  ingestion rather than the legacy acquisition model
* Install/bootstrap flows work with the compile-time-embedded validator and do
  not depend on external schema files being copied beside the binary

**Posture**

* test-first

### Unit 18 — Add valid standardized-markdown end-to-end workflow coverage

**Goal**

Prove the happy-path docline-markdown workflow works end-to-end.

**Primary surfaces**

* integration tests under `tests/`

**Acceptance criteria**

* A valid docline-emitted markdown workspace syncs successfully end-to-end
* Chunk IDs, persisted paths, and stored titles reflect the canonical logical
  identity model
* Two distinct sources targeting the same database can ingest identical content
  at the same `source_path` without chunk-ID or edge aliasing
* The installed-workspace path is covered at least once for packaged-schema
  parity

**Posture**

* characterization-first

### Unit 19 — Add migrated-state regression coverage

**Goal**

Prove the migrated-state gate and prune/rebuild path against copied fixtures.

**Primary surfaces**

* integration tests under `tests/`

**Acceptance criteria**

* A copied pre-pivot state/database fixture exercises the DB gate plus
  migration prune/rebuild path
* Repeat full-sync rename/delete scenarios prove old logical-path rows are
  pruned or rebuilt rather than left searchable
* Migrated-state fixtures cover source records plus chunks/edges/code/vectors

**Posture**

* characterization-first

### Unit 20 — Add bootstrap and diagnostics parity regression coverage

**Goal**

Prove the clean-workspace bootstrap flow and parity-sensitive error surfaces.

**Primary surfaces**

* integration tests under `tests/`

**Acceptance criteria**

* A clean-workspace bootstrap flow (`install`/`init` → first `sync`/`status`
  or surfaced MCP status) verifies embedded-schema availability, generated
  editor MCP config behavior, explicit registry placement, and in-tree
  containment of writes
* Missing or invalid registry diagnostics remain consistent across human CLI,
  `--json`, and surfaced MCP status/failure paths

**Posture**

* characterization-first

### Unit 21 — Update docs, CLI reference, and agent instructions

**Goal**

Align durable documentation with the new contract.

**Primary surfaces**

* `README.md`
* `docs/ARCHITECTURE.md`
* `docs/configuration.md`
* `docs/source-registry-guide.md`
* `docs/cli-reference/`
* `docs/mcp-tools.md`
* `docs/incremental-sync.md`
* `docs/troubleshooting.md`
* `.github/instructions/graphtor-docs.instructions.md`

**Acceptance criteria**

* Repository docs describe docline-standardized markdown as the supported
  ingestion model
* Retired Git/URL/PDF/DOCX/HTML modes are removed from CLI, configuration, MCP,
  and troubleshooting references
* MCP/reference docs describe the explicit standardized-registry model and the
  new diagnostics/remediation contract
* Troubleshooting covers missing-registry, invalid-frontmatter, and migrated
  state reset/prune behavior
* Agent-facing instructions no longer advertise raw acquisition or parser modes

**Posture**

* docs-first

## Dependency Graph

1. Unit 0 first — tracked contract artifacts must exist before code/tests depend
   on them
2. Units 1 and 2 establish the red-phase contract, rejection matrix, and
   workspace-boundary expectations
3. Unit 3 depends on Unit 1 and Unit 0
4. Unit 11 depends on Unit 3 and provides the fail-closed sync-state gate
   before the namespaced identity remap ships
5. Unit 12 depends on Units 3 and 11 and provides the DB-level fail-closed gate
   before legacy-retirement runtime units ship
6. Unit 4 depends on Units 3, 11, and 12
7. Unit 5A depends on Units 4, 11, and 12
8. Unit 5B depends on Units 4 and 12
9. Unit 6 depends on Unit 2
10. Units 7 and 8 depend on Units 3, 6, and 12
11. Units 9 and 10 depend on Units 6 and 12, and Unit 2 must be observed
    failing before either legacy acquisition retirement begins
12. Unit 13 depends on Units 7 through 12
13. Unit 14 depends on Units 2, 6, 11, and 12
14. Unit 15 depends on Unit 14
15. Unit 16 depends on Units 14 and 15
16. Unit 17 depends on Units 3, 6, 14, 15, and 16
17. Unit 18 depends on Units 4, 5A, 5B, 6, 11, 12, 14, 15, 16, and 17
18. Unit 19 depends on Units 4, 5A, 11, 12, and 13
19. Unit 20 depends on Units 15, 16, and 17
20. Unit 21 depends on Units 15 through 20

## Decisions and Rationale

* **Keep a dedicated contract boundary** instead of making tolerant YAML
  extraction the contract owner. This resolves the current `unwrap_or_default()`
  drift and keeps version policy out of generic markdown parsing.
* **Treat contract `source_path` as a required graphtor-docs identity field**
  even if the broader schema supplies a default. The runtime needs one stable
  namespaced logical document identity `{logical_source_namespace, source_path}`
  across parse, pipeline, reingest, delete, traversal, and chunk correlation.
* **Reuse existing DB title/path fields where possible** instead of widening the
  schema for the full docline metadata set. This keeps scope focused on the
  ingest-model pivot while still correcting the currently null title/path drift.
* **Treat missing registries as read-only errors, not discovery prompts.** The
  standardized contract must be explicit; broad auto-discovery and implicit
  stub creation are the legacy behaviors being retired.
* **Invalidate incremental sync and migrated databases explicitly** rather than
  trusting pre-pivot mtimes, commit hashes, or stored rows. Without an epoch and
  cleanup path, stale data would remain searchable under the new contract.
* **Keep installed-binary validation behavior identical to development** by
  embedding the contract source explicitly rather than relying on repo-relative
  runtime file lookups.

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Strict contract validation breaks existing ad hoc markdown workspaces | Clear missing/invalid registry diagnostics, docs updates, and explicit migration guidance |
| Source identity drift between internal `source_id` and contract `source`/`source_path` | Keep the mapping explicit in Unit 4 and cover it in workflow + migration tests |
| Legacy parser/acquisition dependencies remain linked after capability removal | Tie dependency pruning directly to Units 6–9 |
| Pre-pivot databases remain searchable after the pivot | Add explicit migrated-database prune/reset behavior in Units 12–13 and verify it against copied fixtures |
| Agent/runtime parity drifts during the pivot | Centralize diagnostics in Units 14 and 15 and align durable docs in Unit 19 |

## Plan Hardening Signals

* **Public API, schema, or contract change** — present  
  The docline frontmatter contract becomes the canonical ingest interface.
* **Security, auth, permission, or compliance-sensitive behavior** — present  
  No auth model changes, but workspace-boundary validation, logical-path deletes,
  and installed-workspace packaging all affect trust boundaries and fail-closed
  behavior.
* **Migration, backfill, destructive data/config action, or irreversible step** — present  
  Legacy config/runtime paths are removed; sync-state invalidation and migrated
  database cleanup change how existing workspaces reingest.
* **External integration, operator checkpoint, or external dependency** — present  
  The contract originates in docline and Ship must validate behavior against
  representative repo-contained standardized-markdown and migrated-state
  fixtures.
* **High runtime, rollout, or rollback risk** — present  
  `sync`, `serve`, `status`, and `prewarm` all change behavior materially.

**Requires plan hardening: yes**

## Plan Hardening

### Risk triggers

* Breaking ingest contract/runtime behavior
* Removal of multiple legacy source and parser modes
* Logical-path remapping away from on-disk-relative identity
* Invalidating pre-pivot incremental state and cleaning migrated databases
* Agent/user parity risk around registry discovery and diagnostics

### Protected invariants

* All ingest paths remain local-only and workspace-contained
* Malformed frontmatter never succeeds silently
* `source_path` remains relative and forward-slash normalized
* Runtime commands never create registries implicitly on missing-registry paths
* The runtime never falls back silently to raw workspace auto-discovery
* Pre-pivot sync state cannot silently preserve stale behavior
* Pre-pivot legacy rows cannot remain searchable after the pivot
* CLI/MCP/docs communicate the same supported ingest model

### Proposed actions

#### ProposedAction 1

* `summary`: Promote the preserved contract docs/schema into tracked repo paths
* `targets`: `docs/design-docs/`, `schemas/docline/`
* `change_kind`: local edit
* `action_risk`: low
* `rollback`: revert promoted artifacts and keep stash untouched
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 2

* `summary`: Add failing characterization coverage for contract validation,
  legacy-mode rejection, and fail-closed registry behavior
* `targets`: `tests/`, targeted fixtures
* `change_kind`: local edit
* `action_risk`: low
* `rollback`: revert failing test additions while preserving observed matrices in the plan
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 3

* `summary`: Introduce a dedicated docline contract validator with explicit
  packaged-schema behavior for installed binaries
* `targets`: `src/parse/`, new ingest-contract module, packaged schema source
* `change_kind`: local edit
* `action_risk`: moderate
* `rollback`: revert validator wiring and packaged-schema integration together
  to avoid partial contract ownership
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 4

* `summary`: Establish the canonical logical document identity and wire it
  consistently through parse, pipeline, reingest, and persisted chunk metadata
* `targets`: `src/parse/`, `src/pipeline/`, `src/sync/reingest.rs`, `src/db/chunks.rs`
* `change_kind`: local edit
* `action_risk`: high
* `rollback`: revert logical-path mapping as a coherent set; do not leave mixed
  on-disk-path and contract-path behavior
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 5

* `summary`: Retire each legacy parser/acquisition capability in separately
  bounded units, including its dependencies, tests, and shared source abstractions
* `targets`: `src/parse/`, `src/acquire/`, shared abstractions, `Cargo.toml`
* `change_kind`: local edit
* `action_risk`: high
* `rollback`: restore only the specific retired capability plus its tests and
  dependencies; do not leave partial retirement
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 6

* `summary`: Invalidate pre-pivot sync state and add migrated-database cleanup
  for copied legacy fixtures/workspaces
* `targets`: `src/sync/`, `src/db/`, targeted tests and copied fixtures
* `change_kind`: local edit
* `action_risk`: high
* `rollback`: revert epoch/cleanup enforcement and document the manual reset
  procedure before retry
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 7

* `summary`: Enforce read-only missing-registry behavior with one shared
  diagnostics/remediation contract across CLI, JSON, status, and MCP surfaces
* `targets`: `src/config/mod.rs`, `src/main.rs`, `src/cli/`, `src/mcp/`
* `change_kind`: local edit
* `action_risk`: high
* `rollback`: restore prior config-loading behavior only as a coherent shared
  path; do not leave some commands fail-closed and others permissive
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 8

* `summary`: Run representative runtime verification only against repo-contained
  standardized-markdown and migrated-state fixtures
* `targets`: copied in-tree fixture workspaces, CLI/MCP runtime execution
* `change_kind`: stateful local execution
* `action_risk`: moderate
* `rollback`: stop verification, preserve logs/output, and recreate the copied
  fixture workspace from source before another attempt
* `approval_required`: no
* `action_result`: planned

### Deepened verification

* Verify malformed YAML, unsupported major versions, invalid `source_path`, and
  `content_sha256` mismatches all fail deterministically
* Verify `source_id` does not replace or overwrite contract `source_path`
* Verify logical-path uniqueness and logical-path deletes/reingest stay aligned
  across full sync and incremental sync
* Verify removed parser/acquisition branches also lose their dependency and test
  surfaces
* Verify missing-registry behavior is consistent across `sync`, `serve`,
  `status`, `prewarm`, and surfaced JSON/MCP diagnostics without writing stubs
* Verify a pre-pivot `.sync_state.json` or equivalent legacy state cannot skip
  reprocessing under the new contract
* Verify copied pre-pivot databases do not retain searchable legacy rows after
  the migration path runs

### Rollback procedure

1. Revert the pivot commits as a set
2. Restore the previous logical-path/config-resolution behavior only as a
   coherent shared path
3. Restore the retired parser/acquisition branches only as a coherent set with
   their dependencies and tests
4. Restore pre-pivot migration behavior only together with the runtime that
   expects it
5. Keep characterization tests that exposed the intended docline-only contract,
   adjusting them only if the rollback intentionally reopens legacy behavior

### Monitoring and validation window

Manual observation is required because the pivot changes local CLI and MCP
behavior:

* **SLI / key signals**: valid standardized-markdown sync succeeds; invalid
  frontmatter fails clearly; missing-registry behavior is explicit; legacy state
  reingests rather than silently skipping
* **Observation surface**: local CLI output, `--json` output, MCP `get_status`
  or equivalent surfaced diagnostics, and test logs from copied in-tree
  workspaces/fixtures
* **Owner**: operator running Ship validation
* **Observation window**: first two local in-tree validation runs within
  24 hours of merge, plus one follow-up run after any registry/template change

## Runtime Verification Plan

Ship must validate the pivot with representative repo-contained data:

1. Promote the stashed contract artifacts into tracked files before code work
2. Copy a representative standardized-markdown workspace fixture and any
   pre-pivot state/database fixtures into the repository tree used for
   validation
3. Run the repository quality gates
4. Sync the copied valid standardized-markdown fixture with v1 frontmatter
5. Verify malformed frontmatter and unsupported major versions fail clearly
6. Verify retired Git/URL/PDF/DOCX/HTML modes are rejected explicitly
7. Verify missing or invalid registry paths do not write stubs and surface one
   shared remediation contract across human CLI, `--json`, and surfaced MCP
   diagnostics
8. Verify migrated-state fixtures exercise contract-epoch reprocessing and
   legacy-row prune/reset behavior successfully
9. Verify `install`/`init` and generated editor MCP config flows keep all writes
   inside the copied workspace tree and resolve the intended registry path
10. Verify `serve`/status/JSON parity still reflects the standardized-markdown
    model accurately

## Quality Gates

Ship must run and pass:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`
3. `cargo test --all-targets`
4. `cargo audit`

## Plan Review

### Review summary

* Reviewed against architecture cohesion, scope boundaries, constitution
  compliance, agent/MCP parity, and Rust implementation feasibility
* Blocking issues resolved before this PASS:
  * pinned the preserved stash aliases to immutable commit SHAs for executable
    traceability
  * made the validator's schema source embedded/packaged for installed-binary
    parity
  * switched from bare `source_path` identity to a namespaced logical identity
    model and pulled chunk correlation into scope
  * moved sync-state and DB-level pivot gates ahead of user-visible retirements
  * separated registry validation, structured diagnostics, help/editor-config
    parity, bootstrap scaffolding, and regression coverage into bounded units

### Gate decision

**PASS**

### Findings

#### P2

* **Documentation surface breadth**  
  Unit 21 intentionally groups the durable docs/reference/instruction updates so
  the contract change lands atomically. If that unit expands beyond a focused
  docs session during implementation, split it into user-facing docs and
  agent/reference docs before claiming it.

* **Parity verification depth**  
  Keep Unit 20 anchored to generated editor MCP configs, `serve` startup
  failures, and CLI help snapshots so parity stays machine-checkable rather than
  drifting back to prose-only assertions.

* **Rollout sequencing discipline**  
  Units 11–13 (epoch + DB gate + migrated-data cleanup) and Units 14–16
  (registry validation + diagnostics + bootstrap scaffolding) should not be
  partially shipped out of order even though they are backlog-sized separately.
