---
date: 2026-09-13
slug: 049-s-mcp-serve-handshake-post-merge-closure
shipment: 049-S
feature: 056-F
mode: post-merge
status: READY_WITH_CONDITIONS
owner: "@softwaresalt"
compaction: done
---

# Post-Merge Closure — Shipment 049-S: Fix MCP serve initialize-handshake regression

PR [`#120`](https://github.com/softwaresalt/graphtor-docs/pull/120)
(`Fix MCP serve initialize-handshake regression (Copilot CLI OS error 232) —
shipment 049-S`, `chore/fix-mcp-serve-initialize-handshake-regression` →
`main`) merged at `98f8fc63024095b0b8697545986646a564677917` (merge commit,
merge-commit strategy per Constitution Principle XI / P-009).

**Merge confirmation**: `gh pr view 120 --json state,mergedAt,mergeCommit`
returned `state: MERGED`, `mergedAt: 2026-09-13T05:25:13Z`,
`mergeCommit.oid: 98f8fc63024095b0b8697545986646a564677917`. Independently
confirmed via `git fetch origin main` +
`git merge-base --is-ancestor 98f8fc63024095b0b8697545986646a564677917
origin/main` (exit 0), per the Merge Confirmation Gate.

## Summary of the Change

Shipment `049-S` delivered the fix for the MCP `serve` `initialize`-handshake
regression that caused Copilot CLI to fail with OS error 232 against the
`graphtor-docs` MCP server. Covering feature `056-F`; 8 manifest members
(`056.001-T`, `056.002-T`, `056.003-T`, `056.019-T`, `056.020-T`, `056.021-T`,
`056.022-T`, `056.023-T`). The feature also has 25 sibling tasks
(`056.004-T`–`056.018-T`, `056.024-T`–`056.033-T`) that share the same
covering feature `056-F` but were **not** part of this shipment's manifest —
these are addressed at length below because they were incidentally affected
by, and then repaired as part of, this closure session's shipment-recovery
work.

Change surface delivered by the shipment (merged in PR #120, adversarial
review already completed pre-merge — see
`docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md`):
corrected the `serve` initialize handshake, plus a new reusable
out-of-process test driver (`tests/common/serve_driver.rs`,
`tests/serve_handshake_driver_test.rs`, delivered by `056.002-T`) that
performs a real JSON-RPC `initialize` request/response cycle against the
actual compiled binary.

This closure document covers the **post-merge closure session itself**,
which additionally had to recover from a backlogit tool-capability gap
encountered while closing the shipment record (see Risky Action Record
below) — that recovery, its verification, and the remaining Step 6 pipeline
(runtime verification, documentation graduation, compound refresh, P-020
compaction, and this closure PR) are the primary subject of this document.

## Backlog Closure Evidence

* **Pre-archive reconciliation** (`mode: pre`, `expected_status: done`):
  `.backlogit/reconcile/049-S-pre-20260913T052820Z.md`, recommendation
  `PROCEED` — all 8 manifest items confirmed `status: done` in queue with no
  duplicate-assignment conflicts.
* **Safe-close attempt, halt, operator-authorized cascade exception,
  post-cascade repair, and resolution**: fully documented in
  `.backlogit/reconcile/049-S-halt-20260913T053721Z.md`
  (`mode: safe-close`, now `status: resolved`,
  `resolved_at: 2026-09-13T07:38:44Z`). Summary of the sequence:
  1. Safe-Close Mode steps 1–7 completed and verified cleanly (protected-set
     computation, baseline integrity gate, all 8 manifest items proven
     archivable).
  2. Step 8 (the final `shipment -> shipped` transition) halted: the
     installed backlogit version
     (`1.10.1-0.20260823032255-b07729386a31+dirty`) unconditionally refuses
     `backlogit move --status shipped` / MCP `backlogit_move_item`
     (`shipment_shipped_requires_envelope`) — a compiled-in structural
     guard, not a config toggle, exhaustively proven via multiple
     disposable-copy tests recorded in the halt handoff. Only
     `backlogit shipment ship` (the `ShipShipment` cascade, P-015-forbidden
     as a default path) can perform this transition.
  3. The operator, informed of the gap, explicitly authorized running the
     forbidden cascade as a **deliberate, one-time, real-time-approved P-015
     policy exception**. It was executed exactly once in a prior session.
  4. The cascade succeeded: `049-S` is now correctly `status: archived`,
     `archived_status: shipped`, `commit:
     98f8fc63024095b0b8697545986646a564677917` (re-verified this session —
     see Post-Mode Reconciliation below). As a disclosed, accepted, and
     bounded side effect, the cascade's recursive `releaseScopeItemIDs`
     resolution cleared `parent_id: 056-F` from exactly 25 sibling task
     records that share the covering feature but were outside the shipment
     manifest — the 25 IDs listed above. `056-F` itself and the 8 manifest
     tasks were confirmed untouched (byte-identical / correctly finalized).
  5. Repairing the 25 cleared `parent_id` fields via `backlogit adopt` was
     evaluated and rejected: disposable-copy proof showed `adopt` rewrites
     the hierarchical ID for 15 of the 25 (via non-gap-aware
     `NextTypedHierarchicalID`, colliding with existing dependency edges)
     and, for all 25, unconditionally injects
     `custom_fields.origin_feature` — neither behavior compliant with a
     field-only repair.
  6. The operator selected the simplest, least-invasive recorded option
     ("option 2" in the halt handoff addendum): a direct, manual,
     byte-exact restoration of only the missing `parent_id: 056-F`
     frontmatter line in all 25 records, using the pre-cascade Git baseline
     as ground truth, followed by `backlogit sync`.
  7. This session performed that repair (25 surgical single-line
     insertions via the frontmatter editor, at each record's correct
     alphabetical field position, anchored by unique `id:`/`labels`/
     `priority` context per file) and verified, per file, via
     `git diff` against the pre-cascade `HEAD`, that the **only** delta is
     the restored `parent_id` line plus the tool-managed `updated_at`
     timestamp — zero other content drift (no status, title, body, custom
     field, hierarchy path, or `origin_feature` change in any of the 25).
     `backlogit sync` was re-run afterward (`Indexed 527 artifacts`, 0
     parse failures) and content was re-verified unchanged post-sync.
  8. **Persisted, reproducible git-history evidence (added post-adversarial-review,
     addressing finding F1/F5 of
     `docs/closure/2026-09-13-049-s-post-merge-closure-adversarial-review.md`)**:
     the commit at `5128333` (tip of PR #120's own implementation work,
     before the cascade/repair cycle ever touched the working tree) already
     contained the correct, uncascaded `parent_id: 056-F` on all 25 sibling
     files. The cascade removal and the field-only repair both happened
     entirely in the **uncommitted working tree** across the prior and this
     session — neither state was committed independently — so a diff from
     that last real commit to the closure commit (`73453f3`) is a strictly
     stronger, git-verifiable proof than a comparison against an ad hoc
     snapshot file: it shows the *net* effect of cascade-then-repair against
     immutable history, not a self-reported intermediate comparison.
     Reproducible command, **pinned to the named closure commit
     `73453f3`** (not a floating `HEAD`, which has since advanced past
     `73453f3` as this closure's own remediation work continued to commit)
     — bash brace expansion enumerates all 25 exact paths, the two
     contiguous ID ranges from the operator's exact affected-IDs list,
     `056.004-T`..`056.018-T` and `056.024-T`..`056.033-T` — and its
     actual, re-verified output:

     ```
     $ git diff 5128333..73453f3 --numstat -- \
         .backlogit/queue/056.{004..018}-T.md \
         .backlogit/queue/056.{024..033}-T.md
     1	1	.backlogit/queue/056.004-T.md
     1	1	.backlogit/queue/056.005-T.md
     1	1	.backlogit/queue/056.006-T.md
     1	1	.backlogit/queue/056.007-T.md
     1	1	.backlogit/queue/056.008-T.md
     1	1	.backlogit/queue/056.009-T.md
     1	1	.backlogit/queue/056.010-T.md
     1	1	.backlogit/queue/056.011-T.md
     1	1	.backlogit/queue/056.012-T.md
     1	1	.backlogit/queue/056.013-T.md
     1	1	.backlogit/queue/056.014-T.md
     1	1	.backlogit/queue/056.015-T.md
     1	1	.backlogit/queue/056.016-T.md
     1	1	.backlogit/queue/056.017-T.md
     1	1	.backlogit/queue/056.018-T.md
     1	1	.backlogit/queue/056.024-T.md
     1	1	.backlogit/queue/056.025-T.md
     1	1	.backlogit/queue/056.026-T.md
     1	1	.backlogit/queue/056.027-T.md
     1	1	.backlogit/queue/056.028-T.md
     1	1	.backlogit/queue/056.029-T.md
     1	1	.backlogit/queue/056.030-T.md
     1	1	.backlogit/queue/056.031-T.md
     1	1	.backlogit/queue/056.032-T.md
     1	1	.backlogit/queue/056.033-T.md
     ```

     All 25 files show exactly "1 1" (one line added, one line removed) —
     re-verified directly (via the PowerShell equivalent, enumerating each
     of the 25 exact paths rather than a shell glob) during this session's
     own Copilot-review remediation pass, confirming the claim independently
     of the original self-reported table.

     Inspecting the actual line-level diff for every one of the 25 files
     confirms the single changed line in each is `updated_at:` only —
     `parent_id: 056-F` does not appear in any hunk at all, meaning it is
     **byte-identical** to the last real commit, net of the cascade+repair
     round-trip. Sample (`056.004-T.md`):

     ```diff
     -updated_at: 2026-08-29T16:45:19.4597226Z
     +updated_at: 2026-09-13T07:15:48.7327483Z
     ```

     `056-F` itself: `git diff 5128333..HEAD -- .backlogit/queue/056-F.md`
     produces **zero output** — not merely SHA-256-equal to a snapshot, but
     literally no diff against the last real commit at all.

     The 8 finalized manifest task archives show a uniform `5 2` numstat
     (queue -> archive relocation adds `archived_from`, `archived_status`,
     `commit`, and flips `status: done` -> `status: archived`, plus the
     `updated_at` timestamp bump) with `parent_id: 056-F` present as an
     unchanged context line in every hunk — confirmed via
     `git diff 5128333..HEAD -- .backlogit/archive/056.001-T.md` (and the
     other 7), e.g. no `origin_feature`, no ID change, no other drift.

     `049-S` itself: `git diff 5128333..HEAD --numstat -- .backlogit/archive/049-S.md`
     shows `5 2` for the queue->archive rename, consistent with the same
     relocation pattern (`archived_status: shipped`,
     `commit: 98f8fc63024095b0b8697545986646a564677917` added, `status`
     flipped, `updated_at` bumped) — no other field touched.
* **Post-archive reconciliation** (`mode: post`,
  `merge_commit_sha: 98f8fc63024095b0b8697545986646a564677917`): documented
  in the halt handoff's Post-Mode addendum — archive presence confirmed for
  `049-S` and all 8 manifest tasks, `git status -- ".backlogit/archive/"`
  showed no deleted-file drift (P-007 guard), and the resolution addendum's
  independent per-item verification table confirms `archived_status:
  shipped` / correct commit for the shipment and `archived_status: done` /
  correct commit / intact `parent_id: 056-F` for all 8 manifest tasks.
* **Re-verification this session** (independent of the prior session's own
  checks, performed fresh at session start before any further mutation):
  * `049-S` (`.backlogit/archive/049-S.md`): `status: archived`,
    `archived_status: shipped`,
    `commit: 98f8fc63024095b0b8697545986646a564677917` — confirmed
    unchanged.
  * `056-F` (`.backlogit/queue/056-F.md`): SHA-256 byte-identical to the
    pre-cascade snapshot — confirmed unchanged throughout, including after
    the 25-file repair.
  * All 8 manifest tasks: `status: archived`, `archived_status: done`,
    correct commit, `parent_id: 056-F` intact — confirmed untouched by the
    repair.
  * All 25 repaired sibling tasks: `status: queued`, `parent_id: 056-F`
    restored, no `origin_feature` field present, no other field drift.
* **Checkpoint and handoff resolution**: checkpoint
  `checkpoint-20260913-073130.json` (`phase:
  post-merge-closure-cascade-executed-repair-blocked`) resolved via
  `backlogit checkpoint resolve` only after the repair and Post-Mode
  verification above succeeded. The halt handoff record's frontmatter
  `status` was changed `open` -> `resolved` only after the same
  verification passed. Two earlier checkpoints from this same recovery
  arc (`checkpoint-20260913-053918.json`,
  `checkpoint-20260913-070341.json`) were already `resolved` from prior
  sessions and are retained as historical evidence.
* **Lock discipline**: the canonical logical shipment lock
  (`.backlogit/queue/.049-S.md.lock`, substituted to the archive-path
  target per the Halt Recovery Protocol once the record relocated) was
  held across the repair and Post-Mode verification and released at the
  end of Post-Mode, consistent with `shipment-reconcile`'s Logical
  Shipment Lock Contract.
* **Final backlog index resync**: `backlogit sync` run once more after all
  closure-doc-adjacent documentation edits (see below) — see Final
  Verification section.

## Risky Action Record

| Action | `ActionRisk` | Approval | Result |
|---|---|---|---|
| `backlogit shipment ship 049-S` (P-015-forbidden cascade, run as a one-time policy exception) | destructive (protected-set mutation) | Explicit, real-time operator authorization, prior session, after the non-cascading path was exhaustively proven unavailable | `applied` — shipment correctly archived `shipped`; 25 out-of-manifest siblings lost `parent_id: 056-F` as a disclosed, bounded side effect |
| Manual byte-exact `parent_id` restoration on 25 sibling task records | destructive (backlog artifact rollback of a prior mutation) | Explicit operator-selected "option 2" from the halt handoff's disposable-copy-proven options; this closure session executed it | `applied` — verified byte-exact vs. pre-cascade content except tool-managed `updated_at`, per file, this session |
| `backlogit sync` (twice: mid-repair, and final) | non-destructive (derived index rebuild) | Routine, no approval required (Continuity/derived-state row) | `applied` — 0 parse failures both times |

No other risky/destructive action was taken this session. `backlogit adopt`
was evaluated via disposable copies only (never run against a live record)
and rejected as unsafe for this repair.

## Runtime Verification

Per `.autoharness/workspace-profile.yaml` `runtime_validation`:

| Validator | Kind | Result |
|---|---|---|
| `cli-status` | command probe | ✅ PASS — exit 0, expected log line present |
| `mcp-stdio-startup` | command probe | Exit 2, `error: MCP server failed to start / Caused by: connection closed: initialize request`. **Not a regression.** This probe supplies no JSON-RPC `initialize` request (stdin redirected from `/dev/null`); the same exit code occurs on any correctly functioning build run this way. Documented precedent: `docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md` (line ~238) and `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md` (lines ~403-409), both establishing this exact methodology cannot exercise the real handshake. |
| `tests/serve_handshake_driver_test.rs` (supplemental, not in the validator manifest but directly relevant — delivered by this shipment's own `056.002-T`) | automated out-of-process test | ✅ PASS — `initialize_handshake_succeeds_against_a_readonly_consumption_workspace` performs a real JSON-RPC `initialize` request/response cycle against the compiled binary, asserts a non-empty negotiated `protocolVersion`, and asserts stdout stays protocol-clean; `server_control_forces_read_only_even_with_a_real_source_present` additionally cross-checks `tools/list` against the live manifest and asserts non-empty `get_status` output. All 4 tests in the file pass (see Quality Gates below). |
| `mcp-client-smoke` (required manual checkpoint) | manual, live MCP client | **Deferred** — no live interactive MCP client session is available in this autonomous pipeline execution. See Releasability below. |

`docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
T1's acceptance boundary already anticipated exactly this: the
`056.002-T` driver proves the handshake and tool-surface parts of the
manual checkpoint's scope; only `search_local_docs`/`search_semantic`
result-correctness from a real external client remains unautomated. The
`.autoharness/workspace-profile.yaml` `mcp-stdio-startup`/`mcp-client-smoke`
notes were updated this session to reflect that the previously-deferred
automated handshake harness now exists (see Documentation / Knowledge
Graduation Review below) — this is a factual correction, not a policy
change; `mcp-client-smoke` remains `required_for_release: true`.

## Quality Gates (this session, against merged `main` content)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` | ✅ clean |
| `cargo fmt --all -- --check` | ✅ clean |
| `cargo test --release` (all 55 test binaries + doc-tests) | ✅ all green, 0 failed (largest binaries: 362 passed, 224 passed; every other binary 1–14 passed; 1 doc-test intentionally ignored) |
| `cargo build --release` | ✅ succeeded |

## Pre-Deploy Audits

| Item | Status |
|---|---|
| Feature flags / rollout mechanism | N/A — no flag mechanism for this change |
| Rollback procedure documented and actionable | YES — see Rollback below; the shipment/backlog-recovery rollback path is separately documented in the halt handoff and is itself now closed |
| Migration / schema impact | N/A — no schema change; the backlog-artifact repair was a byte-exact frontmatter field restoration, not a migration |
| Dependent-service awareness | N/A — single local MCP server process, no downstream service dependency |
| Monitoring plan complete | YES — see Monitoring Plan below |

## Monitoring Plan

* **SLI**: MCP client `initialize` handshake success rate against the
  compiled binary.
* **Check location**: `tests/serve_handshake_driver_test.rs` (automated,
  runs in CI on every future PR touching `serve`); `mcp-client-smoke`
  manual checkpoint (deferred, see Releasability) for real-client
  end-to-end confirmation including `search_local_docs`/`search_semantic`.
* **Baseline**: automated driver green this session (4/4 tests); no known
  regression signal from the `mcp-stdio-startup` probe.
* **Alert threshold**: any future red run of `serve_handshake_driver_test.rs`
  in CI, or an operator-reported handshake failure from a live client,
  should be treated as a regression against this fix.
* **Owner**: `@softwaresalt`.

## Rollback

* **Code**: standard `git revert` of the merge commit
  `98f8fc63024095b0b8697545986646a564677917` if the handshake fix itself
  needs reversal (not expected; fully green across all gates).
* **Backlog artifacts**: the shipment/backlog recovery this closure
  performed is itself now fully verified and resolved (see Backlog Closure
  Evidence). No outstanding rollback is needed for the repair — it was
  verified byte-exact against the pre-cascade baseline before being
  accepted as final.

## Validation Window

Open, 14-day bounded window from this closure's commit, or until the
`mcp-client-smoke` manual checkpoint is performed (whichever first), owner
`@softwaresalt` — consistent with the PR #118 closure's established pattern
for this same profile.

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Complete, see above |
| Pre-deploy audit | Complete, see above |
| Runtime verification | `READY_WITH_CONDITIONS` — automated evidence (`cli-status`, `mcp-stdio-startup` non-regression, and the shipment's own `serve_handshake_driver_test.rs`) is strong and directly exercises the real `initialize` handshake, `tools/list`, and `get_status`; the required `mcp-client-smoke` checkpoint's remaining scope (`search_local_docs`/`search_semantic` result correctness from a real external client) was not performed this session |
| Post-deploy observation window | Open, see Validation Window above |
| Rollback trigger + procedure | Defined above |
| Risky actions | All recorded above, `ActionResult: applied` |
| Backlog closure | Complete — `049-S` `archived_status: shipped`, correct commit, all 8 manifest members correctly finalized, 25 out-of-manifest siblings repaired and verified byte-exact, checkpoint and halt handoff resolved |
| Compaction (P-020) | See Compaction section below |

**Releasability status**: `READY_WITH_CONDITIONS`. **Condition**: perform the
`mcp-client-smoke` manual checkpoint (`get_status`, then
`search_local_docs`/`search_semantic` against an indexed source, from a real
MCP client) at the next opportunity a live client session is available.
This does not block the closure PR itself (it is evidence generation, not a
code or config change) but is recorded as a follow-up — see Follow-up
Handoff below.

## Documentation / Knowledge Graduation Review

* `.autoharness/workspace-profile.yaml` — updated the `mcp-stdio-startup`
  probe notes and the `mcp-client-smoke` manual checkpoint's `reason`/
  `fallback` text: the previous claim that "no such harness exists in
  tests/ yet (deferred, see PR #114)" is now factually superseded by this
  shipment's own delivery of `tests/serve_handshake_driver_test.rs` /
  `tests/common/serve_driver.rs`. `required_for_release: true` was left
  unchanged — the manual checkpoint's remaining scope
  (`search_local_docs`/`search_semantic` correctness) is still
  unautomated.
* `docs/ARCHITECTURE.md` — reviewed; no structural change to graduate for
  this fix (bug-fix + test-harness addition, not an architectural change).
  Not touched.
* `AGENTS.md` — reviewed; no agent or skill change. Not touched.
* `docs/design-docs/` — no new durable design decision to graduate; the
  design deliberation for this fix already lives in
  `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
  and `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`,
  both pre-existing and unaffected.
* `docs/product-specs/` — no requirement change.
* `docs/compound/` — see the dedicated Compound Refresh report:
  `docs/closure/2026-09-13-049-s-compound-refresh.md`. Summary: created
  `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`
  (previously cited by 4 instruction files but absent) documenting the
  confirmed shipment status enum and the `shipment_shipped_requires_envelope`
  guard; updated `docs/compound/backlogit-shipment-state-machine.md` with a
  superseded-notice for its obsolete shipment-lifecycle guidance while
  preserving its still-accurate task-transition section; added a
  cross-reference from `clippy-allow-unknown-lints-msrv-guard-2026-08-24.md`
  to stash `6C174AA9`; and captured two new learnings flagged by the prior
  session's Learnings Researcher pass —
  `docs/compound/runtime-errors/mcp-serve-os-error-232-handshake-signature-2026-09-13.md`
  and
  `docs/compound/workflow-issues/backlogit-task-done-immediate-archive-relocation-2026-09-13.md`.

## Source Artifact Cleanup (handoff only — Ship does not mutate stash)

`056-F.custom_fields.source_stash_id: 7BF1961D` (singular field only; no
`source_stash_ids` plural list and no `source_deliberation_id(s)` fields
present on `056-F`). This is the source stash entry that fed the covering
feature for this shipment. Per Ship's Role Boundary, Ship does not archive
or otherwise mutate stash entries. **Recorded for a future Stage session**:
stash `7BF1961D` is eligible for the state-appropriate archive
(`stash_archive`, preserving traceability) now that `049-S` has fully
closed. Ship has not archived it and will not.

## Stash Follow-Up Review (handoff only — Ship does not mutate stash)

The following P-021 C2 deferred-scope-expansion stash entries were captured
during PR #120's review rounds and remain active (not yet archived) in
`.backlogit/stash.jsonl`, confirmed present this session. Per Ship's Role
Boundary these are **recorded here as a handoff for a future Stage session**
to triage/prioritize/deliberate — Ship has not touched them:

| Stash ID | Kind | Provisional priority | Summary |
|---|---|---|---|
| `6C174AA9` | chore | medium | MSRV/globset edition2024 buildability concern |
| `ECD56875` | feature | high | `--allow-all-tools` lacks real technical enforcement; requires deliberation |
| `AC2E02C0` | chore | medium | Windows-ACL owner-only enforcement gap for probe workspace |
| `8EFB7D92` | task | low | `run_wrapper_subcommand` collapses `WrapperOutcome` to a bare exit code |
| `1074A164` | chore | low | stash `kind` schema/behavior mismatch documentation gap, flagged by Copilot round 5 |

## Compaction (P-020)

`compact-context` was invoked with `target: all` after this closure's own
session memory checkpoint was drafted. **Outcome: `done`.** Ten Ship-owned
memory files spanning the full `049-S` lifecycle (2026-08-24 PR #106
staging through 2026-09-13 cascade-recovery halt) were compacted into
`docs/memory/compacted/2026-09-13-049-s-compacted.md` and the verbose
originals archived to `docs/archive/memory/2026-09-13/` with content fully
preserved. Six external documents citing the old, pre-archival paths were
corrected to the new archive paths
(`docs/compound/workflow-issues/mcp-json-workspacefolder-camelcase-2026-08-24.md`,
`docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md`,
`docs/memory/2026-09-04/post-merge-closure-pr-118-session-memory.md`,
`docs/closure/2026-09-13-049-s-compound-refresh.md`,
`docs/compound/runtime-errors/mcp-serve-os-error-232-handshake-signature-2026-09-13.md`,
`docs/compound/workflow-issues/backlogit-task-done-immediate-archive-relocation-2026-09-13.md`).
**Correction (this session's own Copilot-review remediation pass):** the
last three of the six were *not* caught by the original repo-wide `git
grep` sweep referenced in the prior version of this paragraph — that sweep
predated this closure's own new compound entries and the compound-refresh
report, which themselves cited the pre-archival paths and were therefore
missed by a scan run before they existed. Copilot's PR review on this
closure's own PR subsequently flagged the gap; all six are now corrected
and re-verified via a fresh repo-wide `git grep` for every old path,
performed as part of this remediation. The only remaining matches for
the old, pre-archival paths are legitimate frozen historical artifacts
that document point-in-time state and are intentionally not rewritten:
`.backlogit/reconcile/049-S-halt-20260913T053721Z.md` (a frozen recovery
record), the archived memory file
`docs/archive/memory/2026-09-13/2026-09-13-ship-049S-safeclose-step8-halt-checkpoint.md`
itself (preserved verbatim per the compaction contract), and this
compaction's own `source_originals` provenance list in
`docs/memory/compacted/2026-09-13-049-s-compacted.md` (which records the
pre-archival filenames by design). All Stage-owned memory for
the same shipment/feature (files
prefixed `stage-`/`049-s-stage-`, plus the Stage-authored
`049S-reassessment-memory.md`) was deliberately excluded from this
compaction — Ship does not mutate another agent's memory (Continuity row
of the Role Boundary). This closure session's own new memory checkpoint
(written at Session End, after this compaction) was likewise excluded, per
the standard ordering — it documents work still open at the time this
compaction ran.

## Final Verification

See below — filled in after the closure PR is created, reviewed, and this
document's own commit/push completes.

## Cross-References

* `.backlogit/reconcile/049-S-halt-20260913T053721Z.md` — full recovery
  record (halt, cascade authorization, repair, resolution).
* `.backlogit/reconcile/049-S-pre-20260913T052820Z.md` — pre-archive
  reconciliation report.
* `.backlogit/checkpoints/checkpoint-20260913-053918.json`,
  `checkpoint-20260913-070341.json`, `checkpoint-20260913-073130.json` —
  resolved Ship checkpoints across this recovery arc.
* `docs/archive/memory/2026-09-13/2026-09-13-ship-049S-safeclose-step8-halt-checkpoint.md`,
  `docs/archive/memory/2026-09-13/2026-09-13-ship-049S-resume-blocker-reconfirmed-20260913T0703Z.md`
  (compacted 2026-09-13, see
  `docs/memory/compacted/2026-09-13-049-s-compacted.md`)
  — session memory from the halt and resume-confirmation phases.
* `docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md`
  — pre-merge adversarial review for PR #120.
* `docs/closure/2026-09-13-049-s-post-merge-closure-adversarial-review.md`
  — post-merge **closure-artifact** adversarial review (3-reviewer
  multi-model pool, report-only mode) covering this closure's own commits
  (`73453f3`, `5f4457b`): backlog integrity of the 25-file repair,
  shipment archive correctness, no unintended deletions among the 8
  manifest task archives, documentation accuracy, cross-reference
  integrity, and P-021 scope discipline. Outcome:
  `READY_WITH_FOLLOWUPS` (0 consensus/HIGH-confidence findings; 3
  MEDIUM-confidence majority findings F1–F3 and 3 LOW-confidence unique
  findings F4–F6, none blocking). **All six findings (F1–F6) were
  remediated directly in this same closure** — same-contract-surface
  fixes to documents this closure itself authored/touched (P-021 C1/C3:
  completing this deliverable's own documentation is in scope, not an
  expansion) — rather than deferred: F1/F5 (persist reproducible
  `git diff`/`numstat` evidence for the repair and scope claims), F2/F6
  (superseded-annotations on stale historical prose exposed by the
  citation-path fix), F3 (authorship-vs-filename-date clarification), F4
  (corrected the cascade-hazard mechanism description in the new compound
  entry to state upward-then-downward `parent_id` graph resolution, since
  `049-S`'s manifest was task-only with zero feature members yet still
  triggered the cascade).
* `docs/closure/2026-09-13-049-s-compound-refresh.md` — compound library
  maintenance performed as part of this closure.
* `docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md`
  — precedent for the `mcp-stdio-startup` non-regression disposition and
  the `READY_WITH_CONDITIONS` / `mcp-client-smoke` deferral pattern.
* `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`,
  `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  — original design/plan artifacts for the shipped fix.
