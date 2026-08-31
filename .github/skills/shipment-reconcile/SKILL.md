---
name: shipment-reconcile
description: "GI/GR reconciliation gate for shipment manifests — verifies every manifest item exists in queue (pre-mode) or archive (post-mode) with the expected status, and closes shipments with the single-artifact safe-close procedure that archives ONLY manifest item IDs and closes the shipment record via live `shipped` -> verify -> explicit archive -> verify `archived_status: shipped`, instead of the destructive cascade backlogit_ship_shipment — except for the narrow, machine-verified P-015 fully-covered-root case, where the cascade op is the permitted and independently-verified close path."
---

# Shipment Reconcile

Provides a double-entry (GI/GR) integrity check for shipment manifests. Run
`mode: pre` before closing a shipment and `mode: post` after the archive +
restore steps complete. Run `mode: safe-close` **in place of** the destructive
cascade `backlogit_ship_shipment` call to archive only the shipment manifest's
explicit item IDs one artifact at a time, verifying after each that the parent
feature and any unshipped sibling tasks survive — safe-close's own Step 0 first
runs the P-015 verified fully-covered-root classification and, only when every
precondition holds, delegates to the Cascade Close Sub-Procedure instead.

> **Why safe-close exists.** `backlogit_ship_shipment` treats a shipment as a
> proxy for its covering feature and cascade-archives the whole feature subtree.
> For **partial-feature** shipments (which intentionally exclude the parent
> feature and some sibling tasks) this is destructive: it archives the parent
> feature and orphans unshipped siblings that are **not in the shipment
> manifest**. Safe-close prevents that corruption instead of merely detecting it
> after the fact. See **P-015** in `workflow-policies` for the governing policy.

## When to Use

* **Ship Step 6 closure** (mandatory): run `mode: pre`, then `mode: safe-close`
  **instead of** the cascade `backlogit_ship_shipment` call, then `mode: post`.
  Safe-close's own Step 0 selects the close path from the machine-checkable
  P-015 classification; it archives manifest item IDs individually and never
  calls the cascade op directly **unless** that classification confirms the
  narrow verified fully-covered-root exception, in which case the Cascade
  Close Sub-Procedure runs (and is itself independently verified) instead.
* **Ship Step 0.5** (sanity check): pre-mode at intake with `expected_status: queued`
  (or `active` if the shipment was already claimed in a prior session)
  to catch Stage-side over-inclusion before any build work begins.
* **Ad-hoc audit**: any time an operator suspects manifest drift.
* **`mode: detect-mixed-role`** (operator-invoked, READ-ONLY, no lock, no mutation):
  any time an operator wants a diagnostic scan for the queued-with-active-work /
  mixed-role silently-dropped-claim signature (936C68F3 part 2, re-scoped
  report-only per 013-DL Addendum G / 112-F) across one shipment or ALL
  shipments. Composed entirely from EXISTING read-only backlogit reads
  (`backlogit_list_shipments` + per-shipment/per-task status reads via
  `backlogit_get_shipment` and per-task `backlogit_get_item`). EMITS a report-only
  diagnostic plus operator-remediation guidance. NEVER mutates, NEVER calls
  `backlogit_claim_shipment` or any status-write operation, and needs no
  `file-lock` acquisition because no backlog/shipment artifact is ever
  mutated — the mode's own diagnostic report, audit-log entry, and telemetry
  event (steps 6–8 below) are additive-only writes to non-backlog-state
  locations, never applied to a queue/archive item.

## Inputs

| Parameter | Required | Values | Notes |
|---|---|---|---|
| `mode` | yes | `pre` \| `post` \| `safe-close` \| `detect-mixed-role` | Controls which check/close/detect phase runs |
| `shipment_id` | yes for `pre`/`post`/`safe-close`; optional for `detect-mixed-role` | e.g. `004-S` | The shipment to reconcile; for `detect-mixed-role`, omit to scan ALL shipments via `backlogit_list_shipments` |
| `expected_status` | pre-mode only | `queued` \| `active` \| `done` | `queued` for fresh intake; `active` when shipment already claimed in a prior session; `done` for pre-ship check |
| `merge_commit_sha` | post-mode and safe-close | git SHA | The **actual delivered-work merge commit**: the commit that merged the shipped work into `main`, already confirmed present on `origin/main` by the caller's merge-confirmation gate. In safe-close it is recorded as closure evidence on each item **before** that item is archived. It is never an anticipated, reconstructed, or branch-tip SHA, and never a PR planning or decision/closure-authority SHA |

## Output

A structured **reconciliation report** stored at
`.backlogit/reconcile/{shipment_id}-{mode}-{timestamp}.md`.

Every item in the manifest is classified as one of:

| Classification | Pre-Mode Meaning | Post-Mode Meaning |
|---|---|---|
| `matched` | Queue file present AND declared status matches `expected_status` | Archive file present for this item |
| `current-delivery-pending-finalization` | No queue file found but an archive file exists because **this shipment's own execution** moved the task to terminal `status: done`, and the installed status routing relocated it into `.backlogit/archive/` immediately — before the PR merge SHA existed — so its final archive markers/evidence are not yet complete. Valid **only** when the fail-closed provenance proof in Safe-Close Mode step 4 passes; ambiguous provenance halts | N/A (all items are expected in archive; use `matched` / `missing`) |
| `pre-archived` | No queue file found but an archive file exists because an **earlier shipment** delivered/archived the item, or it otherwise **predates** this shipment — distinct from `current-delivery-pending-finalization`, which is this shipment's own in-flight delivery; its commit evidence belongs to that earlier delivery and is never overwritten with this run's SHA | N/A (all items are expected in archive; use `matched` / `missing`) |
| `missing` | No queue or archive file found for this manifest item | Archive file not found for this manifest item |
| `status-mismatch` | Queue file present but declared status does not match `expected_status` | N/A (post-mode does not check status fields) |
| `conflict` | This manifest item ID also appears in the `custom_fields.items` manifest of **another live shipment** (queued or active) — a duplicate/overlapping assignment | N/A (post-mode does not scan live shipment manifests) |

> **Two archived cases, never one.** A manifest item found in `.backlogit/archive/` is
> **not** automatically an earlier shipment's delivery. The installed status routing
> relocates a task into the archive directory the moment it reaches the terminal status
> `done` — before the PR merge SHA exists — and Ship applies that transition when it
> completes the task, so this shipment's **own** members routinely land there mid-run (see
> `.backlogit/registry.yaml`). Classifying them as `pre-archived` would forbid writing this
> shipment's SHA and then halt on the resulting missing evidence, deadlocking closure
> permanently. The two cases are therefore split, and the split is decided by the
> **provenance proof** in Safe-Close Mode step 4 — never by membership or a missing commit
> alone.

> Classification semantics are mode-dependent. Pre-mode checks the queue
> for status correctness; post-mode checks the archive for file presence only.
>
> Shipment membership lives **only** in the shipment record's
> `custom_fields.items` list. There is **no reverse per-item `shipment_id`
> field** in the backlogit schema, so a reverse "which shipment claims this
> queue file?" scan is not expressible. `conflict` replaces it with the
> forward, schema-supported direction: an overlap scan across live shipment
> manifests. A manifest ID with no queue and no archive file is not a
> conflict — it stays `missing` through the normal per-item check in step 3.

### Shipment-Record-Status Classification (record scope, distinct from the five per-item classifications above)

