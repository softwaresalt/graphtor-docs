# Compound Learning: backlogit Shipment Status Constraints

**Category:** Workflow / Tooling
**Discovered:** 2026-05-07 (cited by agent/skill instructions since); confirmed and
corrected with direct empirical evidence during shipment `049-S` closure, 2026-09-13.
**Created:** 2026-09-13 — this file did not exist before this date. The
`2026-05-07` filename/date reflects the date the concept was first cited by
other instruction files as a forward reference to an authoritative source
that had not yet been written, not the authorship date of this document.
This closure filled that pre-existing citation gap; see the
`2026-09-13-049-s-compound-refresh.md` closure report for the audit trail.
**Context:** This filename is cited by `.github/agents/_orchestrator.agent.md`,
`.github/agents/_ship.agent.md`, `.github/instructions/backlogit.instructions.md`,
and `.github/skills/shipment-reconcile/SKILL.md` as the authoritative source for
"Backlogit does not define a shipment `blocked` status." The file did not
previously exist in `docs/compound/` — this entry fills that gap and supersedes
the shipment-lifecycle portion of `docs/compound/backlogit-shipment-state-machine.md`
(see that file's own superseded-notice).

## Problem

Multiple agent/skill instruction files assert shipment status constraints
(no `blocked` status; a specific enum) without a citable, evidence-backed
source, and an older compound entry (`backlogit-shipment-state-machine.md`,
2026-04-29) documents a shipment lifecycle — `queued -> active -> done ->
(archive)` via `backlogit update <id> --status done` + `backlogit archive`
— that is **no longer correct** against the installed backlogit version
observed in this workspace (`1.10.1-0.20260823032255-b07729386a31+dirty`).

## Confirmed Shipment Status Enum (backlogit 1.10.1)

```
queued -> active -> shipped
                  -> abandoned
```

* There is **no** `done` status for a shipment record. `done` is a valid
  terminal status for **task** artifacts only (see the task-transition
  section retained in `backlogit-shipment-state-machine.md`).
* There is **no** `blocked` status for a shipment record. Checkpoint schema
  V1 and shipment records are unrelated surfaces; do not conflate a
  Ship/Stage checkpoint's own state with shipment lifecycle status.
* There is **no** `released` status reachable in the currently installed
  version (the 2026-04-29 entry's claim that `released` "exists in the
  schema but is not reachable" could not be re-confirmed and should be
  treated as obsolete terminology from an earlier backlogit version).

## Confirmed: `shipped` Requires the Envelope/Cascade Operation

Empirically proven during the `049-S` closure recovery (see
`.backlogit/reconcile/049-S-halt-20260913T053721Z.md` for the full
disposable-copy proof trail):

* `backlogit move <shipment-id> --status shipped` (CLI) and the MCP
  `backlogit_move_item` equivalent are **unconditionally refused** by the
  installed version with a compiled-in structural guard, error code
  `shipment_shipped_requires_envelope`. This is not a config toggle and
  cannot be worked around by any direct status-field mutation.
* **Only** `backlogit shipment ship <id>` (MCP `backlogit_ship_shipment`,
  the `ShipShipment` cascade operation) can transition a shipment record to
  `shipped`.
* The cascade operation is **P-015-forbidden as a default path** because it
  recursively resolves `releaseScopeItemIDs` for every manifest item —
  including every descendant at every depth via the live `parent_id`
  graph. Critically, this resolution is **not** limited to walking
  *downward* from an explicitly-included feature member: for a **task**
  manifest member, `ShipShipment` first resolves **upward** to that task's
  covering feature (via `parent_id`), then walks **downward** again from
  that feature to collect its full descendant set — and requeues/detaches
  and re-parents any descendant in that set that is **not** itself a
  manifest member. `049-S`'s own manifest proves this: it was **task-only**
  — `056-F` (the covering feature) was **not** a manifest member at all —
  yet the cascade still cleared `parent_id` on 25 out-of-manifest siblings
  of a manifest task's covering feature (`056.004-T`–`056.018-T`,
  `056.024-T`–`056.033-T`) when run under an explicit, deliberate,
  operator-authorized one-time P-015 exception. A future reader should
  **not** conclude that a feature-free/task-only manifest is safe from this
  hazard — the opposite is true: any manifest task whose covering feature
  has out-of-manifest siblings is exposed, regardless of whether that
  feature is itself a manifest member.
* The safe default close path is `shipment-reconcile`'s **safe-close
  mode**: a non-cascading sequence that archives only the shipment's
  explicit manifest members, never touching descendants outside that
  manifest. **This path does not, by itself, autonomously complete
  shipment closure on the installed version.** Its own final step
  attempts the same unconditionally-refused `backlogit move <id> --status
  shipped` transition described above; per the `shipment-reconcile`
  skill's own documented halt behavior, that refusal makes safe-close
  **halt before the shipment record reaches `shipped`**, having already
  archived the manifest items — exactly what happened for `049-S`. Do not
  read "safe-close" as a self-sufficient way to reach `shipped`: it
  performs manifest-item archival/finalization and then requires either
  the verified fully-covered-root exception below, or an explicit,
  real-time, operator-authorized cascade exception (with a documented
  recovery plan for the parent-link-clearing side effect on out-of-manifest
  siblings), to actually finish transitioning the shipment record itself.
