---
title: "Release Sync Hardening Plan"
status: draft
source_deliberation: "docs/decisions/2026-05-29-release-sync-hardening-deliberation.md"
created: 2026-05-29
revised: 2026-05-29
review_attempt: 0
revision_reason: "Operator intervention after plan-review retry limit — addressing 4 blocking findings"
---

# Release Sync Hardening Plan

## Objective

Make the release-built `graphtor-docs sync` workflow trustworthy for the current
workspace registries by:

1. characterizing and validating release-mode sync against
   `.graphtor/config/*.yaml`
2. fixing embedding-model lookup and operator-facing diagnostics in `sync`
3. surfacing useful progress feedback during sync execution without breaking
   scriptable output paths

## Scope

### In scope

* Release-smoke verification criteria using the current workspace source registries
* Shared embedding-model resolution behavior for `sync`, `prewarm`, and `serve`
* Actionable sync diagnostics when embeddings are unavailable or mismatched
* Sync progress output on human-facing CLI paths
* Tests that characterize both operator output and degraded embedding paths

### Out of scope

* New hosted model providers or remote embedding services
* Backlog-wide refactors outside the sync/prewarm/serve entry points
* New CLI commands unrelated to the existing `sync` workflow
* Direct edits to `.graphtor/config` source registries
* **Shared MCP status surface extensions** (`src/mcp/` type changes) — unless a
  separately planned and reviewed follow-up proves strict necessity

## Constitution Check

| Principle | Applicability | How this plan complies |
|---|---|---|
| I. Safety-First Rust | Materially applicable | All implementation stays in Rust; no unsafe code introduced; clippy pedantic enforced via quality gates |
| II. Test-First Development | Materially applicable | Units 1A, 1B, 4, 6 write failing tests before any implementation in Units 2, 3, 5, 7 |
| III. Workspace Isolation | Materially applicable | Release verification uses current workspace config read-only; no path traversal beyond workspace root |
| IV. CLI Workspace Containment | Materially applicable | All file operations remain within cwd tree; release verification reads `.graphtor/config` in-tree only |
| V. Structured Observability | Materially applicable | Progress output and diagnostics are explicit deliverables; stderr discipline preserved |
| VI. Single Responsibility | Materially applicable | No new dependencies assumed; prefer existing stderr/progress plumbing before adding crates |
| VII. Destructive Command Approval | Low applicability | No destructive operations planned; release verification is read-only against local config |
| VIII. Safety Modes | Materially applicable | Plan declares investigate-first for embedding diagnosis and freeze-scope for implementation surfaces |
| IX. Git-Friendly Persistence | Low applicability | No new persistence formats introduced |
| X. Context Efficiency | Low applicability | No new agent tool surfaces introduced |
| XI. Merge Commit Preservation | Low applicability | Standard merge workflow applies at PR time |

**Task granularity**: Every unit stays within one skill domain and targets fewer than 3 files, fewer than 5 functions, and fewer than 4 test scenarios

## Source of Truth

* Deliberation: `docs/decisions/2026-05-29-release-sync-hardening-deliberation.md`
* Intake stash: `3848CFD7`
* Current config surface: `.graphtor/config/*.yaml`

## Safety Mode

* **Investigate-first** for embedding-failure diagnosis and real-config fixture shaping
* **Freeze-scope** to `src/main.rs`, `src/embed/`, `src/sync/`, and targeted test files.
  `src/mcp/` is frozen-out (not a valid edit target for this plan)

## Architecture Constraints

* The shared embedding resolver must live under a reusable library boundary
  (`src/embed/` or equivalent), with `src/main.rs` limited to orchestration
* Do not introduce new source-registry config semantics unless a separate,
  explicitly planned runtime-config type is required
* The "shared progress shape" for this plan means the existing `prewarm`-style
  callback pattern (`fn(&str, usize, usize)` or equivalent closure); it does NOT
  mean extending MCP `SyncStatus` types or `src/mcp/` shared state. CLI sync
  progress uses the same callback mechanism as prewarm without widening any
  cross-boundary type contracts

