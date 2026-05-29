---
title: "Release Sync Hardening for Embedding Diagnostics and Operator Progress"
type: deliberation
status: decided
stash_id: 3848CFD7
depth: standard
promote_to: plan
created: 2026-05-29
---

# Release Sync Hardening for Embedding Diagnostics and Operator Progress

## Problem Frame

The operator reported that a manual release-version `graphtor-docs sync` run
against the current `.graphtor/config` source registries is not reliable enough
for release validation:

1. `sync` reports embedding-model lookup failures and other issues without a
   clear operator-facing diagnosis
2. `sync` does not show useful progress while work is in flight, so a long run
   looks stalled
3. We need a release-grade verification path that exercises the currently
   configured source registries, not only synthetic tempdir fixtures

### Who cares

* The operator shipping graphtor-docs releases locally
* Future agents that need a repeatable release-smoke path for real configured
  sources
* Users relying on `sync` rather than `prewarm` for direct CLI ingestion

### Constraints

* Must preserve existing `sync --metrics` machine-readable behavior
* Must remain compatible with the current `.graphtor/config` source registry
  layout, including multiple `*.sources.yaml` files
* Must not regress `prewarm` or MCP `get_status` progress behavior
* Must keep release verification scoped to local workspace resources
* Must surface actionable embedding diagnostics instead of opaque warnings

### Success criteria

* A release-built `graphtor-docs` binary can be exercised against the current
  `.graphtor/config` registries with a clear verification checklist
* When embedding model resolution fails, `sync` reports the cause and the
  fallback behavior in operator-usable terms
* `sync` shows useful progress while sources/files are processed, including
  percentage or equivalent completion context
* Existing JSON metrics and non-interactive scripting paths remain usable

### Scope boundaries

* IN: release-smoke verification path for current workspace source registries
* IN: embedding model resolution and error reporting in `sync`
* IN: progress visibility for the `sync` command
* IN: parity review against existing `prewarm` and background-sync patterns
* OUT: new remote services, hosted model infrastructure, or external telemetry
* OUT: redesign of the acquisition pipeline beyond what is needed for diagnosis

## Research Findings

### Current code surfaces

* `src/main.rs` loads embeddings separately in `cmd_sync`, `cmd_prewarm`, and
  `cmd_serve`
* `cmd_sync` currently falls back silently to `None` embeddings after a warning,
  then prints only a final completion line
* `prewarm` already has per-file stderr progress via `sync_source(..., Some(cb))`
* MCP background sync already exposes `SyncStatus::InProgress { source, current, total }`
* `tests/prewarm_progress_test.rs` and `tests/sync_cli_metrics_test.rs` provide
  reusable verification patterns for CLI output

### Relevant prior learnings

* `docs/compound/runtime-errors/sync-reingest-canonical-source-root-2026-05-21.md`
  warns that path-normalization failures can masquerade as sync/runtime issues
* `docs/compound/best-practices/shared-status-type-binary-library-2026-05-06.md`
  recommends extending the shared sync-status surface instead of inventing a
  second progress model
* `docs/compound/best-practices/pipeline-source-metadata-lookup-2026-04-29.md`
  recommends resolving runtime metadata from parsed config objects, not proxy
  IDs or fallback strings
* `docs/decisions/2026-05-21-prewarm-sync-progress-reporting.md` shows that
  file-level progress and structured telemetry are already accepted patterns

## Options

### Option A: Minimal sync parity patch

Add the existing prewarm progress callback to `sync` and improve the warning text
around `EmbeddingModel::load`, but keep the current duplicated model-loading
logic and rely on ad hoc release testing.

* **Pros**: small delta, low implementation risk
* **Cons**: keeps duplicated model-resolution paths; release verification remains weak
* **Effort**: low
* **Fit**: medium

### Option B: Shared release-sync hardening path

Introduce a shared embedding-resolution helper, add operator-meaningful sync
progress output that reuses existing progress primitives, and define a release
smoke path that runs the release binary against the current `.graphtor/config`
registries with explicit acceptance checks.

* **Pros**: addresses the operator's full problem; improves parity across sync,
  prewarm, and serve; creates a durable release-validation workflow
* **Cons**: touches shared CLI/runtime behavior; requires careful output-contract review
* **Effort**: medium
* **Fit**: high

### Option C: New dedicated release verification command

Add a separate CLI command for release verification, richer telemetry, and
embedding diagnostics, leaving `sync` mostly unchanged.

* **Pros**: isolates release workflow concerns
* **Cons**: new surface area, duplicates `sync` semantics, overshoots the intake
* **Effort**: high
* **Fit**: low

## Decision

**Chosen: Option B — Shared release-sync hardening path**

Rationale:

1. The operator explicitly needs the existing `sync` command to become more
   usable, not a parallel command
2. The repository already has accepted patterns for progress reporting
   (`prewarm`) and shared status plumbing (MCP background sync)
3. Centralizing embedding-model resolution reduces divergence between `sync`,
   `prewarm`, and `serve`
4. Release validation must exercise the current workspace registries, so the
   plan must carry a real-config smoke path forward

### Rejected alternatives

* **Option A**: improves symptoms but leaves duplicate model-loading logic and
  an under-specified release-validation path
* **Option C**: creates unnecessary CLI surface area for a problem the operator
  already scoped to `sync`

### Risks and mitigations

| Risk | Mitigation |
|---|---|
| Human-readable sync progress could break scripting expectations | Preserve `--metrics` output contract and keep progress on stderr |
| Embedding lookup diagnosis could hide a different root cause | Add characterization tests against current config patterns and reuse path-normalization learnings |
| Release smoke run could be slow or noisy | Define a bounded verification checklist and keep it as runtime verification, not a mandatory unit test for every CI path |

### Unresolved questions

* Should `sync` emit only source-level progress or file-level progress when the
  source contains many documents? Recommendation: show source + file context with
  percentage, while keeping the existing final summary
* Should embedding failure remain best-effort fallback or become a hard failure?
  Recommendation: keep fallback for compatibility, but make the warning explicit
  about what functionality is degraded and how to recover
