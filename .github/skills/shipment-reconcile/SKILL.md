---
name: shipment-reconcile
description: "GI/GR reconciliation gate for shipment manifests — verifies every manifest item exists in queue (pre-mode) or archive (post-mode) with the expected status, and closes shipments with the single-artifact safe-close procedure that writes frontmatter commit evidence through the selected invocation transport before any terminal mutation, archives ONLY manifest item IDs with a verify-after-each invariant under a stable logical shipment lock, and halts for approval-gated recovery on cascade instead of the destructive cascade backlogit_ship_shipment."
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

The report ends with a `recommendation`:

* `PROCEED` — all items are `matched`, `current-delivery-pending-finalization`, or `pre-archived`; no action needed
* `HALT — operator reconcile required` — one or more missing, status-mismatch, or conflicting (duplicate-assignment) items (pre-mode)
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

Safe-close additionally records, for **each member and for the shipment record**, the
commit **evidence source** and the SHA **value**:

| Evidence source | Meaning |
|---|---|
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
   * If found, read its frontmatter and compare `status` to `expected_status`
     — classify as `matched` or `status-mismatch`
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

5. **Produce report** and store at
   `.backlogit/reconcile/{shipment_id}-{mode}-{timestamp}.md`.

6. **Gate decision**:
   * If all items are `matched`, `current-delivery-pending-finalization`, or `pre-archived`
     and no conflicts exist → `recommendation: PROCEED`
   * If any `missing`, `status-mismatch`, or `conflict` items exist →
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

6. **Release the logical shipment lock** (acquired in step 1 of pre-mode):
   Invoke `file-lock` release for the **original queue path**
   `.backlogit/queue/{shipment_id}.md`, which removes
   `.backlogit/queue/.{shipment_id}.md.lock`. Release supports a missing or moved target —
   safe-close has already relocated the shipment record — so a warning that the target is
   gone is expected and is not a failure. If release itself fails, log a warning; stale
   locks are operator-recoverable.

### Safe-Close Mode

Runs **in place of** the destructive cascade `backlogit_ship_shipment` call.
Archives only the shipment manifest's explicit item IDs, one artifact at a time,
verifying after each archival that the parent feature and any unshipped sibling
tasks survive. Invoked between pre-mode (`PROCEED`) and post-mode, under the logical
shipment lock pre-mode already holds — the same
`.backlogit/queue/.{shipment_id}.md.lock` identity, still held across the relocation this
mode performs. If invoked standalone, acquire the lock per pre-mode step 1 first and release
it by the original queue path on completion.

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

