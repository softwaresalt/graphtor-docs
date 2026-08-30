---
name: shipment-reconcile
description: "GI/GR reconciliation gate for shipment manifests — verifies every manifest item exists in queue (pre-mode) or archive (post-mode) with the expected status, and closes shipments with the single-artifact safe-close procedure that records commit evidence before archival, archives ONLY manifest item IDs with a verify-after-each invariant, and halts for approval-gated recovery on cascade instead of the destructive cascade backlogit_ship_shipment."
---

# Shipment Reconcile

Provides a double-entry (GI/GR) integrity check for shipment manifests. Run
`mode: pre` before closing a shipment and `mode: post` after the archive +
restore steps complete. Run `mode: safe-close` **in place of** the destructive
cascade `backlogit_ship_shipment` call to archive only the shipment manifest's
explicit item IDs one artifact at a time, verifying after each that the parent
feature and any unshipped sibling tasks survive.

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
  Safe-close archives only the manifest item IDs individually; it never calls the
  cascade op.
* **Ship Step 0.5** (sanity check): pre-mode at intake with `expected_status: queued`
  (or `active` if the shipment was already claimed in a prior session)
  to catch Stage-side over-inclusion before any build work begins.
* **Ad-hoc audit**: any time an operator suspects manifest drift.

## Inputs

| Parameter | Required | Values | Notes |
|---|---|---|---|
| `mode` | yes | `pre` \| `post` \| `safe-close` | Controls which check/close phase runs |
| `shipment_id` | yes | e.g. `004-S` | The shipment to reconcile |
| `expected_status` | pre-mode only | `queued` \| `active` \| `done` | `queued` for fresh intake; `active` when shipment already claimed in a prior session; `done` for pre-ship check |
| `merge_commit_sha` | post-mode and safe-close | git SHA | The **actual delivered-work merge commit**: the commit that merged the shipped work into `main`, already confirmed present on `origin/main` by the caller's merge-confirmation gate. In safe-close it is recorded as closure evidence on each item **before** that item is archived. It is never an anticipated, reconstructed, or branch-tip SHA, and never a PR planning or decision/closure-authority SHA |

## Output

A structured **reconciliation report** stored at
`.backlogit/reconcile/{shipment_id}-{mode}-{timestamp}.md`.

Every item in the manifest is classified as one of:

| Classification | Pre-Mode Meaning | Post-Mode Meaning |
|---|---|---|
| `matched` | Queue file present AND declared status matches `expected_status` | Archive file present for this item |
| `pre-archived` | No queue file found but archive file exists — item already archived before this shipment ran; treated as valid | N/A (all items are expected in archive; use `matched` / `missing`) |
| `missing` | No queue or archive file found for this manifest item | Archive file not found for this manifest item |
| `status-mismatch` | Queue file present but declared status does not match `expected_status` | N/A (post-mode does not check status fields) |
| `orphan` | Queue file declares this `shipment_id` in its frontmatter but is NOT in the manifest | N/A (post-mode does not scan queue files) |

> Classification semantics are mode-dependent. Pre-mode checks the queue
> for status correctness; post-mode checks the archive for file presence only.

The report ends with a `recommendation`:

* `PROCEED` — all items are `matched` or `pre-archived`; no action needed
* `HALT — operator reconcile required` — one or more missing, status-mismatch, or orphan items (pre-mode)
* `HALT — restore archives` — missing archive files or unrestored deletions (post-mode)
* `CLOSED` — safe-close archived every manifest item individually, archived the
  shipment record itself, and the protected set (parent feature + unshipped siblings)
  is intact
* `HALT — cascade detected, revert required` — safe-close found a non-manifest artifact (parent feature or a sibling task) archived or deleted; the unintended change must be reverted before any commit

For `mode: safe-close`, the report also records the **protected set** (the parent
feature file and every unshipped sibling task file that must survive closure) and,
per manifest item, whether it was `matched` (archived by this run) or
`pre-archived` (already archived before this run; skipped to avoid
double-archival and false-positive cascade flags).

Safe-close additionally records, for **each member and for the shipment record**, the
commit **evidence source** and the SHA **value**:

| Evidence source | Meaning |
|---|---|
| `atomic-archive-metadata` | The archive operation carried commit metadata, so archival and evidence committed together |
| `track_commit` | `backlogit_track_commit` recorded the delivered-work merge SHA on the item before it was archived |
| `pre-existing` | A pre-archived member's own earlier merge commit, preserved and verified — never overwritten with this run's closure SHA |

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
  this skill may move or archive are the shipment manifest's explicit item IDs and
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
* **Commit evidence before archival.** Safe-close records the actual delivered-work merge
  commit on each live manifest item and on the shipment record **before** that artifact is
  archived — via atomic archive commit metadata when available, otherwise via
  `backlogit_track_commit`. It never writes evidence after archival, never overwrites a
  pre-archived member's existing merge commit with the current closure SHA, and halts
  rather than fabricating missing or contradictory evidence.
