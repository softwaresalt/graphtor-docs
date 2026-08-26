---
title: "Identity-bound no-follow handle for store.rs read-only permission ops (sibling TOCTOU fix)"
description: "Establish a fail-closed, identity-bound no-follow handle design that closes the sibling symlink-swap TOCTOU races in EngineReadonlyGuard and clear_stale_readonly_lock without any path-based chmod fallback"
topic: "Filesystem-security hardening of src/db/store.rs read-only lock/clear permission mutations"
depth: "deep"
doc_type: "decision"
source: "stash:5905CDEE"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md"
stash_ids:
  - "E86A6E56"
  - "5905CDEE"
tags:
  - "security"
  - "toctou"
  - "filesystem"
  - "symlink"
  - "reparse"
  - "no-follow"
  - "windows"
  - "unix"
---

## Problem Frame

`src/db/store.rs` contains two sibling **check-then-use (TOCTOU)** races on the
same mechanism — filesystem read-only permission mutation of a database file and
its `-wal`/`-shm`/`-journal` sidecars — on two *different* code paths. Both were
surfaced during the 970AE45A/5D98DBCC adversarial review and PR #96 review, then
deferred as separate security-mechanism changes (Constitution Principles III/IV).

**Path 1 — `EngineReadonlyGuard::lock` / `Drop` (stash `5905CDEE`, Security
Reviewer P1).** The guard captures each entry's exact original permissions,
marks it filesystem-readonly, and restores the exact permissions on `Drop`
(`src/db/store.rs:472`/`:582`, current lines ~600–701). All permission
operations are **path-based** (`fs::metadata`, `set_readonly` →
`fs::set_permissions`), which follow links. The main db file (index 0) is **not**
re-checked with `is_reparse_point` in `lock`/`Drop` (only sidecars index > 0
are), relying on the caller's earlier `validate_path`. Because the guard is held
for the whole `DataStore` lifetime, an attacker with workspace write access can
replace the db file with a symlink to an external target *after* open; on `Drop`
the guard would `chmod` that external target. The per-sidecar `is_reparse_point`
re-check is *itself* the same TOCTOU: the path can be swapped between the check
and the `set_permissions` call.

**Path 2 — `clear_stale_readonly_lock` (stash `E86A6E56`, Correctness Reviewer
P3).** The write-mode self-heal loop (`src/db/store.rs:734`) no-follow-checks each
candidate with `is_reparse_point(&candidate)` and *then*, on separate later lines,
performs path-based operations that follow the link: `candidate.exists()`,
`fs::metadata(&candidate)`, and `set_readonly(&candidate, false)` →
`fs::set_permissions(path, …)`. An attacker can swap a sidecar for a
symlink/junction to an external target **between** the `is_reparse_point` check and
the `set_permissions` call; the write-mode open path would then clear the
read-only bit on that external target, and the subsequent write-mode open could
proceed against the linked file.

The two are the same-mechanism sibling (RELATES TO). `5905CDEE`'s remedy is scoped
only to the guard `lock`/`Drop` path; `E86A6E56` covers the still-uncovered
write-mode clearing path. The operator approved staging both together as one
carefully reviewed filesystem-security release.

### Constraints

* **`#![forbid(unsafe_code)]`** at crate root (`src/lib.rs`, `src/main.rs`) —
  NON-NEGOTIABLE. No `unsafe` blocks may be introduced.
* **Rust edition 2021, MSRV `rust-version = 1.75`** — no newer std/lang features.
* **Constitution Principle VI (Single Responsibility)** — new dependencies must be
  justified.
* **Constitution Principles III/IV** — all filesystem operations must stay within
  the workspace; a linked entry must never redirect a permission change outside it.
* Fail-closed: if the safe mechanism cannot be established, refuse the operation
  rather than falling back to a path-trust re-resolve.

### Success Criteria

* No permission mutation (`capture`, lock-time apply, acquisition-error rollback,
  `Drop`-time restore, write-mode probe, write-mode clear) is ever driven through a
  re-resolved path that can be swapped after the safety check.
