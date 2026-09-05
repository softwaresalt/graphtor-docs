---
date: 2026-09-04
slug: pr-118-startup-checkpoint-recovery-post-merge-closure
shipment: "N/A — chore carry-forward PR, no shipment claimed (see Shipment Applicability below)"
mode: post-merge
status: READY_WITH_CONDITIONS
owner: "@softwaresalt"
compaction: done
---

# Post-Merge Closure — PR #118 Startup Checkpoint Recovery

PR [`#118`](https://github.com/softwaresalt/graphtor-docs/pull/118)
(`chore(startup): fix startup checkpoint recovery and quarantine legacy
checkpoints`, `fix/graphtor-startup-checkpoint-recovery` → `main`) merged at
`255020e14df99767549253d56ec3d53aa0b2bbd7` (merge commit, merge-commit
strategy per Constitution Principle XI / P-009). PR head at merge:
`176a1983a4482afd8264dd4a33a16328c996277a`.

**Merge confirmation**: `gh pr view 118 --json state,mergedAt,mergeCommit`
returned `state: MERGED`, `mergedAt: 2026-09-05T00:06:18Z`,
`mergeCommit.oid: 255020e14df99767549253d56ec3d53aa0b2bbd7`. Independently
confirmed via `git fetch origin main` +
`git merge-base --is-ancestor 255020e14df99767549253d56ec3d53aa0b2bbd7
origin/main` (exit 0).

## Shipment Applicability (non-applicable — recorded rationale)

This PR was executed as a standalone chore-branch session, never claimed
against any backlogit shipment. `backlogit search "checkpoint recovery"`
returns zero backlog items; no feature, task, or shipment record references
this branch or PR anywhere in `.backlogit/`. Per explicit operator scope for
this closure, shipments `048-S` (archived, `archived_status: active`) and
`049-S` (queued, blocked on `048-S` provenance per
`docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`)
were **not** claimed, mutated, archived, or otherwise touched by this session
or this closure.

Consequently:

* **`shipment-reconcile` is non-applicable.** The skill's `mode: pre` /
  `mode: safe-close` / `mode: post` all require a `shipment_id` and operate
  against a shipment manifest (`custom_fields.items`); there is no shipment
  record for this PR to reconcile against. Fabricating one would itself be a
  P-010 violation (Ship has no shipment-creation authority) and was not done.
* **No `backlogit_archive_item` / `backlogit_move_item` / safe-close
  sequence runs in this closure.** There is no manifest to archive and no
  shipment status to transition.
* **`autoharness gate pipeline-topology --mode manual --phase ambient`**
  (ambient is the only phase that resolves without a `--shipment` target) was
  run instead, as a topology sanity check independent of shipment scope: `PASS`
  (`worktree_topology: WORKTREE_TOPOLOGY_OK`, `active_shipment_invariant:
  active_shipment_ids: []`) — confirming no shipment is active in this
  workspace and the single-worktree invariant (P-016) holds.

## Summary of the Change

Resolved a startup/launcher regression plus accumulated backlogit checkpoint
schema debt on PR #118. GitHub's repository ruleset auto-triggered a Copilot
review on every push (`review_on_push: true`); across the branch's full
lifecycle this produced **7 Copilot review submissions**, verified via the
GraphQL `reviews` connection (`submittedAt` + `comments.totalCount` per
node), not the 4 stated in an earlier draft of this closure. Of those 7:

* **5 rounds carried new, actionable comments** requiring a reply and/or a
  fix: round 1 (5 comments, `3080dc8`) → 4 fixed in-branch, 1 deferred as
  stash `CCAC612D`; round 2 (1 comment — the `start.sh` cwd-anchoring bug)
  → fixed in `25a1290`; round 3 (2 comments — `.mcp.json` shim conflict +
  stale readiness) → 1 deferred as stash `578B8678`, fixed in `d4f0029`;
  round 4 (2 comments — generated-launcher/template-drift) → deferred as
  stash `BAD41DF2`/`8AFB7B3A` per P-021 C2 (see below); round 5 (4 comments
  — stash-entry `kind` field mismatches + stale doc/circuit-breaker wording)
  → fixed in `176a198`.
* **2 rounds were clean/informational** (`🔵`, "0 new" top-level comments):
  one mid-lifecycle pass, and the **final pre-merge pass** (immediately
  before the `255020e` merge). That final pass reported "0 new" but surfaced
  **7 previously-suppressed comments** in its review body (GitHub suppresses
  repeat comments on unchanged code across rounds) that were never
  individually replied to, fixed, or captured: 6 identical findings that the
  new `.backlogit/archive/checkpoints/*.disposition.json` files publicly
    record a specific Windows domain account identifier (redacted here; see
    the merged `.disposition.json` files themselves for the literal value —
    intentionally not repeated in this documentation to avoid creating an
    additional public copy) in their `operator` field, plus 1 finding that the
    in-flight PR-body readiness block was already stale relative to the
    then-current head. See the new Follow-up Handoff entry below — this
    closure cannot rewrite already-merged `main` history to redact these
    disposition-file contents.

Change surface:

* **`start.sh` / `start.ps1`** — anchored every child process to the
  workspace directory regardless of invocation cwd
  (`cd "$script_dir"` / `Set-Location -LiteralPath $PSScriptRoot`), and made
  `.env.local` loading behave identically on both launchers with
  unconditional-override precedence (each `KEY=VALUE` line replaces any
  value already inherited from the invoking shell/CI). Previously, `.env.local`
  auto-loading was **not** PowerShell-only, and `start.sh` was **not**
  missing it entirely: the pre-fix Bash launcher already loaded
  `.env.local`, but guarded each assignment with a skip-if-already-set check
  (`export` only `if [[ -z "${!env_name+x}" ]]`), so any variable already
  present in the invoking shell's environment silently shadowed the
  operator's `.env.local` value. Commit `25a1290` removed that guard,
  matching `start.ps1`'s unconditional-override precedence and eliminating
  the actual parity bug (a Copilot review finding on PR #118 confirmed this
  distinction; see `git show 25a1290 -- start.sh`).
  `docs/configuration.md` was updated in the same PR to describe the
  corrected, symmetric behavior.
* **`.mcp.json`** — added `${workspaceFolder}` env bindings
  (`BACKLOGIT_WORKSPACE`, `ENGRAM_WORKSPACE`) to the `backlogit`/`engram`
  server entries, fixed a Windows-only backslash path
  (`.copilot\graphtor-mcp-shim.cjs` → `.copilot/graphtor-mcp-shim.cjs`) on
  the `graphtor-docs` entry, and removed the `--read-only` flag from that
  entry's `serve` invocation.
* **`.autoharness/config.yaml`** — removed `graphtor-docs` from the
  `sidecars` auto-sync list (now `["backlogit", "engram"]`), consistent with
  `graphtor-docs` being launched as a full (non-read-only, non-sidecar)
  MCP server entry per the `.mcp.json` change above.
* **`.backlogit/checkpoints/`** — quarantined six schema-invalid legacy
  checkpoint records (parseable JSON, but non-compliant with CheckpointV1:
  missing required fields, or lifecycle statuses such as `blocked` /
  `superseded-closed` that are not valid states) to
  `.backlogit/archive/checkpoints/` with disposition metadata, under
  explicit operator approval. Confirmed a stale-looking `active` Stage
  checkpoint (`checkpoint-20260829-163933.json`) was in fact superseded by a
  later `resolved` checkpoint for the same session/shipment/feature, not an
  interrupted run. `backlogit checkpoint list` now reports
  `needs_quarantine: 0`, `quarantined: 0`, `total: 9`, all `status: resolved`
  — reconfirmed independently in this closure session (see Post-Deploy
  Checks below).
* **New compound-learning entry** (added within the PR itself, not by this
  closure): `docs/compound/workflow-issues/checkpoint-schema-and-lifecycle-controls-2026-09-03.md`.
* **`.gitignore`** — added `docs/scratch/`, `.backlogit/logs/`,
  `.backlogit/runtime/` (ephemeral/tool-runtime paths, not previously
  excluded).
* **`scripts/deploy-harness.ps1` / `.sh`, `scripts/git_commands.py`,
  `scripts/run_git_commands.sh`** — ad hoc diagnostic/deploy-harness script
  changes made during the session's own investigation; the two new
  `git_commands.py` / `run_git_commands.sh` helpers were flagged by Copilot
  review as an out-of-scope hygiene concern and deferred rather than
  fixed in-branch (see P-021 Deferred Findings below).
* **`.github/copilot/settings.local.json`** — new file, local Copilot CLI
  effort/model override (`effortLevel: high`, `model: gpt-5.6-sol`).
* No Rust source (`src/`), `Cargo.toml`, or `Cargo.lock` changes — the
  `graphtor-docs` binary's own implementation is untouched. This is **not**
  the same as "zero runtime surface touched": the launcher/`.mcp.json`
  changes above DO touch the required `cli` runtime surface defined by
  `runtime_validation.validator_manifest` (see Validator Evidence below,
  which corrects an earlier, incorrect `N/A` framing of this point).

Across the full 7-round Copilot review history above, all review threads
were resolved before merge, `mergeStateStatus: CLEAN`, and
`autoharness gate copilot-review 118 --enforcement auto` reported
`SATISFIED: PASS` at the final head (`176a198`) prior to merge.

## P-021 Deferred Findings

Four `DEFERRED SCOPE EXPANSION` stash entries were captured during the PR's
own lifecycle (prior to this closure session), all still `active` in the
stash as of this closure. A fifth entry (`67BA0629`) was captured **by this
closure session itself**, during its own review-remediation pass — the
single P-021 C2 capture-only stash creation Ship's Role Boundary (P-010, C5
carve-out) permits:

