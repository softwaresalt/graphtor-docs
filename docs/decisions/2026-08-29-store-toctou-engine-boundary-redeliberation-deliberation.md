---
title: "Re-deliberation: store.rs TOCTOU engine-boundary path after U8 BLOCKED (059-F / 051-S)"
description: "Post-merge re-deliberation choosing a scoped, honest path for the store.rs read-only permission TOCTOU fix after U8 proved cozo 0.7 re-opens SQLite by bare path, and decoupling the 049-S evidence shipment from the 051-S security-ordering prerequisite"
topic: "Engine-boundary approach for feature 059-F after the U8 (059.008-T) SQLite/Cozo capability-open feasibility gate returned BLOCKED"
depth: "deep"
doc_type: "decision"
source: "shipment:051-S / feature:059-F / task:059.008-T"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md"
  - "docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md"
  - "docs/memory/2026-08-29/ship-051-s-feasibility-blocked-memory.md"
supersedes_sections:
  - "docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md#capability-authority-must-survive-the-sqlite-engine-open"
tags:
  - "security"
  - "toctou"
  - "filesystem"
  - "cozo"
  - "sqlite"
  - "engine-boundary"
  - "residual-risk"
  - "re-deliberation"
---

## Problem Frame

Feature `059-F` set out to close two sibling check-then-use (TOCTOU) symlink/reparse
swap vulnerabilities on the read-only permission-mutation paths in `src/db/store.rs`
(`EngineReadonlyGuard` lock/rollback/`Drop`, stash `5905CDEE`; and
`clear_stale_readonly_lock`, stash `E86A6E56`). During PR #107 review the approved
design was expanded beyond the two originally-reported permission-mutation races to
also require **intermediate-directory containment carried through the actual SQLite/Cozo
engine open** (unit U9, gated by feasibility unit U8 / `059.008-T`).

