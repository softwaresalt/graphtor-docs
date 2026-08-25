---
title: "Stage session — store.rs TOCTOU no-follow handle fix staged as queued shipment 051-S"
type: session-memory
agent: stage
date: 2026-08-25
session: stage-store-toctou-nofollow-handle
shipment: 051-S
feature: 059-F
status: complete
---

## Task

Bounded Stage staging operation: stage EXACTLY stash entries **E86A6E56** and
**5905CDEE** together as one carefully-reviewed filesystem-security release, producing
one coherent reviewed implementation plan and one **queued** shipment (never executed).
Both are sibling check-then-use TOCTOU symlink/reparse-swap bugs on the read-only
permission mutation paths in `src/db/store.rs`. Isolated from 9CEC208C/050-S, 049-S,
8C2E313D, 013.008-T, and all other stash. No source changes, builds, branches, PRs, or
Ship invocation.

## Outcome

* **Queued shipment: 051-S** — "Identity-bound no-follow handle store.rs TOCTOU fix
  (EngineReadonlyGuard + clear_stale_readonly_lock)", priority high, status queued.
* **Covering feature: 059-F** (counter was ahead of the 058 expected at triage).
* **Tasks (hierarchy under 059-F):**
  * 059.001-T — U1 add platform-gated no-follow deps (config)
  * 059.002-T — U2 safe no-follow handle helper + handle-bound perm primitives (code, test-first)
  * 059.003-T — U3 bind EngineReadonlyGuard lock/rollback/Drop to retained handles (from stash 5905CDEE)
  * 059.004-T — U4 bind clear_stale_readonly_lock probe/clear to no-follow handles (from stash E86A6E56)
  * 059.005-T — U5 cross-platform sidecar swap-resistance matrix + per-platform fail-closed signal + junction refusal, deltas (a)-(c) (tests)
  * 059.006-T — U6 integrate the beneath-root opener and permission boundary
  * 059.007-T — U7 prove the capability root/API/MSRV foundation
  * 059.008-T — U8 prove the contained SQLite/Cozo engine boundary
  * 059.009-T — U9 integrate the capability-bound SQLite/Cozo engine open
  * 059.010-T — U10 Windows retained-handle engine non-interference + broader non-name-surrogate reparse regression, deltas (d)-(e) (tests; added by PR #107 review)
  * 059.011-T — U11 deterministic should_refuse_reparse predicate + Windows literal-equality + single-source structural proof, deltas (f)-(g) (tests; added by PR #107 review)
* **Shipment manifest (12 items):** [059-F, 059.001-T, 059.002-T, 059.003-T, 059.004-T,
  059.005-T, 059.006-T, 059.007-T, 059.008-T, 059.009-T, 059.010-T, 059.011-T].
* **Dependencies:** U8←U7; U1←U7+U8; U6←U1+U7+U8; U9←U6+U8;
  U2←U1+U6+U9; U3/U4←U2+U6+U9; U5←U3+U4+U6+U9; U10←U3+U4+U6+U9; U11←U2 (`blocks`).
  U7 or U8 BLOCKED halts all production work. U5/U10/U11 are terminal test units (nothing depends on them), so the split is acyclic.
* **Semantic links:** 059.004-T related_to 059.003-T (sibling same-mechanism);
  059-F related_to 052-F (reparse-point guards prior art); 059-F related_to 056.024-T
  (no-follow config mutation sibling).
* **Stash consumed:** 5905CDEE → 059.003-T, E86A6E56 → 059.004-T (state `harvested`,
  stash_links recorded). Both removed from the active stash queue; no other stash touched.

## Planning / decision artifacts

* Deliberation: `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`
  (Option A retained no-follow handle CHOSEN; B path re-check REJECTED; C identity-verified
  re-open documented fallback). Windows feasibility rows corrected during remediation.
* Impl plan: `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md`
  (units U1–U11, dependency graph, `## Constitution Check`, `## Plan Hardening`,
  `## Plan Review`).

## Review outcome

* **Plan review** — 5 personas (Security Lens, Correctness, Rust, Constitution, Scope
  Boundary). **Initial gate: FAIL** (2×P1 + several P2). Remediated in-artifact
  (attempt 1); **post-remediation gate: PASS**.
* **P1 (fixed):** (F1) Windows `FILE_FLAG_OPEN_REPARSE_POINT` does NOT fail-closed like
  Unix `O_NOFOLLOW` — the open succeeds on the reparse point, so an explicit post-open
  `FILE_ATTRIBUTE_REPARSE_POINT` refusal is a mandated code step (U2). (F2) An
  already-read-only file cannot open `GENERIC_WRITE` on Windows (`ERROR_ACCESS_DENIED`);
  handle must use `FILE_READ_ATTRIBUTES|FILE_WRITE_ATTRIBUTES` (U2/U4).
* **P2 (fixed):** explicit `access_mode`/`share_mode` (U2/U5); transient sidecar cleanup
  is path-level link-safe not handle-bound (U3); junction-aware disambiguation via reparse
  attribute (U4); added `## Constitution Check`; pinned `windows-sys = 0.61` no-duplicate (U1).
* **P3 advisories** folded into per-unit acceptance (clippy::pedantic/no-unwrap, scenario
  new-vs-regression labels, refusal-branch observability, final-component-only scope,
  explicit rollback test, U5 delta focus).

## Feasibility assessment (confirmed)

All APIs safe std, unsafe-free, MSRV 1.75: `OpenOptionsExt::custom_flags` (1.10),
`access_mode`/`share_mode` (1.35), `File::set_permissions` (1.16, handle-bound via
fchmod/SetFileInformationByHandle), `File::metadata`. `File` is Send+Sync so retaining
handles in `Arc<EngineReadonlyGuard>` preserves auto-traits. `libc` + `windows-sys 0.61`
already transitive in Cargo.lock; added as platform-gated direct deps (Principle VI).
`#![forbid(unsafe_code)]` preserved.

## Deferred decision (to Ship / implementation, test-first)

Windows retained-handle share/access mode (Option A retained handle vs Option C
identity-verified re-open) — resolved via U10 test evidence (delta d) during implementation, recorded
in the release PR. Not a Stage blocker.

## Changed artifacts

* Created: 059-F, 059.001-T...059.009-T, shipment 051-S.
* Created docs: deliberation + impl-plan (dated 2026-08-24).
* Modified docs: plan file (remediation edits, Constitution Check, Plan Review); deliberation
  (Windows feasibility rows).
* Stash state: 5905CDEE, E86A6E56 → harvested (+ stash_links).

## Correction — duplicate feature 058-F reconciliation (2026-08-24 follow-up)

Post-staging verification found an **abandoned intermediate feature 058-F** left queued
with the exact canonical title of 059-F. Root cause: during triage the feature counter
produced 058-F first (created `06:15:15Z`), then canonical 059-F was created 17s later
(`06:15:32Z`) and received the full DoD/goals sections, the initial harvested task
hierarchy (059.001-T...059.005-T, later expanded by PR review to 059.006-T through
059.009-T), dependencies, semantic links, and shipment 051-S. 058-F was never
completed — no DoD/goals sections, zero tasks, zero dependencies, zero links, and
referenced by no shipment.

**Reconciliation (backlogit-native, non-destructive):**

* Linked `058-F` **duplicate_of** `059-F` (traceability preserved).
* Appended a reconciliation comment to 058-F's log documenting root cause and evidence.
* **Archived** `058-F` (queued → archived; moved to `.backlogit/archive/058-F.md`) — not deleted.
* Verified 051-S remains **queued** and references only the canonical 059 hierarchy
  ([059-F, 059.001-T...059.009-T]); covering_feature = 059-F.
* Synced index (501 items). Exactly one queued feature now carries the canonical title (059-F).

**Untouched:** 050-S, 049-S, all stash entries, source/config, unrelated plans, branches,
commits, PRs, and Ship. No blockers.

## Blockers

None. Shipment 051-S is queued and NOT executed. 050-S and its uncommitted artifacts
preserved. Do not execute the shipment (Ship's responsibility).


## Update - PR #107 U5 test-scenario split (2026-08-25)

Current-head Copilot review flagged that U5 (`059.005-T`) still carried seven test deltas
(a)-(g), violating the fewer-than-four-scenarios task heuristic. U5 was split into three
bounded test tasks and the current-state bullets above were updated to match:

* U5 (`059.005-T`) keeps deltas (a)-(c): sidecar matrix, per-platform fail-closed signal,
  junction refusal (at most three scenarios).
* U10 (`059.010-T`, NEW) owns deltas (d)-(e): Windows retained-handle engine
  non-interference and broader non-name-surrogate reparse regression (at most two).
* U11 (`059.011-T`, NEW) owns deltas (f)-(g): deterministic `should_refuse_reparse`
  predicate, Windows literal-equality, and single-source structural proof (at most three).
* Dependencies: U5 and U10 depend on U3+U4+U6+U9; U11 depends on U2. Nothing depends on
  U5/U10/U11, so the split stays acyclic.

The shipment `051-S` manifest is now 12 items (`059-F` plus `059.001-T` through
`059.011-T`). Authority: the eleven-task U1-U11 DAG in
`docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md` (pass 6 addendum) and
the matching section in
`docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`. The historical
session-narrative sections above (which created `059.001-T` through `059.009-T`) are left
as written; only the live current-state bullets were updated. Principles III/IV remain
NOT-PASSED until U7/U8 PASS and U6/U9 land.