## Implementation Units

### Unit 1A — Characterize embedding-model lookup failure in release sync

**Goal**

Add a failing test that pins the current broken embedding-model resolution
behavior when `sync` is invoked against a workspace-shaped source registry.

**Primary surfaces**

* `tests/`
* copied or read-only fixture inputs shaped from `.graphtor/config/*.yaml`

**Tasks**

* Add an integration test that exercises the release `sync` command path with
  a fixture registry shaped from the current workspace sources
* Add a deterministic failure-injection seam that reproduces embedding-load
  failure without network dependence
* Confirm the test fails against current behavior before code changes begin

**Acceptance criteria**

* A failing test reproduces the current opaque embedding-model lookup behavior
  without network dependence
* Test fixture reflects the current workspace source-registry shape
* Unit 1A stays focused on embedding diagnosis only; shared-resolver parity is
  Unit 1B; progress contract tests belong to Unit 4

**Posture**

* characterization-first

### Unit 1B — Characterize shared embedding resolver divergence

**Goal**

Add a failing test that proves `sync`, `prewarm`, and `serve` currently
diverge or lack a shared resolver for embedding-model inputs.

**Primary surfaces**

* `tests/`

**Tasks**

* Add a failing test that exercises `sync`, `prewarm`, and `serve` entry points
  with identical runtime inputs and asserts they resolve the same embedding model
* The test must demonstrate the current divergence as a red result

**Acceptance criteria**

* A failing test covers the shared-resolver outcome expected by all three commands
* The test does not depend on network or live model availability
* Unit 1B stays within the test domain only — no production code changes

**Posture**

* characterization-first

### Unit 2 — Extract shared embedding resolver for existing model contract

**Goal**

Implement a shared resolution path so `sync`, `prewarm`, and `serve` interpret
the existing embedding model contract consistently.

**Primary surfaces**

* `src/embed/`
* `src/main.rs`

**Tasks**

* Write or extend failing tests that define the shared resolver outcome expected
  by `sync`, `prewarm`, and `serve` before extracting code
* Extract shared embedding-model resolution logic from duplicated command-entry code
* Keep the resolver anchored to the existing fixed model/runtime contract rather
  than inventing new source-registry config semantics
* Ensure `sync`, `prewarm`, and `serve` all call the same library-owned resolver

**Acceptance criteria**

* `sync`, `prewarm`, and `serve` share the same embedding resolution path
* The shared resolver lives outside the CLI entrypoint
* The fix preserves current fallback compatibility unless a test proves hard-fail
  is required

**Posture**

* test-first

### Unit 3 — Improve embedding diagnostics for operator recovery

**Goal**

Make degraded embedding behavior actionable without changing the underlying
compatibility policy.

**Primary surfaces**

* `src/main.rs`
* `src/embed/`

**Tasks**

* Write or extend failing diagnostics assertions before changing operator-facing text
* Distinguish model unavailable, cache/path lookup failure, no-embed mode, and
  degraded fallback behavior in operator-facing text
* State the impact of degraded mode and the next recovery action explicitly
* Preserve stdout/stderr discipline so diagnostics do not corrupt metrics output

**Acceptance criteria**

* A missing or invalid embedding model reports an actionable message naming the
  degraded behavior and next recovery step
* No-embed and fallback modes are distinguishable to the operator
* Diagnostics align with the shared resolver from Unit 2

**Posture**

* test-first

### Unit 4 — Characterize incremental sync progress output contract

**Goal**

Write failing tests that define what "useful progress" means for the human-facing
`sync` command while preserving machine-readable output modes.

**Primary surfaces**

* `tests/`
* `src/main.rs` output contract as behavior under test

**Tasks**

* Extend CLI-output tests to cover sync progress lines, percentage (or equivalent
  bounded completion indicator), and final summary output