The feasibility gate U8 has now returned **BLOCKED** with source-verified, compile-checked
evidence (persisted in `059.008-T` and PR #111): cozo 0.7 (`storage-sqlite`) opens SQLite
by a **bare path** at `DbInstance::new`, and — decisively — `SqliteStorage::transact()`
**re-opens the original `PathBuf` by path on every pool-empty transaction for the entire
`DataStore` lifetime**. `cozo::DbInstance::new`'s `path: impl AsRef<Path>` parameter
structurally rejects a capability/handle object (`std::fs::File` fails to compile with
`E0277`). No capability-, handle-, or same-identity-bound engine-open hook is reachable
from safe Rust (`#![forbid(unsafe_code)]`) without forking cozo.

Because U8 is BLOCKED, the approved plan's own contingency keeps `059-F` and its nine
downstream code/test tasks blocked, keeps Constitution Principles III/IV **NOT-PASSED**,
and keeps shipment `051-S` **active and blocked**. `051-S` in turn blocks shipment
`049-S` — the evidence/remedy shipment for the original high-priority bug `7BF1961D`
("graphtor-docs MCP server fails during client initialization … OS error 232"). PR #111
(merge commit `72940e92d8fd19638a4cc25a40301a31babdbf1a`) landed only the backlog-state +
memory traceability record; **no production code changed.**

This re-deliberation must (1) choose a technically-honest path for the `059-F`
security work given the engine-boundary infeasibility, and (2) determine whether the
`049-S → 051-S` dependency is still technically necessary now that `051-S` produced no
production code — rather than assuming it.

### Who Cares and Why

* **Security posture** — the two originally-reported permission-mutation TOCTOUs are
  real (a post-open swap can redirect a `chmod` outside the workspace). They should be
  fixed, honestly, without overclaiming coverage the mechanism cannot deliver.
* **Bug `7BF1961D` (`049-S`)** — a high-priority, operator-reproduced client-init
  regression is queued behind a security-ordering prerequisite that can no longer land
  in its originally-approved form. Every day `049-S` waits, the MCP serve regression
  remains unfixed for the affected client.
* **Constitution integrity (Principles III/IV/VI/VII, P-009/P-016)** — any narrowing of
  a security fix must be explicit, approved, and must not falsely claim the original
  fail-closed Definition of Done was met.

### Constraints (fixed)

* Workspace containment; no path traversal; a linked entry must never redirect a
  permission change outside the workspace.
* `#![forbid(unsafe_code)]`; Rust edition 2021, MSRV `rust-version = 1.75`.
* No false claim of identity binding; no path-based `chmod` fallback.
* Stage does not implement source; no silent dependency removal; no cascade close;
  no mutation of the active `051-S` manifest by Stage.
* Preserve P-016 (single active implementation branch/worktree) and the 2-hour task
  width-isolation rule.

### Success Criteria

* The two originally-reported permission-mutation TOCTOUs are closed by a feasible,
  `unsafe`-free, MSRV-1.75 mechanism.
* Any accepted residual risk is named explicitly, bounded, given compensating controls,
  and gated behind operator sign-off before implementation.
* The `049-S` / `7BF1961D` dependency question is answered from evidence, not assumption.
* `051-S` is not closed, archived, or silently dependency-dropped by Stage; the Ship-side
  transition is planned precisely.

### Out of Scope (Non-Goals)

* Forking or vendoring cozo in this release unit (kept as a tracked, later, separate
  follow-up — Option A below).
* Replacing the cozo/SQLite storage engine or re-architecting the storage boundary
  (Option C below — rejected).
* Any change to the eight-member `049-S` evidence manifest itself, or to shipments
  `045-S`/`047-S`/`048-S`/`050-S`, or the `056-F` remedy families.
* Any change to `src/`, `Cargo.toml`, or `Cargo.lock` in this Stage pass.
* Re-opening U7 (`059.007-T`, PASS) or superseding the queued later-shipment follow-up
  `059.012-T`.

## Research Findings

### Engram availability (degraded)

The engram daemon was **unavailable** for this pass (daemon failed to reach Ready within
30s on repeated attempts; ~10 stale `engram.exe` processes present, `--direct` blocked by
the workspace lock). Per the workspace's agent-engram fallback rule, unified/graph queries
were **not** substituted with broad grep for graph claims. All dependency and manifest
facts below come from **authoritative backlogit records** (`backlogit get` / `backlogit
query` against `.backlogit/backlogit.db`) and exact reads of the decision, plan, task, and
memory artifacts. Process termination was declined in a shared environment.

### U8 evidence (authoritative, from `059.008-T` + PR #111)

* `new_cozo_sqlite(path)` → `sqlite::Connection::open_thread_safe(&path)` — bare path; the
  `options` JSON is never forwarded to `sqlite3_open_v2` flags (matches the existing
  `configure_sqlite_wal` comment: "Cozo's SQLite backend ignores the `options` string").
* **`SqliteStorage::transact()` re-opens by path on every pool-empty transaction**
  (`Connection::open_thread_safe(&self.name)`, `self.name` = original `PathBuf`), for the
  full `DataStore` lifetime. A one-time capability-bound open at construction cannot close
  this gap.
* The underlying `sqlite` crate exposes only `OpenFlags::with_uri()` (`SQLITE_OPEN_URI`),
  which cozo never sets and callers cannot inject; the `/proc/self/fd` trick has no Windows
  equivalent short of `unsafe` Win32 FFI (excluded by `#![forbid(unsafe_code)]`; no vetted
  safe wrapper found).
* **Compiled negative proof** (trybuild, throwaway harness against production cozo 0.7.6):
  `cozo::DbInstance::new("sqlite", file, "")` fails to compile with `E0277` (`File: AsRef<Path>`
  not satisfied). The absence of a capability hook is a **type-system fact**, not an
  unexercised path.
* Conclusion: no safe capability-/identity-bound cozo SQLite open exists without forking
  cozo (out of scope, Principle VI).

### U7 evidence (authoritative, PASS — `059.007-T`)

The **permission-mutation** containment foundation is **feasible** under MSRV 1.75 with safe
APIs: `cap-std 4.0.3` + `cap-primitives 4.0.3` (`open_dir_nofollow` for a genuine no-follow
workspace-root bootstrap relative to the trusted parent), a capability-relative component
walk that refuses absolute paths, `..` escapes, intermediate-directory symlink swaps, and
in-bounds leaf symlinks. Dual-platform, dual-toolchain execution (Windows + Linux, stable +
`+1.75.0`) recorded. This proves the **chmod paths** (U2 leaf primitives + U6 beneath-root
opener) can be fully contained — independent of the engine-open question.

### Key decomposition insight

The engine-open containment (U9/U8) and the permission-mutation containment (U2/U6) are
**separable**. U8's BLOCKED result kills only the engine-open capability binding. It does
**not** affect the feasibility of binding the `chmod` operations — the two originally-reported
vulnerabilities — to no-follow, root-relative, identity-bound handles (proven feasible by U7).

### `049-S → 051-S` dependency analysis (from evidence, not assumption)

* `049-S` members are eight MCP-serve tasks (`056.001/002/003/019/020/021/022/023-T`):
  differential serve probe, out-of-process handshake driver, `cmd_serve` diagnostics, H3-B
  adjudication, MCP byte proxy, probe workspace/config fixtures, probe process identities,
  probe observation/capture. **None touch `src/db/store.rs`** or the permission paths.
* The `2026-08-29-mcp-serve-discover-preinitialize-evidence.md` release-unit routing records
  the edge as a **security-prerequisite ordering** (`050-S → 051-S → 049-S`) and states
  explicitly: reordering would require "bypassing or reordering `051-S`, a queued security
  prerequisite, **with no causal evidence linking H3-A to that ordering**", and that it
  "delivers **no schedule gain**". The `051-S security prerequisite ordering` row of the
  H3-A substitution table is marked "Unchanged" purely as an ordering choice.
* `050-S` is **archived** (done); it does not gate `049-S` independently (`049-S`'s only
  declared dependency is `051-S`).
* **Finding:** the `049-S → 051-S` edge is a **sequencing/priority ordering, not a technical
  or code dependency.** The serve-handshake regression fix does not consume, import, or
  otherwise rely on the store.rs permission fix, and unblocking `049-S` does not worsen the
  security posture of the already-shipped serve path (the store.rs TOCTOU is a pre-existing
  latent condition independent of the handshake regression).

## Causal / Dependency Graph

```text
BUG 7BF1961D (MCP serve init OS error 232, high)  ── harvested ──▶ 056-F (remedy/evidence feature)
        │
        ▼
049-S  (evidence shipment: 056.001/002/003/019/020/021/022/023-T; queued)
        │  depends_on (blocks)              ← SEQUENCING-ONLY ordering edge
        ▼                                     (no code coupling; evidence doc: "no causal
051-S  (active, BLOCKED)                       evidence linking H3-A to that ordering";
        │  manifest = [059-F, 059.007-T(done), 059.008-T(blocked)]   "no schedule gain")
        │  depends_on 050-S (archived/done)
        ▼
059-F  (feature, BLOCKED)  ── DoD requires U7 PASS *and* U8 PASS + U6/U9 land
        ├── 059.007-T  U7  root/API/MSRV capability feasibility ........ DONE (PASS)
        ├── 059.008-T  U8  cozo/SQLite engine-open feasibility .......... BLOCKED  ◀── ROOT CAUSE
        │                    (cozo 0.7 re-opens by bare path per transact(); no safe
        │                     capability hook; type-system-proven; fork = Principle VI cost)
        ├── 059.001-T  U1  adopt proven deps (cap-std/cap-primitives/libc/windows-sys)
        ├── 059.002-T  U2  leaf no-follow + handle-bound permission primitives  ] FEASIBLE
        ├── 059.006-T  U6  beneath-root permission-boundary opener (composes U2)] (per U7)
        ├── 059.003-T  U3  bind EngineReadonlyGuard lock/rollback/Drop          ]
        ├── 059.004-T  U4  bind clear_stale_readonly_lock probe/clear           ]
        ├── 059.005-T  U5  cross-platform sidecar swap matrix (tests)           ]
        ├── 059.010-T  U10 Windows retained-handle non-interference (tests)     ]
        ├── 059.011-T  U11 deterministic reparse predicate (tests)             ]
        ├── 059.009-T  U9  integrate capability-bound ENGINE open ......... INFEASIBLE now
        │                    (depends on U8; deferred to Option A upstream-cozo)
        └── 059.012-T  U12 explicit custom_flags leaf-primitive proof (queued, later shipment)
```

**Isolation of the blocker:** the single infeasibility (U8) is confined to the
**engine-open** binding (U9). Every permission-mutation unit (U1, U2, U6, U3, U4, U5, U10,
U11) is feasible per U7 and is only *transitively* gated because the original plan wired
U1/U6 to wait on U8 so U1 would adopt one consistent dependency set.

## Options Evaluated

### Option A — Upstream/fork/patch cozo for a capability-/identity-bound open

Add or await an upstream cozo API (or maintain a fork) that accepts an identity-bound /
capability open honored throughout the `DataStore` lifetime (including `transact()` reopen).

* **Pros**: Only option that fully closes the engine-open intermediate-directory redirection;
  restores the expanded PR #107 fail-closed bar in full.
* **Cons**: External dependency on an upstream maintainer's timeline (unknown, not
  investigated to acceptance); a maintained fork is a standing supply-chain + maintenance
  burden that Constitution Principle VI weighs against; blocks the *already-feasible*
  permission-mutation fix and the high-priority `7BF1961D` remedy indefinitely.