| Stash ID | Finding | Disposition |
|---|---|---|
| `CCAC612D` | `scripts/git_commands.py` / `scripts/run_git_commands.sh` are ad hoc, hardcode a single-developer path, embed uncommented debugging SHAs | Captured pre-PR (threadless path); requires Stage triage/deliberation |
| `578B8678` | `.mcp.json` `graphtor-docs` entry's shim is gitignored/unverified by install/tune, and `x-graphtor-managed: true` risks the generator silently overwriting its custom shape on next install | Captured + thread resolved on PR #118; requires Stage triage/deliberation |
| `BAD41DF2` | The externally-versioned `autoharness` template `scripts/start.ps1.tmpl` was not updated to match the `start.ps1` fix, so a future `autoharness install`/`tune` could regenerate a stale launcher | Captured + thread resolved on PR #118; confirmed no `.tmpl` file exists anywhere in this repo's tracked source — genuinely outside this repository's contract surface |
| `8AFB7B3A` | Same finding as `BAD41DF2`, for `scripts/start.sh.tmpl` | Captured + thread resolved on PR #118; same rationale as `BAD41DF2` |
| `67BA0629` | This closure artifact's `## Validation Window` `Duration` bullet anchors to "this closure's merge" (PR #119, still pending) rather than PR #118's actual runtime-affecting merge (`255020e`, already landed), which would omit the already-elapsed observation interval | Captured + thread resolved on PR #119 (`PRRT_kwDORiB5E86ff2Hy`), during this closure PR's own review-remediation pass; requires Stage triage/deliberation |

