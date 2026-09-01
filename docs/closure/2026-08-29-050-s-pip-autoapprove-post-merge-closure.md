---
date: 2026-08-29
slug: 050-s-pip-autoapprove-post-merge-closure
shipment: 050-S
mode: post-merge
status: READY
owner: "@softwaresalt"
---

# Post-Merge Closure — 050-S Harden VS Code pip Auto-Approve Allow-List

PR [`#109`](https://github.com/softwaresalt/graphtor-docs/pull/109) merged
shipment `050-S` at `4fba2500797c46fe2bd9d79e1e8e1ca350367725` (merge commit,
merge-commit strategy per Constitution Principle XI / P-009; parents
`16913bf` + `adee5e8`, tree identical to `adee5e8` — a fast-forwardable
merge). Confirmed `MERGE_CONFIRMED` independently via
`gh pr view 109 --json state,mergedAt,mergeCommit` (`state: MERGED`,
`mergedAt: 2026-08-29T17:47:47Z`) and
`git merge-base --is-ancestor 4fba250 origin/main` (exit 0).

## Summary of the Change

Removed the blanket `"pip": true` entry from
`chat.tools.terminal.autoApprove` in `.vscode/settings.json`. That bare key
auto-approved any command line containing the substring `pip`, including
arbitrary package-install commands that can execute package build-backend
code inside agent terminal sessions — a least-privilege violation flagged by
deliberation `003-DL`. No replacement entry was added because no documented
workflow invokes `pip` directly. The three existing anchored
`/^python \.scripts\/...$/` auto-approve entries are byte-for-byte
unchanged. Backlog items `057-F` (feature) and `057.001-T` (task) were
archived by the implementation branch; this closure archives the shipment
record `050-S` itself.

Config-only change: no Rust source, no build/test surface touched.

## Invariants to Preserve

* No bare-substring or non-anchored auto-approve key exists in
  `chat.tools.terminal.autoApprove`.
* The three existing anchored Python-script entries remain byte-for-byte
  unchanged.
* `.vscode/settings.json` remains valid JSON.

Verified post-merge (this session): current `.vscode/settings.json` on
`origin/main` contains exactly 3 entries, all anchored `/^...$/` regexes
with `matchCommandLine: true`; no `pip` key of any form remains.

## Validator Evidence (Runtime Verification)

No runtime surface changed — this is a static editor-configuration file
consumed by VS Code's Copilot Chat terminal auto-approve engine, not by any
`graphtor-docs` binary or MCP server code path. No monitoring or runtime
probe is invented for this surface (per closure scope guidance); the
functional evidence already gathered pre-merge stands as the record:

* `.vscode/settings.json` parses as valid JSON (implementation-branch check,
  re-confirmed on the merge commit in this session).
* No bare or non-anchored `pip` auto-approve key remains (re-confirmed).
* A representative `pip install sample-package` command line matches no
  auto-approve entry (implementation-branch characterization script,
  `docs/scratch/2026-08-29-pip-autoapprove-tdd-check.py`, 5/5 checks passed —
  left untracked/ephemeral per instruction, not committed).
* All three anchored Python-script patterns still match their exact command
  lines (re-confirmed).

**Verdict**: `PASS`. No manual checkpoints applicable (no OAuth/payment/
email/external-service flow in scope).

## Pre-Deploy Audits

Not applicable — no feature flag, migration, schema, or cross-service
dependency. `Full local build: not applicable` (config-only, no Rust
source changed), as recorded in the PR's Local Review Readiness block and
re-confirmed here.

## Deployment / Rollout Path

Merge-only. `.vscode/settings.json` takes effect the next time any VS Code
window opens this workspace — no build, release, or restart of the
`graphtor-docs` binary is involved.

## Post-Deploy Checks

* Confirmed (this session): merged `.vscode/settings.json` matches the
  expected final state exactly (3 anchored entries, no `pip` key).
* No further action required — the change is a static permission removal
  with immediate effect; there is no delayed-activation window to watch.

## Risky Action Record

| ProposedAction | ActionRisk | ActionResult |
|---|---|---|
| Remove blanket `"pip": true` auto-approve grant from `.vscode/settings.json` | low (permission *removal* only — narrows agent auto-approve surface, cannot introduce a new capability) | applied (implementation branch, pre-existing to this session) |
| Archive shipment `050-S` via safe-close (never the cascade `backlogit_ship_shipment`) | low (single-artifact archival; manifest items `057-F`/`057.001-T` were `pre-archived`; protected set empty — complete-feature shipment, verified not assumed) | applied, verified no cascade (see `.backlogit/reconcile/050-S-safe-close-20260829-105547.md`) |
| Create post-merge closure branch `post-merge/057-f-pip-autoapprove-hardening` directly from `origin/main` while carrying an uncommitted operator `.gitignore` edit across the switch | low (read-verified: `.gitignore` committed blob identical between prior HEAD `adee5e8` and `origin/main` before switching; SHA-256 of the dirty file re-verified unchanged after) | applied, verified byte-for-byte preserved |

## Healthy Signals

* `.vscode/settings.json` continues to parse as valid JSON in every future
  session.
* No operator or agent session reports an auto-approved arbitrary `pip`
  command.
* The three anchored Python-script entries continue to auto-approve their
  exact documented commands without operator friction.

## Failure Signals

* Any future edit accidentally reintroduces a bare `pip` (or other
  bare-substring) auto-approve key.
* A documented workflow that legitimately needs `pip` automation is blocked
  with no reviewed alternative in place.

## Monitoring Plan

Manual observation only — this is a single-developer, local-only editor
setting with no dashboard, log stream, or alerting surface to wire up.
`docs/decisions/2026-08-24-vscode-pip-autoapprove-hardening-deliberation.md`
and the plan doc remain the durable record of intent for any future review.

## Rollback Trigger

A documented clone/automation workflow hits unexpected manual-approval
friction that blocks legitimate work with no reviewed alternative.

## Rollback Procedure

Never restore the blanket `"pip": true` grant. If automation is genuinely
required, add exactly one separately reviewed, anchored
`/^<exact command line>$/` entry with `matchCommandLine: true`, mirroring
the existing three entries.

## Validation Window

None open — the change is a static, immediately-effective config removal
with no async rollout. Verified in place on `origin/main` as of this
closure.

## Owner

`@softwaresalt` (sole maintainer).

## Backlog Closure Evidence

* Pre-mode: `.backlogit/reconcile/050-S-pre-20260829-105327.md` — `PROCEED`
  (both manifest items `pre-archived`, 0 orphans).
* Safe-close: `.backlogit/reconcile/050-S-safe-close-20260829-105547.md` —
  `CLOSED` (protected set empty and verified, not assumed; shipment record
  archived as its own single artifact with merge SHA `4fba250` recorded;
  never the cascade `backlogit_ship_shipment`).
* Post-mode: `.backlogit/reconcile/050-S-post-20260829-105618.md` —
  `PROCEED` (all archive files present, no deletions).
* `backlogit doctor`: 140 pre-existing issues found, 0 related to `050-S`,
  `057-F`, `057.001-T`, or `051-S` (all pre-existing `archived_from_self_ref`
  / orphan debt on unrelated legacy items — out of scope for this closure).
* `051-S` (dependent shipment, `dependencies: [050-S]`) confirmed `queued`
  and dependency-unblocked now that `050-S` reached the terminal `archived`
  status.

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Manual observation (proportionate — no runtime surface changed) |
| Pre-deploy audit | N/A — no migration/flag/cross-service dependency |
| Runtime verification | `PASS` — static config validated pre- and post-merge |
| Post-deploy observation window | Closed — no async rollout, effect is immediate and already confirmed |
| Rollback trigger + procedure | Defined: fail closed, never restore blanket grant |
| Risky actions | All recorded above, `ActionResult: applied` |
| Backlog closure | `CLOSED` — see Backlog Closure Evidence above |

**Releasability status**: `READY` — all closure work is complete, verified,
and requires no further conditions or open observation window.

## Source Artifact Cleanup

For the shipped scope (`057-F`, `057.001-T`), read `custom_fields` on the
covering feature: `057-F.custom_fields = {harness_status: pending}` — no
`source_stash_id` or `source_deliberation_id` key present. Per protocol,
this counts as "not present → skip and log":

* **Stash**: no `source_stash_id` field on `057-F`. The historical label
  `stash-9CEC208C` records provenance only; confirmed via
  `backlogit stash get 9CEC208C` that the entry no longer exists (already
  harvested/removed in a prior session) — nothing to archive.
* **Deliberation**: no `source_deliberation_id` field on `057-F`. Deliberation
  `003-DL` is linked via a semantic `informs` link (not `custom_fields`) and
  remains `status: queued`. Left untouched — archiving it is outside the
  explicit `custom_fields.source_deliberation_id` cleanup criteria and would
  be planning-artifact scope creep beyond this closure's mandate.

## Documentation / Knowledge Graduation Review

* `docs/ARCHITECTURE.md` — no structural change; not touched.
* `AGENTS.md` — no agent or skill change; not touched.
* `docs/design-docs/` — no new durable design decision to graduate; the
  existing deliberation (`003-DL`) and plan
  (`docs/exec-plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md`)
  already record the rationale.
* `docs/product-specs/` — no requirement change.
* `docs/compound/` — no existing entry references `autoApprove` or
  `chat.tools.terminal`; nothing to consolidate or mark stale. A new entry
  was added for the post-merge branch technique used in this closure:
  `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`.

## Cross-References

* `docs/decisions/2026-08-24-vscode-pip-autoapprove-hardening-deliberation.md`
* `docs/exec-plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md`
* `docs/archive/memory/2026-08-29/ship-050-s-recovery-memory.md`
* `docs/compound/workflow-issues/post-merge-branch-preserve-dirty-file-2026-08-29.md`
* Follow-up items stashed: none (see Stash Follow-Up Review below).

## Stash Follow-Up Review

No follow-up tasks identified from this closure, the runtime verification
(config-only, no runtime surface), or the local review readiness record
(PR #109: `Follow-ups: none`). No stash entries created.