* **Effort**: high (unbounded upstream). **Fit**: correct end-state, wrong critical-path now.

### Option B — Ship the feasible permission-mutation containment; accept + document the engine-open residual (CHOSEN)

Land the identity-bound, no-follow, root-relative permission-mutation fix for the two
originally-reported vulnerabilities — **leaf primitives (U2) AND intermediate-directory
containment of the `chmod` paths (U6), both proven feasible by U7** — and **explicitly accept
and document** the residual "cozo re-resolves the db path on every `transact()`" engine-open
intermediate-directory redirection as a known, bounded gap with compensating controls. Defer
the engine-open closure to Option A as a separate, later, non-blocking follow-up.

* **Pros**: Ships the real security value now (closes both reported findings, fully contained
  `chmod`, no `chmod` can escape the workspace); feasible today under MSRV 1.75 / no `unsafe`;
  strictly stronger than a pure leaf-only narrowing because U6 also contains the intermediate
  directory for the permission paths; honest — does **not** claim the expanded engine-open bar
  is met; keeps Option A open as tracked future work; unblocks `049-S`/`7BF1961D`.
* **Cons**: Principles III/IV are **PASSED only for the permission-mutation threat**, NOT for
  the engine-open intermediate-directory redirection — this must be stated plainly and signed
  off. Requires an amended DoD, an accepted-residual-risk record, and operator sign-off before
  implementation.