For the first four entries, this closure session performed **no** stash
mutation of any kind (create, edit, harvest, or archive) — read-only lookup
only, per Ship's Role Boundary (P-010); all four remain open for a future
Stage session. The fifth entry (`67BA0629`) was created by this same
closure session under the narrow P-021 C2 capture-only carve-out — a single
permitted creation, never an edit, harvest, or archive of any entry by
Ship (including that one, once created). All five remain open for a
future Stage session's triage/deliberation of the underlying Validation
Window anchor expansion. **Update**: `67BA0629`'s structured `kind` field
was subsequently corrected from the schema-invalid `chore` to the valid
`task` by Stage (not Ship) — committed by Ship on Stage's behalf at
`f33aa91` — resolving the separate schema-validity defect flagged by
Copilot thread `PRRT_kwDORiB5E86ff6Uc`; see Follow-Up Handoff item 9.

## Invariants to Preserve

* `start.sh` and `start.ps1` both anchor to their own script directory
  before any further work, regardless of invocation cwd.
* `.env.local` loading is unconditional-override (never skip-if-already-set)
  on both launchers, so operator-authored `.env.local` values always win
  over inherited process environment.
* `.mcp.json`'s `graphtor-docs` entry uses a forward-slash-relative shim
  path and no longer forces `--read-only`.
* No bare/malformed checkpoint record exists in `.backlogit/checkpoints/`;
  quarantined originals remain byte-preserved in
  `.backlogit/archive/checkpoints/` (never deleted).
* `048-S` and `049-S` remain exactly as they were before this session
  (archived / queued respectively) — untouched by this PR or this closure.

Verified post-merge (this session): `backlogit checkpoint list` — `total: 9`,
`needs_quarantine: 0`, `quarantined: 0`, all `status: resolved`; `backlogit
get 048-S` — still `status: archived`, `archived_status: active` (unchanged);
`backlogit get 049-S` — still `status: queued` (unchanged).

## Validator Evidence (Runtime Verification)

**Corrected during this closure PR's own local review**: an earlier draft of
this artifact recorded runtime verification as `N/A`. That was incorrect.
`.autoharness/workspace-profile.yaml`'s `runtime_surfaces` block
(`web_ui`/`public_api`/`background_jobs`, all `false`) is a *different*
section from `runtime_validation.validator_manifest`, which explicitly
defines a required `cli` surface — `cli-status` and `mcp-stdio-startup`
command probes (both `required: true`) plus a `mcp-client-smoke` manual
checkpoint (`required_for_release: true`). PR #118 changed exactly the
mechanisms that invoke this surface: `start.sh`/`start.ps1` (the launcher
that anchors cwd before any child process, including a `graphtor-docs
serve` invocation, starts) and `.mcp.json`'s `graphtor-docs` entry (the
command VS Code/Copilot uses to start the MCP server, including a path fix
and the `--read-only` removal). The `runtime_surfaces` flags being `false`
does not exempt this change from the `validator_manifest` contract.

No Rust source (`src/`, `Cargo.*`) changed, so the `graphtor-docs` binary's
own behavior is unaffected by this PR — but the probes below still confirm
no regression was introduced in how that binary is invoked, and a direct,
isolated replay of the launcher fix's own logic validates the specific
regression this PR addressed.

**Automated probes (this closure session, against the merge commit)**:

| Probe | Command | Result |
|---|---|---|
| `cli-status` (required) | `cargo run --release -- status` | ✅ PASS — reported 5 configured sources and opened the read-only SQLite datastore with no error |
| `mcp-stdio-startup` (required) | `graphtor-docs.exe serve` with stdin redirected from `NUL` | ⚠️ DOES NOT LITERALLY MEET THE DECLARED SIGNAL — the server started cleanly with no startup-phase error (posture resolution, DB open, embedding model load, "starting MCP STDIO server" all logged with no error), but the manifest's declared success signal is "logs no startup error, **and exits cleanly on stdin EOF**." On stdin EOF this process instead exited with `error: MCP server failed to start / Caused by: connection closed: initialize request` (exit code 2) — not a clean exit. The manifest's own note explains why: this probe sends no JSON-RPC `initialize` request and therefore "cannot distinguish the regression from a normal handshake," deferring real handshake coverage to the `mcp-client-smoke` manual checkpoint; the accompanying `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md` (lines 403-409) independently documents this exact exit-on-EOF behavior as expected for a closed-stdin, no-`initialize` invocation, and not diagnostic of a regression — the same result would occur on any correctly functioning build run this way, not only PR #118's merge commit. Net assessment: **no evidence of a regression introduced by PR #118**, but this probe's current form cannot, by itself, certify a literal clean exit per the manifest's own wording; an automated initialize-sending harness does not yet exist (manifest note: deferred, see PR #114). |

**Targeted launcher-fix verification (this closure session)**: rather than
running the full interactive `start.ps1` (which launches the Copilot CLI
interactively and would hang this automated session), the two specific
logic blocks changed by commit `25a1290` were replayed verbatim against an
isolated scratch directory (never touching the real workspace `.env.local`):
cwd was anchored away from the target directory, a scratch `.env.local` set
`TEST_OVERRIDE_VAR="from-env-local"`, and the process environment was
pre-seeded with `TEST_OVERRIDE_VAR=from-inherited-shell` (simulating an
inherited shell/CI value) before replaying the `Set-Location` + `.env.local`
loader logic. Result: `Get-Location` resolved to the scratch directory
(cwd-anchoring confirmed) and `$env:TEST_OVERRIDE_VAR` became
`from-env-local` (unconditional-override precedence confirmed, matching the
fixed behavior, not the pre-fix skip-if-already-set behavior). Scratch
directory removed after the check; no tracked file was touched.

**Manual checkpoint — `mcp-client-smoke` (required for release)**: **NOT
PERFORMED** in this closure session. No live MCP client capable of driving
a real `initialize` → `search_local_docs`/`search_semantic` session was
available in this automated context. Per the validator manifest's own
documented fallback: *"Record the cli-status and mcp-stdio-startup probe
evidence and note the client check as deferred. Because this checkpoint is
required_for_release, that fallback yields READY_WITH_CONDITIONS at best —
the initialize handshake stays unvalidated — and never a clean READY."*
This closure follows that fallback exactly; see the downgraded
Releasability Evidence below.