* A post-open symlink/junction swap of *any* guarded entry — including the main db
  (index 0), previously uncovered — cannot redirect a `chmod` outside the workspace.
* Existing exact-permission capture/restore, pre-existing-readonly preservation,
  byte-identical db/sidecars, non-empty `-wal`/`-shm`/`-journal` preservation,
  stale-lock self-heal, and symlinked/dangling-symlinked-sidecar refusal tests
  continue to pass unchanged. The empty-transient-removal expectation changes
  explicitly to fail-closed retention unless U3 proves same-identity deletion.
* Behavior is correct and fail-closed on both Unix and Windows.

### Out of Scope

* Cross-guard / cross-process liveness coordination (the documented F6 best-effort
  limitation of `EngineReadonlyGuard`) — unchanged.
* Any change to shipments 047-S/048-S, 049-S, 050-S, or the `9CEC208C`/`8C2E313D`/
  `013.008-T` scopes.
* The unrelated `is_reparse_point` root-guard call sites (serve/install/uninstall,
  shipment 045-S) — they remain valid; this work only changes the store.rs
  permission-mutation paths.

## Research Findings

### Prior learnings (`docs/compound/`)

* **`reparse-point-fail-closed-containment-2026-07-16.md`** (confidence: high,
  shipment 045-S / PR #90). Establishes `is_reparse_point`
  (`src/path/security.rs:184`) as a *pre-check root guard* using `symlink_metadata`
  (inspects the link entry, never follows). Key insight for this work: that
  primitive is correct for a **root** trust anchor checked once before traversal,
  but it is **insufficient** for a permission mutation that happens later in time —
  exactly the gap both stash entries call out. The remedy here therefore goes
  *beyond* the compound learning: identity binding, not a repeated path pre-check.

### Codebase precedent

* **No-follow open already exists in-repo**: `src/workspace/mcp_config.rs:603`
  uses `std::os::unix::fs::OpenOptionsExt` with an exclusive-create temp-file
  pattern to close a predictable-path TOCTOU, and drives permissions through the
  returned handle. This proves the safe (no-`unsafe`) handle-first pattern is an
  established convention here.
* **`is_reparse_point`** (`src/path/security.rs:184`) — reused as the *fast reject*
  where a link is detected up front, but no longer relied on as the sole guard for
  the mutation.
* **Test infrastructure** already supports cross-platform planted symlinks:
  `try_symlink_file` in `store.rs` tests uses `std::os::unix::fs::symlink` /
  `std::os::windows::fs::symlink_file` and gracefully **skips** when the platform
  refuses unprivileged symlink creation (`if try_symlink_file(...).is_err() {
  return; }`). New tests reuse this exact pattern.
* **Existing regression tests to preserve** (all in `store::tests`):
  `open_engine_readonly_leaves_db_and_sidecars_byte_identical_after_read_cycle`,
  `open_engine_readonly_restores_writability_on_drop`,
  `open_engine_readonly_preserves_a_pre_existing_readonly_db_after_drop`,
  `open_engine_readonly_preserves_exact_unix_mode_after_drop`,
  `open_engine_readonly_removes_an_empty_transient_sidecar_on_drop`,
  `open_engine_readonly_never_removes_a_non_empty_transient_sidecar_on_drop`,
  `open_engine_readonly_never_removes_a_non_empty_shm_sidecar_on_drop`,
  `open_engine_readonly_refuses_a_symlinked_wal_sidecar`,
  `open_sqlite_refuses_a_symlinked_wal_sidecar`,
  `open_sqlite_refuses_a_dangling_symlinked_wal_sidecar`,
  `open_sqlite_clears_a_stale_readonly_lock_left_by_a_crashed_session`.

### Platform / API feasibility (bounded research spike)

| Concern | Finding |
|---|---|
| Unix no-follow open | `std::fs::OpenOptions` + `std::os::unix::fs::OpenOptionsExt::custom_flags(libc::O_NOFOLLOW)`. A symlink final component fails the open with `ELOOP` — the fail-closed signal. **Safe, no `unsafe`.** |
| Unix handle chmod | `std::fs::File::metadata()` (fstat) to capture, `File::set_permissions(Permissions)` (fchmod on the open fd) to apply/restore. Both operate on the identity behind the fd, not a path. **Safe, no `unsafe`; available well before MSRV 1.75.** |
| Windows no-follow open | `std::os::windows::fs::OpenOptionsExt::custom_flags(FILE_FLAG_OPEN_REPARSE_POINT \| FILE_FLAG_BACKUP_SEMANTICS)`. **Important asymmetry with Unix:** `FILE_FLAG_OPEN_REPARSE_POINT` opens the reparse point *itself* and **succeeds** on a symlink/junction — it is NOT an `O_NOFOLLOW`/`ELOOP`-style open failure. Fail-closed on Windows is therefore an **explicit code step, not a byproduct of the open**: after opening, inspect the handle's `std::os::windows::fs::MetadataExt::file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT` and return the refusal error when the reparse bit is set. **Safe, no `unsafe`.** |
| Windows handle access / share mode | The retained handle must use `OpenOptionsExt::access_mode(FILE_READ_ATTRIBUTES \| FILE_WRITE_ATTRIBUTES)` and `share_mode(FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE)` (both stable since 1.35, safe). Attribute-level access is required because (a) `SetFileInformationByHandle(FileBasicInfo)` needs `FILE_WRITE_ATTRIBUTES`, `GetFileInformationByHandle` needs `FILE_READ_ATTRIBUTES`, and (b) an **already read-only** file cannot be opened with `GENERIC_WRITE` (`CreateFile` returns `ERROR_ACCESS_DENIED`), so `clear_stale_readonly_lock` MUST open with attribute access, not data-write access. Plain std `OpenOptions::read/write` request `GENERIC_*` and are insufficient/over-granting. |
| Windows handle chmod | `File::set_permissions` on Windows toggles the read-only attribute via `SetFileInformationByHandle(FileBasicInfo)` on the open handle. **Handle-bound, safe, no `unsafe`.** |
| `#![forbid(unsafe_code)]` | The entire design is expressible through safe std + `OpenOptionsExt::custom_flags` (safe) + integer flag constants. **No `unsafe` required.** |
| Rust 1.75 compatibility | `OpenOptions`, `OpenOptionsExt::custom_flags`, `File::set_permissions`, `File::metadata` all predate 1.75. **Compatible.** |
| Dependency impact | `O_NOFOLLOW` is not exposed by std and is platform-value-specific (fragile to hardcode across Unix variants), so add `libc` as a `[target.'cfg(unix)'.dependencies]` entry for the constant. `FILE_FLAG_OPEN_REPARSE_POINT` (0x0020_0000) / `FILE_FLAG_BACKUP_SEMANTICS` (0x0200_0000) are ABI-stable Win32 constants; add `windows-sys` (feature `Win32_Storage_FileSystem`) as a `[target.'cfg(windows)'.dependencies]` entry rather than hardcoding. **Both crates are already transitive (present in `Cargo.lock`), so this adds negligible build/supply-chain surface (Principle VI satisfied).** |
| Handle lifetime / ownership (KEY RISK) | The guard must retain the `File` handles for the `DataStore` lifetime so a later swap cannot redirect the `Drop` restore. On Unix an open fd never blocks other opens. On **Windows**, a retained handle can conflict with the engine's own open of the db/WAL if opened with data access or a restrictive share mode. Mitigation: open the no-follow handle with **attribute-only access and full share mode** (`FILE_SHARE_READ \| WRITE \| DELETE`) so it can read/write the read-only attribute without blocking Cozo/SQLite data access. If a suitable handle cannot be retained on a platform, **fail closed** (refuse the read-only / write-mode open) rather than restoring via a re-resolved path — per both stash directives. |

## Options Evaluated

### Option A: Identity-bound no-follow retained handle (CHOSEN)

Open each guarded entry (main db index 0 **and** every `-wal`/`-shm`/`-journal`
sidecar) exactly once at lock time with no-follow / no-reparse semantics, retain
the `File` handle, and drive **every** permission operation through that handle:
original-permission capture, lock-time read-only application, acquisition-error
rollback, and `Drop`-time restore for the guard; and the read-only probe + clear
for `clear_stale_readonly_lock`. Prohibit any path-based `chmod` fallback. Remove
the per-sidecar `is_reparse_point` re-check (superseded — a successful no-follow
open already proves the entry is not a followed link; an `ELOOP`/reparse open
failure is the fail-closed refusal). If a handle cannot be obtained/retained,
fail closed.

* **Pros**: Closes both races completely, including the previously-uncovered main
  db (index 0) swap; no residual check→use gap; expressible in safe std; reuses an
  established in-repo pattern; preserves exact-permission restore and non-empty
  sidecar content. Empty-sidecar cleanup changes to fail-closed retention unless
  U3 proves same-identity deletion.
* **Cons**: Windows handle-lifetime interaction with the engine must be validated
  (share/access mode); adds two small platform-gated direct dependencies; the guard
  struct changes shape (`Vec<(PathBuf, Permissions)>` → handle-carrying entries).
* **Effort**: medium. **Fit**: strongest — directly satisfies every success criterion.

### Option B: Repeated path re-check immediately before each `set_permissions`

Re-run `is_reparse_point` (or `symlink_metadata`) immediately before every
`set_permissions`.

* **Pros**: Small diff; no new dependencies.
* **Cons**: **Does not fix the race.** The path can still be swapped between the
  re-check and the `set_permissions` call — the exact TOCTOU both stash entries
  explicitly warn against. Rejected on correctness grounds.
* **Effort**: low. **Fit**: fails the primary success criterion.

### Option C: Identity-verified no-follow re-open at mutation time (no retained handle)

Capture a stable identity (device+inode on Unix; volume serial + file index on
Windows) at lock time; at each later mutation, re-open no-follow, `fstat` the
handle, compare identity, and mutate through that fresh handle only on an exact
match; fail closed on mismatch.

* **Pros**: Avoids holding a handle for the whole lifetime (sidesteps the Windows
  lifetime concern); still identity-bound, never path-trust.
* **Cons**: More moving parts (identity capture + compare on two platforms); a tiny
  race remains between re-open and the identity assertion unless the mutation uses
  the same handle it verified (which it does here, so acceptable); more code and
  more tests than Option A. Retained as the **documented fallback** for any platform
  where Option A's lifetime retention proves infeasible.
* **Effort**: medium-high. **Fit**: acceptable fallback, not the primary.

## Trade-off Comparison

| Criterion | A: Retained handle | B: Path re-check | C: Identity re-open |
|---|---|---|---|
| Closes the TOCTOU | Yes (fully) | **No** | Yes |
| Covers main db (index 0) swap | Yes | Partial | Yes |
| `unsafe`-free | Yes | Yes | Yes |
| MSRV 1.75 | Yes | Yes | Yes |
| New deps | +libc/+windows-sys (already transitive) | none | +libc/+windows-sys |
| Windows lifetime risk | Must validate share/access mode | n/a | Avoided |
| Preserves existing tests | Yes | Yes | Yes |
| Complexity | Medium | Low | Medium-high |

## Decision

Adopt **Option A — identity-bound no-follow retained handle** as the primary
mechanism for both paths, with **Option C** documented as the platform fallback if
handle retention proves infeasible (Windows only). Under no circumstance fall back
to Option B (path re-check) or any path-based `chmod`. If neither an Option A handle
nor an Option C identity-verified handle can be established for an entry, **fail
closed**: refuse the read-only serve (`open_engine_readonly`) or the write-mode open
(`open_sqlite`), matching the existing symlink-refusal behavior and its tests.

Rationale: Option A is the only approach that removes the check→use gap entirely
(binding every mutation to a file identity rather than a re-resolvable path),
covers the previously-uncovered main db swap, is fully expressible under
`#![forbid(unsafe_code)]` and MSRV 1.75, reuses an established in-repo no-follow
pattern, and preserves the exact-permission and sidecar-content invariants the
existing tests protect.

## Rejected Alternatives

* **Option B (path re-check)** — does not close the race; the swap window persists
  between the re-check and the mutating call. Explicitly warned against by both
  stash entries.
* **Hardcoding platform flag constants** instead of `libc`/`windows-sys` — rejected
  for `O_NOFOLLOW` (value differs across Unix variants; fragile). The Windows
  constants are ABI-stable but sourcing them from `windows-sys` keeps the two
  platforms symmetric and self-documenting; both crates are already transitive.

## Unresolved Questions

1. **Windows handle share/access mode** — confirm during implementation (test-first)
   that an attribute-only, full-share retained handle on the main db does not block
   Cozo/SQLite's own open of the db or WAL. If it does, fall back to Option C for
   Windows and record the decision in the plan/PR.
2. **Handle for transient sidecars** — resolved by the PR #107 addendum below. A
   separate emptiness check and name-based unlink cannot prove same identity. Leave
   transient sidecars unless the U3 implementation independently proves an
   identity-bound deletion API.
3. **`clear_stale_readonly_lock` non-existent candidates** — a candidate that does
   not exist must remain a skip (not a failure), but a *dangling symlink* must still
   fail closed (the existing `open_sqlite_refuses_a_dangling_symlinked_wal_sidecar`
   test). A no-follow open of a dangling symlink fails (target absent) → the
   fail-closed branch must distinguish "genuinely absent" (skip) from "present but a
   link / unopenable" (refuse). Resolve with an explicit `symlink_metadata`
   existence probe *before* the no-follow open decision, then bind the mutation to
   the opened handle.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Windows retained handle blocks the engine open | Open attribute-only + full share mode; test-first validation; Option C fallback documented. |
| Introducing `unsafe` via a raw-handle path | Design uses only safe std (`OpenOptionsExt::custom_flags`, `File::set_permissions`, `File::metadata`); CI `#![forbid(unsafe_code)]` enforces this. |
| Regression in exact-permission / sidecar-content behavior | Capture/restore stays exact and non-empty content tests stay unchanged. Empty-sidecar cleanup changes deliberately to fail-closed retention unless same-identity deletion is proven. |
| Dependency addition friction (Principle VI) | Both crates already transitive; add platform-gated with a justification comment; no new supply-chain surface. |
| Dangling-symlink self-heal edge case | Explicit existence-vs-link disambiguation before the no-follow open (Unresolved Question 3). |

## Addendum — PR #107 review (2026-08-25): intermediate-directory threat, U7 feasibility gate, honest Principles III/IV

This decided deliberation is amended (not reopened) to record the PR #107 Copilot
review outcome and the corresponding plan/backlog changes. The original decision
(identity-bound, fail-closed, no-follow, no path-based `chmod` fallback) stands; the
amendments below strengthen it and correct earlier overclaims.

### Intermediate-directory threat (new)

The original design bound permission mutations to a final-component no-follow handle.
Review established that a **final-component** `O_NOFOLLOW`/`OPEN_REPARSE_POINT` does
**not** stop an **intermediate parent-directory swap** performed after `validate_path`
but before the mutation. Full containment requires resolving **every path component**
relative to a retained **workspace-root directory handle**, not just refusing a leaf
link. This is added as unit **U6** (containment-safe beneath-root, directory-handle-
relative opener) which gates the code units U2–U5.

**Threat model (explicit):** an attacker may create/write/swap entries **inside** the
workspace root but **not** its trusted parent. The workspace-root directory handle is
bootstrapped **once** from the trusted parent and **retained for the `DataStore`
lifetime**; every subsequent open walks components relative to that handle.

### `cap-std` is a candidate, not a proven solution — U7 feasibility/evidence gate (new)

Earlier drafts **overclaimed** that `cap-std` already provides the exact
root/intermediate/leaf beneath-root semantics and compiles under Rust 1.75. That is
**not yet proven**. A bounded, test-first feasibility/evidence gate **U7**
(`059.007-T`) is introduced to prove, using **safe APIs only** under MSRV 1.75:

1. Atomic workspace-root directory-handle bootstrap with no-follow/no-reparse
   semantics — Unix safe `OpenOptions` `O_DIRECTORY|O_NOFOLLOW` with read access;
   Windows safe `OpenOptions` directory handle using
   `FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT`, reparse-attribute
   rejection, and explicit sharing/access flags — retained for the `DataStore`
   lifetime.
2. Conversion to/from `cap_std::fs::Dir`/`File` (or whichever safe capability API is
   selected), including the **`cap_std::fs::File` vs `std::fs::File` boundary**
   decision (either `into_std` conversion or capability-file helper signatures) —
   **PENDING U7 PASS**; the boundary is **not decided until U7 records PASS**, at which
   point U7 records the exact APIs so no ambiguity remains for U6/U2.
3. A component walk that prevents escape and **refuses intermediate symlink/junction
   swaps**.
4. A final in-bounds leaf symlink/reparse that is **refused, not merely prevented from
   escaping**.
5. The `cap-std` candidate **compiles with Rust 1.75**.

If any required API/semantics fail, **U7 returns BLOCKED to Stage** and Principles
III/IV remain **not-passed**. No vague `unsafe` and no path-based fallback is accepted;
any hand-rolled `rustix`/Windows fallback would require its own separate evidence gate.
Dependencies: **U7 depends on U1; U6 depends on U7**; U2–U5 remain gated transitively.
U7 is added to the `051-S` shipment manifest.

### Transient sidecar cleanup fails closed on identity ambiguity

The retained workspace-root capability `Dir` contains lookup, but a relative
`symlink_metadata` emptiness probe followed by relative `remove_file` is still a
check/use race at the final name. A writer can replace an observed empty sidecar with a
non-empty live WAL/SHM before unlink. U3 therefore leaves transient sidecars in place unless U3 proves a safe API that
binds the emptiness observation and deletion to the same file identity. A separate
metadata-check + name-based unlink is not accepted.

### Capability authority must survive the SQLite engine open

Protecting permission mutations is insufficient if `DataStore::open_sqlite` later
passes the original `safe_path` to `configure_sqlite_wal` and
`open_sqlite_instance`/`DbInstance::new`. An intermediate directory can be swapped in
that window and redirect the engine outside the retained root. U7 must prove a safe,
MSRV-1.75-compatible capability- or identity-bound SQLite/Cozo open, including engine
sidecar creation. U6 must carry that mechanism through the production open paths. If
the engine only accepts a re-resolved path and no safe binding exists under
`#![forbid(unsafe_code)]`, U7 returns BLOCKED and Principles III/IV remain not-passed.

### Deterministic Windows reparse predicate (clarified)

The broader fail-closed reparse policy is pinned by a **module-private, target-independent
literal bit constant `REPARSE_ATTR` (`0x0000_0400`)** and a pure predicate (`should_refuse_reparse`)
compiled/tested on **Linux CI**; the predicate is the **single source of truth** — the
production Windows refusal branch MUST call it (never an inline reparse-bit mask, and
the numeric literal occurs exactly once, inside `REPARSE_ATTR`, with every other reference
using `REPARSE_ATTR`), plus a Windows-only assertion
that `REPARSE_ATTR` equals windows-sys `FILE_ATTRIBUTE_REPARSE_POINT` and that the
production branch drives the predicate (so it cannot be a decorative unused helper).
The policy is the **intentionally broader any-reparse-point** refusal (refuse ANY
`FILE_ATTRIBUTE_REPARSE_POINT` entry), not merely matching `is_reparse_point` breadth.

### Honest Principles III/IV status

Until **U7 records PASS and U6 lands**, Principles III (Workspace Isolation) and IV
(CLI Containment) are **NOT-PASSED (provisional)**: final-component no-follow alone
leaves the intermediate-directory swap race open. Continuous MSRV evidence (a dedicated
Rust 1.75 CI check, or a proven equivalent repository gate) is required on U1 during
implementation; Stage does not alter the workflow now.

## Final PR #107 correction — bounded U7/U8 feasibility and U6/U9 integration

The preceding addendum is historical where it describes one six-obligation U7 or
one U6 carrying both permission and engine work. Current authority is:

* U7 has no dependency and proves three root/API/MSRV scenarios in an isolated
  harness, including candidate-version discovery before the product manifest changes.
* U8 depends on U7 and proves three SQLite/Cozo engine-boundary scenarios.
* U1 depends on U7 and U8 and adopts only their proven dependency versions.
* U2 depends only on U1, extracts the leaf no-follow/permission primitives in three
  scenarios, and precedes U6 — it does not depend on U6 or U9.
* U6 depends on U1, U2, U7, and U8, composes U2's leaf primitives, and integrates the
  beneath-root opener and permission boundary in three scenarios.
* U9 integrates the actual WAL/SQLite/Cozo engine open in three scenarios.
* U3–U5 and U10 depend on both U6 and U9; U2 precedes U6 (which composes U2's primitives); U11 depends on U2.

Either feasibility gate may return BLOCKED before U1 runs. Principles III/IV remain
NOT-PASSED until U7/U8 PASS and U6/U9 land.


## PR #107 U5 test-scenario split (2026-08-25)

Current-head Copilot review flagged that U5 (`059.005-T`) still carried seven
independently countable test deltas (a)-(g), violating the fewer-than-four-scenarios
task heuristic. The "Final PR #107 correction" section above remains authoritative for
the U7/U8 feasibility and U6/U9 integration structure, but its single U5 test unit is
now split into three bounded test tasks. This section is the current authority for the
test-unit shape.

* U5 (`059.005-T`) keeps at most three cross-platform deltas: (a) the sidecar-type
  matrix fail-closed refusal on both open paths, (b) the per-platform fail-closed signal
  (Unix `ELOOP` versus Windows explicit reparse refusal), and (c) the junction-variant
  refusal on unprivileged CI.
* U10 (`059.010-T`) owns (d) the Windows retained-handle engine non-interference
  assertion and (e) the broader non-name-surrogate reparse regression; at most two
  scenarios.
* U11 (`059.011-T`) owns (f) the deterministic target-independent `should_refuse_reparse`
  predicate unit test, (g) the Windows literal-equality assertion, and the structural
  single-source proof; at most three scenarios.
* Dependencies: U5 and U10 depend on U3, U4, U6, and U9 (identical gating); U11 depends
  on U2 and is therefore transitively feasibility- and integration-gated. Nothing depends
  on U5, U10, or U11, so the split is acyclic.

**Current authority (PR #107 U2-before-U6 rewire, 2026-08-25):** eleven-task U1-U11 DAG
(U7 -> U8 -> U1 -> U2 -> U6 -> U9 -> U3/U4 -> U5/U10, with U11 after U2). U2 depends only
on U1 and precedes U6; U6 composes U2 and depends on U1+U2+U7+U8; U9 depends on U6+U8;
U3/U4 depend on U2+U6+U9; U5 and U10 depend on U3+U4+U6+U9 (U10 owns the Windows
handle-mode / non-interference deltas d-e); U11 depends on U2. Shipment `051-S` contains
`059-F` plus `059.001-T` through `059.011-T` (12 items). Principles III/IV remain
NOT-PASSED until U7/U8 PASS and U6/U9 land. Wherever this deliberation earlier writes the
shorthand `U2-U5`, read it as: U2 is a leaf primitive preceding U6; U3-U5 plus U10 are
gated on U6+U9; U11 is gated after U2.