* **Effort**: medium (the already-decomposed feasible U1–U6/U3–U5/U10/U11 tasks, minus U9).
  **Fit**: strongest available honest path.

### Option C — Alternative engine / storage boundary or architecture

Replace or wrap the cozo SQLite backend (different engine, or an in-house VFS/shim) to obtain
a capability-bound open.

* **Pros**: Could obtain engine-boundary containment without an upstream cozo change.
* **Cons**: Disproportionate to a security-hardening scope; a new engine or a custom SQLite
  VFS is a large architecture change with its own correctness, performance, and audit surface;
  a safe VFS injection point is not established and would likely require `unsafe` FFI.
* **Effort**: very high. **Fit**: rejected — violates YAGNI / Single Responsibility for the
  problem at hand.

### Option D — Abandon/defer 059-F and decouple 049-S

Shelve the security feature and remove the `049-S → 051-S` block.

* **Pros**: Immediately unblocks `7BF1961D`.
* **Cons**: Abandoning `059-F` silently drops a real, feasible security fix (the two reported
  findings) — not acceptable. Its **decoupling component**, however, is independently
  justified by the dependency analysis above and is adopted as part of the chosen path.
* **Effort**: low. **Fit**: partial — the decouple is adopted; the abandonment is rejected.

## Trade-off Comparison