* Pin stderr/stdout separation so progress remains human-visible but does not
  corrupt structured metrics output
* Cover no-work, single-source, and multi-source incremental-sync cases

**Acceptance criteria**

* Failing tests define percentage or equivalent completion context expectations
* Tests prove `--metrics` stays parseable after progress work
* Tests cover both advancing progress and completion-summary behavior for
  non-`--full` sync execution

**Posture**

* characterization-first

### Unit 5 — Implement incremental sync progress reporter

**Goal**

Add operator-meaningful progress reporting to `sync` using existing progress
primitives from `prewarm` instead of inventing a new progress model.

**Primary surfaces**

* `src/main.rs`
* `src/sync/`

**Scope exclusion**: `src/mcp/` is explicitly OUT OF SCOPE for this unit.
Shared MCP status surface extensions are not required for sync CLI progress
and must not be introduced unless a separate, explicitly planned and reviewed
follow-up proves necessity.

**Tasks**

* Thread a progress callback through the incremental sync CLI path similarly to `prewarm`
* Emit percentage or equivalent bounded progress with source/file context on stderr
* Keep final completion summary and metrics output coherent
* Do NOT extend or modify `src/mcp/` types or shared status surfaces

**Acceptance criteria**

* A release-built `sync` run shows visible in-flight progress for long-running work
* Progress output includes enough context to distinguish current source/file and
  completion state
* The change is limited to non-`--full` sync execution and does not widen pipeline contracts
* No changes to `src/mcp/` or shared MCP status types are introduced
* Existing prewarm progress surfaces remain unchanged

**Posture**

* test-first

### Unit 6 — Characterize full-sync stage progress contract

**Goal**

Write failing tests that define the bounded stage-progress expectations for
`sync --full` without requiring fine-grained pipeline callbacks.

**Primary surfaces**

* `tests/`
* `src/main.rs` full-sync output contract as behavior under test

**Tasks**

* Add failing tests for `--full` stage-entry/stage-complete progress on stderr
* Pin stdout/stderr separation for `--full` plus metrics compatibility
* Bound the contract to stage progress, not per-file percentage, unless an
  existing pipeline seam already exposes finer detail

**Acceptance criteria**

* Failing tests define clear stage-progress expectations for `sync --full`
* Tests prove `--full --metrics` remains parseable
* The test scope does not assume new pipeline APIs beyond stage announcements

**Posture**

* characterization-first

### Unit 7 — Implement full-sync stage progress

**Goal**

Provide informative in-flight progress for `sync --full` using bounded
stage-level announcements without broad pipeline refactoring.

**Primary surfaces**

* `src/main.rs`

**Tasks**

* Emit stage-start and stage-complete progress for the full-sync path on stderr
* Keep final completion summary and metrics output coherent
* Defer deeper pipeline callback work unless Unit 6 proves an existing seam is sufficient

**Acceptance criteria**

* `sync --full` no longer appears stalled throughout a long run
* Stage progress remains bounded and operator-meaningful
* The implementation does not require widening `pipeline::run(...)` contracts

**Posture**

* test-first

## Dependency Order

1. Unit 1A (embedding characterization)
2. Unit 1B (shared resolver divergence characterization)
3. Unit 2 depends on Unit 1A and Unit 1B
4. Unit 3 depends on Unit 2
5. Unit 4 depends on Unit 1A (fixture patterns reusable)
6. Unit 5 depends on Unit 4 and should reuse invariants validated in Unit 2
7. Unit 6 depends on Unit 1A
8. Unit 7 depends on Unit 6

## Runtime Verification Plan

Ship must execute a release-built validation run after implementation:

1. Build release binary
2. Run `sync` against the current `.graphtor/config` registries in this workspace
3. Confirm progress appears during execution, not only at the end
4. Confirm embedding diagnostics are actionable if the model cannot be resolved
5. Capture final summary and any degraded-mode messaging for closure notes