**Verdict**: `READY_WITH_CONDITIONS` — `cli-status` passes cleanly; the
launcher-fix logic itself was independently replay-verified; and
`mcp-stdio-startup`'s exit-on-EOF behavior shows no evidence of a
regression introduced by PR #118. Two conditions remain open, both
stemming from the same underlying gap (no automated harness exists yet to
drive a real `initialize` handshake over stdio): (1) `mcp-stdio-startup`
cannot, by itself, literally certify the manifest's declared clean-exit
signal, and (2) the required `mcp-client-smoke` manual checkpoint has not
been performed. No other blocked prerequisite.

## Pre-Deploy Audits

**Corrected during Copilot review round 4 of this closure PR**: an earlier
draft marked this entire section `N/A`. That was wrong — this artifact
elsewhere classifies PR #118 as runtime-affecting (Validator Evidence above)
and records the `.mcp.json` `--read-only` removal as a capability widening
(Risky Action Record). Per the release-observability Pre-Deploy Audit
Checklist, each required check is evaluated individually below instead of a
blanket section-level `N/A`:

* **Feature flags / rollout gates configured** — `N/A`, with rationale:
  `graphtor-docs` has no feature-flag or phased-rollout mechanism; it is a
  single-developer, local-only CLI/MCP server with no staged-deployment
  ring or canary path (see Deployment / Rollout Path below). The
  `--read-only` removal takes effect unconditionally the next time a client
  restarts the `graphtor-docs` MCP server — there is no gate to configure or
  verify, and none is needed for a single always-on local process.
* **Rollback procedure documented and actionable** — YES. If the widened
  write capability from removing `--read-only` causes an unwanted mutation
  via the `graphtor-docs` MCP tool surface, re-add `--read-only` to that
  entry's `serve` invocation in `.mcp.json` and restart/reload the client —
  a single-line config revert, no code change, no data loss, no restart of
  the `graphtor-docs` binary's own release artifact required. This is now
  folded into Rollback Procedure and Failure Signals below as an explicit
  bullet (previously only the launcher-script and checkpoint-quarantine
  rollback paths were documented there).
* **Data migration / schema changes backward-compatible or have a revert
  path** — `N/A`, with rationale: PR #118 shipped no data migration or
  schema change. The `.backlogit/checkpoints/` quarantine action is a
  byte-preserving relocation (see Invariants to Preserve / Risky Action
  Record above), not a schema migration, and already has a documented,
  verified revert path (restore byte-identical originals from
  `.backlogit/archive/checkpoints/`).
* **Dependent services aware of the change (cross-service boundaries)** —
  `N/A`, with rationale: the `graphtor-docs` `.mcp.json` entry is consumed
  only by the sole maintainer's own local VS Code/Copilot CLI client on a
  single workstation; there is no other service, team, CI consumer, or
  downstream integration to notify of this config change.
* **Monitoring plan complete** — YES, via the release-observability
  contract's own no-monitoring-system fallback ("record the monitoring plan
  as a structured checklist... and flag it as a manual observation
  requirement"). **Corrected during Copilot review rounds 5/6**: this still
  carries the same SLI / observation-location / baseline / alert-threshold /
  owner fields the contract requires when a monitoring system exists — only
  the dashboard-or-query mechanism is substituted with a named manual check,
  since `graphtor-docs` has no dashboard, alerting, or metrics surface to
  instrument for launcher/dev-tooling config. See Monitoring Plan below for
  the full structured checklist (cross-linked here per the
  release-observability Closure Integration contract rather than
  duplicated verbatim); one baseline item there (zero unintended writes via
  the widened MCP surface) is explicitly recorded as unverified pending the
  outstanding `mcp-client-smoke` checkpoint, carried into Releasability
  Evidence condition (2) below rather than assumed healthy.

**Full local build applicability**: `Full local build: not applicable for a
documentation/backlog config change` does not apply here either, since the
*original* PR #118 did touch shell-script and JSON/YAML config (not
"documentation-only"); the prior Ship session's Local Review Readiness
record for PR #118 recorded full local build evidence
(`cargo check`/`fmt`/`clippy --pedantic`/`test`, all green) at each of its 5
actionable review-remediation rounds (see the corrected round accounting in
Summary of the Change above). This closure independently re-verified the
same gates against the merge commit itself (see Quality Gates below). This
closure PR itself (`#119`) is documentation-only (no `src/`, `Cargo.*`,
`.mcp.json`, or launcher-script changes) — see the PR's own Local Review
Readiness block for the applicable non-applicability rationale on *that*
build boundary, which is separate from PR #118's build evidence recorded
here.

## Deployment / Rollout Path

Merge-only. `graphtor-docs` is a single-developer, local-only CLI/MCP-server
project with no hosted deployment or release-binary distribution pipeline
gated on this change. The corrected launcher scripts and `.mcp.json`/config
take effect the next time this workspace's `start.sh`/`start.ps1` is invoked
or VS Code reloads its MCP configuration — no build or restart of the
`graphtor-docs` binary itself is required (no product code changed).

## Post-Deploy Checks

Re-verified on the merge commit in this closure session:

* `git merge-base --is-ancestor 255020e origin/main` → exit 0.
* `backlogit checkpoint list` → `total: 9`, `needs_quarantine: 0`,
  `quarantined: 0`, all `status: resolved` (matches the pre-merge state; no
  regression).
* `backlogit get 048-S` / `backlogit get 049-S` → unchanged
  (`archived`/`archived_status: active` and `queued`, respectively).
* `gh pr checks 118` → `build` pass (2m12s), `detect code changes` pass
  (12s), `pipeline topology gate` pass (13s) — all green at merge.
* Quality gates re-run locally against the merge commit — see below.

## Risky Action Record