* **Single-writer lock.** When invoked from Ship Step 6, this skill holds the
  `.backlogit/queue/{shipment_id}.md` file lock (via the `file-lock` skill) for
  the duration of pre-mode → safe-close → post-mode. See lock protocol in the
  Required Protocol section below.
* **Halt on RECONCILE_FAIL.** Do not proceed to safe-close unless pre-mode
  returns `PROCEED`. Do not commit backlog state if safe-close returns
  `HALT — cascade detected, revert required`. Surface the report path to the operator.

## Required Protocol

### Pre-Mode

1. **Acquire single-writer lock** (Ship Step 6 invocations only, not intake):
   Invoke the `file-lock` skill to acquire `.backlogit/queue/{shipment_id}.md`.
   If lock acquisition fails, count as a session stall (circuit-breaker protocol)
   and prompt the operator.

2. **Load manifest** via `backlogit_get_item(shipment_id)`.
   Extract the `items` list.

3. **Check each manifest item**:
   * Attempt to locate the file at `.backlogit/queue/{id}.*`
   * If found, read its frontmatter and compare `status` to `expected_status`
     — classify as `matched` or `status-mismatch`
   * If NOT found in queue, check `.backlogit/archive/{id}.*`
     — if archive file exists, classify as `pre-archived` (valid; item already shipped)
     — if no file in either location, classify as `missing`

4. **Orphan scan**:
   Scan `.backlogit/queue/` for any files whose YAML frontmatter declares
   `shipment_id: {shipment_id}` but whose ID is NOT present in the manifest `items` list.
   Classify each such file as `orphan`.

5. **Produce report** and store at
   `.backlogit/reconcile/{shipment_id}-{mode}-{timestamp}.md`.

6. **Gate decision**:
   * If all items are `matched` or `pre-archived` and no orphans exist → `recommendation: PROCEED`
   * If any `missing`, `status-mismatch`, or `orphan` items exist →
     `recommendation: HALT — operator reconcile required`
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

6. **Release lock** (acquired in step 1 of pre-mode):
   Invoke `file-lock` release for `.backlogit/queue/{shipment_id}.md`.
   If release fails, log a warning — stale locks are operator-recoverable.

### Safe-Close Mode

Runs **in place of** the destructive cascade `backlogit_ship_shipment` call.
Archives only the shipment manifest's explicit item IDs, one artifact at a time,
verifying after each archival that the parent feature and any unshipped sibling
tasks survive. Invoked between pre-mode (`PROCEED`) and post-mode, under the lock
pre-mode already holds. If invoked standalone, acquire the lock per pre-mode
step 1 first and release it on completion.

1. **Load manifest** via `backlogit_get_item(shipment_id)`. Extract the
   `items` list. These IDs are the **only** artifacts safe-close may move or
   archive.

2. **Compute the protected set** (partial-feature detection):
   * Derive the covering feature ID from the manifest item hierarchy
     (e.g. a task `055.002-T` belongs to feature `055-F`).
   * If the covering feature ID is **not** in the manifest `items`, this is a
     **partial-feature shipment**. Add the covering feature to the protected set.
   * Enumerate every task sharing the covering feature's hierarchy prefix whose ID
     is **not** in the manifest `items` (the unshipped siblings) by scanning **both**
     `.backlogit/queue/` **and** `.backlogit/archive/` (plus the
     feature file's declared children when available). Add each to the protected set.
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

4. **Record commit evidence, then archive, each manifest item individually** (loop over
   `items` ONLY). Commit evidence is always written **before** the artifact moves, because
   once an item is archived its source record may no longer be mutable:
   * If the item's file is in `.backlogit/queue/` (a **live** manifest item):
     * **Evidence first.** If the backlog registry's `archive_item` operation supports
       **atomic commit metadata**, pass `merge_commit_sha` to that operation using the
       tool's configured field (for backlogit, `commit_sha`) so archival and evidence
       commit together. Otherwise call `backlogit_track_commit` (registry `track_commit`;
       CLI mapping `backlogit update {id} --commit {sha}`) to record `merge_commit_sha` on
       the item **BEFORE** the `backlogit_move_item` / `backlogit_archive_item` calls.
     * If neither an atomic archive field nor a source-mutating `track_commit` is
       available, **halt** with `RECONCILE_FAIL` and report the missing capability. Never
       attempt to write commit evidence **after archival**, and never treat a
       report-only note as a substitute for evidence on the item.
     * **Then archive.** Move the item to `done` via `backlogit_move_item`, then archive
       that single artifact via `backlogit_archive_item` (CLI fallback
       `backlogit archive {id}`). Classify `matched` and record the evidence source
       (`atomic-archive-metadata` or `track_commit`) and the recorded SHA value.
   * If the item's file is already in `.backlogit/archive/` (a **pre-archived** member):
     classify `pre-archived` and **skip** — do not re-archive. Reusing the `pre-archived`
     classification prevents false-positive cascade flags on items that were legitimately
     shipped earlier. Its commit evidence belongs to the earlier shipment that delivered
     it, so:
     * **Preserve** the item's existing actual merge commit. **Verify and report** it —
       never **overwrite** it with this run's closure SHA. A pre-archived member was
       delivered by a different merge, so applying the current closure SHA would
       falsify its provenance.
     * Record the preserved value and its evidence source (`pre-existing`) in the report.
     * If the required evidence is **missing** (no commit recorded on the pre-archived
       item) or **contradictory** (a recorded commit that conflicts with the item's
       documented delivery), **halt** with `RECONCILE_FAIL` and name the item. Never
       fabricate, infer, or backfill a SHA to close the gap.
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

8. **Record the shipment record's own commit evidence, then archive it** (single artifact,
   non-cascading):
   * **Evidence first.** Record the shipment record's **actual delivered-work merge
     commit** — the `merge_commit_sha` input, which merged this shipment's delivered work
     into `main` — **BEFORE** archiving the record. Use the `archive_item` operation's
     atomic commit metadata field when it supports one; otherwise call
     `backlogit_track_commit` on `{shipment_id}` first. If neither is available, halt with
     `RECONCILE_FAIL` rather than archiving without evidence — never write the evidence
     after archival.
   * Do not substitute a decision or closure-authority SHA here. The commit that recorded
     the decision or approval to close authorizes the closure; it is not the delivered
     work.
   * **Then archive** only the `{shipment_id}` artifact via `backlogit_archive_item` (CLI
     fallback `backlogit archive {id}`). Closing the shipment record is the point of
     safe-close, so it is **not** in the protected set — but it must be archived as its own
     single artifact, **never** via the cascade `backlogit_ship_shipment`.
   * Then re-run the verify-after-each invariant (step 5) to confirm archiving the shipment
     record did not disturb the protected set.