### Release smoke pass/fail criteria

The release verification passes only if all of the following are true:

* The release binary starts and reads the current `.graphtor/config` registry set
  without requiring manual config edits
* During a multi-source incremental run, `sync` emits in-flight progress on
  stderr with bounded completion context
* During a `sync --full` run, stderr shows bounded stage progress instead of
  final-only output
* If the embedding model cannot be resolved, the operator sees a message that
  names the cause class, degraded behavior, and recovery hint
* `sync --metrics` remains parseable and unpolluted by progress text
* The final summary accurately reports synced/updated/skipped counts

The release verification fails if any of the following occur:

* No visible in-flight progress appears during a long-running multi-source sync
* `sync --full` provides only final-only output with no bounded stage visibility
* Diagnostics mention an embedding failure without describing impact or recovery
* Metrics output contains human progress text
* A previously working local config now hard-fails without an explicit policy decision

## Requires plan hardening

yes

## Plan Hardening

### Risk triggers

* Runtime-affecting CLI behavior change
* Shared command-entry embedding resolution path
* Release verification depends on real workspace configuration and local data

### Protected invariants

* `sync --metrics` must remain machine-readable
* `.graphtor/config` files remain read-only during implementation
* Existing fallback compatibility is preserved unless tests prove otherwise
* Path normalization stays consistent across Windows local-source flows

### Proposed actions

#### ProposedAction 1

* `summary`: Add hermetic characterization tests using copied or read-only
  workspace-derived registry fixtures
* `targets`: `tests/`, copied fixture inputs shaped from `.graphtor/config/*.yaml`
* `change_kind`: local edit
* `action_risk`: low
* `rollback`: revert test additions while keeping fixture provenance notes
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 2

* `summary`: Centralize embedding resolution across `sync`, `prewarm`, and `serve`
* `targets`: `src/embed/`, `src/main.rs`
* `change_kind`: local edit
* `action_risk`: moderate
* `rollback`: revert to prior command-local resolver wiring
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 3

* `summary`: Add operator-facing degraded embedding diagnostics without changing
  machine-readable output contracts
* `targets`: `src/main.rs`, `src/embed/`
* `change_kind`: local edit
* `action_risk`: moderate
* `rollback`: revert messaging changes while keeping shared resolver if safe
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 4

* `summary`: Add human-facing incremental sync progress output on stderr
* `targets`: `src/main.rs`, `src/sync/`
* `change_kind`: local edit
* `action_risk`: moderate
* `rollback`: revert CLI formatting and callback wiring
* `approval_required`: no
* `action_result`: planned

#### ProposedAction 5

* `summary`: Run release-build verification against the current workspace registries
* `targets`: release binary execution, `.graphtor/config/*.yaml`, local data paths
* `change_kind`: stateful local execution
* `action_risk`: high
* `rollback`: stop the run, preserve logs, and restore or re-sync any local index
  state affected during validation if the run exposes a regression
* `approval_required`: yes — reconfirm before running against the operator's live working set
* `action_result`: planned

#### ProposedAction 6

* `summary`: Add bounded full-sync stage progress without widening pipeline contracts
* `targets`: `src/main.rs`
* `change_kind`: local edit
* `action_risk`: low
* `rollback`: revert full-sync stage announcements
* `approval_required`: no
* `action_result`: planned

### Deepened verification

**Unit 1**

* Verify fixture shape matches the current `.graphtor/config` multi-file registry layout
* Reproduce the current embedding warning/fallback path in a failing test

**Unit 2**

* Verify config-derived model resolution does not regress no-embed paths
* Verify error text distinguishes lookup/config/path failures
* Verify sync/prewarm/serve resolve the same model settings for the same config

**Unit 3**

* Verify degraded diagnostics stay on stderr and preserve metrics parseability
* Verify recovery guidance differs between missing model, disabled embeddings,
  and cache/path lookup failures

**Unit 4**