8. **Write the shipment record's own frontmatter commit evidence, then archive it** (single
   artifact, non-cascading): close `{shipment_id}` via **exactly one** of the two **mutually
   exclusive** paths below. **Dispatch once**, on the same condition used in step 4 — whether
   the **selected invocation transport** for `archive_item` itself accepts the delivered SHA
   and guarantees atomic commit metadata persistence to the archived artifact's frontmatter
   `commit` — and then follow only that path. Registry parameter presence alone is not that
   guarantee. The paths are alternatives, never a sequence: the shipment record is archived
   **exactly once**.
   * **Path A — atomic archive** (the selected transport carries that tool-contract
     guarantee). Invoke `backlogit_archive_item` on `{shipment_id}` **exactly once**, passing
     the **actual delivered-work merge commit** — the `merge_commit_sha` input, which merged
     this shipment's delivered work into `main` — in the operation's configured field (for
     backlogit, `commit_sha`), so archival and frontmatter evidence commit together in that
     single call. **No later move/archive runs at all**: do **not** afterwards call
     `backlogit_move_item`, `backlogit_archive_item`, `backlogit_update_item`, or
     `backlogit_track_commit` on the shipment record; it has already left the queue.
     Evidence source: `atomic-archive-metadata`.
   * **Path B — live frontmatter-commit update** (the selected transport carries no such
     guarantee — always the case for the installed CLI mapping `backlogit archive {id}`,
     which has **no commit flag**). While `{shipment_id}` is still **live** in
     `.backlogit/queue/`, write the delivered-work merge SHA into its frontmatter `commit`
     with a **commit-only** update — MCP `backlogit_update_item` carrying **only** `commit`,
     or CLI `backlogit update {shipment_id} --commit {sha}` — and **verify** it on the live
     record **before any terminal mutation**. **No other field** may be set in that call.
     Only then run the configured non-atomic terminal/archival sequence **exactly once**:
     transition the shipment record to its valid terminal shipment status `shipped` via
     `backlogit_move_item`, then apply the archive markers exactly once via
     `backlogit_archive_item` (CLI `backlogit archive {shipment_id}`). The shipment status
     enum is `queued` / `active` / `shipped` / `abandoned`: `done` is **not** a shipment
     status and MUST NOT be used for the shipment record — it is the task terminal status
     used in step 4. Where that terminal transition **relocates** the record out of
     `.backlogit/queue/` before the archive-marker operation, the record is no longer
     writable at its original path, so this contract **never** writes evidence after that
     transition. Evidence source: `frontmatter-commit-update`. On **MCP** an optional
     `backlogit_track_commit` commit-link entry is **supplemental** only and never
     substitutes for frontmatter `commit`; on **CLI** `track_commit` maps to the same
     `backlogit update {shipment_id} --commit {sha}` already executed above, so it is
     **never** double-called as a supplemental step.
   * **Neither path available.** If the selected transport neither guarantees atomic
     frontmatter-`commit` persistence **nor** offers a commit-only frontmatter update on the
     live record, **halt** with `RECONCILE_FAIL` **before any terminal mutation** rather than
     archiving the shipment record without evidence — never write the evidence after archival.
   * Do not substitute a decision or closure-authority SHA here. The commit that recorded
     the decision or approval to close authorizes the closure; it is not the delivered
     work.
   * **Verify after either successful path** (one shared invariant, whichever branch ran):
     confirm `{shipment_id}` now appears in `.backlogit/archive/` **exactly once**,
     carries frontmatter `commit` equal to `merge_commit_sha`, and is gone from
     `.backlogit/queue/`; record the evidence **source** and the SHA **value** in the report.
     Closing the shipment record is the point of safe-close, so it is **not** in the
     protected set — but it is archived as its own single artifact, **never** via the cascade
     `backlogit_ship_shipment`.
   * Then re-run the verify-after-each invariant (step 5) to confirm archiving the shipment
     record did not disturb the protected set.

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

## Quality Criteria

* `mode: pre` runs before closing a shipment in Ship Step 6
* `mode: pre` with `expected_status: queued` (or `active` for already-claimed shipments) runs at Ship Step 0.5 intake
* Manifest membership is read explicitly from the shipment record's `custom_fields.items`, never from a top-level `items` field
* `mode: safe-close` runs **in place of** the cascade `backlogit_ship_shipment` call and archives only the manifest item IDs (one artifact at a time) plus the shipment record itself
* Safe-close computes the protected set (parent feature + unshipped siblings) from expected IDs and proves it is fully present in queue at a baseline gate before archiving anything
* Safe-close verifies the protected set survives after every single-item archival (verify-after-each invariant), with no pre-archived exemption for protected-set members
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

## Related Artifacts

* `.github/skills/file-lock/SKILL.md` — lock acquisition/release primitives
* `.github/agents/.ship.agent.md` — integration points (Step 0.5, Step 6 safe-close)
* `.github/agents/.stage.agent.md` — scope guard (Step 5.5)
* `.github/policies/workflow-policies.md` — P-007 archive integrity policy; P-015 single-artifact closure (cascade prohibition)

## Model Routing

This skill operates at **Tier 2 (Standard)** — file scanning and frontmatter
comparison do not require frontier-level reasoning.

Generated by autoharness | Template: shipment-reconcile/SKILL.md.tmpl