| ProposedAction | ActionRisk | ActionResult |
|---|---|---|
| Quarantine six schema-invalid legacy checkpoint records (moved, not deleted, to `.backlogit/archive/checkpoints/`) | low (byte-preserving relocation with disposition metadata; explicit operator approval obtained in the underlying PR session before this closure) | applied (pre-existing to this closure; re-verified, not re-applied) |
| Remove `--read-only` from the `graphtor-docs` `.mcp.json` entry | low-moderate (widens the MCP server's write capability for this local dev-tooling config; reviewed and accepted by Copilot review + operator during PR #118's own lifecycle) | applied (pre-existing to this closure) |
| Create post-merge closure branch `post-merge/startup-checkpoint-recovery` directly from freshly-fetched `origin/main` | low (standard P-016-compliant closure branch creation; verified single worktree before and after) | applied, verified |
| Defer 2 out-of-scope autoharness-template findings (`BAD41DF2`, `8AFB7B3A`) via P-021 C2 capture instead of editing an externally-versioned tool project | low (no repository file exists to edit; capture-only, no code change) | applied (pre-existing to this closure) |
| Capture 1 out-of-scope Validation Window anchor finding (`67BA0629`) via P-021 C2 instead of rewriting the window's anchor logic outside this session's authorized scope | low (stash-only creation, no code/content change; the sole permitted mutation under the P-021 C5 capture-only carve-out) | applied by this closure session; downstream consequence: the captured entry's `kind: chore` value was not a member of the stash schema's enum (`PRRT_kwDORiB5E86ff6Uc`) — Ship had no authority to correct this, so it was left for operator or Stage action (see Follow-Up Handoff item 9); Stage subsequently corrected the field to `task`, committed by Ship at `f33aa91` — the underlying Validation Window anchor expansion remains open for Stage triage/deliberation |

This closure session's own new risky action is the `67BA0629` stash
capture above (low risk, capture-only, no code/content change) plus branch
creation and read-only verification/reporting; the checkpoint-quarantine
and `--read-only` removal actions were already applied and merged prior to
this session and are listed here for completeness of the closure record.

## Healthy Signals

* `start.sh` and `start.ps1` both `cd`/`Set-Location` into their own script
  directory before doing any further work, on every future invocation.
* `.env.local` values consistently override inherited environment on both
  launchers.
* No new schema-invalid checkpoint record appears in
  `.backlogit/checkpoints/` (i.e., `needs_quarantine` stays `0`).
* `048-S`/`049-S` remain in their current, unmodified state until a future
  Stage/operator session deliberately remediates the `049-S` topology
  blocker described in
  `docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`
  (explicitly preserved, not compacted or altered by this closure — see
  Compaction below).

## Failure Signals

* A future session creates a checkpoint that bypasses `schema_version: 1`
  validation (the recurrence-control gap noted in the compound entry as
  already tracked upstream in the backlogit stash, not duplicated here).
* `start.sh`/`start.ps1` regress to cwd-dependent behavior (e.g. a future
  autoharness `install`/`tune` regenerates them from a stale external
  template — this exact risk is the subject of deferred stash entries
  `BAD41DF2`/`8AFB7B3A`).
* `049-S`'s topology-gate blocker (`PREDECESSOR_NOT_SHIPPED` against
  archived `048-S`) is worked around instead of properly remediated by a
  future session.
* An unintended write or mutation occurs via the `graphtor-docs` MCP tool
  surface as a direct result of the `--read-only` removal (the capability
  widening recorded in Risky Action Record and evaluated in Pre-Deploy
  Audits above) — the symptom to watch for is any write operation the
  operator did not explicitly request completing successfully against this
  workspace's indexed sources during a live MCP client session.

## Monitoring Plan

**Manual observation only** — single-developer, local-only tool; no
dashboard, alerting, or metrics system exists for launcher scripts or
dev-tooling config in this repository. **Corrected during Copilot review
rounds 5/6**: the release-observability contract's no-monitoring-system
fallback still requires the plan recorded as a structured checklist (the
same SLI / observation-location / baseline / alert-threshold / owner fields
a dashboard-backed plan would carry, substituting a named manual check for
the missing dashboard/query) rather than a free-form note — this section
was previously prose-only and did not satisfy that structure:

| Field | Value |
|---|---|
| SLIs / key metrics | (1) launcher cwd-independence — `start.sh`/`start.ps1` resolve script-relative paths correctly regardless of invocation directory; (2) checkpoint schema health — the count of records in `.backlogit/checkpoints/` failing `schema_version: 1` validation (`needs_quarantine`); (3) MCP write-capability scope — whether any write/mutation occurs via the `graphtor-docs` MCP tool surface that the operator did not explicitly request |
| Observation query / check location | No dashboard exists; the manual checks are: (1) invoke `start.sh`/`start.ps1` from a working directory other than the repo root and confirm no path-resolution error; (2) inspect `.backlogit/checkpoints/` for any record failing `schema_version: 1` (validation logic recorded in the compound entry below); (3) during any live MCP client session against this workspace, observe tool-call activity for unrequested writes — the outstanding `mcp-client-smoke` manual checkpoint (see Validator Evidence above) is the first such observation opportunity |
| Baseline | (1) zero path-resolution regressions since the `25a1290` anchoring fix (re-verified this closure); (2) `needs_quarantine: 0` as of this closure (6 pre-existing invalid records already quarantined, byte-preserving, not deleted); (3) zero writes observed to date via the widened `--read-only`-removed surface — but this specific baseline item is **unverified by a live client session**, since `mcp-client-smoke` has not yet been performed (carried into Releasability Evidence condition (2) below as open, not silently assumed healthy) |
| Alert / investigation threshold | Any single occurrence of a Failure Signal above (a path-resolution regression, a new schema-invalid checkpoint record, or one unrequested write via the MCP tool surface) triggers investigation immediately — no rate/frequency threshold applies at this single-developer, single-consumer scale |
| Owner | `@softwaresalt` (sole maintainer) |
| Post-deploy observation window | See Validation Window below — not duplicated here per the release-observability Closure Integration contract |
| Rollback trigger / procedure | See Rollback Trigger and Rollback Procedure below — not duplicated here per the release-observability Closure Integration contract |

