---
title: Compound Refresh — shipment 049-S post-merge closure
date: 2026-09-13
mode: apply
scope: recent
shipment: 049-S
feature: 056-F
pr: 120
---

# Compound Refresh — 049-S Post-Merge Closure

## Context

During `049-S` post-merge closure, the previous Ship session's cascade
recovery (see `.backlogit/reconcile/049-S-halt-20260913T053721Z.md`)
generated direct, exhaustively-proven empirical evidence about the
installed backlogit version's shipment status machine — evidence that
contradicts an existing `docs/compound/` entry and fills a citation gap
referenced by four separate agent/skill instruction files.

## Phase 1: Evidence Gathered

* `.backlogit/reconcile/049-S-halt-20260913T053721Z.md` — disposable-copy
  proofs that `backlogit move --status shipped` / MCP `backlogit_move_item`
  is unconditionally refused (`shipment_shipped_requires_envelope`) on the
  installed `1.10.1-0.20260823032255-b07729386a31+dirty` build, and that
  `backlogit adopt` rewrites hierarchical IDs (not gap-aware) and
  unconditionally injects `custom_fields.origin_feature`.
* `git --no-pager grep -rln "backlogit-shipment-status-constraints"` —
  confirmed four instruction files
  (`.github/agents/_orchestrator.agent.md`, `.github/agents/_ship.agent.md`,
  `.github/instructions/backlogit.instructions.md`,
  `.github/skills/shipment-reconcile/SKILL.md`) cite
  `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md` as an
  authoritative source, but the file did not exist in the repository.
* `docs/compound/backlogit-shipment-state-machine.md` (2026-04-29) —
  documents a shipment lifecycle (`queued -> active -> done -> (archive)`
  via `backlogit update <id> --status done` + `backlogit archive`) that
  directly contradicts this session's empirical findings and the current
  Ship agent template's own Mutation Classification table (which states
  `done` is a task-only terminal status and the shipment enum is
  `queued`/`active`/`shipped`/`abandoned`).

## Phase 2: Classification

| Entry | Classification | Rationale |
|---|---|---|
| `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md` (missing) | **replace** (created) | Cited by 4 instruction files as authoritative but absent; created with the confirmed enum, the `shipment_shipped_requires_envelope` guard, the cascade/safe-close distinction, and the `adopt` repair-tool hazard, all backed by this session's direct evidence. |
| `docs/compound/backlogit-shipment-state-machine.md` | **update** | Shipment-lifecycle section (done→archive) is obsolete against the currently installed backlogit version; **task**-status-transition section (`queued -> active -> done -> (archive)`) remains accurate and unaffected — this is a genuinely mixed entry, not a wholesale replacement. Added a superseded-notice pointing to the new entry, preserved original content with corrections inline rather than deleting history. |
| `docs/compound/mcp-formatter-source-verification-2026-05-06.md` | **keep** | Reviewed; unrelated to this shipment's scope (formatter/source verification, not shipment lifecycle or MCP serve/initialize). No drift found. |
| `docs/compound/backlogit-level1-id-collision-across-parent-types.md` | **keep** | Reviewed; adjacent topic (ID collision) but describes a different mechanism (level-1 ID collision across parent types) than this session's `adopt` finding (non-gap-aware `NextTypedHierarchicalID` under a single parent). No contradiction; left as-is — a future refresh could consider cross-referencing both entries from a shared "backlogit ID assignment hazards" index if a third related entry appears, but two entries do not yet justify consolidation. |

## Phase 3: Applied Maintenance (`mode=apply`)

* Created `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`
  with the confirmed shipment status enum, the envelope/cascade
  requirement, the P-015 verified-fully-covered-root exception summary,
  the `adopt` repair-tool hazard, and full evidence citations.
* Updated `docs/compound/backlogit-shipment-state-machine.md` in place:
  added a superseded-notice at the top pointing to the new entry;
  annotated the `## Solution` and `## State Machine Summary` headings as
  superseded-for-shipments; left the `## Task Status Transitions` section
  and its guidance completely untouched (still accurate).
* No entries deleted or consolidated this cycle.

## Additional Cross-Reference (same cycle)

* `docs/compound/workflow-issues/clippy-allow-unknown-lints-msrv-guard-2026-08-24.md`
  — **update**. The Learnings Researcher pass recorded during `049-S`'s
  standard review phase
  (`docs/archive/memory/2026-09-13/2026-09-12-ship-049S-progress-checkpoint.md`,
  compacted 2026-09-13, see
  `docs/memory/compacted/2026-09-13-049-s-compacted.md`) flagged that
  this entry's own MSRV/`globset`/`edition2024` follow-up note (Prevention
  point 4) had never been cross-referenced to the P-021 deferred-scope
  stash entry that independently re-captured the same gap
  (`6C174AA9`, chore/medium). Added a dated cross-reference paragraph
  citing the stash ID and this closure record; no other content changed.

## New Learnings Captured (compound, not compound-refresh — recorded for completeness)

Two of the four candidate learnings flagged by the same Learnings
Researcher pass were captured as new compound entries during this closure
(the other two — a containment-reimplementation pattern and a typed
observability-seam pattern — were reviewed and intentionally left
uncaptured this cycle as lower-priority architecture-pattern write-ups; not
lost, since the source pass remains cited in
`docs/archive/memory/2026-09-13/2026-09-12-ship-049S-progress-checkpoint.md`
(compacted 2026-09-13, see
`docs/memory/compacted/2026-09-13-049-s-compacted.md`) for a future
session to pick up):

* `docs/compound/runtime-errors/mcp-serve-os-error-232-handshake-signature-2026-09-13.md`
  — the OS error 232 root-cause signature and the evidence-first
  investigation discipline it required.
* `docs/compound/workflow-issues/backlogit-task-done-immediate-archive-relocation-2026-09-13.md`
  — the backlogit 1.10.1 task-level auto-archive-on-`done` behavior and the
  deferred-commit-evidence workaround it required.

## Follow-up Items Requiring Manual Review

* None identified. The `adopt`/ID-collision cross-reference noted above
  (Phase 2, last row) is a low-priority future-refresh suggestion, not a
  blocking follow-up — recorded here for visibility only, not as a Stage
  handoff item (compound library maintenance is a Ship-owned documentation
  action, not a backlog/stash item).
* The two uncaptured candidate learnings noted above (containment-
  reimplementation pattern, typed observability-seam pattern) remain
  available for a future compound-capture pass, sourced from the
  "Adversarial multi-model review + remediation" section of
  `docs/archive/memory/2026-09-13/2026-09-12-ship-049S-progress-checkpoint.md`
  (compacted 2026-09-13, see
  `docs/memory/compacted/2026-09-13-049-s-compacted.md`) — not urgent, no
  code/backlog impact.
