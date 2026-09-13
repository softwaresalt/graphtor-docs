# Ship Checkpoint — 049-S Post-Merge Safe-Close Halted at Step 8

**Timestamp:** 2026-09-13T05:37Z
**Shipment:** 049-S ("Fix MCP serve initialize-handshake regression / Copilot CLI OS error 232")
**PR:** #120 — **MERGED**, merge SHA `98f8fc63024095b0b8697545986646a564677917`
**Branch:** `chore/fix-mcp-serve-initialize-handshake-regression` (still checked out; the
post-merge closure branch `post-merge/fix-mcp-serve-initialize-handshake-regression` has
**not** been created yet — creating it is deferred until safe-close reaches `CLOSED`).

## Status: HALTED — awaiting operator disposition

This is a genuine non-bypassable blocker, not a partial-completion stall. Full detail is in
the halt handoff record: `.backlogit/reconcile/049-S-halt-20260913T053721Z.md`
(`status: open`). Summary below.

## What is fully complete (pre-merge and merge)

* All 8 manifest tasks implemented, tested, reviewed (standard + adversarial), and merged.
* PR #120 went through 9 rounds of Copilot review remediation (all in-scope per P-021 C1),
  converged to 0 unresolved threads, P-018 gate `SATISFIED`.
* Merge executed with merge-commit strategy (`gh pr merge 120 --merge`), matching
  `allowed_merge_methods: ["merge"]` (P-009 compliant).
* Merge Confirmation Gate passed: PR state `MERGED`; merge SHA confirmed ancestor of
  `origin/main` via `git merge-base --is-ancestor`.
* Post-merge closure Pre-Mode reconciliation: `PROCEED`
  (`.backlogit/reconcile/049-S-pre-20260913T052820Z.md`).
* Safe-Close Mode steps 0–7 complete:
  * Close-path classification: **SAFE_CLOSE** (partial-feature shipment; `056-F` covering
    feature intentionally excluded from the manifest per Stage handoff; no feature member
    exists to evaluate for the cascade "verified fully-covered-root exception").
  * Protected set computed: `056-F` + 25 unshipped sibling tasks
    (`056.004-T`–`056.018-T`, `056.024-T`–`056.033-T`).
  * Baseline integrity gate: all 26 protected-set members confirmed present in
    `.backlogit/queue/`.
  * Archived-member evidence preflight: all 8 manifest items proven **Case A**
    (`current-delivery-pending-finalization`) — membership, Ship-owned completion
    evidence (this session's own checkpoints, e.g.
    `docs/memory/2026-09-12/ship-049S-progress-checkpoint.md`), no foreign delivery,
    merge proven, record consistent.
  * All 8 manifest items **finalized**: commit-only frontmatter write
    (`commit: 98f8fc63024095b0b8697545986646a564677917`) + final archive markers
    (`status: archived`, `archived_status: done`) applied and verified for each:
    `056.001-T`, `056.020-T`, `056.022-T`, `056.023-T`, `056.021-T`, `056.002-T`,
    `056.003-T`, `056.019-T`.
  * Verify-after-each invariant: protected set confirmed intact after every single
    archival (no cascade, no unexpected relocation).

## What is blocked

**Safe-Close Mode step 8** (close the shipment record `049-S` itself). The installed
backlogit CLI (confirmed version 1.10.1) refuses the generic non-cascading path:

```
backlogit move 049-S --status shipped
# → move shipment 049-S to shipped via generic path: backlogit: shipment must be
#   shipped via ShipShipment, not a direct status update  (exit 9)
```

The only other CLI path (`backlogit shipment ship 049-S`) is the P-015-forbidden
destructive cascade for a task-only, partial-feature manifest (it would requeue/
detach/archive scope beyond the 8-item manifest, threatening `056-F` and its 25 other
unshipped siblings — exactly what Safe-Close Mode exists to prevent). No scope-limiting
flag exists on `shipment ship` (confirmed via `--help`). No other CLI subcommand exists
(confirmed via `backlogit shipment --help`).

This is **not** the pre-authorized topology-gate override (scoped exclusively to
`PREDECESSOR_NOT_SHIPPED` / `048-S` at `pre_claim`/`post_claim`/`lifecycle` phases). It is
an unrelated tool-capability ambiguity, matching the operator's explicit dark-mode stop
condition: "ambiguity unrelated to the explicitly overridden legacy gate."

## Recovery actions taken (per shipment-reconcile Halt Recovery Protocol)

1. Stopped before any further mutation (shipment record itself was never mutated — both
   attempts confirmed no-ops via unchanged `status`/`updated_at`).