The compound entry
(`docs/compound/workflow-issues/checkpoint-schema-and-lifecycle-controls-2026-09-03.md`)
and the preserved 049-S topology-blocker memory file remain the durable
record for any future session picking up related work.

## Rollback Trigger

Any Failure Signal above observed by the operator or a future agent session.

## Rollback Procedure

* Launcher regression: revert `start.sh`/`start.ps1` to the pre-`25a1290`
  behavior is **not recommended** (reintroduces the cwd-dependent bug this
  PR fixed); instead, re-apply the same anchoring fix.
* Checkpoint quarantine: quarantined files remain byte-identical in
  `.backlogit/archive/checkpoints/` and can be restored if a false-positive
  quarantine is ever identified (none is known as of this closure).
* `--read-only` capability widening: if the widened write capability causes
  an unwanted mutation via the `graphtor-docs` MCP tool surface (see the
  matching Failure Signal above), re-add `--read-only` to the
  `graphtor-docs` entry's `serve` invocation in `.mcp.json` and
  restart/reload the client — a single-line config revert, no code change,
  no data loss.
* No code rollback applies to `048-S`/`049-S` — this session made no change
  to either.

## Validation Window

**Open, bounded observation window** (release-observability contract:
runtime-affecting work requires an explicit duration and owner, not a
"no window" declaration): the launcher/`.mcp.json` change is
runtime-affecting (see Validator Evidence above), and the required
`mcp-client-smoke` manual checkpoint remains outstanding, so this closure
cannot treat observation as already closed out.

* **Duration**: 14 calendar days from this closure's merge, **or** until
  the `mcp-client-smoke` manual checkpoint is performed (whichever comes
  first) — the checkpoint's own completion is the natural close-out
  signal for this window, since it is the one piece of evidence this
  closure could not automate.
* **Owner**: `@softwaresalt` (sole maintainer).
* **What to watch**: the Healthy/Failure Signals above during ordinary use
  of `start.sh`/`start.ps1` and any live MCP client session against this
  workspace.
* **Outcome recording**: whoever closes this window (by performing
  `mcp-client-smoke` or at the 14-day mark) MUST record the outcome —
  healthy, degraded, or rolled back — as a follow-up note; this closure
  cannot pre-record an outcome it has not observed. This is folded into
  Follow-Up Handoff item 6 below rather than duplicated as a separate
  item.

## Owner

`@softwaresalt` (sole maintainer).

## Quality Gates (Re-Verified Post-Merge, this closure session)