* Verify stderr carries progress while stdout remains suitable for structured outputs
* Verify no-work and already-synced runs still report meaningful completion
* Verify multi-source runs show advancing bounded progress

**Unit 5**

* Verify progress callback wiring does not regress final summary output
* Verify percentage or equivalent completion stays monotonic and bounded
* Verify background-sync/shared status surfaces remain compatible if extended

**Unit 6**

* Verify `--full` stderr carries bounded stage progress while stdout remains suitable
  for structured outputs
* Verify `--full` completion still reports meaningful final summary behavior

**Unit 7**

* Verify `--full` stage progress no longer leaves long runs silent
* Verify the implementation stays in `src/main.rs` without widening pipeline APIs

### Rollback procedure

1. Revert the release-sync hardening commits
2. Restore prior command-entry embedding loading behavior
3. Remove sync progress output changes if they prove noisy or break automation
4. Keep characterization tests that exposed the issue, adjusted to describe the reverted behavior if needed

### Monitoring and validation window

Manual observation is required because this is a local CLI runtime surface:

* **SLI / key signals**: progress updates appear during release sync; embedding
  errors are actionable; final completion summary remains accurate
* **Observation surface**: local release-terminal output and any generated logs
* **Baseline**: current release sync provides final-only output and opaque model warnings
* **Alert threshold**: no visible in-flight progress on a multi-source run, or a
  model failure message that does not identify cause plus fallback impact
* **Owner**: operator running Ship validation
* **Observation window**: first release-validation run after merge, plus one
  follow-up manual sync after any config change touching embeddings

### Pre-deploy audit checklist

* Verify no `.graphtor/config` files are modified by the implementation
* Verify rollback is a normal code revert with no data migration required
* Verify `sync --metrics` parseability is covered by tests before merge
* Verify live-workspace release validation has explicit operator reconfirmation
* Verify the monitoring/validation window above is carried into closure
* Verify release validation instructions still target local-only workspace resources

## Quality Gates