2. Persisted the halt handoff record
   (`.backlogit/reconcile/049-S-halt-20260913T053721Z.md`, `status: open`) **before**
   touching the lock, per protocol.
3. Released the canonical shipment lock by its original queue path
   (`.backlogit/queue/049-S.md`) — confirmed released
   (`.backlogit/queue/.049-S.md.lock` no longer exists).

## Working tree (uncommitted, on `chore/fix-mcp-serve-initialize-handshake-regression`)

```
 M .backlogit/archive/056.001-T.md
 M .backlogit/archive/056.002-T.md
 M .backlogit/archive/056.003-T.md
 M .backlogit/archive/056.019-T.md
 M .backlogit/archive/056.020-T.md
 M .backlogit/archive/056.021-T.md
 M .backlogit/archive/056.022-T.md
 M .backlogit/archive/056.023-T.md
 M .backlogit/hooks_queue.jsonl
?? .backlogit/reconcile/049-S-pre-20260913T052820Z.md
?? .backlogit/reconcile/049-S-halt-20260913T053721Z.md
```

Not committed — per Ship Step 6.0(e), backlog state is committed only after safe-close
returns `CLOSED` and post-mode returns `PROCEED`. Neither reached. This is recoverable,
fully-described state; nothing has been lost.

## Next steps (on resume, after explicit operator disposition)

1. If operator authorizes a scoped/alternate close mechanism, or explicitly authorizes the
   `ShipShipment` cascade with informed acceptance of its blast radius: re-acquire the
   `049-S` lock, re-verify the protected set is still intact, then complete Safe-Close
   Mode step 8 (and steps 9–10), Post-Mode, branch/commit, `operational-closure`,
   `compound-refresh`, follow-up handoffs, mandatory P-020 `compact-context`, index
   resync, closure PR, return to `main`.
2. If operator defers, leave state exactly as-is (shipment `active`, 8 tasks archived and
   evidenced, halt handoff record `open`) until a follow-up session.
3. Mark the halt handoff record `status: resolved` only once a terminal recommendation is
   reached or the operator records an explicit revert/defer decision.

## Stop condition triggered

Yes — per the operator's dark-mode brief: "ambiguity unrelated to the explicitly
overridden legacy gate" and instruction 6 ("Close any dedicated closure PR only if
authorization applies and all gates pass; otherwise halt with exact state"). This is a
genuine non-bypassable blocker at the tool-capability level, not a policy violation by
Ship and not a partial/incomplete effort — it is reported for explicit operator decision.

## Addendum 2026-09-13T05:41Z — `update --status shipped` alternative declined (P-010)

Operator authorized autonomous resume and explicitly excluded the `ShipShipment`
cascade, then proposed `backlogit update <id> --status shipped` (validated first against
a disposable workspace copy) as a non-cascading alternative to the refused `move` path.