9. **Produce safe-close report** per the same schema, recording the protected set, each
   item's classification, the shipment-record archival, and the recommendation. For **each
   member and for the shipment record**, record the commit **evidence source** — one of
   `atomic-archive-metadata`, `track_commit`, or `pre-existing` (preserved from an earlier
   shipment) — alongside the exact SHA **value** recorded or preserved, so a reviewer can
   audit per member where each piece of commit evidence came from.

10. **Gate decision**:
    * All manifest items `matched` or `pre-archived`, the shipment record archived,
      and the protected set intact → `recommendation: CLOSED`. Proceed to post-mode.
    * Any cascade detected → `recommendation: HALT — cascade detected, revert required`
      (see step 6). Do not proceed to the commit step.

### Lock-Conflict Scenario

If pre-mode cannot acquire the lock because another process holds it:

1. Retry once after 30 seconds.
2. If retry also fails, count as a session stall and prompt the operator:
   `Shipment lock conflict on {shipment_id}. Another process holds the lock.`
3. Do NOT proceed without the lock. Do NOT call `backlogit_ship_shipment`.

## Quality Criteria

* `mode: pre` runs before closing a shipment in Ship Step 6
* `mode: pre` with `expected_status: queued` (or `active` for already-claimed shipments) runs at Ship Step 0.5 intake
* `mode: safe-close` runs **in place of** the cascade `backlogit_ship_shipment` call and archives only the manifest item IDs (one artifact at a time) plus the shipment record itself
* Safe-close computes the protected set (parent feature + unshipped siblings) from expected IDs and proves it is fully present in queue at a baseline gate before archiving anything
* Safe-close verifies the protected set survives after every single-item archival (verify-after-each invariant), with no pre-archived exemption for protected-set members
* Safe-close archives the shipment record as its own single artifact and never via the cascade op
* Safe-close records commit evidence (actual delivered-work merge SHA) on each live manifest item and on the shipment record **before** archiving that artifact, via atomic archive commit metadata or `backlogit_track_commit`, and never after archival
* Safe-close preserves a pre-archived member's existing merge commit — verifying and reporting it rather than overwriting it with the current closure SHA — and halts rather than fabricating missing or contradictory evidence
* Safe-close reports the commit evidence source and value per member and for the shipment record
* Safe-close halts on cascade detection and executes `git restore`/`git revert` recovery only after explicit real-time operator approval, scoped to the exact identified paths or exact revert commit, with a P-005 violation event; it never force/resets, never broadens to unrelated paths or history, and never auto-prunes the manifest
* `mode: post` runs after the safe-close archive sequence in Ship Step 6
* All five item classifications are represented in the schema
* Lock is acquired before pre-mode and released after post-mode (or on any halt)
* Report-and-halt in pre/post mode; safe-close mutation is strictly manifest-scoped with no auto-prune

## Related Artifacts

* `.github/skills/file-lock/SKILL.md` — lock acquisition/release primitives
* `.github/agents/.ship.agent.md` — integration points (Step 0.5, Step 6 safe-close)
* `.github/agents/.stage.agent.md` — scope guard (Step 5.5)
* `.github/policies/workflow-policies.md` — P-007 archive integrity policy; P-015 single-artifact closure (cascade prohibition)

## Model Routing

This skill operates at **Tier 2 (Standard)** — file scanning and frontmatter
comparison do not require frontier-level reasoning.

Generated by autoharness | Template: shipment-reconcile/SKILL.md.tmpl
