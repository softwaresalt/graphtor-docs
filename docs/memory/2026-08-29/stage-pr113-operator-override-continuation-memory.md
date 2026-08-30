---
type: session-memory
title: "Stage continuation: PR #113 operator-authorized exceptional review remediation (7-thread correction graph)"
timestamp: "2026-08-30T02:10:00Z"
date: "2026-08-29"
agent: "Stage"
skill: "direct (operator-authorized exceptional continuation)"
operation: "PR #113 Stage review remediation — bounded correction set, 7 threads"
supersedes_halt: "stage-pr113-circuit-breaker-reviewfix-cap-memory.md (halt preserved, not erased)"
feature: "059-F"
shipment: "051-S (active, NOT mutated)"
pr: 113
repo: "softwaresalt/graphtor-docs"
branch: "chore/stage-059-f-redeliberation"
head_before: "f8d0d2202d523026a2b33016cd3dc96a0c7abe98"
status: "remediation applied; awaiting commit/push + thread reply/resolve + CI"
---

## Operator Override (explicit, bounded)

The prior circuit-breaker halt (review-fix cycle cap 3/3;
`stage-pr113-circuit-breaker-reviewfix-cap-memory.md`) was **explicitly and
intentionally overridden** by the operator for this **bounded** correction set
only. Operator instruction: "keep working autonomously until fully finished;
this overrides the prior review-cycle halt for this bounded correction set."
Scope remains Stage-only: planning/docs/backlog edits, **no** source
implementation, **no** Ship actions, **no** shipment close, **no** merge.
`.gitignore` (pre-existing operator change) and `docs/scratch/` preserved
untouched. The halt record is retained as historical evidence, not erased.

## Verified store-open call paths (authoritative, exact source reads)

Enumerated every command that reaches a Cozo store constructor. All three
constructors funnel through the same bare-path `open_sqlite_instance`
(`src/db/store.rs:134,175,256`), so the cozo per-`transact()` re-resolution
redirection reaches read-only reads exactly as it reaches write-mode opens.

| Command | Constructor | Call site | Mode |
|---|---|---|---|
| `serve` (generation) | `open_sqlite` + `open_sqlite_readonly` companion | `src/main.rs:2390,2409` | read/write + read |
| `serve` (read-only) | `open_engine_readonly` | `src/main.rs:2423` | fs-enforced read |
| `sync` | `open_sqlite` via `with_locked_database_store` | `src/main.rs:499`→`612` | read/write |
| `prewarm` | `open_sqlite` via `with_locked_database_store` | `src/main.rs:3780`→`612` | read/write |
| `status` | `open_sqlite_readonly` (`load_status_databases`) | `src/main.rs:2768` | app-level read (engine opens RW-create) |
| query subcommands (`search`, `search-semantic`, `research`, `traverse`, `list-sources`, `get-chunk`, `get-document`) | `open_sqlite_readonly` (`QueryCtx::open_stores`) | `src/main.rs:2978` | app-level read (engine opens RW-create) |

Key nuance: `open_sqlite_readonly` is an **application-level** read-only guard
only; its underlying cozo connection still opens with the engine's hard-coded
read-write-create flags (`src/db/store.rs:150-186`). This confirms the reviewer:
`status`/query read-only opens are exposed to the same redirection.

## Seven-thread correction graph (all one consistency set)

| # | Thread / comment | Surface | Fix |
|---|---|---|---|
| 1 | `PRRT_kwDORiB5E86deAwT` / 3888193657 | `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md` | Rewrote "Operator trust boundary" subsection to enumerate **every** store constructor/call path (serve rw+ro, sync, prewarm, status, all query subcommands); noted all funnel through `open_sqlite_instance`; broadened "not limited to serve/write-mode". |
| 2 | `PRRT_kwDORiB5E86deAwZ` / 3888193668 | deliberation `:330` residual | Residual now applies "while any store-opening command holds the store" (serve/sync/prewarm/status/query), with read branch on `open_sqlite_readonly`. |
| 3 | `PRRT_kwDORiB5E86deAwc` / 3888193674 | deliberation `:381` Control #3 | Control #3 now requires trusted dirs for read-only `open_sqlite_readonly` callers (status + query) alongside serve + write-mode. |
| 4 | `PRRT_kwDORiB5E86deAwm` / 3888193686 | `.backlogit/queue/059.014-T.md` | Sign-off precondition enumerates read-only status/query `open_sqlite_readonly` paths; "MUST NOT limit to serve window **or write-mode**". |
| 5 | `PRRT_kwDORiB5E86deAwu` / 3888193696 | deliberation `:417` | Rewrote so **only** `059.014-T` sign-off is the near-term precondition; `059.013-T` is explicitly a later, non-blocking follow-up, MUST NOT be a prerequisite. |
| 6 | `PRRT_kwDORiB5E86deAwx` / 3888193701 | deliberation `:410` | Corrected "nine" → "eight" (U1/U2/U3/U4/U5/U6/U10/U11). |
| 7 | `PRRT_kwDORiB5E86deAw0` / 3888193709 | `.backlogit/queue/059.001-T.md` | U1 acceptance now declares `cap-primitives` as a **direct** dependency (declaration + duplicate-tree check) because `cap_primitives::fs::open_dir_nofollow` is called directly; not implicit via cap-std. |

Also swept the severity-bounding paragraph (deliberation `:360-371`) and Control
#3 tail (`:399-400`) for the same concepts and broadened them for consistency,
per "make all authority surfaces consistent, not just comment lines". Left line
46 (original pre-rescope contingency "nine downstream tasks") and the exec-plan's
explicitly-superseded "nine-task U1–U9 DAG" review-finding records unchanged
(historical / superseded).

## Preservation confirmations

- Source code: **unchanged** (verified via exact reads only; no edits to `src/`).
- Shipment `051-S`: **not mutated / not closed**.
- `.gitignore`: pre-existing operator change (`.backlogit/checkpoints/`,
  `.backlogit/runtime/`) left **unstaged and untouched**.
- `docs/scratch/`: untracked, **not committed**.
- Prior halt memory: preserved; only a non-destructive forward-pointer added.

## backlogit workflow

- `059.014-T` and `059.001-T` updated via backlogit-supported `update --section`
  (body-preserving; `updated_at` bumped; index refreshed). `backlogit sync`
  rehydrated 517 artifacts. Body-only edits do not emit `hooks_queue.jsonl`
  events (that queue tracks create/status/title/lifecycle deltas), so there were
  no generated hook events to persist for these content edits.

## Next steps

Focused plan review (correctness/security/constitution/scope/template-integrity)
→ commit/push → reply to each of the 7 threads with the fixing commit → resolve
each thread via GraphQL → update PR body readiness to current HEAD → wait CI +
inspect auto review threads → address genuinely valid new comments within ≤3
exceptional cycles (stop on same-error×3). No merge.