In addition to the five per-item classifications, pre-mode also classifies the
shipment **record's own** `status` against the aggregate status of its manifest
**task** items. **Task-artifact filter (mandatory)**: the manifest `items` list
is untyped and may include the covering feature id (e.g. a fallback-assembled
manifest); this classification aggregates only items whose `artifact_type` is
`task` (already read from each item's frontmatter during the per-item check
above — no new read), excluding any non-task entry, so a covering feature that
happens to be `active`/`done` outside the shipment's own
scope can never be misread as a "conflicting task". This mirrors the task-artifact
filter the Ship agent's intake early-warning already applies to
`custom_fields.items` (`templates/agents/_ship.agent.md.tmpl`). **Scope**: the
three named inconsistency cases below apply only when the
record's own status is `queued` or `blocked` — the two
statuses where a manifest task already being `active`/`done`
is itself the drift signal (the queued/blocked record has not "caught up" to its
tasks). **`blocked` itself is a non-standard/legacy value**: backlogit
1.8.0's `ShipmentStatus` enum is only `queued|active|shipped|abandoned` (see
`docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`) — a
persisted `blocked` record can still exist in real workspaces from a
historical `backlogit move` CLI defect that silently accepted invalid status
writes. Pre-mode's `record-blocked-with-active-work`/`record-blocked-with-done-work`
cases below classify such a leftover record defensively (it is real data that may
be present); `mode: detect-mixed-role`'s `malformed-legacy` classification
(defined further below, in the mode's own classification section) reports the
identical underlying fact — a non-standard/legacy status value, never a normal
current-day state — for a different manifest scan. Both agree `blocked` is never fabricated or
transitioned into/out of; only the case label differs by mode. When the record's
own status is `active` or `done`
(or archived), the record is **by definition** `record-consistent` for this
check — an `active` record is the normal in-progress state while its
tasks move `queued` → `active` → `done`, and an
`done`/archived record reflects a shipment already closed. This is an
explicit scope boundary, not a silent default: the four cases below are
**mutually exclusive** because they are evaluated in this fixed order and every
record status value maps to exactly one of them.

| Classification | Condition |
|---|---|
| `record-consistent` | Record status is `active` or `done`/archived (always consistent — the normal in-progress/closed lifecycle, out of scope for this check), OR record status is `queued`/`blocked` and none of the three inconsistency conditions below match (e.g. record `queued` with all task items `queued`) |
| `record-queued-with-active-work` | Record status is `queued` AND at least one **task-artifact** manifest item is `active` or `done` — the classic "silently-dropped claim" inconsistency |
| `record-blocked-with-active-work` | Record status is `blocked` AND at least one **task-artifact** manifest item is `active`. **Precedence**: when a `blocked` record has BOTH an `active` task and a `done` task, classify here — active work takes precedence over `record-blocked-with-done-work` below, because it is the more severe/earlier-stage drift signal |
| `record-blocked-with-done-work` | Record status is `blocked` AND no **task-artifact** manifest item is `active` AND at least one **task-artifact** manifest item is `done` |

These four cases are mutually exclusive: every possible record-status value
(`queued`, `active`, `blocked`, `done`)
is covered — `active`/`done` always resolve to
`record-consistent`, and for `queued`/`blocked` the
active-over-done precedence rule (applied only to task-artifact items) guarantees
exactly one of the remaining three cases applies. This check is
**detect-and-report only — NO auto-repair**: it
never mutates the shipment record or any task; operators must manually
reconcile.

The report ends with a `recommendation`:

* `PROCEED` — all items are `matched` or `pre-archived` AND the
  shipment-record-status classification is `record-consistent`; no action needed
* `HALT — operator reconcile required` — one or more missing, status-mismatch, or orphan items, OR a non-`record-consistent` shipment-record-status classification (pre-mode)
* `HALT — restore archives` — missing archive files or unrestored deletions (post-mode)
* `CLOSED` — safe-close archived every manifest item individually, archived the
  shipment record itself, and the protected set (parent feature + unshipped siblings)
  is intact
* `HALT — cascade detected, revert required` — safe-close found a non-manifest artifact (parent feature or a sibling task) archived or deleted; the unintended change must be reverted before any commit

For `mode: safe-close`, the report also records the **protected set** (the parent
feature file and every unshipped sibling task file that must survive closure) and,
per manifest item, whether it was `matched` (archived by this run),
`current-delivery-pending-finalization` (this shipment's own member, already relocated
by its terminal `done` transition and finalized in place once its provenance proof
passed), or `pre-archived` (delivered by an earlier shipment; skipped to avoid
double-archival and false-positive cascade flags).

### Mixed-Role Detection Classification (`mode: detect-mixed-role` only, distinct from the record-scope classification above)

This classification is **separate** from the "Shipment-Record-Status
Classification" above: that check compares a single shipment record's own
status against the *aggregate* active/done state of its manifest tasks (four
record-scope cases). This check instead classifies **each manifest task
individually** by its per-task **ROLE**, to precisely describe the
mixed-role "silently-dropped-claim" signature — a `queued` shipment
record whose manifest tasks have kept progressing to `active` and
even fully `done`/archived while the record itself never advanced.
Per **013-DL Addendum G** (re-scoped by Copilot PR #304 finding 1), this
classification is used **ONLY to DESCRIBE** the inconsistency in a report —
**NEVER** to gate a mutation. There is no `--confirm` flag because nothing is
ever mutated.

**Per-task ALLOWED ROLE** (a task-artifact manifest item must be a UNIQUE,
NON-CONFLICTING record for exactly one of these three roles):

| Role | Definition |
|---|---|
| `live-queued` | UNIQUE record in `.backlogit/queue/` with `status: queued`; NO archive record for the same id |
| `live-active` | UNIQUE record in `.backlogit/queue/` with `status: active`; NO archive record for the same id |
| `archived-completed(done)` | UNIQUE record in `.backlogit/archive/` ONLY, in EITHER valid representation: (a) TERMINAL RELOCATION — `status: done` (provenance `archived_status`/`archived_from` NOT required), OR (b) EXPLICIT ARCHIVAL — `status: archived` AND `archived_status: done` AND valid, well-formed `archived_from` provenance; NO conflicting live queue record for the same id |

**Per-item ANOMALY** (fail closed — REPORT and HALT on ANY of these; a
manifest task satisfying none of these is role-clean):

| Anomaly | Definition |
|---|---|
| `duplicate` | Same task id present in BOTH `.backlogit/queue/` AND `.backlogit/archive/` |
| `conflicting` | Queue status disagrees with the declared role, an archive record exists alongside a live queue record, or a live `status: done` record is found in the QUEUE (a completed task lives ONLY in `archive/`, never live-done in `queue/`) |
| `missing` | Manifest task id has no file in either `.backlogit/queue/` or `.backlogit/archive/` |
| `malformed-provenance` | An archive record with `status: archived` is missing or has ill-formed `archived_status`/`archived_from` (a `status: done` archive record legitimately carries no provenance and is NOT malformed) |
| `any-other-archived-status` | An archive record whose status is NEITHER `done` NOR `archived`-with-`archived_status: done` |
| `orphan` | A queue file declares this `shipment_id` in its frontmatter but is NOT present in the manifest `items` list (reuses the pre-mode orphan-scan definition) |
| `out-of-role` | Task status falls outside the allowed lifecycle set `queued` \| `active` \| `done`/`archived` (e.g. a non-lifecycle or otherwise malformed status value) |
| `torn-partial` | Any other ambiguous, incomplete, or inconsistent signal for the task that cannot be cleanly assigned to a role (e.g. partially-written frontmatter) |

**Malformed-legacy shipment record**: backlogit 1.8.0 has NO `blocked`
shipment status (`ShipmentStatus` is only `queued|active|shipped|abandoned`).
A shipment record whose persisted status is anything other than a valid 1.8.0
lifecycle value (e.g. a legacy `blocked` value) is described in the report as
`malformed-legacy` — REPORT it, HALT, and never fabricate a `blocked->queued`
or any other transition. This is the same underlying fact the pre-mode
Shipment-Record-Status Classification table above documents for its own
scan (`record-blocked-with-active-work`/`record-blocked-with-done-work`
defensively classify a leftover legacy `blocked` record because it
may exist in real workspaces); the two modes describe the identical
non-standard value under mode-appropriate labels — never treat a persisted
`blocked` value as a normal current-day state in either mode.

**Mixed-role signature**: a shipment record `queued` whose
task-artifact manifest items (filtered by `artifact_type` to exclude any
non-task entry, e.g. a covering feature id, mirroring the existing
task-artifact filter above) include AT LEAST ONE `live-active` or
`archived-completed(done)` role task, with every task otherwise role-clean
(no anomaly), is the reportable "silently-dropped-claim" signature. On ANY
per-item anomaly (duplicate / conflicting / missing / malformed-provenance /
any-other-archived-status / orphan / out-of-role / torn-partial) or any other
ambiguity, REPORT the specific anomaly and HALT — never mutate, never
attempt to resolve or repair.

**Detection outcomes** (structured audit entry + telemetry event on every
run — see "Mixed-Role Detection Audit + Telemetry" below): exactly one of
`DETECTED` (scan completed; no mixed-role signature or anomaly found —
`record-consistent`, nothing to report), `REPORTED` (scan completed; the
mixed-role signature and/or one or more per-item anomalies were found and
described in the report), or `DEGRADED` (backlogit was unreachable; the
degraded condition is reported and the scan halts). There is **NO**
`succeeded` / `repaired` / `refused` / two-active outcome — nothing is ever
mutated or repaired by this mode.
Safe-close additionally records, for **each member and for the shipment record**, the
commit **evidence source** and the SHA **value**:
| Evidence source | Meaning |
| `atomic-archive-metadata` | The **selected invocation transport** for the archive operation itself accepted the delivered SHA **and** its own tool contract guarantees the value is persisted to the archived artifact's frontmatter `commit` atomically with archival, so archival and evidence committed together |
| `frontmatter-commit-update` | The delivered-work merge SHA was written to the still-**live** artifact's frontmatter `commit` by a commit-only update (MCP `backlogit_update_item` carrying only `commit`, or CLI `backlogit update {id} --commit {sha}`) and verified there before any terminal mutation |
| `current-delivery-post-terminal` | **This** shipment's own member had already been relocated into `.backlogit/archive/` by its terminal `done` transition **before** the PR merge SHA existed, so the delivered-work SHA was written to the **archived** record's frontmatter `commit` by the same commit-only update and verified there — but only after the fail-closed provenance proof in Safe-Close Mode step 4 passed. An **explicit exception** to the live-before-terminal rule, forced by the installed status routing; never silent inference and never a backfill |
| `pre-existing` | A pre-archived member's own earlier merge commit, preserved and verified — never overwritten with this run's closure SHA |
> **Canonical evidence, resolved by transport.** The artifact's frontmatter `commit` is
> safe-close's **canonical** closure evidence. The two installed surfaces reach it
> differently, so resolve `track_commit` by transport **before** calling anything:
>
> * **MCP.** The canonical write is `backlogit_update_item` carrying **only** `commit`.
>   MCP `backlogit_track_commit` writes commit_links — a separate store that never
>   substitutes for frontmatter `commit` — so an optional MCP `backlogit_track_commit`
>   call is **supplemental** provenance recorded *in addition to*, never instead of, the
>   canonical evidence.
> * **CLI.** The installed registry maps `track_commit` to
>   `backlogit update {id} --commit {sha}` — the **same** CLI command as the canonical
>   frontmatter update, not a distinct supplemental call. On CLI, execute that command
>   **once** and classify it as the **canonical** frontmatter evidence write. Never
>   **double-call** it as a supplemental commit-link: a second invocation would merely
>   re-run the identical canonical write, and this mapping produces no separate commit_link
>   to record.
> The recorded SHA is always the **actual delivered-work merge commit** (the commit that
> merged the shipped work into `main`). It is distinct from any **decision/closure-authority
> SHA** — the commit that recorded the decision or approval to close. The latter authorizes
> the closure; it is never written as delivered-work evidence.

## Behavioral Constraints

* **Report-and-halt only.** This skill NEVER modifies the shipment manifest or
  queue/archive files **outside the safe-close mode's manifest-scoped archival**.
  In pre- and post-mode it only reports; operators must manually reconcile via
  existing backlog tools and re-invoke Ship Step 6.
* **Manifest-scoped mutation only.** In `mode: safe-close`, the ONLY artifacts
  this skill may move or archive are the shipment manifest's explicit item IDs — read from
  the shipment record's `custom_fields.items` — and
  the shipment record itself (`{shipment_id}`). It must NEVER archive the parent
  feature or any sibling task that is not in the manifest. It never calls the cascade
  `backlogit_ship_shipment`; the shipment record is closed as its own single artifact.
* **No prune / no auto-repair.** Auto-mutation of the manifest itself is reserved
  for a future version. Safe-close never prunes the manifest and never
  auto-deletes non-manifest artifacts. On cascade detection it **halts** and records a
  `ProposedAction` for recovery; the `git restore` / `git revert` recovery is
  `ActionRisk: destructive` and requires explicit real-time operator **approval** before
  it executes, scoped to the exact identified paths or the exact revert commit. Without
  approval the skill stays halted and never commits the corrupt state.
* **Commit evidence before any terminal mutation, through the selected transport.** Safe-close
  writes the actual delivered-work merge commit to each live manifest item's and the shipment
  record's frontmatter `commit` **before** that artifact undergoes any terminal transition or
  archival, via **exactly one** of two mutually exclusive paths chosen at a single dispatch
  point. The dispatch condition is a property of the **selected invocation transport**, not of
  the registry: Path A runs only when the surface actually being invoked both accepts the
  delivered SHA **and** guarantees, in its own tool contract, that the value is persisted to
  the archived artifact's frontmatter `commit` atomically with archival. Registry parameter
  presence alone is **not** proof — the registry declares `archive_item.params.commit_sha`,
  but the installed CLI mapping is `backlogit archive {id}`, which has **no commit flag**, so
  a CLI invocation MUST take Path B. Path B writes the SHA to frontmatter `commit` on the still
  live record with a commit-only update, verifies it, and only then runs the non-atomic
  terminal/archive sequence **exactly once**. Each artifact is archived **exactly once**; the
  paths are never chained. If neither path is available it halts before any terminal mutation.
  It never writes evidence after archival on these live paths, never overwrites a
  pre-archived member's existing merge commit with the current closure SHA, and halts rather
  than fabricating missing or contradictory evidence.
* **Two archived cases, split by provenance proof.** A manifest item found in
  `.backlogit/archive/` is classified `current-delivery-pending-finalization` **only** when a
  fail-closed provenance proof shows it is **this** shipment's own member, relocated there by
  its terminal `done` transition before the merge SHA existed; otherwise it is `pre-archived`
  (an earlier shipment's delivery). Membership in `custom_fields.items` alone and a missing
  `commit` alone are **never** proof, ambiguous provenance halts, and a record already fully
  `status: archived` is out of safe-close's scope entirely — it routes to historical
  evidence remediation.
* **Single-writer logical lock.** When invoked from Ship Step 6, this skill holds the
  `.backlogit/queue/.{shipment_id}.md.lock` file (via the `file-lock` skill) for the
  duration of pre-mode → safe-close → post-mode. That original queue-path lock file is the
  canonical **logical** shipment lock, and it remains held for the whole sequence even after
  the shipment record itself is relocated out of `.backlogit/queue/`. See the lock protocol
  in the Required Protocol section below.
* **Halt on RECONCILE_FAIL.** Do not proceed to safe-close unless pre-mode
  returns `PROCEED`. Do not commit backlog state if safe-close returns
  `HALT — cascade detected, revert required`. Surface the report path to the operator.
* **`mode: detect-mixed-role` is strictly READ-ONLY.** It NEVER mutates any
  shipment record or task, NEVER calls `backlogit_claim_shipment` (no
  re-claim, no repair mode — a record-only forward re-claim of a
  queued-with-active-work shipment is UNSUPPORTED by backlogit 1.8.0; see
  "Operator-Remediation Guidance" below), and requires NO `file-lock`
  acquisition because no backlog/shipment artifact is ever mutated (its own
  diagnostic report, audit-log entry, and telemetry event are additive-only
  writes to non-backlog-state locations). DEGRADED (backlogit unreachable)
  is REPORTED and the mode HALTS — it never guesses or acts blind.

## Required Protocol

### Logical Shipment Lock Contract

The file-lock primitive is **advisory** and path-derived: acquiring on
`.backlogit/queue/{shipment_id}.md` creates `.backlogit/queue/.{shipment_id}.md.lock`
beside it. Safe-close relocates the shipment record out of `.backlogit/queue/`, so this
contract fixes the lock's identity explicitly:

* **Canonical identity.** `.backlogit/queue/.{shipment_id}.md.lock` — the lock file created
  from the **original queue path** — is the canonical logical shipment lock across the whole
  pre-mode → relocation → safe-close → post-mode sequence. Its identity is the queue path it
  was created from, not the current location of the artifact.
* **Held across relocation.** The lock file remains held, and remains the authoritative
  logical lock, even after the shipment record moves out of `.backlogit/queue/`. The lock
  file itself does **not** follow the artifact, and after relocation it no longer provides
  same-directory physical-file protection over the archived copy — do not claim that it
  does. What survives relocation is the logical claim on `{shipment_id}`, not a filesystem
  guard on the moved file.
* **Every conforming writer honors it.** All conforming safe-close writers MUST honor that
  original lock identity for the whole sequence. Because acquisition requires the target file
  to exist, a **second acquisition** attempted on the now-missing queue target **fails
  closed** — that failure is the intended single-writer guarantee, never a signal to proceed
  unlocked or to re-derive a lock from the artifact's new path.
* **Release by original path.** Release with the **original queue path**
  `.backlogit/queue/{shipment_id}.md`. Release supports a missing or moved target: it removes
  the queue-path lock file and warns (rather than failing) when the target or the lock file is
  already gone.
* **Ambiguity halts.** If the target or lock state is ambiguous — lock file present with an
  unknown or foreign holder, lock missing mid-sequence, or the artifact found in neither queue
  nor archive — **halt** and report. Never guess, never force-break a lock this session did
  not create, and never continue a terminal mutation under an ambiguous lock.

### Pre-Mode

1. **Acquire the logical shipment lock** (Ship Step 6 invocations only, not intake):
   Invoke the `file-lock` skill to acquire `.backlogit/queue/{shipment_id}.md`, creating
   `.backlogit/queue/.{shipment_id}.md.lock` — the canonical logical shipment lock defined in
   the Logical Shipment Lock Contract above. If lock acquisition fails, count as a session
   stall (circuit-breaker protocol) and prompt the operator.

2. **Load manifest** via `backlogit_get_item(shipment_id)`.
   Read the manifest membership explicitly from the shipment record's
   `custom_fields.items` list. That list is the **only** membership surface; never read a
   top-level `items` field and never infer membership from anywhere else.

3. **Check each manifest item**:
   * Attempt to locate the file at `.backlogit/queue/{id}.*`
   * If found, read its frontmatter (including `status` and `artifact_type`) and
     compare `status` to `expected_status` — classify as `matched` or
     `status-mismatch`
   * If NOT found in queue, check `.backlogit/archive/{id}.*`
     — if an archive file exists, decide **which** archived case it is: classify
     `current-delivery-pending-finalization` when the Safe-Close Mode step 4 provenance
     proof holds (this shipment's own member, terminal-relocated by its `done` transition
     before the merge SHA existed), otherwise classify `pre-archived` (delivered by an
     earlier shipment). Both are valid at this gate; neither is inferred from membership
     alone, and ambiguous provenance halts
     — if no file in either location, classify as `missing`. This normal per-item check is
     the only source of `missing`; the scan in step 4 never produces it

4. **Live-shipment overlap (duplicate-assignment) scan**:
   Enumerate every **live** shipment — both `queued` and `active` — and read each one's
   `custom_fields.items`. Because `backlogit shipment list --status` accepts exactly **one**
   status string per call and no multi-status array is documented, run **two** calls
   (`--status queued`, then `--status active`) or one unfiltered list plus a client-side
   filter to the two live statuses. Never pass a multi-status value.
   Classify as `conflict` every current manifest item ID that also appears in the
   `custom_fields.items` of any live shipment other than `{shipment_id}` — that is a
   duplicate/overlapping assignment.
   This is the schema-supported direction of the check. Shipment membership exists only in
   `custom_fields.items`; there is **no reverse per-item `shipment_id` field**, so this skill
   makes **no reverse-orphan claim** and never scans queue frontmatter for a back-reference.

5. **Shipment-record-status classification** (reuses in-hand data — NO new scan):
   Using the shipment record's own `status` already loaded via `backlogit_get_shipment`
   in step 2, and the manifest items' statuses already read in step 3, classify the
   record scope per the Shipment-Record-Status Classification table in the Output
   section above, evaluated in this order. **Filter to task artifacts first**: the
   manifest `items` list is untyped and may include the covering feature id (e.g. a
   fallback-assembled manifest); reuse the `artifact_type` already read from each
   item's frontmatter in step 3 to exclude any non-task entry before aggregating
   task statuses below — the same task-artifact filter the Ship agent's intake
   early-warning applies to `custom_fields.items` (`templates/agents/_ship.agent.md.tmpl`)
   — so a covering feature that is `active`/`done` outside the
   shipment's own manifest scope can never be misread as a "conflicting task" and
   falsely halt an otherwise-consistent shipment.
   * Record `active` or `done`/archived →
     `record-consistent` (always — this check is scoped to `queued`/
     `blocked` records only; an active/done record is the normal
     in-progress/closed lifecycle state, not evaluated further).
   * Record `queued` AND any manifest **task** is `active` or
     `done` → `record-queued-with-active-work`.
   * Record `blocked` AND any manifest **task** is `active` →
     `record-blocked-with-active-work` (takes precedence over the case below when
     both an active and a done task are present).
   * Record `blocked` AND no task `active` AND any manifest
     **task** is `done` → `record-blocked-with-done-work`.
   * Record `queued` or `blocked` matching none of the above
     → `record-consistent` (e.g. record `queued` with all tasks
     `queued`).
   This step is **detect-and-report only — NO auto-repair**: it never mutates the
   shipment record or any task.

6. **Produce report** and store at
   `.backlogit/reconcile/{shipment_id}-{mode}-{timestamp}.md`.

7. **Gate decision**:
   * If all items are `matched` or `pre-archived`, no orphans exist, AND the
     shipment-record-status classification is `record-consistent` →
     `recommendation: PROCEED`
   * If any `missing`, `status-mismatch`, or `orphan` items exist, OR the
     shipment-record-status classification is `record-queued-with-active-work`,
     `record-blocked-with-active-work`, or `record-blocked-with-done-work` →
     `recommendation: HALT — operator reconcile required`, naming the shipment id,
     the record's own status, and the conflicting manifest task ids
   * On `HALT`: emit the report path, release the lock, and halt with
     `RECONCILE_FAIL`. Do NOT call `backlogit_ship_shipment`.
   * On `PROCEED` from Ship Step 6: retain the lock until post-mode completes.

### Post-Mode

1. **Verify archive presence**:
   List `.backlogit/archive/` and confirm a file exists for the shipment itself
   (`{shipment_id}.*`).

2. **Per-item archive check**:
   For every item in the manifest, verify a corresponding archive file exists.
   If any are absent, flag them in the report.

3. **Deleted-file guard** (known `backlogit_ship_shipment` quirk — see P-007):
   Run `git status -- ".backlogit/archive/"` and inspect for deletions.
   If any archive files are reported as deleted, report them and **recommend** the
   approval-gated recovery `git restore -- {exact deleted archive paths}` before the
   commit step. Post-mode only reports: it never executes the restore itself, and the
   caller must obtain explicit operator approval (`ActionRisk: destructive`) before
   running it, scoped to exactly those paths.

4. **Produce post-mode report** per the same schema.

5. **Gate decision**:
   * If all archive files present and no deletions detected → `recommendation: PROCEED`
   * If missing archive files or unrestored deletions detected →
     `recommendation: HALT — restore archives`
   * On `HALT`: release the lock and report. Ship must restore archives before committing.

6. **Release the logical shipment lock** (acquired in step 1 of pre-mode):
   Invoke `file-lock` release for the **original queue path**
   `.backlogit/queue/{shipment_id}.md`, which removes
   `.backlogit/queue/.{shipment_id}.md.lock`. Release supports a missing or moved target —
   safe-close has already relocated the shipment record — so a warning that the target is
   gone is expected and is not a failure. If release itself fails, log a warning; stale
   locks are operator-recoverable.

### Safe-Close Mode

Runs **in place of** the destructive cascade `backlogit_ship_shipment` call —
**except** in the narrow P-015 verified fully-covered-root case selected by
Step 0 below, where the cascade op is the *permitted* close path and safe-close
steps 1–10 are skipped entirely. Archives only the shipment manifest's explicit
item IDs, one artifact at a time, verifying after each archival that the parent
feature and any unshipped sibling tasks survive. Invoked between pre-mode
(`PROCEED`) and post-mode, under the lock pre-mode already holds. If invoked
standalone, acquire the lock per pre-mode step 1 first and release it on
completion.

0. **Load manifest, snapshot pre-close state, then select close path (P-015
   verified fully-covered-root exception — select from the verified check,
   never from prose alone)**: safe-close is the default.
   a. **Load the manifest first**, regardless of which path is ultimately
      selected: invoke `backlogit_get_shipment(shipment_id)` and extract the
      `items` list. This load happens here in Step 0 — not deferred to step 1
      below — because the classification in (c) and the cascade
      pre/post-comparison in the Cascade Close Sub-Procedure both require it,
      and the cascade path skips steps 1–10 entirely.
   b. **Snapshot pre-close `parent_id` and declared `status` for every task
      item** in the manifest by reading each task's current frontmatter from
      whichever of `.backlogit/queue/` or
      `.backlogit/archive/` currently contains it — a manifest
      task item may already be
      pre-archived when this snapshot runs (see the
      Cascade Close Sub-Procedure's pre-archived-member preamble below), and
      its snapshot must still be captured from wherever it actually resides.
      If a task item's record is found in **both** locations (an
      ambiguous/torn state) or in **neither** (missing), halt immediately
      with `RECONCILE_FAIL_SNAPSHOT_AMBIGUOUS` or
      `RECONCILE_FAIL_SNAPSHOT_MISSING` respectively — never guess which
      copy or location is authoritative. Retain this snapshot in memory
      for the duration of this close operation; it is the baseline the
      Cascade Close Sub-Procedure's step 4 parent-preservation check and
      step 3 declared-status two-set gate both compare against, and it must
      be captured **before** any mutating call (cascade or otherwise) runs,
      never reconstructed after the fact.

      **Declared `status` is read from the record's own frontmatter `status`
      field — never inferred from, nor substituted by, which of
      `queue/`/`archive/` currently holds the record.** A record residing in
      `.backlogit/archive/` while declaring `status: done` is
      **not** truly archived; only a declared `status: archived` counts as
      truly archived for the Cascade Close Sub-Procedure's step 3 gate
      below — location alone is never sufficient. This declared-status
      snapshot MUST be captured here, in Step 0(b), **before** the cascade
      invocation, for the identical reason already stated for `parent_id`:
      `status` is the very field the cascade mutates, so a post-close read
      would report `archived` for everything the cascade just archived,
      collapsing `required_ids` (Cascade Close Sub-Procedure step 3) to
      empty and silently disabling the completeness check entirely. Never a
      freshly-read or assumed value.
   c. **Classify the close path**: run the machine-checkable classification
      described in P-015 over the manifest `items` loaded in (a). Workspaces
      with a Python implementation installed reuse a
      `classify_shipment_close_path(manifest_items, workspace_backlog_dir)`-shaped
      function (this self-hosting repository's own implementation lives at
      `src/autoharness/gates/shipment_closure.py`); other workspaces implement
      the equivalent check directly against `.backlogit/queue/` +
      `.backlogit/archive/`. The cascade close path is permitted
      **only** when, for **every** feature member of the manifest: it is a
      root (no `parent_id`); it is fully covered (every one of its
      **descendants — at every depth, not only direct children** —
      enumerated by walking the full `parent_id` graph live from
      `.backlogit/queue/` + `.backlogit/archive/`
      starting at the feature, is also a manifest member); and, if it
      enumerates to zero descendants, that childlessness is **positively
      verified** against the live workspace (never inferred from an
      incomplete or failed enumeration) and the feature is additionally
      terminal (no manifest member declares it as parent). **A
      direct-children-only check is insufficient** (155-S, PR #407 review,
      thread PRRT_kwDORzpWpM6b2MJv): Backlogit's own `releaseScopeItemIDs`
      recursively adds every descendant of each manifest item — not just
      the feature's immediate children — before `collectArchiveCandidateIDs`
      archives terminal descendants, so a manifest such as `[feature, task]`
      where that task has an out-of-manifest subtask (of any
      `artifact_type`, not only `task`) would otherwise wrongly qualify for
      `CASCADE`, and the destructive cascade would archive that subtask
      before the Cascade Close Sub-Procedure's step 3 gate ever sees it —
      halting only **after** the mutation. The manifest must contain nothing
      beyond the qualifying root feature(s) and their descendants at every
      depth. If **any** feature member fails **any** precondition, the
      **whole manifest** falls back to safe-close (steps 1–10 below) —
      qualification is never per-member, and no feature ID is ever
      special-cased.

      **When this classification identifies qualifying feature members**
      (i.e. selects `CASCADE`): extend the same pre-close declared-status
      snapshot from (b) — still **before** the cascade invocation, never
      after — with each qualifying feature member's own declared `status`
      field, read the identical way (frontmatter's own `status` field only,
      never inferred from `queue/`/`archive/` location). The resulting
      combined map (manifest task statuses captured in (b), plus qualifying
      feature statuses added here) is the single pre-close declared-status
      snapshot the Cascade Close Sub-Procedure's step 3 two-set gate reads
      from; "qualifying feature members" for that gate means exactly the set
      this classification determines here — never a separate
      re-derivation, and independent of *how* the engine happens to
      transition any given member.

      **Linked-deliberation snapshot extension (155-S, PR #407 review).**
      Backlogit's own cascade engine (`internal/core/shipment_lifecycle.go`
      `collectArchiveCandidateIDs`) appends, for every explicit qualifying
      feature member, that feature's `linkedDeliberationIDs` — collected
      from the feature's `custom_fields.source_deliberation_id` (taken as a
      complete literal ID string, never regex-scanned), plus any
      deliberation ID embedded in the feature's description, and any
      deliberation the feature references — the latter two, and only the
      latter two, scanned with the engine's own
      `internal/core.deliberationIDPattern` matcher (given exactly below) —
      never a broader "any embedded deliberation ID" reading, which can
      match a substring the engine's own matcher would not — de-duplicated,
      and restricted to
      IDs that resolve to an **existing** artifact whose own `artifact_type`
      is `deliberation` — before `archiveItems` runs. A qualifying feature
      with such a live linked deliberation therefore archives it during the
      same cascade invocation. To keep the two-set gate strict without a
      blanket allowance for arbitrary IDs, extend Step 0(c)'s classification
      here, still **before** the cascade invocation: for each qualifying
      feature member, independently collect its linked deliberation IDs
      using exactly those same three engine-defined sources — the literal
      `custom_fields.source_deliberation_id` string taken as-is, and the
      description/references text scanned with the identical
      `\b(?:DL\d+|[0-9]+(?:\.[0-9]+)*-DL)\b` matcher, never a wording-level
      approximation of it — and the
      identical existence / `artifact_type: deliberation` validation — never
      any other ID, and never an ID that fails either check. For each
      validated linked deliberation ID, resolve its record location the
      identical way Step 0(b) resolves a manifest task item: if found in
      **both** `.backlogit/queue/` and
      `.backlogit/archive/` (an ambiguous/torn state) or in
      **neither** (missing), halt immediately with
      `RECONCILE_FAIL_SNAPSHOT_AMBIGUOUS` or
      `RECONCILE_FAIL_SNAPSHOT_MISSING` respectively — never guess which
      copy or location is authoritative, and never compute `required_ids`
      from an arbitrary copy before the destructive cascade invocation.
      Once resolved to its single authoritative location, read its own
      declared `status` field (frontmatter only, never location-inferred)
      into the same combined
      pre-close declared-status snapshot as the qualifying feature statuses
      above. The further-extended combined map (manifest task statuses from
      (b), qualifying feature statuses, and now qualifying-feature
      linked-deliberation statuses, all added here in (c)) is what the
      Cascade Close Sub-Procedure's step 3 two-set gate reads from; "linked
      deliberation of a qualifying feature member" for that gate means
      exactly the set this sub-step determines here — never a separate
      re-derivation, and independent of whether the engine transitions,
      skips (already truly archived), or otherwise handles any given one of
      them.
   * **CASCADE selected** → skip directly to the **Cascade Close
     Sub-Procedure** below (reusing the manifest and snapshot from (a)/(b)/(c)
     above — do not reload) in place of steps 1–10, then proceed to
     post-mode.
   * **SAFE_CLOSE selected** (default, including any classifier error,
     ambiguity, or unresolved precondition) → continue to step 1 below
     (step 1's own manifest load is idempotent with (a) above — reuse the
     already-loaded manifest rather than issuing a second call).

1. **Load manifest** via `backlogit_get_item(shipment_id)`. Read membership explicitly from
   the shipment record's `custom_fields.items` list — never a top-level `items` field. These
   IDs are the **only** artifacts safe-close may move or archive.

2. **Compute the protected set** (partial-feature detection):
   * Derive the covering feature ID from the manifest item hierarchy
     (e.g. a task `055.002-T` belongs to feature `055-F`).
   * If the covering feature ID is **not** in the manifest membership
     (`custom_fields.items`), this is a
     **partial-feature shipment**. Add the covering feature to the protected set.
   * Enumerate every task sharing the covering feature's hierarchy prefix whose ID
     is **not** in the manifest membership (the unshipped siblings) by scanning **both**
     `.backlogit/queue/` **and** `.backlogit/archive/` (plus the
     feature file's declared children when available). Add each to the protected set.
   * **Sequence-aware exclusion (serial partial-feature shipments)**: when a sibling
     belongs to a predecessor shipment in the same feature-split sequence, exclude it
     from the protected set **ONLY** when that predecessor shipment record itself has
     **verified archived provenance** `archived_status: shipped` (or normalized legacy
     `done`). Mere archive-file presence, `archived_status: active|queued|blocked|abandoned`,
     generic `status: archived` without shipped/done provenance, or missing/ambiguous
     provenance are **NOT** sufficient — in those cases the sibling stays protected
     fail-closed.
   * The **protected set** is the parent feature plus every unshipped sibling task
     that MUST remain in `.backlogit/queue/` after closure. It is computed
     from **expected IDs**, not merely the files currently present in queue, so a
     sibling or parent that was already wrongly archived is still detected.

3. **Baseline integrity gate** (before archiving anything): Run
   `git status --short -- ".backlogit/"` and record the pre-closure
   working-tree state so any later archival or deletion of a protected-set path can be
   attributed to this procedure. Then confirm **every** protected-set member currently
   exists in `.backlogit/queue/`. If any protected-set member is already in
   `.backlogit/archive/` or missing from the working tree, a cascade has
   **already** occurred (or the shipment scope is wrong): halt immediately with
   `HALT — cascade detected, revert required`, name the affected artifact IDs, and do
   NOT archive any manifest item. The `pre-archived` exemption (step 4) applies to
   **manifest items only** — never to the protected set.

4. **Write frontmatter commit evidence, then archive, each manifest item individually** (loop over
   the `custom_fields.items` membership ONLY). Commit evidence is always written **before** the
   artifact moves, because once an item is archived its source record may no longer be mutable:
   * If the item's file is in `.backlogit/queue/` (a **live** manifest item), close it via
     **exactly one** of the two **mutually exclusive** paths below. **Dispatch once**, before
     any mutation, on the single condition *"does the **selected invocation transport** for
     `archive_item` — the surface actually being invoked — itself accept the delivered SHA
     **and** guarantee, in its own tool contract, that the value is persisted to the archived
     artifact's frontmatter `commit` atomically with archival?"*, then follow only the path that
     condition selects. Registry parameter presence alone is **not** proof of that guarantee: a
     registry may declare atomic commit metadata (`archive_item.params.commit_sha`) while the
     mapping for the transport in use cannot carry the value at all. On this installation the
     CLI mapping is `backlogit archive {id}`, which has **no commit flag**, so a CLI invocation
     MUST take Path B; only an MCP `backlogit_archive_item` whose tool contract states the
     frontmatter-`commit` persistence guarantee may take Path A. The paths are alternatives,
     never a sequence: the item is archived **exactly once**, by whichever path runs.
     * **Path A — atomic archive** (the selected transport accepts the SHA **and** its tool
       contract guarantees the value lands in the archived artifact's frontmatter `commit`
       atomically with archival). Invoke `backlogit_archive_item` **exactly once**, passing
       `merge_commit_sha` in the operation's configured field (for backlogit, `commit_sha`),
       so archival and frontmatter evidence commit together in that single call. That call
       **is** the archival, and **no later move/archive runs at all**: do **not** afterwards
       call `backlogit_move_item`, `backlogit_archive_item`, `backlogit_update_item`, or
       `backlogit_track_commit` on this item — it has already left `.backlogit/queue/`, and a
       second move/archive would operate on an artifact that is no longer there. Verify the
       resulting archived record exists at `.backlogit/archive/{id}.*` and carries frontmatter
       `commit` equal to `merge_commit_sha`. Classify `matched` with evidence source
       `atomic-archive-metadata`.
     * **Path B — live frontmatter-commit update** (the selected transport carries no such
       guarantee — always the case for the installed CLI archive mapping). While the item is
       still **live** in `.backlogit/queue/`, write `merge_commit_sha` into its frontmatter
       `commit` with a **commit-only** update: MCP `backlogit_update_item` carrying **only**
       `commit`, or CLI `backlogit update {id} --commit {sha}` — which is exactly what the
       installed registry's `track_commit` CLI mapping resolves to, so on CLI that single
       command **is** the canonical frontmatter write. **No other field** may be set in that
       call — no status,
       title, description, labels, assignee, priority, section, or any other planning field.
       Then **verify** that the live record's frontmatter `commit` equals `merge_commit_sha`
       **before any terminal mutation** — this is the last point at which the source record is
       reliably mutable. Only then run the registry's configured non-atomic terminal/archival
       sequence **exactly once**: for a **task** member, apply the task's valid terminal
       transition to `done` via `backlogit_move_item` when it is not already terminal, then
       apply the archive markers exactly once via `backlogit_archive_item` (CLI
       `backlogit archive {id}`). On backlogit installations where `move ... --status done`
       already **relocates** the file out of `.backlogit/queue/` before the final
       archive-marker operation, the record is no longer writable at its original path, so this
       contract **never** writes evidence after that move. Classify `matched` with evidence
       source `frontmatter-commit-update`. On **MCP** an optional `backlogit_track_commit`
       commit-link entry may be recorded **in addition**; commit_links are **supplemental**
       provenance and **never substitute** for the canonical frontmatter `commit`. On
       **CLI** there is no such extra call to make — `track_commit` maps to the same
       `backlogit update {id} --commit {sha}` already executed above, so it is **never**
       double-called as a supplemental step.
     * **Neither path available.** If the selected transport neither guarantees atomic
       frontmatter-`commit` persistence **nor** offers a commit-only frontmatter update on the
       live record, **halt** with `RECONCILE_FAIL` and report the missing capability **before
       any terminal mutation** — do not move, archive, or otherwise close the item. Never
       attempt to write commit evidence **after archival**, and never treat a report-only note
       or a supplemental MCP commit_link as a substitute for frontmatter `commit` on the item.
     * **Verify after either successful path** (one shared invariant, whichever branch ran):
       confirm the item now appears in `.backlogit/archive/` **exactly once** and
       carries frontmatter `commit` equal to `merge_commit_sha`, confirm it is gone from
       `.backlogit/queue/`, and record the evidence **source**
       (`atomic-archive-metadata` or `frontmatter-commit-update`) and the SHA **value** in the
       report before moving to the next item.
   * If the item's file is already in `.backlogit/archive/`, it is **not** automatically an
     earlier shipment's delivery. The installed status routing relocates a task into the
     archive directory the moment it reaches the terminal status `done` — before the PR
     merge SHA exists — and Ship applies that transition when it completes the task, so this
     shipment's own members routinely land there mid-run. Decide **which** of the two
     archived cases applies **before** touching the record; never assume either one.
   * **Case A — `current-delivery-pending-finalization`** (this shipment's own in-flight
     delivery): the task reached terminal `status: done` during **this** shipment's
     execution, was relocated into `.backlogit/archive/` by that status routing, and its
     final archive markers/evidence are not yet complete. This case is **fail-closed**: it
     applies only when **all** of the following conditions are proven **before** anything is
     written to the record, and **ambiguous** provenance **halts** with `RECONCILE_FAIL`
     instead of defaulting into it:
     * **Membership** — the ID is in **this** shipment's `custom_fields.items`. Membership
       alone is **never** sufficient proof, and a **missing** `commit` alone is **never**
       proof of current delivery — an earlier shipment's member can be listed here too, and
       can equally lack evidence.
     * **Ship-owned completion evidence** — a **Ship-owned** checkpoint or task-completion
       record, from the current session or a prior session in the **same** scope, ties this
       item's `done` transition to **this exact shipment's** execution.
     * **No foreign delivery** — there is no evidence that another or earlier shipment
       delivered this item; it appears as a delivered member of no other shipment manifest,
       live or archived.
     * **Merge proven** — this shipment's delivered-work merge SHA is already confirmed
       present on `origin/main` by the caller's merge-confirmation gate.
     * **Record consistent** — the archived record is terminal `done` (terminal-relocated,
       **not** final-marker archived) and carries no existing `commit` that would
       **contradict** this shipment's delivered-work SHA.
     If **every** condition above holds, finalize the record in place:
     * Write the SHA with a **commit-only** frontmatter update on the **archived** record —
       MCP `backlogit_update_item` carrying **only** `commit`, or CLI
       `backlogit update {id} --commit {sha}`. **Even when** the artifact already lives in
       `.backlogit/archive/`, that command still updates its frontmatter `commit`. **No
       other field** may be set — no status, title, description, labels, assignee,
       priority, section, or any other planning field.
     * **Verify** that the archived record's frontmatter `commit` **equals** the exact
       `merge_commit_sha` before continuing.
     * Then apply the final archive markers with `backlogit_archive_item` **exactly once**,
       and only **if they are not already applied** — never re-run markers on a record that
       already carries them.
     * Classify the member closed with evidence source `current-delivery-post-terminal`, and
       record the **provenance proof** (which conditions were satisfied, and from which
       artifacts) alongside the **exact SHA** value in the report.
     * This is an **explicit exception** to the live-before-terminal evidence rule, forced by
       the installed status routing relocating the task before the merge SHA exists. It is
       **not** silent inference and **not** a backfill: it writes only an authoritatively
       proven, `origin/main`-confirmed SHA for **this** shipment's own delivery, and records
       on what proof it was permitted.
     * If the record is instead **fully** `status: archived` (final markers already applied)
       but **lacks** or **contradicts** the expected `commit`, **halt** with
       `RECONCILE_FAIL` and route it to the separate **historical evidence-remediation**
       workflow described below. Safe-close **never silently rewrites fully archived
       provenance** during closure.
   * **Case B — `pre-archived`** (delivered by an earlier shipment, or otherwise predating
     this shipment; the Case A proof above did **not** hold):
     classify `pre-archived` and **skip** — do not re-archive. Reusing the `pre-archived`
     classification prevents false-positive cascade flags on items that were legitimately
     shipped earlier. Its commit evidence belongs to the earlier shipment that delivered
     it, so:
     * **Preserve** the item's existing actual merge commit as the **authoritative** existing
       delivery evidence. **Verify and report** it — never **overwrite** it with this run's
       closure SHA. A foreign (earlier-shipment) archived member is **never** overwritten
       with the current shipment's SHA, because it was delivered by a different merge and
       applying this closure SHA would falsify its provenance.
     * Record the preserved value and its evidence source (`pre-existing`) in the report.
     * If the required evidence is **missing** (no commit recorded on the pre-archived
       item) or **contradictory** (a recorded commit that conflicts with the item's
       documented delivery), **halt** with `RECONCILE_FAIL` and name the item. Never
       fabricate, infer, or backfill a SHA to close the gap.
     * **Remediation boundary.** Repairing a previously completed record is the job of a
       separate **historical evidence-remediation** workflow, run **outside** this
       safe-close operation, only with **authoritative provenance** for the correct
       delivered-work SHA (for example the merge commit proven on `origin/main` for the
       shipment that actually delivered the item) and only with an **audit note** recording
       what was changed, why, and on whose authority. That workflow repairs provenance; it
       **does not legalize chronology** — a record repaired later was still not evidenced
       before its archival, and the audit note must say so. Safe-close itself never silently
       infers or backfills evidence, and never repairs a pre-archived record inline — it
       halts and defers to that workflow.
   * If the item's file is in neither location: classify `missing`, halt with
     `RECONCILE_FAIL`, and do not continue archiving.

5. **Verify-after-each invariant** (run immediately after each item's archival):
   * Confirm **every** protected-set member is still present in
     `.backlogit/queue/` — not moved to `.backlogit/archive/`,
     not deleted from the working tree.
   * Run `git status --short -- ".backlogit/"` and confirm no
     protected-set path appears as a deletion, rename into `archive/`, or new
     `archive/` addition beyond the baseline captured in step 3.
   * The protected set was proven fully present in queue at the baseline gate
     (step 3), so **any** protected-set member now found in `archive/` or missing from
     the working tree is a cascade. There is **no** pre-archived exemption for the
     protected set — the exemption in step 4 covers manifest items only.

6. **Approval-gated cascade recovery** (never automatic): If the invariant fails (a
   protected-set artifact was archived or deleted by the preceding archival):
   * **Cascade detected. Stop.** Do **not** run any recovery command yet. Recovery here is
     a Git mutation (`git restore` / `git revert`) and is therefore
     `ActionRisk: destructive` with `change_kind: rollback`, which Constitution VII
     (Destructive Command Approval) requires an operator to approve first.
   * **Record a `ProposedAction`** naming the **exact** identified protected-set paths to
     restore, or the **exact** revert commit that introduced the cascade, plus the
     `rollback` field and `approval_required: true`. Set `ActionResult: blocked`.
   * **Emit a P-005 violation event** naming the cascaded artifact IDs and **request
     explicit real-time operator approval** for that exact recovery — via `agent-intercom`
     when that capability pack is installed, otherwise via a direct operator prompt.
   * **Only after approval**, execute exactly the approved recovery:
     `git restore -- {exact identified protected-set paths}` for working-tree
     moves/deletions, or `git revert {exact cascade commit}` if the cascade was already
     committed. Never broaden to unrelated paths or unrelated history, never touch paths
     outside the identified protected-set artifacts, and never use `git reset` or any force
     operation. Set `ActionResult: applied`.
   * **Re-run the invariant** to confirm the protected set is intact again, and record that
     verification in the report.
   * **Halt** with `HALT — cascade detected, revert required` and do **NOT** commit the
     backlog state. Do not auto-prune the manifest. Recovery restores the protected set; it
     does not resume closure.
   * If approval is **unavailable, withheld, or denied**, remain halted with
     `ActionResult: blocked`, leave the recovery unexecuted, and do **NOT** commit the
     corrupt backlog state.

7. **Final invariant re-check**: After the loop completes, re-confirm the full
   protected set is intact in `.backlogit/queue/`.

8. **Close the shipment record itself** (single artifact, non-cascading; authoritative order):
   * Move **ONLY** the live shipment record to `status: shipped` via the generic,
     non-cascading `backlogit move <shipment_id> --status shipped`.
   * Re-read and verify the live shipment record now reports `status: shipped`. If the
     record remains `active`, is already `archived`, is missing, or resolves to any other
     shape, halt fail-closed with `RECONCILE_FAIL_SHIPMENT_RECORD_LIVE_STATUS`. Do **NOT**
     auto-retry by calling the cascade op, and do not archive an `active` shipment record.
   * Archive **ONLY** the shipment record via `backlogit archive <shipment_id>` (single-
     artifact archive; this stamps `archived_status` from the live status at archive time).
   * Re-read and verify the archived record now reports `archived_status: shipped`.
     A legacy `archived_status: done` is accepted only when it pre-existed as an older,
     already-correct terminal provenance. Missing archive, live+archived duplication,
     generic archived-without-provenance, or any non-shipped archived provenance halt
     fail-closed with `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.
   * Re-run the verify-after-each invariant (step 5) to confirm the protected set is still
     intact after the shipment-record close sequence.

9. **Produce safe-close report** per the same schema, recording the protected set, each
   item's classification, the shipment-record archival, and the recommendation. For **each
   member and for the shipment record**, record the commit **evidence source** — one of
   `atomic-archive-metadata`, `frontmatter-commit-update`, `current-delivery-post-terminal`
   (this shipment's own terminal-relocated member, finalized after its provenance proof), or
   `pre-existing` (preserved from an earlier shipment) — alongside the exact SHA **value**
   recorded or preserved, the provenance proof behind any
   `current-delivery-post-terminal` write, and any supplemental MCP commit_link recorded in
   addition, so a reviewer can audit per member where each piece of commit evidence came
   from.

10. **Gate decision**:
    * All manifest items `matched`, `current-delivery-pending-finalization`, or
      `pre-archived`, the shipment record archived, and the protected set intact →
      `recommendation: CLOSED`. Proceed to post-mode.
    * Any cascade detected → `recommendation: HALT — cascade detected, revert required`
      (see step 6). Do not proceed to the commit step.

### Cascade Close Sub-Procedure (P-015 verified fully-covered-root exception ONLY)

Runs **only** when Step 0 of Safe-Close Mode above selects `CASCADE`, and
reuses the manifest and pre-close `parent_id`/declared-`status` snapshot
Step 0 already captured in (a)/(b)/(c) — this sub-procedure never reloads
the manifest or attempts to reconstruct pre-close state after the fact.
Replaces steps 1–10 above entirely for this shipment's closure; there is no
partial mixing of the two paths.

**Pre-archived manifest members (expected and tolerated)**: before invoking
step 1 below, classify each manifest member's **location** as `queued` or
`pre-archived` by checking whether its record currently resides in
`.backlogit/queue/` or `.backlogit/archive/`, and
retain that location set for the cascade-close report (step 5 below). This
location label is **descriptive only** — it names where the record
currently resides, and is **never** a substitute for, nor evidence of, the
record's own declared `status` field. A record residing in
`.backlogit/archive/` while declaring `status: done` is **not**
truly archived; only a declared `status: archived` is truly archived for
step 3's two-set gate below. The sole authority for "truly archived" is the
declared-status snapshot Step 0(b)/(c) already captured before this
sub-procedure runs.

A `pre-archived` (by location) manifest member is **expected and tolerated
on this path**: it does **not** disqualify the `CASCADE` verdict, does
**not** constitute a classifier ambiguity or unresolved precondition, and
does **not** authorize a fallback to safe-close. Step 0(c)'s classifier
already resolves each manifest member by scanning **both** `queue/` and
`archive/`, so archived inputs were already accounted for when the
verdict was selected — this clause states the execution-time consequence
of that fact and does not add a new classifier precondition; Step 0(c)'s
precondition wording is unchanged.

This tolerance applies to **manifest members only** — it does not
restate, weaken, or cross-apply to the protected set, which has no
pre-archived exemption (see Safe-Close Mode steps 3/5 above). A manifest
that qualifies for `CASCADE` has no protected set by construction (full
coverage is itself a Step 0(c) precondition), so no protected set arises
on this path.

**`archived_ids` is a transition log, not a manifest echo.** The cascade
operation invoked in step 1 below reports, in `archived_ids`, only the
artifacts it **actually transitioned** to archived during that invocation
(backlogit engine source, `internal/core/shipment_lifecycle.go`
`archiveItems()`: an item whose declared `status` is already `archived` is
skipped and never appended to the slice that becomes `archived_ids`). A
manifest task item, or a qualifying feature member's validated linked
deliberation, that was already truly `status: archived` before the call
therefore has no transition to report and is **correctly absent** from
`archived_ids` — this is expected engine behavior, not an anomaly and not a
cascade failure. **This never extends to the shipment record or to a
qualifying feature member itself** (155-S, PR #407 review, thread
PRRT_kwDORzpWpM6b0kit): step 3 below makes both unconditionally required
regardless of their own pre-close declared status, so neither can ever be
"correctly absent" the way a task item or linked deliberation can — see
step 3 for the full statement of that rule. The live fail-closed guard
over this result is the two-set `allowed_ids` / `required_ids` gate
specified in step 3 below, evaluated **against the Step 0(b)/(c) pre-close
declared-status snapshot** — never against location, and never against a
post-close re-read.

   **SUPERSESSION NOTE (155-S, 2026-08-24).** This paragraph previously
   claimed the cascade operation "is idempotent over pre-archived members",
   citing
   `docs/spikes/2026-08-18-cascade-close-pre-archived-member-behavior.md`
   as authority for the claim that it "returns \[pre-archived members] in
   its `archived_ids` result exactly as it does newly-archived members",
   and stated that step 3's exact-match post-condition, "evaluated against
   the manifest's full item set", "must never be relaxed". **That claim,
   and the spike cited for it, are WITHDRAWN.** The spike's arms were built
   with `move --status done`, which relocates a record but leaves it
   declaring `status: done` — never truly `status: archived` — so none of
   its arms ever exercised the case this paragraph claimed to cover; its
   finding is valid only for relocated-but-`done` records (see the spike's
   own superseded banner). The two safety properties this paragraph
   protected — nothing out-of-scope archived, nothing required left
   unarchived — are now carried, at full strength, by the two-set gate in
   step 3 below, keyed on declared pre-close status rather than a full-set
   echo of the manifest.

**No-substitution rule**: once Step 0 selects `CASCADE`, that verdict is
final for this closure — between the verdict and step 1's invocation
below, substituting manual safe-close is a **P-005 process deviation**,
never a permitted fallback, regardless of the manifest's archival state.
This complements, and does not restate or contradict, step 2's separate
rule against falling back to safe-close **after** a cascade has already
executed: together the two rules close both the pre-execution and
post-execution substitution windows. The asymmetry is intentional and
one-directional — this rule forbids `CASCADE -> manual safe-close`
substitution only; it grants no license to invoke cascade when Step 0
selects `SAFE_CLOSE`, which remains governed by the P-015 default
prohibition. If a genuine unhandled error occurs during the cascade
operation, halt and disclose it per the verification steps below — never
silently switch to safe-close instead.

Ship performs **no** manual per-item archive loop on this path: the
cascade operation in step 1 below performs all remaining archival
itself, consistent with the "no partial mixing of the two paths" rule
above.

1. Invoke `backlogit_ship_shipment(shipment_id, merge_commit_sha)` directly
   (CLI: `backlogit shipment ship <shipment_id> --sha <merge_commit_sha>
   --message <merge_commit_message> --author <merge_commit_author>`).
2. **Verify the result matches the classifier's own precondition**:
   `returned_ids` MUST be empty (`[]`). A non-empty `returned_ids` means the
   live engine found an unreleased descendant the classifier's live-workspace
   enumeration did not — this is a TOCTOU/engine-behavior mismatch, not a
   recoverable state. Halt immediately with
   `HALT — cascade returned non-empty returned_ids, classifier/engine
   mismatch` and emit a **P-005** violation; do NOT retry, do NOT fall back to
   safe-close after a cascade has already executed.
3. **Verify `archived_ids` against the two-set `allowed_ids` / `required_ids`
   gate** (replaces exact full-set equality — see the SUPERSESSION NOTE
   above and the P-015 policy's own supersession note for why):
   * **Compute `allowed_ids`** = the manifest's task items + every
     qualifying feature member identified by Step 0(c)'s own classification
     (defined by reference to that determination — never a separate
     re-derivation — and independent of *how* the engine happens to
     transition any given member) + every validated linked deliberation ID
     of each qualifying feature member captured by Step 0(c)'s
     linked-deliberation snapshot extension above (same reference-only
     rule: never a separate re-derivation, and never any ID beyond what
     that engine-defined, existence-and-`artifact_type`-validated
     collection produced) + the shipment record itself.
   * **Compute `required_ids`** = the shipment record and every qualifying
     feature member (**both unconditionally** — never omitted, and never
     conditioned on either artifact's own pre-close declared status) +
     every other `allowed_ids` member (a manifest task item, or a
     qualifying feature member's validated linked deliberation) that was
     **not** truly `status: archived` in the pre-close declared-status
     snapshot (Step 0(b) for manifest task items, extended by Step 0(c) for
     qualifying feature members and their validated linked deliberations —
     all captured **before** this step 1 invocation, never a freshly-read or
     assumed post-close value).
   * **Two separately-labelled, independently-failing conditions.** Neither
     may be evaluated as a precondition of the other, and the two MUST NOT
     be merged into a single combined test (conflating two questions into
     one condition is the documented root cause of external defect
     `B57F9E24`):
     - **Unexpected-artifact check**: if `archived_ids - allowed_ids` is
       non-empty, halt with
       `HALT — cascade archived unexpected artifact {id}` and emit a
       **P-005** violation.
     - **Missing-required-artifact check**: if `required_ids - archived_ids`
       is non-empty, halt with
       `HALT — cascade did not archive required artifact {id}` and emit a
       **P-005** violation.
   * An `allowed_ids` **non-shipment** member (a manifest task item, or a
     qualifying feature member's validated linked deliberation — never the
     qualifying feature member itself, which is unconditionally required;
     see below) that was already truly `status: archived` in the pre-close
     snapshot MAY be included in or omitted from `archived_ids` by
     the engine — neither outcome fails either check (it is outside
     `required_ids` by construction, and if present in `archived_ids` it is
     still inside `allowed_ids`). This is exactly the 147-F → archived
     027-DL case: 027-DL is a linked deliberation already truly
     `status: archived` pre-close, so its absence from `archived_ids` is
     tolerated by construction, and its presence, if the engine reports it,
     is equally tolerated. **This tolerance never extends to the shipment
     record itself**, which is unconditionally a `required_ids` member per
     the computation above regardless of its own
     pre-close declared status: if the shipment record were ever reported
     pre-close as already truly `status: archived` — an anomalous state for
     an artifact this same closure step is actively transitioning to
     `shipped` — its absence from `archived_ids` still fails the
     missing-required-artifact check exactly as any other missing
     `required_ids` member would; no engine behavior toward the shipment
     record ever gets a pass under this tolerance.

     **Nor does it extend to a qualifying feature member itself (155-S, PR
     #407 review, thread PRRT_kwDORzpWpM6bzlFl).** Backlogit's own
     `ShipShipment` (`internal/core/shipment_lifecycle.go`) unconditionally
     calls `setArtifactStatus(featureID, models.StatusDone, "feature
     released")` for every explicit shipment-member feature — regardless of
     that feature's own pre-close declared status, including an already
     truly `status: archived` one — **before** `collectArchiveCandidateIDs`
     runs. `setArtifactStatus` only no-ops when the artifact's current
     status already equals the requested one, and the requested status here
     is `done`, never `archived`, so an already-archived qualifying feature
     is unconditionally relocated to `done` first, with no terminal-status
     bypass of the kind `completeReleaseScope` grants a manifest task item
     already truly `status: archived` (that task-only skip is exactly what
     makes the tolerance above valid for tasks, and it has no counterpart in
     the feature-forcing loop). By the time `collectArchiveCandidateIDs`
     loads the feature, its declared status is therefore always `done`,
     never still `archived` — that function's own
     `feature.Status != models.StatusArchived` check is always true for it
     — so the feature is always appended to the candidate list
     `archiveItems` archives. A qualifying feature member can therefore
     never be "correctly absent" from `archived_ids` the way a truly
     pre-archived manifest task item or linked deliberation can — its
     absence is always an anomaly, never expected engine behavior. A
     qualifying feature member is therefore an unconditional `required_ids`
     member exactly like the shipment record, and is never eligible for
     this tolerance.
4. **Verify no `parent_id` was cleared**: re-read every archived task's
   frontmatter and confirm `parent_id` is unchanged from the pre-close
   snapshot captured in Step 0(b) — never a freshly-read or assumed value,
   since the field being verified is the very one a cascade could have just
   cleared. Any cleared or altered `parent_id` is a
   cascade-detection failure equivalent to step 6 of safe-close: halt with
   `HALT — cascade cleared parent_id on {id}, revert required` and emit a
   **P-005** violation; do NOT commit the mutated backlog state.
5. **Produce cascade-close report** recording the classifier's verdict,
   qualifying feature IDs, and their validated linked deliberation IDs
   (all Step 0(c)), the pre-close declared-status
   snapshot (Step 0(b)/(c)), the `backlogit_ship_shipment` result
   (`shipment_status`, `archived_ids`, `returned_ids`, `commit_sha`),
   `allowed_ids`, `required_ids`, and both set differences
   (`archived_ids - allowed_ids` and `required_ids - archived_ids`) — so a
   vacuous `required_ids` is visible in the report rather than silent — and
   the parent_id-preservation verification outcome (against the Step 0(b)
   snapshot).
6. **Gate decision**: `returned_ids` empty, `archived_ids - allowed_ids`
   empty (no unexpected artifact archived), `required_ids - archived_ids`
   empty (no required artifact left unarchived), and every `parent_id`
   preserved (against the Step 0(b) snapshot) →
   `recommendation: CLOSED`. Proceed to
   post-mode. Any verification failure above → the corresponding `HALT`; do
   not proceed to post-mode or any commit step.

### Mixed-Role Detection Mode (`mode: detect-mixed-role`, operator-invoked, READ-ONLY)

No lock is acquired for this mode — no backlog/shipment artifact is ever
mutated. (The mode's own diagnostic report, audit-log entry, and telemetry
event — steps 6, 8 below — ARE writes, but they are additive-only writes to
non-backlog-state locations, never applied to a queue/archive item, so no
`file-lock` is required.) This mode is composed entirely from EXISTING
read-only backlogit reads; it introduces NO new gate/CLI code (single
template family: this SKILL's prose only).

1. **Enumerate shipments**: call `backlogit_list_shipments`. If `shipment_id`
   was given, narrow to that single shipment; otherwise scan every shipment
   returned.

2. **Load each candidate shipment record** via `backlogit_get_shipment`.
   Skip (no report entry) any shipment whose record status is
   `active`, `shipped`, `abandoned`, or archived — those are the
   normal in-progress/closed lifecycle **shipment-record** states and are out
   of scope for this check (mirrors the `record-consistent` scope boundary
   above). Note: `shipped`/`abandoned` are the shipment record's own terminal
   statuses (`ShipmentStatus` enum), distinct from `done` which is
   a **task**-artifact status — a live shipment record is never itself
   `done`. If a candidate's persisted status is not a valid
   backlogit 1.8.0 shipment lifecycle value (e.g. a legacy `blocked` value),
   classify it `malformed-legacy`, add it to the report, and continue to the
   next candidate — never fabricate a transition. Any other unrecognized
   persisted value is likewise `malformed-legacy` rather than silently
   skipped or silently matched to the queued branch below — every possible
   persisted value maps to exactly one of: skip (`active`/
   `shipped`/`abandoned`/archived), scan (`queued`, step 3), or
   `malformed-legacy` (anything else).

3. **Filter to task-artifact manifest items**: for each remaining
   `queued` candidate, read its manifest `items` list and each
   item's frontmatter (`status`, `artifact_type`) via per-item reads
   (`backlogit_get_item` per task id). Exclude any non-task entry (e.g. a covering
   feature id in a fallback-assembled manifest) before classifying — the same
   task-artifact filter the Ship agent's intake early-warning applies to
   `custom_fields.items` (`templates/agents/_ship.agent.md.tmpl`).

4. **Classify each task-artifact manifest item** against the per-task ALLOWED
   ROLE table and the per-item ANOMALY table in the "Mixed-Role Detection
   Classification" section above. Locate each id in
   `.backlogit/queue/` and `.backlogit/archive/` to
   determine role/anomaly; for an archive hit, read `status`, `archived_status`,
   and `archived_from` to distinguish the two valid archived-completed
   representations from `malformed-provenance` / `any-other-archived-status`.

5. **Determine the outcome for each candidate**:
   * If any per-item anomaly was found → `REPORTED`, naming the shipment id,
     the anomalous task id(s), and the specific anomaly for each.
   * Else if the mixed-role signature is present (at least one `live-active`
     or `archived-completed(done)` role task, all tasks otherwise role-clean)
     → `REPORTED`, naming the shipment id, record status, and each task's role.
   * Else (all tasks role-clean and no mixed-role signature — e.g. a
     genuinely fresh `queued` shipment with all-`live-queued`
     tasks) → `DETECTED` with no anomaly/signature to report for that
     candidate.
   * If backlogit is unreachable at any point in steps 1–4 → `DEGRADED`;
     report the degraded condition for the affected candidate(s) and HALT the
     scan. Do not guess or proceed on partial data.

6. **Emit the report-only diagnostic** at
   `.backlogit/reconcile/{shipment_id-or-"all"}-detect-mixed-role-{timestamp}.md`,
   listing per candidate: the shipment id, its record status, the per-task
   role classification, any per-item anomaly, and the outcome (`DETECTED` /
   `REPORTED` / `DEGRADED`).

7. **Emit Operator-Remediation Guidance** inline with any `REPORTED` or
   `DEGRADED` entry — see "Operator-Remediation Guidance" below. This mode
   performs NO mutation of any kind in response to what it finds.

8. **Write the audit entry and emit telemetry** for every candidate's outcome
   — see "Mixed-Role Detection Audit + Telemetry" below.

#### Operator-Remediation Guidance

When this mode reports a mixed-role signature or a per-item anomaly, include
this guidance verbatim (adapted with the specific ids/anomaly found) in the
report:

> autoharness performs **NO auto-repair** of this inconsistency. A
> record-only forward re-claim (`queued` → `active` on
> the shipment record alone) is **UNSUPPORTED** by backlogit 1.8.0: evidence
> (read-only inspection of `C:\Source\GitHub\backlogit`, NOT mutated) —
> `ClaimShipment` (`internal/core/shipment_lifecycle.go`) is **manifest-wide**
> activation (it moves the shipment `queued`→`active`
> AND THEN activates every still-`queued` manifest member,
> cascading parent-feature status, with all-or-nothing rollback on any
> mid-flight failure) and is **STRICTLY SINGLE-SHOT**
> (`isValidShipmentTransition` in `internal/core/shipment.go` permits ONLY
> `queued`→`active` and
> `active`→`{shipped,abandoned}`; a re-claim on an already-`active`
> shipment returns `ErrShipmentConflict`). There is **NO**
> `active`→`queued` transition and **NO** `blocked`
> shipment status in 1.8.0 — never expect or fabricate either. The SUPPORTED
> manual remediation path is entirely through backlogit's own sanctioned
> lifecycle transitions: the operator inspects the shipment and its manifest
> tasks directly (`backlogit get <id>` / `backlogit shipment get <id>`) and
> decides, case by case, whether the tasks are legitimately progressing (in
> which case the operator may let the shipment proceed to closure normally
> once all tasks complete, using this skill's own `mode: pre` →
> `mode: safe-close` → `mode: post` sequence) or whether the state reflects a
> genuinely torn/partial session (in which case the operator investigates and
> resolves the affected tasks manually before any closure attempt). This
> guidance is descriptive only; this skill never performs any of these steps
> itself.

#### Mixed-Role Detection Audit + Telemetry

Mirrors the `pipeline-topology` force-audit + telemetry pattern
(`src/autoharness/cli.py` `_audit_pipeline_topology_force` /
`_emit_pipeline_topology_telemetry`), adapted for a detection-only outcome —
there is NO repair/mutation/confirm/post-condition field, because nothing is
ever mutated.

1. **Audit log**: for EVERY candidate outcome (`DETECTED` / `REPORTED` /
   `DEGRADED`), append one structured JSON line to
   `.autoharness/gates/shipment-reconcile-detection-audit.log` (creating the
   `.autoharness/gates/` directory if needed), containing: `timestamp`
   (UTC ISO-8601), `actor` (the invoking operator/session identity, e.g. from
   `USERNAME`/`USER`), `shipment_id`, `record_status`, `outcome` (`DETECTED` \|
   `REPORTED` \| `DEGRADED`), `per_task_roles` (an array of
   `{task_id, role, anomaly}` — `anomaly` is `null` when the task is
   role-clean), `remediation_guidance_emitted` (boolean), and `report_path`
   (the reconciliation report path from step 6). NO `repair`, `mutation`,
   `confirm`, or `post_condition` field is ever present — those concepts do
   not apply to this read-only mode.
2. **Telemetry event**: emit one telemetry event per candidate outcome,
   mirroring the pipeline-topology `ToolTelemetryEvent` shape
   (`schemas/tool-telemetry-event.schema.json`): `tool_surface: "builtin"`
   (the schema's `tool_surface` enum is `mcp` \| `cli` \| `shell` \| `builtin`
   \| `api` \| `unknown` — `"skill"` is NOT a valid value; `shipment-reconcile`
   is an agent-invoked prose skill with no separate CLI/MCP/API backend of its
   own, so `builtin` is the correct surface),
   `tool_name: "shipment-reconcile"`, `operation: "detect-mixed-role"`,
   `status` mapped from the outcome (`DETECTED` → `success`, `REPORTED` →
   `blocked`, `DEGRADED` → `failed`), `sensitivity: "internal"`,
   `redaction_applied` and `metric_sources`/`metric_quality` set per the
   schema's structural requirements, `shipment_id`/`backlog_item_id` set to
   the candidate shipment id, and `artifact_refs` including the audit log
   path and the report path. When invoked inside an active Ship task epoch
   with a live `context_ref` (from `autoharness telemetry begin`), emit via
   `autoharness telemetry event --context-ref <ref> --from-json <payload>`;
   when invoked standalone (e.g. an ad-hoc operator audit outside any Ship
   task loop, the most common case for this mode) there is no `context_ref`
   to attach to, so telemetry emission for that invocation is skipped and the
   audit log entry (step 1) remains the durable record — this mirrors
   telemetry's existing fail-open, observational contract (an absent or
   unavailable telemetry surface is a no-op and NEVER blocks detection,
   reporting, or the HALT decision).

### Lock-Conflict Scenario

If pre-mode cannot acquire the logical shipment lock because another process holds it:

1. Retry once after 30 seconds.
2. If retry also fails, count as a session stall and prompt the operator:
   `Shipment lock conflict on {shipment_id}. Another process holds the lock.`
3. Do NOT proceed without the lock. Do NOT call `backlogit_ship_shipment`.
4. Mid-sequence, a failed acquisition on the already-relocated queue target is the
   **fail-closed** outcome described in the Logical Shipment Lock Contract, not a conflict to
   retry around: the run that opened the sequence already holds the logical lock, and any
   other writer must halt.



## Deterministic Safe-Close Scenario Matrix

* **114-S -> 115-S -> 116-S serial-close success chain**: 114-S safely closes while 115-S/116-S remain protected; once 114-S carries verified `archived_status: shipped` (or legacy `done`), 115-S may exclude 114-S-owned siblings from its protected set; once 115-S carries the same verified provenance, 116-S may exclude 115-S-owned siblings.
* **Negative — archive-while-active**: if the shipment record is archived while still `status: active`, producing `archived_status: active`, halt with `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.
* **Negative — non-shipped-live-before-archive**: if the shipment record cannot be re-read and verified as live `status: shipped` before the archive step, halt with `RECONCILE_FAIL_SHIPMENT_RECORD_LIVE_STATUS`.
* **Negative — missing archive**: if the archive file for the shipment record is missing after `backlogit archive <shipment_id>`, halt with `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.
* **Negative — archived abandoned**: if the shipment record archives with `archived_status: abandoned`, halt with `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.
* **Negative — missing provenance**: if the shipment record is archived but lacks `archived_status: shipped|done`, keep predecessor siblings protected and halt with `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.

## Quality Criteria

* `mode: pre` runs before closing a shipment in Ship Step 6
* `mode: pre` with `expected_status: queued` (or `active` for already-claimed shipments) runs at Ship Step 0.5 intake
* Manifest membership is read explicitly from the shipment record's `custom_fields.items`, never from a top-level `items` field
* `mode: safe-close` runs **in place of** the cascade `backlogit_ship_shipment` call and archives only the manifest item IDs (one artifact at a time) plus the shipment record itself — **except** when Step 0's P-015 verified fully-covered-root classification selects `CASCADE`, in which case the Cascade Close Sub-Procedure runs instead and safe-close steps 1–10 are skipped entirely
* Close-path selection is made **only** from the machine-checkable classification result (Step 0), never inferred from prose or manifest shape alone; any classifier error, ambiguity, or unresolved precondition falls back to safe-close
* Step 0(c)'s "fully covered" check walks the qualifying feature's **full descendant tree, at every depth** (via a full `parent_id` graph, not a single-level scan of direct children only) — a manifest such as `[feature, task]` where that task has an out-of-manifest subtask must fall back to safe-close, never wrongly qualify for `CASCADE` (155-S, PR #407 review, thread PRRT_kwDORzpWpM6b2MJv)
* The Cascade Close Sub-Procedure independently verifies `returned_ids` is empty, `archived_ids` against the two-set `allowed_ids` / `required_ids` gate (step 3: `allowed_ids` = manifest tasks + qualifying feature members + each qualifying feature member's validated linked deliberation IDs (Step 0(c), same engine-defined `source_deliberation_id` (taken as a complete literal string, never regex-scanned) / description / references sources — the latter two scanned with the exact `\b(?:DL\d+|[0-9]+(?:\.[0-9]+)*-DL)\b` matcher Backlogit's own `internal/core.deliberationIDPattern` uses, never a broader "any embedded deliberation ID" reading — and existence-and-`artifact_type: deliberation` validation Backlogit's own `collectArchiveCandidateIDs` uses — never a blanket allowance for arbitrary IDs) + the shipment record; `required_ids` = the shipment record and every qualifying feature member, both unconditionally, plus every other allowed member (a manifest task item, or a qualifying feature member's validated linked deliberation) NOT truly `status: archived` in the Step 0(b)/(c) pre-close declared-status snapshot; `archived_ids - allowed_ids` non-empty halts with `HALT — cascade archived unexpected artifact {id}`, `required_ids - archived_ids` non-empty halts with `HALT — cascade did not archive required artifact {id}`, evaluated as two independent, never-merged conditions — a truly pre-archived **non-shipment, non-feature** allowed member (i.e. a manifest task item, or a qualifying feature's linked deliberation, e.g. 147-F's already-archived 027-DL) has no transition to report and is correctly, expectedly absent from `archived_ids`, never a mismatch, but this tolerance never extends to the shipment record itself, which remains unconditionally required regardless of its own pre-close declared status, nor does it extend to a qualifying feature member itself, which is likewise unconditionally required regardless of its own pre-close declared status — since Backlogit's own `ShipShipment` forces every explicit qualifying feature member through `status: done` before archive-candidate collection ever runs, so it is never still `archived` by that point either), and no `parent_id` was cleared — a mismatch on any of these halts fail-closed with a P-005 violation even though the cascade path was itself permitted
* Safe-close computes the protected set (parent feature + unshipped siblings) from expected IDs and proves it is fully present in queue at a baseline gate before archiving anything
* Safe-close verifies the protected set survives after every single-item archival (verify-after-each invariant), with no pre-archived exemption for protected-set members
* Safe-close moves the shipment record to live `shipped`, verifies it, explicitly archives only that record, and verifies `archived_status: shipped` before closure completes
* Safe-close archives the shipment record as its own single artifact and never via the cascade op
* Safe-close writes the delivered-work merge SHA to each live manifest item's and the shipment record's **frontmatter `commit`** before any terminal transition or archival of that artifact, and never after archival
* Safe-close archives each live manifest item and the shipment record **exactly once**: it dispatches once on whether the **selected invocation transport** both accepts the SHA and guarantees atomic frontmatter-`commit` persistence, and the atomic and live-update paths are **mutually exclusive** — never chained
* Registry parameter presence alone never selects the atomic path: the installed CLI mapping `backlogit archive {id}` has no commit flag, so CLI invocation always takes the live-update path
* The atomic path calls `archive_item` exactly once with commit metadata and never follows it with `backlogit_move_item` / `backlogit_archive_item` / `backlogit_update_item` / `backlogit_track_commit` on that same artifact
* The live-update path writes frontmatter `commit` with a commit-only update (no other field), verifies it while the artifact is still live, then runs the non-atomic terminal/archival sequence exactly once — `done` for a task member, `shipped` for the shipment record — and never writes evidence after a terminal transition that relocates the file
* `backlogit_track_commit` / commit_links are supplemental provenance only and never substitute for the canonical frontmatter `commit`
* The CLI `track_commit` mapping resolves to the **same** `backlogit update {id} --commit {sha}` command as the canonical frontmatter write, so on CLI it is executed **once** and classified as the canonical write, never double-called as a supplemental commit-link; only on MCP is `backlogit_track_commit` a distinct supplemental commit_links call
* An archived manifest member is split into exactly two cases: `current-delivery-pending-finalization` (this shipment's own member, terminal-relocated by its `done` transition before the merge SHA existed) and `pre-archived` (an earlier shipment's delivery) — never one blanket case
* `current-delivery-pending-finalization` is fail-closed: it requires current-shipment `custom_fields.items` membership, Ship-owned same-scope completion evidence tying the `done` transition to this exact shipment, no evidence of delivery by another shipment, the merge SHA confirmed on `origin/main`, and a non-contradictory terminal `done` record — membership alone and a missing commit alone are never proof, and ambiguous provenance halts
* A proven `current-delivery-pending-finalization` member is finalized with a commit-only frontmatter update on the archived record, verified against the exact SHA, then final archive markers applied exactly once only if not already applied, recorded with evidence source `current-delivery-post-terminal` plus its provenance proof — an explicit, evidence-bound exception to live-before-terminal evidence, never silent inference or backfill
* A record already fully `status: archived` that lacks or contradicts its commit halts and routes to historical evidence remediation; safe-close never silently rewrites fully archived provenance
* If **neither** the atomic guarantee **nor** a commit-only live frontmatter update is available, safe-close **halts** with `RECONCILE_FAIL` before any terminal mutation
* After either successful path, one shared verify-after-each invariant confirms the artifact is archived exactly once with the expected frontmatter `commit`, and records the evidence source and SHA value
* Safe-close preserves a pre-archived member's authoritative existing merge commit — verifying and reporting it rather than overwriting it with the current closure SHA — and halts rather than fabricating missing or contradictory evidence
* Repairing a previously completed record is deferred to a separate historical evidence-remediation workflow run outside safe-close, with authoritative provenance and an audit note; that workflow repairs provenance but does not legalize chronology, and safe-close never silently infers or backfills evidence
* Safe-close reports the commit evidence source and value per member and for the shipment record
* Safe-close halts on cascade detection and executes `git restore`/`git revert` recovery only after explicit real-time operator approval, scoped to the exact identified paths or exact revert commit, with a P-005 violation event; it never force/resets, never broadens to unrelated paths or history, and never auto-prunes the manifest
* `mode: post` runs after the safe-close archive sequence in Ship Step 6
* Pre-mode's duplicate-assignment check is a forward overlap scan across live (`queued` and `active`) shipment `custom_fields.items`, run as one status per list call; the skill makes no reverse-orphan claim the schema cannot support
* All six item classifications are represented in the schema
* The canonical logical shipment lock `.backlogit/queue/.{shipment_id}.md.lock` is acquired before pre-mode, honored by every conforming writer across relocation, and released by the original queue path after post-mode (or on any halt)
* Report-and-halt in pre/post mode; safe-close mutation is strictly manifest-scoped with no auto-prune
* `mode: detect-mixed-role` is operator-invoked and strictly READ-ONLY: it
  requires NO `file-lock` acquisition, NEVER mutates any shipment record or
  task, and NEVER calls `backlogit_claim_shipment` or any other status-write
  operation
* `mode: detect-mixed-role` classifies each task-artifact manifest item into
  exactly one per-task ROLE (`live-queued` / `live-active` /
  `archived-completed(done)` — either the terminal-relocation `status: done`
  representation or the explicit-archival `status: archived` +
  `archived_status: done` + valid `archived_from` representation) or flags a
  per-item ANOMALY (`duplicate` / `conflicting` / `missing` /
  `malformed-provenance` / `any-other-archived-status` / `orphan` /
  `out-of-role` / `torn-partial`); role classification is used ONLY to
  DESCRIBE the inconsistency in the report, NEVER to gate a mutation
  (013-DL Addendum G)
* `mode: detect-mixed-role` produces exactly one outcome per candidate
  shipment — `DETECTED` / `REPORTED` / `DEGRADED` — with NO
  `succeeded`/`repaired`/`refused`/two-active outcome, and writes a
  structured audit entry (`.autoharness/gates/shipment-reconcile-detection-audit.log`)
  plus a best-effort telemetry event for every outcome, mirroring the
  `pipeline-topology` force-audit + telemetry pattern
* `mode: detect-mixed-role` includes inline Operator-Remediation Guidance on
  any `REPORTED`/`DEGRADED` outcome, explicitly stating autoharness performs
  NO auto-repair and that a record-only forward re-claim is UNSUPPORTED by
  backlogit 1.8.0 (`ClaimShipment` is manifest-wide + all-or-nothing +
  STRICTLY SINGLE-SHOT; NO `active`→`queued` edge; NO
  `blocked` shipment status), with the read-only source evidence, and never
  fabricates a `blocked`→`queued` or
  `active`→`queued` transition
* `mode: detect-mixed-role` DEGRADED (backlogit unreachable) reports the
  degraded condition and HALTs — it never guesses or acts on partial data

## Related Artifacts

* `.github/skills/file-lock/SKILL.md` — lock acquisition/release primitives
* `.github/agents/_ship.agent.md` — integration points (Step 0.5, Step 6 safe-close)
* `.github/agents/_stage.agent.md` — scope guard (Step 5.5)
* `.github/policies/workflow-policies.md` — P-007 archive integrity policy; P-015 single-artifact closure (cascade prohibition) plus the verified fully-covered-root exception
* `src/autoharness/gates/shipment_closure.py` — this self-hosting repository's own `classify_shipment_close_path` implementation, reused by Step 0 of Safe-Close Mode

## Model Routing

This skill operates at **Tier 2 (Standard)** — file scanning and frontmatter
comparison do not require frontier-level reasoning.

Generated by autoharness | Template: shipment-reconcile/SKILL.md.tmpl