This agent declined to run the proposed validation or apply the change. The Ship agent's
own Role Boundary Mutation Classification table classifies `backlogit_update_item`
(CLI `backlogit update`) as allowed **only** for the `commit` field; `status` is
explicitly named as forbidden, with no operator-override clause present for that row
(unlike the Git-recovery row, which is explicitly conditioned on real-time operator
approval). This is the same unconditional, fail-closed pattern the agent's instructions
apply elsewhere ("operator confirmation cannot authorize this... no
operator-confirmation carve-out"). `backlogit_move_item` is the classified mechanism for
this exact transition, but the installed CLI's `move` implementation refuses it — a
tool-capability gap, not license to substitute a differently-classified, explicitly
forbidden field. A disposable-copy proof of technical safety does not change that
classification, so the proof was not run and no mutation was attempted.

State is unchanged from above: shipment `049-S` still `status: active`, halt handoff
record still `status: open`
(`.backlogit/reconcile/049-S-halt-20260913T053721Z.md`, addendum appended there too),
lock released, working tree preserved uncommitted. Remaining options: (1) a mechanism
already covered by this agent's own classification table (none found this session), or
(2) defer the shipment record's terminal close to a follow-up session/tooling fix, with
all 8 constituent tasks already durably archived and evidenced.

## Addendum 2026-09-13T05:47Z — MCP `backlogit_move_item` disposable-copy proof (definitive, blocker confirmed exhausted)

Operator authorized evaluating the MCP transport form of `backlogit_move_item` (the one
mechanism this agent's classification table actually permits for the shipment `active →
shipped` transition), proven first against a disposable copy of the workspace under the
repo's gitignored `.autoharness/staging/` area.

Fetched the tool schema via `backlogit manifest` (`{id, status, commit_sha?}`), copied the
full `.backlogit/` tree (847 files, SHA-256 baseline hashed), started
`backlogit --cwd <disposable> mcp` and drove it directly over JSON-RPC 2.0
(`initialize` → `notifications/initialized` → `tools/call backlogit_move_item
{id:"049-S", status:"shipped"}`).

**Result: identical structural refusal to the CLI** —
`{"error":"shipment_shipped_requires_envelope","message":"...shipment must be shipped via
ShipShipment, not a direct status update"}`, `isError: true`. Re-hashed all 847 files
afterward: zero artifact-content mutation (only the transient SQLite `-shm` index file
touched, not a backlog artifact). The disposable `049-S.md` remained `status: active`
throughout. Cleaned up all disposable/test artifacts (gitignored; confirmed nothing
entered git history). Confirmed the real workspace's `049-S.md` unchanged throughout.

**This proves `shipment_shipped_requires_envelope` is a structural, transport-independent
guard in backlogit 1.10.1** — CLI `move` and MCP `backlogit_move_item` both hit the same
validation; there is no non-cascading path to `shipped` in either transport.
`ShipShipment` (excluded by explicit operator instruction) is the tool's only implemented
path. This exhausts every mechanism this agent's Role Boundary classifies as permitted for
this transition. The blocker is now fully confirmed non-bypassable under current authority
and the tool's actual capability. State remains fully preserved and unchanged; reported to
the operator for final disposition.

## Addendum 2026-09-13T06:04Z — `ShipShipment` cascade blast-radius proof (disposable copy, report-only, NOT applied live)

Operator authorized a disposable-copy blast-radius test of the actual forbidden cascade
(`backlogit shipment ship 049-S --sha ... --message ... --author ...`) against a fresh
copy of the **current** partially-finalized state, explicitly scoped as report-only (a
clean result does not itself authorize live application).

Copied the current real `.backlogit/` (847 files) into a fresh disposable directory,
SHA-256-hashed all files as baseline, ran the cascade with `--log-level debug`. The
command took **~5m55s** wall-clock (CPU climbing steadily throughout — genuinely
computing, not deadlocked) then exited 0 with:
```json
{"shipment_id":"049-S","shipment_status":"shipped","archived_ids":["049-S"],
 "returned_ids":["056.004-T",...,"056.033-T" (25 total)],
 "commit_sha":"98f8fc63024095b0b8697545986646a564677917"}
```

**Full hash diff (847→867 files) result:**
* `049-S` moved queue→archive (`status: archived`, `archived_status: shipped`) — expected.
* **All 25 `returned_ids` sibling tasks' queue `.md` files changed content** — confirmed
  by direct frontmatter inspection (`056.011-T` sampled): `parent_id: 056-F` is entirely
  **removed**, `updated_at` bumped, `status` unchanged (`queued`). This is a real,
  substantive content mutation of 25 backlog records outside the 049-S manifest.
* `056-F.md` (covering feature): **hash-identical, completely untouched** — not a
  manifest member, so it never enters the cascade's "unconditional done" feature path.
* No other shipment record (048-S/050-S/052-S/053-S/etc.) touched.
* Remaining diff is expected audit/log/index noise: new/appended `.jsonl` + `.jsonl.lock`
  log files for the shipment and each returned task, one new `hooks_queue.jsonl` entry,
  and an index rebuild in `backlogit.db`.
* All 8 already-archived manifest task files untouched (not in the diff at all).

**Conclusion — per the operator's own stated conditional** ("if and only if... changes
solely 049-S plus its own audit/log/index artifacts... if it touches anything else,
confirm the cascade path remains prohibited"): the proof does **not** clear that bar — 25
protected sibling task records are directly content-mutated (parent-link removal), not
merely logged. **This confirms the cascade path remains prohibited.** Reported only; not
applied to the real workspace. Separately noted: the ~6-minute runtime for a 26-item
protected set is itself a practical operability concern for this backlogit version,
independent of the P-015 verdict.

Cleaned up the disposable directory and all baseline/output artifacts (gitignored, nothing
entered git history). Confirmed the real workspace fully unaffected before, during, and
after: `049-S.md` `status: active` unchanged, `056-F.md` `status: active` unchanged,
`056.011-T.md` still carries `parent_id: 056-F` unchanged, `git status --short` shows only
the same pre-existing uncommitted set from Safe-Close steps 1–7. Every role-permitted
mechanism and the forbidden cascade's actual blast radius are now all empirically proven.
Remaining options unchanged: (1) defer the shipment record's terminal close to a follow-up
session/tooling fix, or (2) an explicit, blast-radius-aware operator authorization of the
cascade (not granted — this test was explicitly report-only).