| Criterion | A: upstream/fork cozo | B: scoped permission fix + accepted residual (CHOSEN) | C: alt engine/arch | D: abandon + decouple |
|---|---|---|---|---|
| Closes 2 reported permission TOCTOUs | Yes | **Yes** | Yes | No (abandons) |
| Closes engine-open intermediate redirect | Yes | No (accepted residual) | Yes | No |
| Feasible now under MSRV 1.75 / no `unsafe` | No (upstream/fork) | **Yes** | No | n/a |
| Honest III/IV claim | Full pass (eventually) | **Scoped pass, explicit residual** | Full pass | n/a |
| Principle VI (deps/complexity) cost | High (fork) | **Low** | Very high | Low |
| Unblocks 7BF1961D / 049-S | No (blocks longer) | **Yes** | No | Yes |
| Security fix preserved | Yes | **Yes** | Yes | No |
| Execution width / 2h isolation | n/a | **Preserved** | Violated | n/a |

## Decision

**Adopt Option B (scoped permission-mutation containment with an explicit, signed-off
accepted engine-open residual), combined with the evidence-justified decoupling component of
Option D (remove the sequencing-only `049-S → 051-S` edge). Keep Option A (upstream/fork cozo)
as a tracked, later, separate, non-blocking follow-up. Reject Option C.**

Concretely:

1. **Rescope `059-F`** to close the two originally-reported permission-mutation TOCTOUs via
   the feasible units: U1 (adopt `cap-std`/`cap-primitives`/`libc`/`windows-sys`), U2 (leaf
   no-follow + handle-bound permission primitives), U6 (beneath-root permission-boundary
   opener composing U2), U3/U4 (bind the two paths to retained/identity handles), U5/U10/U11
   (tests). This delivers full containment of the `chmod` paths (leaf **and** intermediate
   directory) — no permission mutation can be redirected outside the workspace.

2. **Remove the engine-open binding (U9) from the near-term critical path.** The engine-open
   intermediate-directory redirection becomes an **accepted residual** (see the record below),
   tracked for closure by Option A (a new upstream-cozo item, `059.013-T`), on a later separate
   shipment. Nothing in the near-term feasible path depends on U9.

3. **Honest Principles III/IV posture.** Principles III (Workspace Isolation) and IV (CLI
   Containment) are **PASSED for the permission-mutation threat only** once U2/U3/U4/U5/U6/U10/U11
   land. They remain **NOT-PASSED for the engine-open intermediate-directory redirection**,
   which is the named accepted residual. The original `059-F` fail-closed DoD (which required
   U8 PASS and U6/U9 to land) is **not** claimed as satisfied and is explicitly amended.

4. **Gate on operator sign-off.** Implementation (U1 onward) MUST NOT begin until the operator
   signs off on the accepted-residual-risk record (new gate item `059.014-T`). This makes the
   "explicit, approved" requirement a tracked precondition rather than an implied approval.

5. **Decouple `049-S` from `051-S`** (explicit, documented, non-silent): the edge is
   sequencing-only with no code coupling and no schedule gain; `059-F`'s rescoped security work
   continues independently and is **not** abandoned. `051-S` stays active/blocked and is neither
   closed nor dependency-dropped by Stage.

### Rationale