* A narrow **verified fully-covered-root exception** permits the cascade
  operation directly when, for every feature member of the manifest, it is
  a root with **every** descendant at every depth also present in the
  manifest (positively verified, never inferred) — see P-015 in
  `.github/policies/workflow-policies.md` and the `shipment-reconcile`
  skill's Cascade Close Sub-Procedure for the authoritative classifier.
  Outside that narrow, machine-checked case, prefer safe-close for its
  manifest-item archival step, understanding that shipment-record
  finalization to `shipped` still requires the operator-authorized cascade
  handoff described above.

## Practical Guidance

* Do not attempt `backlogit update <shipment-id> --status done` for a
  shipment record — this is a task-only terminal status.
* Do not attempt `backlogit move <shipment-id> --status shipped` (or the
  MCP equivalent) expecting it to succeed directly — it is refused by
  design; this is not a transient bug.
* Default to `shipment-reconcile`'s Safe-Close Mode for manifest-item
  archival and finalization. On the currently installed backlogit version,
  Safe-Close Mode's own final shipment-record transition halts (it is not
  a silent no-op success) because of the same unconditional
  `shipment_shipped_requires_envelope` guard — plan for an explicit
  operator-authorized handoff to actually reach `status: shipped` unless
  the verified fully-covered-root exception applies. Reach for the cascade
  `backlogit shipment ship` only when the verified fully-covered-root
  exception's preconditions are positively confirmed, or under an
  explicit, real-time, one-time operator-authorized exception with a
  documented recovery plan for any cascaded protected-set damage
  (parent-link clearing on out-of-manifest siblings).
* If a cascade is run outside the verified exception and clears
  `parent_id` on out-of-manifest siblings, `backlogit adopt` is **not** a
  safe repair tool for this specific damage: it rewrites the target's
  hierarchical ID via `NextTypedHierarchicalID` (not gap-aware — collides
  with existing dependency edges when other tasks already occupy the
  computed next ID under the true parent) and unconditionally injects
  `custom_fields.origin_feature`. The safe repair is a direct, surgical,
  byte-exact frontmatter restoration of the single missing `parent_id`
  field per affected record, verified against a pre-cascade Git snapshot.

## Evidence

* `.backlogit/reconcile/049-S-halt-20260913T053721Z.md` — full halt record,
  disposable-copy proofs of the `shipment_shipped_requires_envelope` guard
  and the `adopt` ID-rewrite/`origin_feature`-injection behavior, and the
  resolution addendum documenting the surgical repair and verification.
* `docs/closure/2026-09-13-049-s-mcp-serve-handshake-post-merge-closure.md`
  — this shipment's own post-merge closure artifact.
* `.github/skills/shipment-reconcile/SKILL.md` — Safe-Close Mode and
  Cascade Close Sub-Procedure (authoritative protocol; this entry is a
  pointer/evidence record, not a substitute).
* `.github/policies/workflow-policies.md` — P-015 (cascade-close
  restrictions and the verified fully-covered-root exception).

## Reconfirmation — 054-S Closure (2026-09-14)

Independently reconfirmed against a **second**, unrelated shipment during
`054-S` (task-only manifest `[056.028-T]`, covering feature `056-F`):
`backlogit move 054-S --status shipped` was refused with the identical
`shipment_shipped_requires_envelope` error. The P-015 verified
fully-covered-root exception classifier — implemented, for workspaces with a
Python implementation installed, at
`.copilot/installed-plugins/autoharness/autoharness/src/autoharness/gates/shipment_closure.py`
(a gitignored, locally-installed plugin path, not part of this repository's
committed tree; see the committed
`.github/skills/shipment-reconcile/SKILL.md` Safe-Close Mode Step 0 and
`.github/policies/workflow-policies.md` P-015 for the followable,
repository-committed specification) — correctly returned `SAFE_CLOSE` (zero
feature members in the manifest, and the sole task's ancestry does not lead
back to any manifest-member root feature).
No cascade was attempted; item-level safe-close for `056.028-T` completed and
verified cleanly, and the shipment record itself was left `status: active`
pending an explicit operator decision. See
`.backlogit/reconcile/054-S-safe-close-20260914T061736Z.md` and
`.backlogit/reconcile/054-S-halt-20260914T061736Z.md` (open) for the full
record, and
`docs/closure/2026-09-14-054-s-mcp-probe-ci-post-merge-closure.md` for this
shipment's closure artifact. This reconfirms the entry's classification as
**KEEP** — no update to the constraint description itself is warranted; a
future backlogit release or an explicit real-time operator-authorized cascade
remains the only two paths to fully close a partial-feature, task-only
shipment on the currently installed version.