Ship must run and pass the repository gate sequence before merge:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`
3. `cargo test --all-targets`
4. `cargo audit`

### Rollback triggers

* `sync --metrics` output becomes non-parseable
* Release sync hangs or appears stalled without progress on a multi-source run
* Embedding resolution regresses a previously working config

### Unresolved operator decisions

None. The operator already specified the desired release-validation scope and
the need for actionable progress visibility.

<!-- plan-review-attempt: 1 -->

## Plan Review

### Review summary

* Reviewed against constitution compliance, scope boundary, task granularity, test-first ordering, strict-safety, dependency coherence, acceptance criteria, release-observability, and learnings alignment
* Verified the previously blocking revisions:
  * all listed `ProposedAction` entries now include `action_risk`
  * the Constitution Check covers Principles I-XI with applicability notes
  * Unit 1 is split into Unit 1A and Unit 1B
  * Unit 5 explicitly excludes `src/mcp/` and shared-surface changes in its scoped work
* Hardening was required for this runtime-affecting plan and is materially present, but one blocking scope/architecture contradiction remains

### Gate decision

**FAIL**

The revised plan fixes the prior blocking findings, but it still leaves Unit 5
internally inconsistent about whether progress remains CLI-only or must preserve
a canonical shared progress shape. That ambiguity is likely to force either
shared-surface scope creep or a second progress model during harvest.

### Findings

#### P1

* **Scope boundary / learnings alignment**  
  Unit 5 now declares `src/mcp/` and shared status changes out of scope, but the
  plan still requires "one canonical shared progress event/state shape" and the
  deepened verification notes still mention shared status compatibility "if
  extended". The current shared `SyncStatus::InProgress` only carries
  `{source, current, total}`, while Unit 5's acceptance criteria require
  source/file progress context on stderr.  
  **Recommendation**: resolve this before harvest by choosing one path
  explicitly: either keep Unit 5 CLI-only and remove the canonical
  shared-shape/shared-status language from this plan, or create a separate,
  explicitly scoped follow-up unit for shared status and parity work.

#### P2

* **Task granularity**  
  Unit 2 and Unit 3 still combine test authoring with cross-command production
  changes across `src/main.rs` and `src/embed/`. That is unlikely to satisfy the
  plan's own "fewer than 3 files" heuristic without further decomposition.  
  **Recommendation**: either rely on the characterization units for red tests
  and keep Units 2 and 3 implementation-only, or split each into smaller
  follow-on units before harvest.

* **Release-observability**  
  The monitoring section names signals, thresholds, and an owner, but the
  observation window is event-based rather than time-bounded. The
  `release-observability` rules require an explicit duration and closure
  condition.  
  **Recommendation**: add a concrete observation window duration and closeout
  rule for post-merge validation.

* **Strict-safety scope discipline**  
  The top-level freeze-scope still allows "the minimal shared status surface if
  required", which weakens the explicit Unit 5 scope exclusion and leaves a live
  escape hatch for the blocked shared-surface work.  
  **Recommendation**: remove that escape hatch from this plan or move it into a
  separately reviewed follow-up.

#### P3

* **Acceptance criteria precision**  
  Unit 5 uses subjective phrases such as "visible in-flight progress" and
  "enough context".  
  **Recommendation**: quantify the minimum expected progress signal so harvest
  does not need to infer the contract.

### Dimensions with no blocking findings

* Constitution Check coverage across Principles I-XI is now materially complete
* Characterization units still precede the corresponding implementation units in
  the dependency order
* Every unit includes at least one verifiable acceptance criterion

<!-- plan-review-attempt: 2 -->

## Plan Review

### Review summary

* Re-reviewed after the Unit 5 scope and architecture revision
* Verified that Architecture Constraints now define the shared progress shape
  as the existing `prewarm`-style callback rather than MCP type extensions
* Verified that Safety Mode freeze-scope explicitly freezes out `src/mcp/`
* Verified that Unit 5, Architecture Constraints, and Safety Mode now align on
  CLI-only progress work
* Hardening remains required and materially present

### Gate decision

**PASS**

The prior P1 is resolved. The plan no longer implies MCP/shared-status type
changes, `src/mcp/` remains frozen out, and Unit 5 is now consistent with the
Architecture Constraints and Safety Mode. No new P0/P1 findings were
identified.

### Findings

#### P2

* **Residual verification wording**  
  Unit 5 deepened verification still says "Verify background-sync/shared status
  surfaces remain compatible if extended" (`lines 486-491`). That wording no
  longer matches the explicit freeze-out and can mislead harvest, even though
  it no longer forces MCP scope creep.  
  **Recommendation**: reword this bullet to say shared status surfaces remain
  unchanged in this plan, or defer any compatibility check to the separately
  planned follow-up.

* **Task granularity**  
  Units 2 and 3 still combine test authoring with production edits across
  `src/main.rs` and `src/embed/`. This remains advisory rather than blocking
  because the characterization units still establish the red phase first.  
  **Recommendation**: if harvest needs tighter task boundaries, keep the red
  tests anchored to Units 1A/1B and split any broad implementation follow-ons.

#### P3

* **Acceptance criteria precision**  
  Unit 5 still uses subjective phrases such as "visible in-flight progress" and
  "enough context" (`lines 266-268`).  
  **Recommendation**: quantify the minimum progress signal so harvest can
  convert the unit into deterministic tasks and tests.

### Dimensions with no blocking findings

* Architecture Constraints no longer imply MCP/shared-status type changes
* Safety Mode freeze-scope no longer permits `src/mcp/` edits
* Unit 5 now stays consistent with the top-level scope exclusion and
  architecture boundary
* Plan hardening remains present with explicit `ProposedAction`,
  `action_risk`, and rollback coverage
* No new P0/P1 findings were identified in constitution, architecture, scope,
  Rust feasibility, or learnings alignment review