Option B is the only path that (a) is feasible today under `#![forbid(unsafe_code)]` and MSRV
1.75, (b) actually closes the two reported vulnerabilities with full `chmod`-path containment,
(c) is honest about what the mechanism cannot deliver (the cozo bare-path reopen), (d) keeps the
full-containment end-state alive as tracked upstream work, and (e) stops a high-priority,
unrelated bug (`7BF1961D`) from being held hostage to a security-ordering choice the evidence
record itself says has no causal basis and no schedule benefit.

## Accepted-Residual-Risk Record (REQUIRES OPERATOR SIGN-OFF — `059.014-T`)

* **Residual**: After the permission-mutation paths are fully contained, an attacker who can
  **swap either the db file/sidecar (leaf) or an intermediate parent directory beneath the
  workspace root while any store-opening command holds the store** (`serve`, `sync`, `prewarm`,
  `status`, or any query subcommand) can still cause cozo's `SqliteStorage::transact()`
  bare-path reopen to resolve the database to a different file. Because cozo re-resolves the
  **original `PathBuf` by path on every pool-empty transaction**, this redirection is reachable by
  a **leaf swap as well as an intermediate-directory swap** (read on `open_engine_readonly` for
  the serve read posture and on `open_sqlite_readonly` for `status` and every query subcommand —
  both reach the same bare-path `open_sqlite_instance` constructor; **read/write, including
  external file creation/write, on `open_sqlite`** for `serve` generation, `sync`, and
  `prewarm`). This is the cozo
  bare-path re-resolution gap proven by U8, and it includes the residual **engine-open-follows-link**
  consequence originally noted in reported finding `E86A6E56` ("the write-mode open could proceed
  against the linked file").
* **What IS fully closed** (the permission-mutation cores of both reported findings): the
  `chmod`/read-only capture, apply, acquisition rollback, and `Drop` restore (`5905CDEE`) and the
  write-mode probe/clear (`E86A6E56`) are bound to no-follow, root-relative, identity handles
  (U2 + U6) and **cannot** be redirected outside the workspace. **What remains as the accepted
  residual**: the **engine's own** subsequent path re-resolution — including the "write-mode open
  could proceed against the linked file" consequence of `E86A6E56`. Honest framing: the
  permission-mutation portion of both reported findings is CLOSED; the engine-open-follows-link
  portion (a consequence surfaced with, and broadened by, the PR #107 engine-boundary expansion)
  is the named accepted residual. It is **NOT** accurate to call the residual "narrower than a
  leaf swap" — cozo's per-`transact()` reopen makes it reachable by leaf or intermediate swap alike.
* **Severity bounding (threat model)**: The graphtor-docs operating model is single-developer,
  local-only (`AGENTS.md`, `docs/design-docs/…consumption-first-serve-and-trust-boundary`). The
  residual requires a **local** attacker with write/delete/rename authority on a leaf entry or on
  **any parent directory component up to and including the workspace root — and on the workspace
  root's own parent directory** (the directory that contains the workspace root, which the U7
  no-follow bootstrap ambiently opens and trusts, `.backlogit/archive/059.007-T.md:47`; a
  real-directory replacement of the root is **not** rejected by `open_dir_nofollow`, which only
  refuses symlinks/reparse points, so either that parent namespace must be protected too or the
  opened root's identity must be verified after open), while a store-opening command holds the
  store. It is **not** limited to an active `serve` window, nor to write-mode commands: `cmd_sync`
  and `cmd_prewarm` reach the same `DataStore::open_sqlite` bare-path reopen via
  `with_locked_database_store` (`src/main.rs:603-617`), and `status` plus every query subcommand
  reach the same bare-path `open_sqlite_instance` via `DataStore::open_sqlite_readonly`
  (`src/main.rs:2768`, `2978`), so the store is exposed even when no server is running. Impact
  by branch: `open_engine_readonly` (serve read posture) and `open_sqlite_readonly` (`status` and
  every query subcommand) are bounded to reading a redirected file (information exposure) at the
  `DataStore` boundary; **`open_sqlite` (write-mode, reached by `serve` generation posture,
  `sync`, and `prewarm`) is the higher-impact branch** — a redirected engine open
  could read/write or create an external target, so operational guidance (control #3) MUST cover
  every store open — read-only `status`/query included — not only the read serve path. Note that
  swapping a leaf requires
  authority on its **parent directory**, so the root directory namespace and every parent component
  must be protected, not just the leaf's write bit (for a `root/graph.db` layout the root directory
  itself is the relevant parent). This is a local-only threat that spans every store-opening command
  (`serve`, `sync`, `prewarm`, `status`, and every query subcommand), and it is **not** "narrower
  than a leaf swap".
* **Compensating controls** (to accompany the accepted residual):
  1. **Full permission-path containment** (this fix): no `chmod` can escape the workspace,
     removing the highest-impact (privilege/permission) branch of the original threat. This is the
     primary, load-bearing control.
  2. **Reparse-point detection aid (best-effort, NOT race-closing)**: the existing
     `is_reparse_point` primitive and the `reparse-point-fail-closed-containment` compound learning
     can surface unexpected reparse points beneath the workspace root, but they are **path-based and
     themselves TOCTOU-prone** and therefore CANNOT reliably detect a transient swap inside a
     `transact()` reopen window. This is a best-effort detection aid only; it is **not** load-bearing
     for the acceptance decision, and no serve-time monitoring task is added to this shipment (YAGNI
     for the local-only model). The acceptance rests on controls #1, #3, and #4.
  3. **Operational guidance**: document that every store-opening command — graphtor-docs `serve`,
     every write-mode command that reaches `DataStore::open_sqlite` (`sync` and `prewarm`, via
     `with_locked_database_store`), AND every read-only command that reaches
     `DataStore::open_sqlite_readonly` (`status` and every query subcommand, `src/main.rs:2768`,
     `2978`) — must run against a workspace whose **root directory namespace,
     the workspace root's own parent directory (ambiently opened and trusted by the U7 bootstrap;
     `open_dir_nofollow` does not reject a real-directory replacement of the root), and every parent
     directory component leading to each database**, as well as the leaf entries, are not
     attacker-writable (consistent with the local-only trust boundary). Directory authority
     is load-bearing: replacing a leaf requires write/delete/rename authority on its parent
     directory, so protecting only the leaf's write bit is insufficient, and the requirement is
     **not** limited to an active serve window or to write-mode commands (it equally covers the
     read-only `status`/query opens). Surface this in the serve trust-boundary design doc.
     **Enacted 2026-08-29:** this guidance is now committed in
     `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md` under the
     "Operator trust boundary: workspace directory write access" subsection, and `059.014-T`
     requires its presence as a hard sign-off precondition (see that item's acceptance criteria).
  4. **Tracked closure path**: Option A (`059.013-T`) keeps the full engine-open closure on the
     roadmap; the residual is time-limited by that follow-up, not permanent-by-default.
* **Sign-off gate**: `059.014-T` (operator sign-off against the serve trust-boundary design
  doc). Until it is `done`, `059-F` implementation (U1 onward) does not begin. The control #3
  design-doc guidance is a **committed precondition** of `059.014-T` (recorded in that item's
  acceptance criteria); sign-off cannot close while it is absent. **This record
  does NOT assert that the original Principles III/IV fail-closed DoD passed.**

## Ship-Side Transition (planned precisely; not executed by Stage)

Stage does not mutate the active `051-S` manifest and does not close/claim shipments. The
precise transition for Ship, on its next cycle, is:

1. **Acknowledge U8 terminal evidence**: `059.008-T`'s BLOCKED conclusion is now an accepted,
   decided input (this document). Its feasibility deliverable is complete.
2. **Re-scope `051-S` (owner operation)**: As the owner of the active in-flight release unit,
   Ship re-scopes `051-S`'s manifest to the rescoped feasible implementation task set
   (`059-F` + U1/U2/U6/U3/U4/U5/U10/U11), which the eight returned-blocked tasks already exist
   for, and proceeds through harness → build → review under the amended DoD. **Before proceeding,
   Ship confirms the enacted member-task dependency edits (the U8/U9 edge-drops on
   059.001/003/004/005/006/010-T and the U9 re-point to 059.013-T) are consistent with the
   manifest it re-scopes**, so the "051-S manifest untouched by Stage" assertion and the enacted
   DAG stay coherent. U9 (`059.009-T`)
   and U12 (`059.012-T`) are **excluded** and remain for a later separate shipment. The **only**
   near-term precondition for beginning the rescoped implementation (U1 onward) is the operator
   sign-off gate (`059.014-T`). The Option A item (`059.013-T`) is a later, **non-blocking**
   follow-up on a separate shipment and MUST NOT be treated as a prerequisite for the near-term
   rescope.
   * **Alternative (operator's choice)**: if a fresh shipment ID is preferred, Ship instead
     closes `051-S` recording "feasibility complete; engine-open binding infeasible/accepted as
     residual; full fix superseded by the rescoped plan", and Stage then assembles a new
     implementation shipment so `059-F` has single-shipment membership. Either way, `051-S` is
     resolved by Ship, not by Stage, and only after the sign-off gate.
3. **Do not treat `051-S` as safe to close merely because its feature was blocked** — closure is
   justified only by this decision's rescope, not by the block itself.

## Rejected Alternatives

* **Option C (alt engine/architecture)** — disproportionate; large new audit/perf surface;
  no established safe VFS injection point (likely `unsafe`). Violates YAGNI / Principle VI.
* **Pure leaf-only narrowing** (the memory record's literal "option (b)") — rejected in favor of
  Option B because U7 proved intermediate-directory containment of the `chmod` paths is feasible;
  narrowing to leaf-only would accept a *larger* residual than necessary. Option B accepts only
  the engine-open residual, not the permission-path intermediate residual.
* **Silent `049-S` decouple / `051-S` close / dependency drop** — forbidden by the fixed
  constraints; the decouple here is explicit and evidence-based, and `051-S` resolution is left
  to Ship.
* **Abandoning `059-F`** — rejected; the permission-mutation fix is real and feasible.

## Unresolved Questions

1. Operator sign-off on the accepted engine-open residual (`059.014-T`) — required before
   implementation.
2. Upstream cozo appetite for a capability-/identity-bound open (Option A, `059.013-T`) — not
   yet investigated to acceptance; determines whether closure is via upstream PR or a maintained
   fork, and on what timeline.
3. Ship's choice between re-scoping `051-S` in place vs. closing it and assembling a fresh
   implementation shipment (both planned above; operator/Ship decision).
4. Windows retained-handle vs identity-verified re-open (Option A/C from the *original*
   deliberation, unit-level) — still decided during U3/U10 implementation, unaffected by this
   re-deliberation.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Accepted residual is read as "the whole fix is weakened" | The record scopes the residual to the engine-open path only; the `chmod` paths are fully contained; sign-off gate + amended DoD make the boundary explicit. |
| Decoupling `049-S` reads as dropping the security fix | Decision states `059-F` continues on its rescoped shipment; decouple is documented and reversible; no code coupling exists. |
| `051-S` accidentally closed/dependency-dropped to "unblock" work | Explicitly forbidden; Ship-side transition is the only sanctioned resolution, gated on sign-off. |
| Rescoped tasks still carry stale U8/U9 gating in the graph | Stage rewires U1/U6/U3/U4/U5/U10 dependencies to the feasible path (documented) so the backlog reflects the decision. |
| Option A never happens; residual becomes permanent | Compensating controls stand on their own for the local-only threat model; `059.013-T` tracks closure; residual is re-reviewed at each serve trust-boundary revision. |
