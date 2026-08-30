---
title: "Stage re-deliberation memory — 059-F engine-boundary path after U8 BLOCKED; 049-S decouple"
date: "2026-08-29"
agent: "Stage"
shipment: "051-S (active, blocked — not mutated)"
feature: "059-F"
decision: "docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md"
status: "planning complete; pending operator sign-off (059.014-T) before implementation"
---

## Task

Mandatory post-merge re-deliberation of blocked shipment `051-S` / feature `059-F` after PR #111
(merge commit `72940e92d8fd19638a4cc25a40301a31babdbf1a`) landed only backlog-state + memory
(no production code). U8 (`059.008-T`) returned BLOCKED. Planning only — no source, no Ship, no
merge, no shipment close, no new worktree.

## Chosen decision (Option B + evidence-based 049-S decouple)

Ship the **feasible permission-mutation containment** (leaf via U2 + intermediate-directory via
U6, both proven feasible by U7) that closes the two originally-reported TOCTOUs (`5905CDEE`,
`E86A6E56`). **Accept + document** the residual "cozo re-resolves the db path per `transact()`"
engine-open redirection (proven by U8) behind an **operator sign-off gate** (`059.014-T`), with
compensating controls. Defer full engine-open closure to **upstream cozo / fork (Option A,
`059.013-T`)** as a later, separate, non-blocking follow-up. Reject Option C (alt engine/arch).
**Decouple `049-S` from `051-S`** — the edge is sequencing-only (no code coupling; evidence doc:
"no causal evidence linking H3-A to that ordering", "no schedule gain").

Honest posture: Principles III/IV **PASSED for the permission-mutation threat only**; **NOT-PASSED**
for the engine-open redirection (named accepted residual). Original fail-closed DoD **not** claimed
satisfied.

## Causal / dependency graph

`7BF1961D` (MCP serve OS err 232 bug) → `056-F` → `049-S` (evidence, 8 serve tasks) → *[was]* blocks
`051-S` (active) → contains `059-F` gated by U8 (`059.008-T` BLOCKED = root cause) + `050-S`
(archived). U8's infeasibility is confined to the **engine-open** binding (U9); every
permission-mutation unit (U1/U2/U6/U3/U4/U5/U10/U11) is feasible per U7 PASS.

## Backlog changes enacted (verified via backlogit query + doctor)

- **Decoupled** `049-S → 051-S` (`dep remove 049-S 051-S`). 049-S now has no deps (ready).
- Created `059.013-T` (Option A upstream cozo; parent 059-F; later separate shipment) and
  `059.014-T` (operator sign-off gate; parent 059-F).
- Rewired feasible DAG (dropped U8/U9 gates):
  - U1 `059.001-T`: deps `[059.007-T, 059.014-T]` (dropped 059.008-T; +sign-off gate)
  - U6 `059.006-T`: deps `[059.001-T, 059.002-T, 059.007-T]` (dropped 059.008-T)
  - U3 `059.003-T`, U4 `059.004-T`: `[U2, U6]` (dropped U9)
  - U5 `059.005-T`, U10 `059.010-T`: `[U3, U4, U6]` (dropped U9)
  - U9 `059.009-T`: reclassified deferred/upstream, deps `[059.006-T, 059.013-T]` (dropped 059.008-T)
  - U2 `059.002-T`, U11 `059.011-T`: unchanged
- Amended `059-F` DoD via new `BEGIN:redeliberation-2026-08-29` section + added decision to
  references. **051-S manifest NOT mutated** (`[059-F, 059.007-T, 059.008-T]` unchanged); not closed.
- `059.012-T` (U12): dependency repointed `059.008-T` -> `059.014-T` (review-fix cycle 1, 2026-08-29) so it becomes ready at the feature-unblock gate; still queued for a later separate shipment.
- Traceability comments appended to 049-S, 051-S, 059-F (actor stage).
- Graph verified acyclic; `doctor` flagged only pre-existing `archived_from_self_ref` warnings on
  unrelated archived items (032/038/041/043…), none on touched items.

## 049-S / 7BF1961D transition (to unblock)

The `049-S → 051-S` block was removed explicitly (documented, not silent). None of 049-S's eight
members (`056.001/002/003/019/020/021/022/023-T`) touch `src/db/store.rs`; the store.rs TOCTOU is a
pre-existing latent condition independent of the serve-handshake regression, so unblocking 049-S
does not worsen security posture. `059-F` security work continues on its rescoped shipment (not
abandoned). Under P-001, Ship still selects one in-flight unit; decoupling only removes the hard block.

## Ship-side transition (planned; Stage did not execute)

After `059.014-T` sign-off, Ship (owner of active `051-S`) either re-scopes `051-S`'s manifest to
the feasible task set (`059-F` + U1/U2/U6/U3/U4/U5/U10/U11) or closes `051-S` (feasibility complete;
engine binding infeasible/accepted) and Stage assembles a fresh implementation shipment. U9/U12/
059.013-T remain later separate shipments.

## Plan review (security-sensitive)

Dispatched Security Lens, Scope Boundary, Constitution reviewers. **No P0/P1.** Applied Security
Lens hardening before sign-off:
- F1: corrected residual to state the engine-open redirection is reachable by **leaf or
  intermediate** swap (cozo per-`transact()` reopen) and includes E86A6E56's engine-open-follows-link
  consequence; removed "narrower than a leaf swap" understatement.
- F2/Scope#1: qualified reparse-monitoring control as best-effort/non-race-closing and non-load-bearing.
- F3: called out higher-impact write-mode `open_sqlite` branch in severity bounding + operational guidance.
- F4: cross-referenced the superseded body Constitution Check row to the 2026-08-29 amendment.
- Scope#3: noted 059.013-T should split (upstream-request vs fork-eval) when scheduled.
Constitution review: fully compliant (III/IV honest, VI justified, VII/VIII sign-off-gated, P-009/P-010/P-016 clean).

## Environment / degraded status

**Engram daemon UNAVAILABLE** this pass (failed to reach Ready within 30s on repeated attempts;
~10 stale `engram.exe` processes; `--direct` blocked by workspace lock). Per the agent-engram
fallback rule, unified/graph queries were **not** substituted with broad grep for graph claims —
all dependency/manifest facts came from authoritative backlogit records + exact artifact reads.
Process termination declined in a shared environment. `backlogit.exe` and `engram.exe` live at
`C:\Tools\`.

## Preservation confirmation

- Branch `chore/stage-059-f-redeliberation` created from `origin/main` (`72940e92…`); committed
  `.gitignore` blob verified identical (`ea76354…`) between HEAD and origin/main before switching.
- Operator's dirty `.gitignore` (adds `.backlogit/checkpoints/`, `.backlogit/runtime/`) preserved
  byte-for-byte (sha256 `9B8D4D54…` before and after switch).
- Untracked `docs/scratch/2026-08-29-pip-autoapprove-tdd-check.py` preserved byte-for-byte
  (sha256 `F42D5FC5…`); left unstaged/untracked.

## Next steps

1. Open Stage PR (docs + backlog planning artifacts) with current-head Local Review Readiness;
   do NOT merge.
2. Operator signs off on the accepted engine-open residual (`059.014-T`) before any `059-F`
   implementation begins.
3. Ship executes the `051-S` transition and (post-sign-off) the rescoped build.