| Gate | Result |
|---|---|
| `cargo check --all-targets` (merge commit `255020e`) | ✅ `Finished dev profile in 8.52s` |
| `cargo fmt --all -- --check` | ✅ clean |
| `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` | ✅ clean |
| `cargo test` (all binaries + doc-tests) | ✅ all green, 0 failed (largest binaries: 362 passed, 215 passed; every other binary 1–14 passed; 1 doc-test intentionally ignored) |
| `cargo audit --ignore RUSTSEC-2026-0041 --ignore RUSTSEC-2025-0056 --ignore RUSTSEC-2025-0141 --ignore RUSTSEC-2025-0057 --ignore RUSTSEC-2025-0119 --ignore RUSTSEC-2024-0436 --ignore RUSTSEC-2026-0249 --deny warnings` (the exact CI-equivalent invocation from `.github/workflows/ci.yml`) | ✅ exit 0 — no new/un-triaged advisory; plain `cargo audit` without the allowlist reports the 1 pre-existing, already-tracked `RUSTSEC-2026-0041` vulnerability (owned by task `013.008-T`, stash `94D655D7`, documented in `audit.toml`) plus 6 pre-existing unmaintained-crate advisories, all pre-dating this PR and unrelated to its changes — not a regression |
| CI (`build`, `detect code changes`, `pipeline topology gate` on PR #118, final head `176a198`/merge `255020e`) | ✅ all pass (`gh pr checks 118`) |

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Manual observation, structured checklist (SLIs, check locations, baseline, alert threshold, owner) — see Monitoring Plan above; one baseline item (zero unintended writes via the widened MCP surface) unverified pending `mcp-client-smoke` (see condition (2) below) |
| Pre-deploy audit | Checklist-evaluated per item (see Pre-Deploy Audits above): flags/rollout `N/A` (no flag mechanism), rollback procedure documented + actionable (YES), migration/schema `N/A` (byte-preserving quarantine only, verified revert path), dependent-service awareness `N/A` (single local consumer), monitoring plan complete (YES) |
| Runtime verification | `READY_WITH_CONDITIONS` — `cli-status` passes cleanly and the launcher fix's own logic was independently replay-verified; `mcp-stdio-startup` shows no evidence of a regression but its exit-on-EOF result does not literally certify the manifest's declared clean-exit signal (no initialize-sending harness exists yet); the required `mcp-client-smoke` manual checkpoint was not performed this session (see Validator Evidence above) |
| Post-deploy observation window | Open — 14-day bounded window (or until `mcp-client-smoke` is performed, whichever first), owner `@softwaresalt`; see Validation Window above |
| Rollback trigger + procedure | Defined above |
| Risky actions | All recorded above, `ActionResult: applied` |
| Backlog closure | `N/A` — no shipment claimed; `048-S`/`049-S` confirmed unmodified |
| Compaction (P-020) | See Compaction section below |

**Releasability status**: `READY_WITH_CONDITIONS`. **Conditions**: (1) the
`mcp-stdio-startup` probe's exit-on-EOF result cannot, by itself, literally
certify the manifest's declared clean-exit signal until an
initialize-sending harness exists (tracked in the manifest's own note, see
PR #114) — no regression evidence was found, but literal automated
certification is not yet possible; (2) perform the
`mcp-client-smoke` manual checkpoint (call `get_status`, then
`search_local_docs`/`search_semantic` against an indexed source, from a real
MCP client) at the next opportunity a live client session is available, to
validate the `initialize` handshake and advertised tool surface — the one
piece of required-for-release evidence this closure could not automate.
Neither condition blocks the closure PR itself (both are evidence
generation, not a code or config change) but should be recorded as
follow-ups for whoever next has an interactive MCP client session against
this workspace, or picks up the initialize-handshake harness work (see
Follow-up Handoff below).

## Compaction (P-020)

`compact-context` was invoked with `target: all` after this closure's own
session memory checkpoint was written (see Cross-References). **Outcome:
`done`.** Two 2026-09-04 memory files that were part of the now-completed
PR #118 lifecycle (`pr-118-readiness-copilot-remediation-memory.md`,
`pr-118-cycle4-circuit-breaker-halt-memory.md`) were compacted into
`docs/memory/compacted/2026-09-04-pr-118-startup-checkpoint-recovery-compacted.md`
and the verbose originals archived to `docs/archive/memory/2026-09-04/`
with content preserved except for the two required cross-reference
corrections described next. Zero external tracked citing documents
required updating (verified via `git grep`); the two files' own mutual
internal citation and one frontmatter `source` self-reference were
corrected to the new archive paths.

The `docs/memory/2026-09-03/checkpoint-quarantine-recurrence-controls-memory.md`
and `docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`
files, and this closure session's own new memory checkpoint, were reviewed
and **deliberately excluded** from compaction — see the full accounting and
rationale in
`docs/memory/compacted/2026-09-04-pr-118-startup-checkpoint-recovery-compacted.md`.
In particular, the `049-S` topology-blocker file documents *open*, not
completed, work and remains fully intact and undisturbed for a future
Stage/operator session.

## Documentation / Knowledge Graduation Review

* `docs/ARCHITECTURE.md` — no structural change; grepped for
  `start.sh`/`start.ps1`/`checkpoint` references, none found; not touched.
* `AGENTS.md` — no agent or skill change; grepped, only pre-existing
  unrelated mentions found; not touched.
* `docs/design-docs/` — no new durable design decision to graduate; this is
  a bug-fix/config-hygiene PR, not a design change.
* `docs/product-specs/` — no requirement change.
* `docs/configuration.md` — already updated within PR #118 itself to
  describe the corrected, symmetric `.env.local`-loading behavior; no
  further edit needed.
* `docs/compound/` — reviewed the two entries most directly relevant to this
  PR's changes:
  * `checkpoint-schema-and-lifecycle-controls-2026-09-03.md` — added within
    PR #118 itself; reviewed, accurate, kept as-is.
  * `mcp-json-workspacefolder-camelcase-2026-08-24.md` — describes the
    identical `${workspaceFolder}` casing fix for a *prior* PR (#106); this
    PR's `.mcp.json` diff shows the `BACKLOGIT_WORKSPACE`/`ENGRAM_WORKSPACE`
    env bindings were **absent** on `main` before this PR (not merely
    mis-cased), meaning the earlier fix's env additions did not persist
    forward from PR #106 to this branch's base. The entry's guidance
    (exact camelCase `${workspaceFolder}`) remains 100% accurate and was
    followed correctly again in this PR — **classification: keep, no
    changes required**. Not creating a duplicate or "recurrence" entry: the
    existing entry's Prevention section already covers this exact class of
    defect, and this closure found no new distinct root cause to document.

## Follow-Up Handoff (for a future Stage session)

Per Ship's Role Boundary, no backlog item or shipment was created, edited,
or archived by this closure, and no stash entry was ever edited, harvested,
or archived by Ship. One stash entry (`67BA0629`) was created under the
narrow P-021 C2 capture-only carve-out during this closure PR's own
review-remediation pass — the sole permitted Ship mutation. It, and the
four pre-existing entries below, are recorded here as a pointer for Stage.
**Update**: `67BA0629`'s structured `kind` field was subsequently
corrected by Stage (not Ship) from the schema-invalid `chore` to the
valid `task`; Ship committed that Stage-authored correction at `f33aa91`
without itself editing the entry's content — see item 9 below.

1. **`CCAC612D`** — ad hoc git diagnostic scripts
   (`scripts/git_commands.py`, `scripts/run_git_commands.sh`) — durable
   rewrite vs. deletion decision needed.
2. **`578B8678`** — `graphtor-docs` `.mcp.json` entry's untracked shim +
   managed-entry generator conflict — needs an architecture decision in
   `src/workspace/mcp_config.rs`'s managed-entry contract.
3. **`BAD41DF2`** / **`8AFB7B3A`** — `start.ps1.tmpl` / `start.sh.tmpl`
   drift in the externally-versioned `autoharness` tool project (outside
   this repository's git tree entirely) — needs coordination with that
   separate project, not a graphtor-docs-repo fix.
4. **`049-S` topology blocker** (not a stash entry — a live shipment-graph
   issue) — `048-S`'s `archived_status: active` has no recorded `shipped`
   lifecycle event despite complete delivery/closure evidence
   (`docs/closure/2026-09-01-047-s-048-s-closure-summary.md`), which blocks
   `049-S`'s pipeline-topology readiness with `PREDECESSOR_NOT_SHIPPED`.
   Backlogit 1.10.1 has no supported repair operation for this gap; Ship has
   no authority to invent one. Full detail preserved (not compacted) in
   `docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`.
5. **Domain account name publicly exposed in 6 merged disposition files**
   (discovered during this closure PR's own local review, via the GraphQL
   `reviews` history for PR #118) — `.backlogit/archive/checkpoints/
   checkpoint-{20260429-214618,20260429-215617,20260701-064559,
   20260822-073402,20260822-090657,20260822-092508}.json.disposition.json`
   each carry a specific Windows domain account identifier in their
   `operator` field (redacted here — see the merged `.disposition.json`
   files themselves for the literal value; deliberately not repeated in
   this documentation to avoid creating an additional public, searchable
   copy of it). GitHub's Copilot reviewer flagged this as 6 "suppressed"
   (previously missed) comments on the PR's **final**, clean
   (0-new-comment) pre-merge review pass — suppressed because the
   underlying code had not changed since an earlier round, so no new
   top-level thread was ever created for Ship to see, reply to, or capture
   during the PR's own lifecycle. This closure **cannot** remediate it: the
   content is already part of merged `main` history (`255020e`), and
   rewriting merged history (`git reset`, force-push, history-editing) is
   unconditionally forbidden under Ship's Role Boundary. Needs a
   Stage-triaged decision: accept as a low-severity,
   non-credential identity disclosure and document, or schedule a follow-up
   commit that replaces the 6 `operator` values with a repository-safe
   actor identifier (the disposition files themselves, not history, would
   be edited — no rewrite required for that remediation path).
6. **`mcp-client-smoke` manual checkpoint outstanding** (required for
   release, not performed this closure session — see Validator Evidence
   above) — needs a live MCP client session (call `get_status`, then
   `search_local_docs`/`search_semantic` against an indexed source) at the
   next opportunity, to validate the `initialize` handshake and advertised
   tool surface for the launcher/`.mcp.json` changes this PR made. This
   checkpoint's completion is also the natural close-out signal for the
   open 14-day post-deploy observation window (see Validation Window
   above); whoever performs it MUST record the window's outcome
   (healthy/degraded/rolled back) at the same time.
7. **`mcp-stdio-startup` cannot literally certify a clean exit** (see
   Validator Evidence above) — the probe as currently specified sends no
   `initialize` request, so any invocation (on any build) exits via the
   same `connection closed: initialize request` error on stdin EOF; the
   manifest's own note defers a real fix to an automated initialize-sending
   test harness that "does not exist in `tests/` yet (deferred, see
   PR #114)." Until that harness exists, this required probe can only ever
   confirm startup-phase health, not the manifest's literal "exits cleanly
   on stdin EOF" signal — worth flagging to whoever picks up PR #114's
   deferred work, since it also affects every future closure that touches
   this surface, not just this one.
8. **`67BA0629`** — this closure artifact's `## Validation Window`
   `Duration` bullet anchors to "this closure's merge" (PR #119, still
   pending) rather than PR #118's actual runtime-affecting merge
   (`255020e`, already landed) — needs the anchor corrected to the actual
   merge date so the observation window does not omit the already-elapsed
   interval. Captured by this closure session itself (P-021 C2), not by
   the underlying PR #118.
9. **Stash entry `67BA0629` carried a schema-non-conformant `kind` value**
   (`chore`, not a member of the `feature`/`task`/`bug`/`epic`/`unknown`
   enum defined in
   `.github/instructions/backlogit-sql-schema.instructions.md:81`) —
   flagged by Copilot review on this closure PR
   (`PRRT_kwDORiB5E86ff6Uc`); Ship's Role Boundary forbade editing any
   stash entry, including one it just created, so it required direct
   operator correction or a future Stage session's stash-triage
   authority. **Resolved**: Stage corrected the structured `kind` field
   from `chore` to `task`; Ship independently verified the diff (exactly
   one field, one entry, changed) and committed the correction at
   `f33aa91` on Stage's behalf, then replied to and resolved the thread.
   The entry's embedded free-text `(6) Kind: chore` string was
   intentionally left unchanged by that narrowly-scoped correction — only
   the structured field was in scope. The underlying Validation Window
   anchor expansion this entry captures remains open for Stage
   triage/deliberation.

Items 1–7 were neither created, edited, harvested, nor archived by this
closure — they are cited here solely so a future Stage/operator session
can locate them. Item 8 (`67BA0629`) was created by this closure session
under the P-021 C2 capture-only carve-out (the sole permitted mutation);
it was never edited, harvested, or archived by Ship afterward. Item 9
documented a data-quality defect in that same entry that this closure
session identified but had no authority to correct — Stage has since
corrected it (see item 9's own update above), and Ship's authority over
the entry remains unchanged (creation only, no edit/harvest/archive).

## Source-Artifact Retirement (backlogit)

Not applicable. This PR shipped no backlog feature or chore item (no
`048-F`/`-C`-style covering artifact exists for this branch), so there is no
`custom_fields.source_stash_id` / `source_deliberation_id` to read or report
on. This section documents that the check was performed and found no
covering item to inspect, per the same "not present → skip and log"
convention used in the `050-S` closure precedent when a field is genuinely
absent.

## Cross-References

* PR: https://github.com/softwaresalt/graphtor-docs/pull/118
* `docs/memory/2026-09-04/post-merge-closure-pr-118-session-memory.md` (this
  closure session's own memory checkpoint)
* `docs/memory/compacted/2026-09-04-pr-118-startup-checkpoint-recovery-compacted.md`
  (compacted summary of the two now-superseded PR-118-lifecycle memory
  files, produced by the P-020 `compact-context` invocation triggered by
  this closure)
* `docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`
  (preserved, **not** compacted — documents open `049-S` blocker work)
* `docs/compound/workflow-issues/checkpoint-schema-and-lifecycle-controls-2026-09-03.md`
* `docs/compound/workflow-issues/mcp-json-workspacefolder-camelcase-2026-08-24.md`
* Follow-up items (stash, read-only pointer, not mutated): `CCAC612D`,
  `578B8678`, `BAD41DF2`, `8AFB7B3A`
* Follow-up item (stash, created by this closure session under the P-021
  C2 capture-only carve-out; never edited/harvested/archived by Ship
  afterward; structured `kind` field subsequently corrected from `chore`
  to `task` by Stage, committed by Ship at `f33aa91`): `67BA0629`
