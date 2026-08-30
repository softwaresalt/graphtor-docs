---
title: "Identity-bound no-follow handle for store.rs read-only permission ops"
description: "Close the sibling symlink-swap TOCTOU races in EngineReadonlyGuard (lock/rollback/Drop) and clear_stale_readonly_lock by binding every permission mutation to a retained no-follow file handle, fail-closed, with no path-based chmod fallback"
doc_type: "plan"
source: "docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md"
stash_ids:
  - "E86A6E56"
  - "5905CDEE"
tags:
  - "security"
  - "toctou"
  - "filesystem"
  - "no-follow"
  - "windows"
  - "unix"
---

## Problem Frame

Two sibling check-then-use (TOCTOU) races in `src/db/store.rs` allow a workspace
writer to redirect a filesystem read-only permission mutation outside the
workspace by swapping a database file or a `-wal`/`-shm`/`-journal` sidecar for a
symlink/junction **after** the safety check:

1. **`EngineReadonlyGuard::lock` / `Drop`** (current lines ~600–701). Permission
   ops are path-based (`fs::metadata`, `set_readonly` → `fs::set_permissions`) and
   follow links. The main db (index 0) is not re-checked with `is_reparse_point`
   (only sidecars index > 0 are), and the guard is held for the whole `DataStore`
   lifetime, so a post-open swap of the db file to an external symlink causes the
   `Drop` restore to `chmod` the external target. The per-sidecar `is_reparse_point`
   re-check is itself the same TOCTOU (swap between check and `set_permissions`).
   Stash `5905CDEE` (Security Reviewer P1).

2. **`clear_stale_readonly_lock`** (current lines ~734–789). The write-mode
   self-heal loop calls `is_reparse_point(&candidate)` and then, on later lines,
   `candidate.exists()`, `fs::metadata(&candidate)`, and `set_readonly(&candidate,
   false)` — all following links. A swap between the check and `set_permissions`
   clears the read-only bit on an external target, and the write-mode open could
   proceed against the linked file. Stash `E86A6E56` (Correctness Reviewer P3).

The remedy (per the accepted deliberation, Option A) is to open each guarded entry
once with no-follow / no-reparse semantics and drive **every** permission operation
through the retained handle (identity-bound, never path-re-resolved), prohibit any
path-based `chmod` fallback, remove the now-superseded per-sidecar
`is_reparse_point` re-check, and fail closed when a safe handle cannot be
established. See `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`.

## Requirements Trace

| Requirement (source) | Implementation action |
|---|---|
| No permission op driven through a re-resolvable path (`5905CDEE`, `E86A6E56`) | Introduce a no-follow handle helper (U2); route guard capture/apply/rollback/restore (U3) and clear-stale probe/clear (U4) through the handle. |
| Cover the previously-uncovered main db (index 0) swap (`5905CDEE`) | U3 opens index 0 no-follow at lock time and restores via that handle on Drop. |
| Prohibit path-based chmod fallback; remove per-sidecar `is_reparse_point` re-check (`5905CDEE`) | U3/U4 replace path-based `set_readonly`/`fs::metadata` with handle-bound calls; delete the in-loop `is_reparse_point` re-check (a successful no-follow open supersedes it). |
| Fail closed when a no-follow handle cannot be obtained/retained (`5905CDEE`, `E86A6E56`) | U3/U4 return the existing refusal error rather than mutating via a re-resolved path. |
| `#![forbid(unsafe_code)]`, MSRV 1.75 | U1/U2 use only safe std + `OpenOptionsExt::custom_flags` + platform flag constants. |
| Preserve exact-permission and sidecar-content safety | U3/U4 keep capture/restore exact (handle-bound), preserve non-empty sidecars, and replace racy empty-sidecar deletion with fail-closed retention unless same-identity deletion is proven; U5 re-verifies. |
| Test-first swap-resistance + platform behavior | U3/U4 add test-first swap-resistance cases; U5 adds the cross-platform matrix; U10 adds the Windows handle-mode validation. |
| Intermediate-directory (parent) swap containment, not just final component (PR #107 review, 2026-08-25) | U7 proves the root/API/MSRV capability foundation; U8 separately proves the SQLite/Cozo engine boundary; U1 adopts only the proven dependencies; U2 provides the leaf no-follow/permission primitives; U6 composes U2 and integrates the beneath-root permission boundary; U9 integrates the actual engine open. U3–U5 and U10 depend on both U6 and U9; U2 precedes U6; U11 depends on U2. |
| Prove the capability design is achievable with safe APIs under MSRV 1.75 before building on it (PR #107 review, 2026-08-25) | U7 (059.007-T) has three bounded root/API/MSRV scenarios and no dependency on U1. U8 (059.008-T) has three bounded engine-boundary scenarios and depends on U7. Either gate may return BLOCKED before U1 changes the product manifest. |
| Prevent transient-sidecar check/use deletion races (PR #107 review, 2026-08-25) | U3 prohibits separate metadata-check + name-based unlink. U3 must either prove a same-identity deletion API within its bounded implementation or fail closed by leaving transient sidecars in place. |

## Implementation Units

### U7 — Prove capability root/API/MSRV feasibility (test-first; gates U8/U1/U6)

* **Why**: a possibly-failing Rust 1.75 candidate probe must be schedulable before
  the product manifest changes. U7 therefore has **no dependency on U1**.
* **Changes / three scenarios**: in an isolated throwaway harness, test-first:
  1. discover the newest candidate version that compiles under Rust 1.75;
  2. bootstrap and retain the workspace-root handle no-follow/no-reparse, document
     the trusted-parent threat model, and prove the selected `Dir`/`File` boundary;
  3. run a table-driven refusal matrix for absolute/`..` escape, intermediate
     symlink/junction swap, and an in-bounds leaf symlink/reparse.
* **Outcome**: PASS records exact versions, APIs, File boundary, threat model, and
  test output. BLOCKED records the failed scenario and keeps U8/U1/U2/U6/U9/U3–U5/U10/U11
  gated. No product dependency, in-crate `unsafe`, or path fallback is added.
* **Backlog**: `059.007-T`; no dependencies; gates U8.

### U8 — Prove contained SQLite/Cozo engine-open feasibility (test-first)

* **Why**: engine binding is a distinct uncertainty and execution domain from the
  root/API/MSRV proof. It must fit a bounded task before production integration.
* **Changes / three scenarios**: using U7's candidate in an isolated harness:
  1. prove `configure_sqlite_wal` and `open_sqlite_instance`/`DbInstance::new`
     consume a capability- or same-identity-bound mechanism without `safe_path`;
  2. swap an intermediate directory before engine open and prove refusal with no
     external database or WAL/SHM/journal creation or mutation;
  3. compile the harness under Rust 1.75 with safe APIs only, or record BLOCKED
     naming the unavailable engine API.
* **Outcome**: PASS records exact engine APIs/versions/results. BLOCKED keeps
  U1/U2/U6/U9/U3–U5/U10/U11 gated and Principles III/IV NOT-PASSED.
* **Backlog**: `059.008-T`; depends on U7; gates U1 and U6.

### U1 — Add proven platform-gated dependencies (config)

* **Changes**: only after U7 and U8 PASS, add `libc`,
  `windows-sys = { version = "0.61", features = ["Win32_Storage_FileSystem"] }`,
  and the exact `cap-std` version proven by U7/U8. U1 does not discover or test an
  unproven candidate in the product manifest.
* **Tests**: host build/check; `cargo +1.75.0 check --all-targets`; continuous Rust
  1.75 CI coverage; before/after `cargo tree -d` plus inverse trees; `cargo audit`.
* **Outcome**: if either feasibility task is BLOCKED, U1 does not run and no
  `cap-std` edge is retained.
* **Backlog**: `059.001-T`; depends on U7 and U8.

### U2 — Safe no-follow handle helper + handle-bound permission primitives (code, test-first)

* **Changes**: Add internal helpers in `src/db/store.rs` (or a small sibling module):
  * `open_no_follow(path: &Path) -> Result<File, GraphtorError>`:
    * **Unix**: `OpenOptions` with `.read(true)` **explicitly set** and
      `custom_flags(libc::O_NOFOLLOW)`. `O_NOFOLLOW` is only a modifier; an
      access mode is still mandatory, so read access MUST be requested alongside
      the flag or the open fails with `EINVAL` on some libc/kernel combinations.
      Read (attribute/metadata) access is sufficient for `File::metadata()` and
      the handle-bound `File::set_permissions` (fchmod) restore; write access is
      not required and is deliberately not requested (least privilege, and it
      avoids denying an already-read-only file). A symlink final component fails
      the open with `ELOOP`; map to the existing refusal error.
    * **Windows**: `OpenOptions` with
      `custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)`,
      `access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)`, and
      `share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)`. Because
      the open **succeeds** on a reparse point (it does NOT fail like Unix
      `O_NOFOLLOW`), the helper MUST then inspect the handle via
      `MetadataExt::file_attributes()` and refuse through the shared pure predicate
      `should_refuse_reparse` (below) — **returning the existing refusal error when the
      reparse bit is set** — fail-closed is an explicit code step on Windows, not a
      byproduct of the open. Attribute-level
      access (not `GENERIC_WRITE`) is required so an already-read-only file can be
      opened without `ERROR_ACCESS_DENIED`.
      * **Reparse-breadth decision (ADOPTED: broader fail-closed policy).**
        `FILE_ATTRIBUTE_REPARSE_POINT` is set for *any* reparse point, which is
        strictly broader than the name-surrogate class (symlink /
        junction/mount-point) that Unix `O_NOFOLLOW` refuses: it also flags
        non-redirecting reparse points such as OneDrive/cloud placeholders,
        dedup/HSM stubs, WSL interop sockets, and app-execution aliases. This
        plan **deliberately adopts the broader policy — refuse the guarded open
        for ANY entry carrying `FILE_ATTRIBUTE_REPARSE_POINT`** — rather than
        attempting a narrower name-surrogate-only refusal. **Justification:**
        (1) a precise name-surrogate test requires reading the reparse tag and
        evaluating `IsReparseTagNameSurrogate`, which is only obtainable via
        `DeviceIoControl(FSCTL_GET_REPARSE_POINT)` — an `unsafe` FFI call
        precluded by `#![forbid(unsafe_code)]` at MSRV 1.75; and
        `std::fs::FileType::is_symlink()` (which does classify name-surrogates)
        is **path-based** (derived from `symlink_metadata`, not the retained
        handle) and so would re-introduce the very TOCTOU this change closes.
        (2) The guarded targets (main db `+ -wal/-shm/-journal` sidecars) are
        expected to be plain regular files; a reparse point of *any* tag on
        these paths is anomalous, so a blanket refusal is the containment-correct,
        fail-closed choice. The resulting Unix/Windows asymmetry (Unix refuses
        name-surrogates via `O_NOFOLLOW`; Windows refuses the broader reparse
        class) is intentional and documented — Windows is simply stricter. This
        breadth MUST be regression-tested for a legitimate non-redirecting
        reparse file (see U10 delta (e)) and for the junction refusal path (U4).
  * **Deterministic refusal predicate (target-independent, Linux-testable; single
    source of truth)**: factor the reparse-bit refusal into a pure predicate
    `pub(crate) fn should_refuse_reparse(file_attributes: u32) -> bool` that tests a
    **module-private literal bit constant** `const REPARSE_ATTR: u32 = 0x0000_0400`
    (the numeric value of `FILE_ATTRIBUTE_REPARSE_POINT`; **not** `pub`/`pub(crate)`,
    **not** re-exported) — i.e. `file_attributes & REPARSE_ATTR != 0`. `REPARSE_ATTR`
    is referenced **only** inside `should_refuse_reparse`, which is the single source of
    truth for the reparse-bit decision. The predicate and its literal are pure and
    target-independent, so they compile with no `windows-sys`, no filesystem, and no
    privilege — which is what lets **U11** unit-test the predicate deterministically on
    **Linux CI**. The **production Windows refusal branch MUST call
    `should_refuse_reparse(file_attributes())`** and MUST NOT inline a separate
    reparse-bit mask or duplicate the literal. U2's **implementation** acceptance is
    limited to building these properties: (i) the numeric literal `0x0000_0400` appears
    exactly once in the module — inside the `REPARSE_ATTR` constant — with every other
    reference (production branch and any test) using `REPARSE_ATTR`; and (ii) the
    production Windows refusal branch calls `should_refuse_reparse`, so the helper cannot
    be dead code and no inline mask can diverge from it. The **tests** that prove these
    properties belong to **U11 (`059.011-T`)**, not U2, and are not U2 completion
    criteria: (f) the deterministic fabricated-bit predicate unit test on Linux CI;
    (g1) the `#[cfg(windows)]` assertion that
    `REPARSE_ATTR == windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT`
    (referencing `REPARSE_ATTR`, not repeating the numeric literal); and (g2) the
    `#[cfg(windows)]` structural single-source / production-branch proof.
  * `capture_perms(&File) -> Result<fs::Permissions>` via `File::metadata()`.
  * `set_readonly_via_handle(&File, bool) -> Result<()>` via `File::set_permissions`
    (fchmod on Unix; `SetFileInformationByHandle(FileBasicInfo)` on the Windows
    handle). Preserve exact perms (never a coarse writable/readonly boolean that
    would widen Unix mode bits).
  * Every helper returns `Result` (no `.unwrap()`/`.expect()`); all error paths map
    to `GraphtorError` with a traceable message.
  * **Scope note (doc-comment)**: the final-component handle enforces **no-follow /
    no-reparse** on the leaf; **full-path (intermediate-directory) containment is
    provided by U6's beneath-root directory-handle-relative walk**, which COMPOSES
    U2's `open_no_follow` primitive (invoked through the U6 opener, never on a raw
    absolute path). A bare `O_NOFOLLOW`/`OPEN_REPARSE_POINT` guards only the final
    component, so U2 alone does **not** close an intermediate parent-directory swap
    after `validate_path`; U6 closes it via a safe capability API (`cap-std`) rather
    than an in-crate `unsafe` `openat`/`O_PATH`. **U2 depends only on U1 and is
    scheduled BEFORE U6 — it does NOT depend on U6 or U9.** U2 is complete as the
    leaf-primitive unit once its own three helper scenarios pass; complete
    intermediate-directory containment is achieved downstream by U6 (which composes
    U2) and U9 (which follows U6). The `cap_std::fs::File` vs `std::fs::File`
    boundary through which `open_no_follow` returns its handle is **PENDING U7 PASS**
    (U7 records the exact APIs — `into_std` conversion or a capability-file helper
    signature); it is **not decided here**. U2 carries in whichever boundary U7's
    recorded evidence selects.
* **Files**: `src/db/store.rs` (+ colocated `#[cfg(test)]`).
* **Tests (test-first, ≤3 NEW scenarios)**: (a) opens a real regular file and
  round-trips exact permissions through the handle; (b) refuses a symlinked/junction
  path — on Unix via `ELOOP` open failure, on Windows via the explicit
  `FILE_ATTRIBUTE_REPARSE_POINT` post-open check — both returning the refusal error;
  (c) `set_readonly_via_handle` toggles then restores the exact original
  `Permissions` (Unix mode bits preserved). Reuse `try_symlink_file` and add a
  Windows junction variant where feasible (junction creation does not require the
  symlink privilege) so the Windows refusal path is not silently skipped.
* **Posture**: test-first. **Domain**: code.
* **Acceptance**: Helpers compile under `#![forbid(unsafe_code)]`; pass
  `clippy::pedantic -D warnings`; contain no `.unwrap()`/`.expect()`; the NEW unit
  tests pass; symlink/junction open fails closed on both platforms.

### U6 — Integrate beneath-root opener and permission boundary (composes U2; gates U9/U3–U5/U10)

* **Changes**: implement the production `open_beneath` boundary proven by U7
  (U8's engine-boundary feasibility recorded **BLOCKED** — superseded gate, the accepted
  engine-open residual; U6 does **not** require the engine binding), after U1 adopts the exact
  dependencies, and U2 provides the leaf no-follow/permission primitives. Resolve
  the main database and sidecars relative to the retained root capability, COMPOSE
  U2's `open_no_follow` / `capture_perms` / `set_readonly_via_handle` primitives
  (do not re-implement or duplicate them), and return identity-bound handles for
  permission operations.
  U6 ends at this opener/permission boundary; it does not integrate the engine.
* **Three test-first scenarios**:
  1. a normal contained database/sidecar opens and round-trips exact permissions
     by composing U2's leaf primitives (not a duplicate implementation);
  2. an intermediate parent swap is refused with no external open or mutation;
  3. a table-driven refusal matrix covers unavailable root authority, absolute/`..`
     escape, and final-component symlink/reparse (delegated to U2's leaf refusal
     primitive) without a path fallback.
* **Acceptance**: integrated `src/db/store.rs` passes Rust 1.75 check, clippy
  pedantic, and targeted tests; no in-crate `unsafe` or unwrap/expect.
* **Backlog**: `059.006-T`; depends on U1, U2, and U7 (U8 dropped — **superseded 2026-08-29**, accepted residual); composes U2; gates U3–U5/U10 (U9 deferred to a later separate shipment). See the re-deliberation replan for the authoritative deps.

### U9 — Integrate capability-bound SQLite/Cozo engine open (code, test-first)

* **Changes**: **DEFERRED (2026-08-29 re-deliberation, later separate shipment).** U8 did
  **not** prove a safe engine boundary — it recorded BLOCKED (cozo 0.7 bare-path reopen). Once
  `059.013-T` (upstream cozo / maintained-fork) delivers a safe capability-/identity-bound engine
  open, implement it through
  `configure_sqlite_wal` and `open_sqlite_instance`/`DbInstance::new`. Never
  convert back to the original `safe_path` for an unbound engine open.
* **Three test-first scenarios**:
  1. a normal database opens and creates any WAL/SHM/journal only beneath root;
  2. an intermediate swap after permission preparation but before engine open is
     refused with no external database or sidecar creation/mutation;
  3. `open_sqlite`, `open_sqlite_readonly`, and `open_engine_readonly` preserve
     expected behavior through the proven bound mechanism.
* **Acceptance**: Rust 1.75, clippy pedantic, and targeted tests pass; no in-crate
  `unsafe`, original-path engine reopen, or success-shaped fallback.
* **Backlog**: `059.009-T`; **DEFERRED** — depends on `059.013-T` and U6 (U8 dropped as a gate; **superseded 2026-08-29**); U3–U5 and U10 NO LONGER depend on U9; U2 precedes U6 and U11 depends on U2.

### U3 — Bind EngineReadonlyGuard lock/rollback/Drop to retained handles (code, test-first)

* **Changes**: Change `EngineReadonlyGuard` to retain the opened `File` for each
  guarded entry (e.g. `guarded: Vec<GuardedEntry { file: File, path: PathBuf,
  original: fs::Permissions }>`). `lock` opens **every existing entry including index
  0** via `open_no_follow`, captures original perms via the handle, applies read-only
  via the handle, and on any mid-loop error rolls back already-locked entries through
  their retained handles. `Drop` restores exact perms via each retained handle. Delete
  the in-loop `is_reparse_point` re-check (superseded — a successful `open_no_follow`,
  which includes the Windows reparse-attribute refusal from U2, already proves the
  entry is not a followed link). Each fail-closed refusal returns a structured,
  traceable `GraphtorError` (Principle V), never a silent early return.
  * **Transient sidecars** (`-wal`/`-shm`/`-journal` the engine — not the guard —
    creates during the read cycle) have no retained identity-bound handle at `Drop`.
    A directory-relative `symlink_metadata` probe followed by `remove_file` is still a
    check/use race because the final name can be replaced between calls. U3 therefore
    leaves transient sidecars in place unless U3 proves a safe API that binds the
    emptiness observation and deletion to the same file identity. No separate
    metadata-check + name-based unlink is accepted.
* **Files**: `src/db/store.rs`.
* **Tests (test-first)** — 2 NEW scenarios plus regression re-verification of the
  existing suite (the existing tests are re-run, not re-authored): NEW (a)
  `open_engine_readonly_ignores_a_post_lock_symlink_swap_of_the_main_db_on_drop`:
  after lock, replace the db file (index 0) with a symlink to an external writable
  target; on Drop the external target's permissions are unchanged and the guard drops
  without panic/leak; NEW (b) rollback case where index 0 is locked and a later
  sidecar handle open fails, asserting index 0 is restored to its exact original perms
  via its retained handle with no double-restore. Regression (must still pass
  unchanged): `..._refuses_a_symlinked_wal_sidecar`, `..._preserves_exact_unix_mode_
  after_drop`, `..._preserves_a_pre_existing_readonly_db_after_drop`, `..._leaves_db_
  and_sidecars_byte_identical_after_read_cycle`, `..._removes_an_empty_transient_
  sidecar_on_drop` (updated to expect fail-closed retention when same-identity deletion
  is unavailable), `..._never_removes_a_non_empty_transient_sidecar_on_drop`,
  `..._never_removes_a_non_empty_shm_sidecar_on_drop`.
  NEW (c) replacement race: swap an observed empty sidecar name to non-empty live
  content before deletion would occur; the replacement remains byte-identical and is
  never unlinked.
* **Posture**: test-first. **Domain**: code (incl. doc-comment updates on the guard).
* **Acceptance**: NEW main-db swap-resistance and rollback tests pass; all listed
  existing guard tests pass unchanged; no path-based `set_permissions`/`fs::metadata`
  remains in `lock`/`Drop` for guarded (index 0..N) entries; passes `clippy::pedantic
  -D warnings`; no `.unwrap()`/`.expect()`.

### U4 — Bind clear_stale_readonly_lock probe/clear to no-follow handles (code, test-first)

* **Changes**: Rewrite `clear_stale_readonly_lock` to, for each candidate:
  disambiguate genuinely-absent from present-but-link using `symlink_metadata`
  (`NotFound` → **skip**; present) — then decide fail-closed vs open. A present entry
  whose link/reparse status is set (keyed off the reparse attribute, i.e. the
  **adopted broader fail-closed policy from U2** — any `FILE_ATTRIBUTE_REPARSE_POINT`
  entry is refused, catching Windows **junctions** and non-name-surrogate reparse
  points, not just Unix symlinks) is **refused** (fail closed), preserving
  `open_sqlite_refuses_a_dangling_symlinked_wal_sidecar`. Otherwise `open_no_follow`
  the candidate, probe read-only via the handle, and clear via the handle. Remove the
  standalone `is_reparse_point` + path-based `exists()`/`metadata`/`set_readonly`
  sequence. Fail closed on link/reparse or unobtainable handle with a traceable error.
* **Files**: `src/db/store.rs`.
* **Tests (test-first)** — 2 NEW scenarios plus regression re-verification: NEW (a)
  `open_sqlite_refuses_to_clear_readonly_through_a_swapped_sidecar_symlink`
  (fail-closed-at-open: a sidecar planted as a link to an external read-only target is
  refused and the external target is never made writable AND the open is refused with
  an error — assert the refusal, not merely the unchanged bit); NEW (b) clearing
  read-only on an **already read-only regular file** succeeds through the
  attribute-access handle (guards the Windows `ERROR_ACCESS_DENIED` pitfall).
  Regression (must still pass unchanged):
  `open_sqlite_refuses_a_dangling_symlinked_wal_sidecar`,
  `open_sqlite_clears_a_stale_readonly_lock_left_by_a_crashed_session`, and a
  genuinely-absent sidecar is skipped (no error).
* **Posture**: test-first. **Domain**: code (incl. doc-comment updates).
* **Acceptance**: NEW swap-refusal and already-readonly-clear tests pass;
  dangling-symlink/junction refusal and stale-lock self-heal preserved; no path-based
  `set_readonly` remains in the function; passes `clippy::pedantic -D warnings`; no
  `.unwrap()`/`.expect()`.

### U5 - Cross-platform sidecar swap-resistance matrix, per-platform fail-closed signal, and junction refusal (tests)

* **Changes**: Add colocated tests covering only the **deltas** U3/U4 do not already
  assert (reference, do not re-implement, the U3/U4 cases): (a) the full sidecar-type
  matrix (`-wal`/`-shm`/`-journal`) fail-closed refusal on both open paths; (b) the
  platform fail-closed signal is asserted correctly per platform (Unix `O_NOFOLLOW`
  produces an `ELOOP` open failure; Windows reparse open produces an explicit
  `FILE_ATTRIBUTE_REPARSE_POINT` refusal); (c) reuse the `try_symlink_file`
  unprivileged-skip pattern and add a Windows junction variant so the Windows refusal
  path executes on unprivileged CI where possible. The Windows retained-handle
  engine-non-interference delta and the broader non-name-surrogate reparse regression
  move to **U10** (`059.010-T`); the deterministic `should_refuse_reparse` predicate
  unit test and the Windows literal-equality / single-source structural proof move to
  **U11** (`059.011-T`). This keeps U5 bounded to at most three independently countable
  scenarios.
* **Files**: `src/db/store.rs` tests.
* **Tests**: the (a)/(b)/(c) delta matrix above (skips gracefully when the platform
  refuses unprivileged symlink/reparse creation; the junction variant runs where
  symlink creation is denied).
* **Posture**: test-first / characterization. **Domain**: tests.
* **Acceptance**: Suite passes on the host platform; the full sidecar-type matrix
  fail-closed refusal holds on both open paths; the per-platform fail-closed signal is
  asserted correctly (Unix `ELOOP`, Windows explicit reparse refusal); the junction
  variant covers the Windows name-surrogate refusal where symlink creation is denied;
  filesystem deltas skip (not fail) where symlink or reparse creation is unprivileged,
  with executed-vs-skipped reported; the intermediate-directory swap delta from U6 is
  referenced (not re-implemented). The Windows retained-handle non-interference and
  broader-policy deltas are validated in U10; the deterministic predicate and
  literal-equality / single-source structural proof are validated in U11.
* **Backlog**: `059.005-T`; depends on U3, U4, and U6 (U9 dropped — **superseded 2026-08-29**; engine-open containment is the deferred accepted residual).

### U10 - Windows retained-handle engine non-interference and broader non-name-surrogate reparse regression (tests)

* **Changes**: Add colocated tests for the two Windows breadth deltas split out of U5:
  (d) validate the Windows retained attribute-access handle
  (`FILE_READ_ATTRIBUTES|FILE_WRITE_ATTRIBUTES`, full share mode) does **not** block
  the engine's own db/WAL open, scoped to that single assertion (Option A versus the
  documented Option C fallback decision - record the outcome in the PR); (e)
  **broader-fail-closed-policy regression (Windows):** create a legitimate
  **non-redirecting** reparse file whose reparse tag is NOT a name surrogate (for
  example, a non-name-surrogate reparse point creatable without the symlink privilege
  where feasible) and assert the guarded `open_no_follow` path **refuses it**
  (consistent with the adopted broader policy in U2), so the intentional Windows/Unix
  breadth asymmetry is pinned by a test rather than left implicit. If a non-redirecting
  reparse file cannot be created on unprivileged CI, the test skips (not fails) and the
  doc-comment's accepted-breadth statement stands as the recorded behavior.
* **Files**: `src/db/store.rs` tests (and/or a single narrowly-scoped `tests/`
  integration file only if a full engine open is required for assertion (d)).
* **Tests**: the (d)/(e) deltas above; the (d) non-interference assertion executes
  where a full engine open is available; the (e) reparse regression skips gracefully
  when unprivileged CI cannot create a non-redirecting reparse file.
* **Posture**: test-first / characterization. **Domain**: tests.
* **Acceptance**: The Windows retained-handle non-interference assertion (d) passes and
  the handle-mode decision (Option A versus C) is recorded in the PR; the broader
  non-name-surrogate reparse regression (e) refuses the non-redirecting reparse file
  where creatable and otherwise skips (not fails); whether each filesystem-dependent
  Windows test executed or skipped is reported; passes `clippy::pedantic -D warnings`;
  no `.unwrap()`/`.expect()`. At most two independently countable scenarios.
* **Backlog**: `059.010-T`; depends on U3, U4, and U6 (U9 dropped — **superseded 2026-08-29**; same gating as U5).

### U11 - Deterministic reparse predicate, Windows literal-equality, and single-source structural proof (tests)

* **Changes**: Add the target-independent predicate coverage split out of U5:
  (f) **deterministic normal-CI predicate unit test (always executes on Linux CI):**
  test the pure, target-independent safe predicate
  `pub(crate) fn should_refuse_reparse(file_attributes: u32) -> bool` (returning
  `file_attributes & REPARSE_ATTR != 0` against a **module-private** literal bit
  constant `const REPARSE_ATTR: u32 = 0x0000_0400`, the numeric value of
  `FILE_ATTRIBUTE_REPARSE_POINT`) with **fabricated attribute-bit inputs** built from
  `REPARSE_ATTR` (`REPARSE_ATTR` set produces refuse; clear produces allow; combined
  with unrelated fabricated bits standing in for `FILE_ATTRIBUTE_READONLY`/`FILE_ATTRIBUTE_HIDDEN`
  still refuses).
  Because the predicate and its literal are target-independent, this test **compiles
  and runs on Linux CI** with no filesystem, no privilege, no reparse fixture, and no
  `windows-sys`. (g) **Windows-only equality assertion (`#[cfg(windows)]`):**
  assert `REPARSE_ATTR == windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT`
  (via `const_assert` or a `#[cfg(windows)]` `#[test]`), referencing `REPARSE_ATTR` so the
  assertion never repeats the numeric literal and the cross-platform constant can never
  drift from the real Win32 constant, **and structurally prove the
  production Windows refusal branch calls `should_refuse_reparse`** - a `#[cfg(windows)]`
  test drives the production branch through the predicate so it cannot be a decorative
  unused helper, and the module contains exactly one occurrence of the numeric literal
  `0x0000_0400` (inside the `REPARSE_ATTR` constant), with production and tests
  referencing `REPARSE_ATTR`, so no duplicated inline mask can exist.
* **Files**: `src/db/store.rs` tests.
* **Tests**: the (f) deterministic predicate unit test (fabricated-bit inputs, always
  on Linux CI) and the (g) `#[cfg(windows)]` literal-equality plus single-source
  structural assertions.
* **Posture**: test-first. **Domain**: tests.
* **Acceptance**: The deterministic `should_refuse_reparse` predicate unit test (f)
  executes on every normal-CI job (no filesystem/privilege) and proves the broader
  fail-closed policy; the `#[cfg(windows)]` assertion (g) proves the production Windows
  refusal branch calls `should_refuse_reparse` (single source of truth) with the
  `REPARSE_ATTR` literal defined module-private and occurring exactly once, so there is
  no decorative unused helper and no duplicated inline mask; passes
  `clippy::pedantic -D warnings`; no `.unwrap()`/`.expect()`. At most three
  independently countable scenarios (fabricated-bit predicate, Windows literal
  equality, structural single-source proof).
* **Backlog**: `059.011-T`; depends on U2 (transitively feasibility-gated via U2 -> U1).
## Dependency Graph

> **SUPERSEDED (2026-08-29 re-deliberation, Option B).** The authoritative near-term
> graph is the *Rescoped authoritative DAG* in the "AUTHORITATIVE re-deliberation
> replan - 2026-08-29" section at the end of this plan. U8 is terminally BLOCKED
> (accepted engine-open residual, not a gate) and U9 is deferred to a later separate
> shipment; U1 and U6 no longer depend on U8, and U3/U4/U5/U10 no longer depend on U9.
> Implementation is gated on the operator sign-off `059.014-T`. The graph and prose
> below are retained for historical context only.

```text
U7 root/API/MSRV proof
  --> U8 engine-boundary proof
      --> U1 adopt proven dependencies
          --> U2 leaf no-follow/permission primitives
              --> U6 integrate contained opener/permissions (composes U2)
              |   --> U9 integrate contained engine open
              |       --> U3 guard lock/Drop --+
              |       --> U4 clear-stale ------+--> U5 sidecar matrix (deltas a-c)
              |                                +--> U10 Windows breadth (deltas d-e)
              --> U11 predicate + literal + single-source proof (deltas f-g)
```

* U7 has no dependency and may return BLOCKED before the product manifest changes.
* U8 depends on U7 and may independently return BLOCKED on the engine API.
* U1 depends on U7 and U8 and adopts only their proven versions.
* U2 depends only on U1 and precedes U6. U6 depends on U1, U2, U7, and U8 and
  composes U2's primitives; U9 depends on U6 and U8.
* U3/U4 depend on U2, U6, and U9. U5 and U10 depend on U3, U4, U6, and U9.
  U11 depends on U2 (transitively feasibility-gated via U2 -> U1). Nothing
  depends on U5, U10, or U11; all three are terminal test units, so the rewire
  adds no cycles.
* Every production unit is gated on both feasibility results and on the split
  permission plus engine integrations.
* No cycles.

## Decisions and Rationale

* **Retained handle over re-check (Option A)** — only a handle bound to the file
  identity removes the check→use gap; a repeated path re-check (Option B) still races.
* **Beneath-root walk via a safe capability API (candidate: `cap-std`) for
  intermediate-directory containment (U6, gated on U7/U8)** — a final-component
  `O_NOFOLLOW`/`OPEN_REPARSE_POINT` leaves an intermediate parent-dir swap after
  `validate_path` unguarded. Resolving every component relative to a retained
  workspace-root directory handle closes it. `cap-std` is the **candidate** *safe*
  capability API for this (Unix `openat`/`openat2` over the already-transitive
  `rustix`; Windows handle-relative `NtCreateFile` wrapped safely), which would keep
  the crate `#![forbid(unsafe_code)]`. **This is not yet proven**: whether `cap-std`
  (a) compiles under Rust 1.75, (b) exposes a safe atomic workspace-root
  directory-handle bootstrap with the required no-follow/no-reparse semantics, (c)
  refuses intermediate symlink/junction swaps during the component walk, and (d)
  refuses an in-bounds leaf reparse/symlink is the exact question **U7** proves
  test-first before U1/U6 start. U8 separately proves the actual engine boundary.
  If either records BLOCKED, the documented fallback is a
  hand-rolled *safe* `rustix` `openat` walk (Unix) + a separately-specified safe
  Windows handle-relative design — each requiring its own evidence gate — chosen over
  an in-crate `unsafe` `openat`/`O_PATH` walk or a path-based re-canonicalization
  (which re-introduces TOCTOU). No vague `unsafe` or path-based fallback is accepted.
* **Remove the per-sidecar `is_reparse_point` re-check** — a successful no-follow open
  already proves the entry is not a followed link; keeping the separate re-check would
  re-introduce a redundant, race-prone step. `is_reparse_point` remains valid at its
  other (root-guard) call sites and is unaffected.
* **`libc` + `windows-sys` as platform-gated direct deps** — `O_NOFOLLOW` is not in std
  and is Unix-variant-specific; the Windows flags are sourced symmetrically. Both are
  already transitive, so build/supply-chain impact is negligible (Principle VI).
* **Option C (identity-verified re-open) as documented fallback** — only if Windows
  handle retention conflicts with the engine open; still identity-bound, never
  path-trust.

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Windows retained handle blocks Cozo/SQLite db/WAL open | Open attribute-only + full share mode; U10 validates; Option C fallback documented. |
| Accidental `unsafe` via raw handles | Safe std only; CI `#![forbid(unsafe_code)]` enforces. |
| Regressing exact-permission / sidecar-content behavior | Capture/restore stays exact (handle-bound); non-empty content tests remain unchanged; empty transient cleanup changes deliberately to fail-closed retention unless same-identity deletion is proven. |
| Dangling-symlink self-heal edge case | Explicit absent-vs-link disambiguation before the no-follow open (U4). |
| Dependency friction (Principle VI) | Both crates already transitive; platform-gated with justification comment (U1). |
| `cap-std` MSRV incompatibility or missing beneath-root semantics | U7 proves three bounded root/API/MSRV scenarios before U1 changes the product manifest. |
| Path-only SQLite/Cozo engine API | U8 proves three bounded engine scenarios before U1/U6; BLOCKED leaves III/IV NOT-PASSED. |
| Empty-sidecar cleanup deletes a replacement live sidecar | U3 forbids metadata-check + name-based unlink and leaves transient sidecars unless the same observed file identity can be deleted safely. |
| Permission guarding is safe but the engine reopens `safe_path` | U8 must prove the bound API and U9 must integrate it; otherwise the gate is BLOCKED. |
| `cap-std` adds a new crate family to the graph | Layers on the already-transitive `rustix`; before/after `cargo tree -d` in U1 proves no unexpected duplicate; `cargo audit` clean. |
| U7 or U8 feasibility gate slips or returns BLOCKED | U1/U2/U6/U9/U3–U5/U10/U11 do not start; Principles III/IV remain **NOT-PASSED**. |
| U6 or U9 integration slips | U3–U5 and U10 remain gated; U2 precedes U6 and U11 follows U2; completeness requires U7/U8 PASS plus U6/U9 landed. |

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Safety-First Rust | PASS | Entirely safe std (`OpenOptionsExt::custom_flags`, `File::set_permissions`, `File::metadata`, Windows `access_mode`/`share_mode`); `#![forbid(unsafe_code)]` preserved; all new helpers return `Result` with no `.unwrap()`/`.expect()`; each code unit gates on `clippy::pedantic -D warnings`. |
| II. Test-First Development | PASS | U2/U3/U4 author NEW failing tests before implementation; U5 characterization; all existing tests re-run unchanged. |
| III. Workspace Isolation / IV. CLI Containment | **NOT-PASSED (provisional; gated on U7/U8 PASS + U6/U9)** — **SUPERSEDED 2026-08-29: see "AUTHORITATIVE re-deliberation replan - 2026-08-29" at the end of this plan; III/IV are now scoped-PASS for the permission-mutation threat and NOT-PASSED (accepted residual) for the engine-open redirection, since U8 is terminally BLOCKED and U9 is deferred.** | U7 proves the root/API/MSRV capability foundation, U8 proves the bound engine API, U6 integrates the contained opener/permission boundary, and U9 integrates the actual WAL/SQLite/Cozo open. Until both proofs PASS and both integrations land, III/IV are not complete. Unsupported sidecar cleanup retains the sidecar. |
| V. Structured Observability | PASS | Each fail-closed refusal returns a traceable `GraphtorError`, not a silent early return. |
| VI. Single Responsibility | PASS | `libc`/`windows-sys` are platform-gated, pinned to an already-transitive lock version, justified; `cap-std` (U6) is the **candidate** single safe capability API for beneath-root containment, layered on the already-transitive `rustix`, with its pin/MSRV/semantics **proven by the U7 evidence gate before adoption**; no speculative abstraction (helpers consumed by both paths). |
| VII. Destructive Command Approval | PASS | No destructive actions; permission changes are transient and restored (see Plan Hardening risky-actions table). |
| VIII. Safety Modes | PASS | Careful-mode risk enumeration present in Plan Hardening. |
| IX. Git-Friendly Persistence | N/A | No workspace-state serialization change. |
| X. Context Efficiency | N/A | No agent-facing data-access change. |
| XI. Merge Commit History | N/A | Enforced by Ship at merge time. |

## Plan Hardening Signals (REQUIRED)

* **public API / schema / contract change** — *absent*. All changes are to
  private/`pub(crate)` internals (`EngineReadonlyGuard`, `clear_stale_readonly_lock`,
  new private helpers); no public signature changes.
* **security, auth, permission, or compliance-sensitive behavior** — **PRESENT**. This
  is a filesystem-containment security fix touching permission mutation and symlink
  handling (Constitution III/IV).
* **migration / backfill / destructive or irreversible step** — *absent*. No data or
  schema migration; permission changes are transient and restored.
* **external integration / operator checkpoint / external dependency** — **PRESENT
  (dependency)**. Adds platform-gated direct dependencies (`libc`, `windows-sys`) and
  the cross-platform safe capability crate `cap-std` (U6, layered on already-transitive
  `rustix`) for beneath-root intermediate-directory containment.
* **high runtime, rollout, or rollback risk** — **PRESENT (moderate)**. The guard is on
  the read-only serve hot path and the write-mode self-heal path; a Windows handle-mode
  mistake could block engine opens. Mitigated by fail-closed design and U10 validation.

**Requires plan hardening: yes**

## Runtime Verification and Closure

* **Changed runtime surface**: all persistent SQLite open paths
  (`open_engine_readonly`, `open_sqlite`, and `open_sqlite_readonly`), their WAL/sidecar
  creation, `EngineReadonlyGuard`, and `clear_stale_readonly_lock`. No CLI/API signature
  change.
* **Runtime verification (Ship)**: run the full `cargo test` store suite on the host
  platform; where possible validate on Windows that a real read-only serve + subsequent
  write-mode open succeed (engine open not blocked by the retained handle), that a
  planted symlinked sidecar is refused on both open paths, and that a planted
  **intermediate-directory** symlink/junction (parent-dir swap) is refused through the
  actual engine open, and that replacing an observed empty sidecar with live non-empty
  content never permits its deletion.
* **Operational closure**: no monitoring/rollback infra needed (local, in-process, no
  network); the rollback trigger is a failing store test or a blocked engine open on
  Windows → revert to Option C for Windows. Owner: Ship agent during the release. Record
  the Windows handle-mode outcome (Option A vs C) in the PR description.

## Plan Hardening

**Hardening required: yes.** Triggers: security/permission-sensitive behavior
(Constitution III/IV filesystem containment), a dependency addition, and moderate
runtime/rollback risk on the read-only serve and write-mode self-heal hot paths. This
section was consulted against `docs/compound/best-practices/reparse-point-fail-closed-containment-2026-07-16.md`
(root-guard vs mutation-time distinction), `.github/instructions/constitution.instructions.md`
(Principles III, IV, VI, and `#![forbid(unsafe_code)]`), `.github/instructions/rust.instructions.md`,
and `.github/instructions/strict-safety.instructions.md`.

### Protected invariants (must not regress)

1. **Exact-permission fidelity** — a private `0o600` db returns as `0o600`; a
   pre-existing operator-readonly file stays readonly. Capture/restore stays exact,
   only the mechanism becomes handle-bound.
2. **Sidecar-content safety** — a non-empty `-wal`/`-shm`/`-journal` is never removed;
   empty transient sidecars are also retained unless a safe API binds the emptiness
   observation and deletion to the same file identity.
3. **Byte-identical db/sidecars** after a read cycle.
4. **Fail-closed containment** — no permission mutation or SQLite/Cozo engine open ever
   reaches a target outside the workspace; a linked/reparse entry **or an intermediate
   parent-directory swap** is refused through the actual engine-open boundary.
5. **`#![forbid(unsafe_code)]`** — zero `unsafe`.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

| ProposedAction | change_kind | targets | ActionRisk | rollback | approval | ActionResult |
|---|---|---|---|---|---|---|
| Add `libc` + `windows-sys` as platform-gated direct deps (U1) | config change | `Cargo.toml`, `Cargo.lock` | moderate | revert Cargo.toml/lock; both already transitive | not required (non-destructive, justified) | planned |
| Add the U7/U8-proven `cap-std` version as a direct dep (U1) | config change | `Cargo.toml`, `Cargo.lock` | moderate | revert Cargo.toml/lock; no candidate is adopted before both proofs PASS | not required (non-destructive, MSRV-verified, justified) | planned |
| Resolve guarded entries relative to a retained workspace-root `Dir` handle (U6) | local code change | `src/db/store.rs` | high | revert to final-component-only `open_no_follow`; the intermediate-dir race re-opens (documented regression) | prefer approval if `cap-std` MSRV/Windows behavior forces the fallback design | planned |
| Carry containment through WAL configuration and SQLite/Cozo open (U8/U9) | local code change | `src/db/store.rs`, engine boundary | high | block the release if no safe bound-open mechanism exists | prefer approval if a new engine/VFS integration is required | planned |
| Replace racy empty-sidecar deletion with fail-closed retention (U3) | local behavior change | `src/db/store.rs` cleanup | moderate | restore cleanup only after same-identity deletion is proven | not required (security-preserving, test-guarded) | planned |
| Retain a `File` handle on the main db for the `DataStore` lifetime (U3) | local code change | `src/db/store.rs` | high | Option C identity-verified re-open (Windows) | prefer approval if Windows engine-open conflict is observed | planned |
| Remove the per-sidecar `is_reparse_point` re-check (U3/U4) | local code change | `src/db/store.rs` | moderate | restore the re-check alongside a no-follow open | not required (superseded, test-guarded) | planned |
| Rewrite `clear_stale_readonly_lock` probe/clear to handle-bound (U4) | local code change | `src/db/store.rs` | high | revert to current path-based function | not required (test-guarded, fail-closed) | planned |

None of these are `destructive` (no data/schema deletion; permission changes are
transient and restored). No operator approval gate is mandatory for Stage/harvest;
the only elevated decision — the Windows handle share/access mode (Option A vs C) — is
a **test-first decision made during implementation** and recorded in the PR, not a
destructive action.

### Deepened verification and rollback detail

* **Environment prechecks (Ship)**: confirm `cargo build`/`cargo check --all-targets`
  compile on the host after U1; confirm `#![forbid(unsafe_code)]` still holds (clippy).
* **Target scenarios**: main-db(index 0) post-lock swap on Drop (U3); sidecar
  swap-between-check-and-clear on the write path (U4); **intermediate parent-directory
  swap after guarded preparation refused through WAL configuration and actual engine
  open (U9)**; empty-sidecar observation followed by a live replacement never unlinks
  the replacement (U3); dangling-symlink fail-closed; stale-lock self-heal; exact-mode
  and non-empty-sidecar preservation; Windows retained handle does not block the engine
  db/WAL open (U10).
* **Blocked-path handling**: if the no-follow handle or capability-bound engine open
  cannot be established, refuse the open — never mutate or open the engine through a
  re-resolved path. If same-identity sidecar deletion cannot be established, leave the
  sidecar.
* **Rollback trigger**: any failing store test, a `cargo audit` advisory from U1, a
  Windows engine-open blocked by the retained handle, or a `cap-std` MSRV-1.75 /
  Windows-behavior incompatibility surfaced in U7/U8. **Rollback procedure**: for the
  Windows-open case, switch that platform to Option C (identity-verified re-open) and
  re-run U10; for a `cap-std` MSRV/Windows incompatibility, keep U1 blocked and return
  to Stage to define the documented
  Unix-only `rustix` `openat` walk plus the separate safe Windows design (recording the
  decision in the PR); for a broader failure, revert the guard/clear-stale changes (the
  functions are self-contained) while keeping U1's deps. **Owner**: Ship agent.
  **Validation window**: the Ship runtime-verification + CI pass for the release PR.
* **Unresolved operator decision blocking safe execution**: none. The Windows
  handle-mode choice is resolvable in-implementation via U10 test evidence.

## Plan Review

<!-- plan-review-attempt: 1 -->

**Dispatched personas (5):** Security Lens Reviewer, Correctness Reviewer, Rust
Reviewer, Constitution Reviewer, Scope Boundary Auditor. **Initial gate: FAIL**
(2×P1, several P2). All P1/P2 remediated in this artifact (attempt 1);
**post-remediation gate: PASS**. P3 advisories folded into unit acceptance criteria.

### Findings and disposition

| # | Severity | Persona(s) | Finding | Disposition |
|---|---|---|---|---|
| F1 | P1 | Security Lens, Correctness | Plan assumed Windows `FILE_FLAG_OPEN_REPARSE_POINT` fails closed like Unix `O_NOFOLLOW`. It does not — the open **succeeds on the reparse point**, so the guard would silently operate on the link and SQLite would then follow the path to the external target. | **Fixed (U2).** Windows path now requires an explicit post-open `file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT` refusal as a mandated code step; deliberation Windows-feasibility table corrected. |
| F2 | P1 | Correctness | `clear_stale_readonly_lock` opening an **already read-only** file with write access would fail with `ERROR_ACCESS_DENIED` on Windows, breaking the self-heal. | **Fixed (U2/U4).** Handle uses attribute-level access (`FILE_READ_ATTRIBUTES|FILE_WRITE_ATTRIBUTES`), not `GENERIC_WRITE`; U4 adds an already-read-only-clear test. |
| F3 | P2 | Rust | Windows handle needs explicit `access_mode` + `share_mode`; unspecified share mode risks sharing violations against the engine's own open. | **Fixed (U2/U10).** Full share mode (`READ|WRITE|DELETE`) specified; U10 validates the engine db/WAL open is not blocked. |
| F4 | P2 | Correctness | Transient sidecars are created by the engine, not the guard, so cleanup **cannot** be handle-bound; the plan implied otherwise. | **Fixed (U3).** Cleanup is performed **relative to the retained U6 root `Dir` handle** (directory-handle-relative `symlink_metadata` emptiness + `remove_file` unlinks the link, never the target), so it is contained through the root handle — not a path-based residual (updated further in the PR #107 pass). |
| F5 | P2 | Correctness, Scope | U4 absent-vs-dangling disambiguation keyed on `is_symlink()` would miss Windows **junctions**. | **Fixed (U4).** Disambiguation keys off the reparse attribute under the **intentionally broader any-reparse-point policy** (refuse ANY `FILE_ATTRIBUTE_REPARSE_POINT` entry), catching junctions and non-name-surrogate reparse points. |
| F6 | P2 | Constitution | Governance requires an explicit `## Constitution Check` section. | **Fixed.** Section added mapping Principles I–XI. |
| F7 | P2 | Rust | `windows-sys` version drift could add a duplicate crate copy. | **Fixed (U1).** Pinned `windows-sys = "0.61"` (already transitive at 0.61.2) with the `Win32_Storage_FileSystem` feature; U1 acceptance asserts no new duplicate via `cargo tree -d`. |
| F8 | P3 | Rust, Scope | Add `clippy::pedantic`/no-`unwrap` to per-unit acceptance; label scenarios new-vs-regression (2-hour rule); note observability of refusal branches; state parent-dir-component swap is out of scope; U5 should focus on deltas; add an explicit rollback test. | **Folded in.** Acceptance criteria, scenario labels, scope note (U2), and the explicit rollback test (U3) updated accordingly. |

### Remediation summary

* Deliberation (`docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`):
  Windows no-follow feasibility rows corrected (open-succeeds-on-reparse; attribute
  access + full share mode).
* Plan (this file): U1 pin + feature constants; U2 Windows reparse-attribute refusal +
  attribute access/share mode + final-component scope note; U3 link-safe transient
  cleanup + explicit rollback test; U4 attribute-access already-readonly clear +
  junction-aware disambiguation; U5 delta focus + engine-open validation; added
  `## Constitution Check`; per-unit `clippy::pedantic`/no-`unwrap` acceptance;
  refusal-branch observability.
* **No P0/P1 remain.** Gate: **PASS** (attempt 1). Cleared for harvest.

### Report-only staging-review addendum — 2026-08-25 (Stage plan-review remediation)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists, so this
pass does not re-gate harvest). This Stage staging-review pass strengthened the
plan and the corresponding task acceptance criteria (`059.001-T`, `059.002-T`,
`059.004-T`) to close four scope-ambiguity findings; no source or config was
changed.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| A1 | P2 | Windows `FILE_ATTRIBUTE_REPARSE_POINT` breadth was ambiguous vs the narrower Unix `O_NOFOLLOW` name-surrogate class (symlink/junction). A blanket attribute check also refuses non-redirecting reparse points (OneDrive/dedup/HSM/WSL/app-alias). | **Resolved (U2/U4/U10).** Explicitly **adopted the broader fail-closed policy** — refuse ANY reparse-point entry — and justified it: a precise name-surrogate test needs the reparse tag via `unsafe` `DeviceIoControl(FSCTL_GET_REPARSE_POINT)` (precluded by `#![forbid(unsafe_code)]` at MSRV 1.75), and `FileType::is_symlink()` is path-based (re-introduces TOCTOU). Added U10 delta (e) regression for a legitimate non-redirecting reparse file (refused; skips unprivileged). Intentional Unix/Windows breadth asymmetry documented. |
| A2 | P2 | New direct deps (`libc`, `windows-sys`) were verified only on the host toolchain, not the declared MSRV. | **Resolved (U1).** Acceptance now **requires** explicit pinned-MSRV verification `cargo +1.75.0 check --all-targets` (or `rustup run 1.75.0 …`). |
| A3 | P2 | Unix `open_no_follow` specified only `custom_flags(libc::O_NOFOLLOW)` without an access mode; `O_NOFOLLOW` is a modifier and needs an access mode. | **Resolved (U2).** Unix `OpenOptions` now **explicitly sets `.read(true)`** alongside `O_NOFOLLOW` (least-privilege; sufficient for metadata + fchmod restore; avoids denying an already-read-only file). |
| A4 | P2 | Duplicate-crate impact was asserted with a post-edit-only `cargo tree -d` snapshot. | **Resolved (U1).** Acceptance now **requires a before/after `cargo tree -d` (and `-i windows-sys`/`-i libc`) comparison** proving no new version copy. |

**Findings summary:** P0 = 0, P1 = 0, P2 = 4 (all resolved in-artifact this pass),
P3 = 0. Task acceptance criteria for `059.001-T`/`059.002-T`/`059.004-T` updated
to match. Gate: **PASS** (advisory). No manifest change; shipment `051-S` remains
queued.

### Report-only staging-review addendum — 2026-08-25 (PR #107 review: intermediate-directory containment + deterministic breadth test)

**Gate: ADVISORY** (report-only). This Stage pass remediates two PR #107 Copilot
review findings against this plan. Backlog changes accompany it: a NEW gating task
`059.006-T` (U6) was created under `059-F`, wired so `059.002-T`/`059.003-T`/
`059.004-T`/`059.005-T` each depend on it, and added to shipment `051-S`. No source
or config was changed by Stage.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| B1 | P1 | Final-component `O_NOFOLLOW`/`OPEN_REPARSE_POINT` (U2) does **not** prevent an **intermediate-directory swap** after `validate_path`; the plan claimed Principles III/IV essentially complete while this race remained. | **Resolved (U6, NEW).** Added a directory-identity/containment-safe **beneath-root walk** (selected design: `cap-std` capability `Dir`, safe API atop already-transitive `rustix` on Unix + safe handle-relative `NtCreateFile` on Windows; no in-crate `unsafe`, no path-based `chmod` fallback). U2's `open_no_follow` is invoked through it. Constitution III/IV re-scored **PASS (gated on U6)**; U2–U5 dependency-gated on `059.006-T`. Requirements Trace, Dependency Graph, Decisions, Risks, Plan Hardening (signals + risky-actions), Runtime Verification, and Rollback updated. Platform-asymmetry fallback (Unix-only `rustix` walk + separate safe Windows design) documented for the `cap-std`-MSRV-incompatible case; `cap-std` added to U1 with pinned-MSRV `cargo +1.75.0` and before/after `cargo tree -d` verification. **(Superseded by C1, pass 2 below: this pass's `cap-std`/MSRV assumption was an overclaim; III/IV are now honestly scored `NOT-PASSED (provisional; gated on U7 PASS + U6)` and the U7 feasibility/evidence gate must prove the design test-first before U6 builds on it.)** |
| B2 | P2 | The U5 Windows non-name-surrogate reparse **integration fixture may always skip** on unprivileged CI, leaving the broader fail-closed policy unproven in normal CI. | **Resolved (U11 delta (f), NEW).** Requires a **deterministic normal-CI predicate unit test** — extract the reparse-bit refusal into a pure `should_refuse_reparse(file_attributes: u32) -> bool` and test it with **fabricated attribute-bit inputs** (no filesystem/privilege), so the broader policy is pinned on every CI run; the real reparse-file fixture (delta (e)) remains **optional integration coverage** with explicit executed/skipped reporting. `059.011-T` acceptance updated to match. Safe Rust 1.75 / `#![forbid(unsafe_code)]` preserved. |

**Findings summary:** P0 = 0, P1 = 1 (B1, resolved in-artifact + new gating task),
P2 = 1 (B2, resolved in-artifact). Backlog: `059.006-T` created and gating
`059.002-T`/`059.003-T`/`059.004-T`/`059.005-T`; `051-S` manifest now includes
`059.006-T`; shipment sequencing `050-S → 051-S → 049-S` encoded as `blocks` edges.
Cycle-free (verified). Gate: **PASS** (advisory).

<!-- plan-review-attempt: 3 -->
<!-- plan-review-verdict: PASS -->
* **Superseded by attempt 4 (2026-08-25 PR #107 pass 2, below) — NOT the final
  authority.** The attempt-3 "all four findings remediated / no unresolved P0/P1"
  statement reflects the **first** PR #107 pass only. Attempt 4 reopened this release
  unit: the `cap-std`/MSRV posture was an overclaim (finding C1), Principles III/IV are
  now honestly scored `NOT-PASSED (provisional; gated on U7 PASS + U6)`, and the U7
  feasibility/evidence gate must record PASS before the containment claims hold. Read
  attempt 4 for the current, authoritative status.
* All four PR #107 review findings against these plans are remediated in-artifact
  and backlog; no unresolved P0/P1 remain in this release unit.

### Report-only staging-review addendum — 2026-08-25 (PR #107 pass 2: feasibility-gate honesty + intermediate-dir cleanup + deterministic predicate)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists, so this
pass does not re-gate harvest). This Stage pass remediates a second round of PR #107
Copilot review blockers by removing overclaims and adding a bounded feasibility
gate. Backlog changes accompany it: a NEW gating task `059.007-T` (U7) was created
under `059-F`, wired so it depends on `059.001-T` and gates `059.006-T` (and thus
transitively `059.002-T`–`059.005-T`), and added to shipment `051-S`. No source or
config was changed by Stage.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| C1 | P1 | The plan and U6 **overclaimed** that `cap-std` already provides the exact root/intermediate/leaf beneath-root semantics and Rust-1.75 compatibility; Principles III/IV were scored **PASS (gated on U6)** on that unproven premise. | **Resolved (U7, NEW).** Added a bounded **test-first feasibility/evidence gate** `059.007-T` proving, safe APIs only under MSRV 1.75: (1) atomic workspace-root directory-handle bootstrap (Unix `O_DIRECTORY\|O_NOFOLLOW` read; Windows `FILE_FLAG_BACKUP_SEMANTICS\|FILE_FLAG_OPEN_REPARSE_POINT` + attribute rejection + share/access flags) retained for the `DataStore` lifetime, with an **explicit threat model** (attacker may write inside the workspace root but not its trusted parent); (2) conversion to/from `cap_std::fs::Dir`/`File`; (3) component walk refuses intermediate symlink/junction swaps; (4) in-bounds leaf reparse/symlink refused; (5) compiles under Rust 1.75. If any obligation fails, U7 returns **BLOCKED** to Stage and Principles III/IV remain **NOT-PASSED** — no vague `unsafe` or path-based fallback. U7 depends on U1; U6 depends on U7; U2–U5 gated transitively. Constitution III/IV re-scored **NOT-PASSED (provisional; gated on U7 PASS + U6)**. |
| C2 | P2 | Transient sidecar (`-wal`/`-shm`/`-journal`) cleanup was described as an acceptable **path-based residual**, leaving an intermediate-directory swap able to redirect deletion. | **Resolved (U3).** Cleanup now performs emptiness probe (`symlink_metadata`) and unlink (`remove_file`) **relative to the retained U6 workspace-root `Dir` handle**, so a swap between probe and unlink cannot redirect the deletion; the path-based-residual claim is removed. |
| C3 | P2 | The Windows reparse policy test risked being **target-gated** (needing `windows-sys`), so it could not run deterministically on Linux CI. | **Resolved (U2 implements, U11 tests).** Defined a **module-private target-independent literal bit constant `REPARSE_ATTR = 0x0000_0400`** and a pure predicate `should_refuse_reparse(u32)` compiled/tested on Linux CI; production Windows code MUST call that predicate; the `#[cfg(windows)]` assertion proves `REPARSE_ATTR == FILE_ATTRIBUTE_REPARSE_POINT` (referencing `REPARSE_ATTR`, not repeating the numeric literal). `059.002-T` implements the predicate + production call; `059.011-T` owns the deterministic and `#[cfg(windows)]` tests. |
| C4 | P2 | `cap_std::fs::File` vs `std::fs::File` boundary was left **ambiguous** for the handle-bound permission primitives. | **Resolved (U7).** U7 feasibility must choose and prove either `into_std` conversion or capability-file helper signatures; the boundary is **PENDING U7 PASS** until U7's evidence records the exact APIs, at which point U6/U2 carry it in — no wording implies it is already decided. |
| C5 | P2 | U4 wording said the reparse disambiguation matched `is_reparse_point` **breadth**, understating intent. | **Resolved (U4).** Reworded to the **intentionally broader any-reparse-point policy** (refuse ANY `FILE_ATTRIBUTE_REPARSE_POINT` entry). |
| C6 | P2 | U1 acceptance lacked **continuous** MSRV evidence (a one-shot local check can regress). | **Resolved (U1).** Acceptance now requires a dedicated **Rust 1.75 CI check** added during implementation (or explicit proof of an equivalent repository gate); Stage does not alter the workflow now. |

**Findings summary:** P0 = 0, P1 = 1 (C1, resolved in-artifact + new gating task U7),
P2 = 5 (C2–C6, all resolved in-artifact). Backlog: `059.007-T` (U7) created, depends
on `059.001-T`, gates `059.006-T` (→ transitively U2–U5); `051-S` manifest now
includes `059.007-T`; shipment sequencing `050-S → 051-S → 049-S` preserved as
`blocks` edges. Cycle-free (verified). Honest Constitution status: **III/IV
NOT-PASSED (provisional; gated on U7 PASS + U6)**. Gate: **PASS** (advisory).

<!-- plan-review-attempt: 4 -->
<!-- plan-review-verdict: PASS -->
* PR #107 pass-2 blockers remediated in-artifact and backlog: overclaims removed,
  U7 feasibility/evidence gate added (gates U6→U2–U5), transient cleanup bound to the
  retained root `Dir` handle, deterministic Linux-CI reparse predicate with a
  Windows-only equality assertion, File-vs-std boundary deferred to U7, broader
  any-reparse-point wording, and continuous MSRV CI evidence required in U1.
* Honest posture: Principles III/IV are **NOT-PASSED (provisional)** until U7 records
  PASS and U6 lands; a U7 BLOCKED halts the chain with no unsafe/path-based fallback.

### Report-only staging-review addendum — 2026-08-25 (PR #107 pass 3: final clarity pass — enforceable helper SSOT, integrated-MSRV, pending File boundary)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists, so this pass
does not re-gate harvest). Third/final PR #107 review-fix cycle applying remaining
**P2/P3 clarity** fixes only. No source/config/build/PR actions; no manifest,
dependency, or shipment changes. All four items resolved in-artifact and in the
corresponding tasks.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| D1 | P2 | `should_refuse_reparse` was described as the decision point but nothing **structurally** prevented a decorative unused helper or a duplicated inline reparse-bit mask in the production Windows branch. | **Resolved (U2/U11).** The reparse-bit literal is now a **module-private** `const REPARSE_ATTR` (not `pub`/`pub(crate)`, referenced only inside the predicate); the predicate is the **single source of truth**; the production Windows refusal branch MUST call `should_refuse_reparse(file_attributes())`. Acceptance **structurally requires** the numeric literal occurs exactly once, inside the `REPARSE_ATTR` constant (the predicate references `REPARSE_ATTR`, it does not restate the literal), and a `#[cfg(windows)]` test drives the production branch through the predicate, so the helper cannot be dead code and no inline mask can diverge. `059.002-T`/`059.011-T` updated. |
| D2 | P2 | U6 acceptance leaned only on U7's throwaway feasibility harness for MSRV evidence, so the **integrated** `src/db/store.rs` was never re-verified under Rust 1.75. | **Resolved (U6).** U6 acceptance now **reruns `cargo +1.75.0 check --all-targets` against the actual integrated `src/db/store.rs` `open_beneath` implementation** with the `cap-std` edge; U7 harness evidence is explicitly **necessary but not sufficient**. `059.006-T` updated. |
| D3 | P2 | The `cap_std::fs::File` vs `std::fs::File` boundary was worded as **already decided/inherited** in several places, understating that it is unproven until U7 records evidence. | **Resolved (U7/U6/U2 + deliberation).** The boundary is now marked **`PENDING U7 PASS`** everywhere it is referenced (U7 obligation 5, U7 acceptance, U6 mechanics/acceptance, U2 scope note, plan C4, deliberation); "decided"/"inherited"/"no late ambiguity" wording that implied it was already settled is removed. It is decided and recorded only on U7 PASS. |
| D4 | P3 | Plan-review **attempt 3**'s "all four findings / no unresolved P0/P1" statement could be mistaken for the final authority even though attempt 4 reopened the unit. | **Resolved.** Attempt 3 is now explicitly marked **superseded by attempt 4** (overclaim C1; III/IV `NOT-PASSED (provisional)`), pointing readers to attempt 4 for the authoritative status. |

**Findings summary:** P0 = 0, P1 = 0, P2 = 3 (D1–D3), P3 = 1 (D4) — all resolved
in-artifact this pass. No manifest/dependency/shipment change; `051-S` remains queued
with sequencing `050-S → 051-S → 049-S`. Honest Constitution status unchanged: **III/IV
NOT-PASSED (provisional; gated on U7 PASS + U6)**. Gate: **PASS** (advisory).

<!-- plan-review-attempt: 5 -->
<!-- plan-review-verdict: PASS -->
* **Superseded by attempt 6 (2026-08-25 PR #107 pass 4, below).** Attempt 5 did not
  cover the final-name sidecar replacement race or the path-only SQLite engine reopen.
* PR #107 pass-3 final clarity fixes applied in-artifact and tasks: `should_refuse_reparse`
  made an enforceable single source of truth (module-private literal, no decorative helper /
  no duplicated inline mask), U6 MSRV re-verified on the integrated `src/db/store.rs`, the
  File-vs-std boundary marked `PENDING U7 PASS` everywhere, and attempt 3 marked superseded
  by attempt 4.
* Honest posture unchanged: Principles III/IV remain **NOT-PASSED (provisional)** until U7
  records PASS and U6 lands.

### Report-only staging-review addendum — 2026-08-25 (PR #107 pass 4: sidecar identity and engine-open containment)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists). This pass
remediates three additional Copilot findings without changing the seven-task DAG or
shipment membership.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| E1 | P1 | Directory-relative `symlink_metadata` followed by `remove_file` still permits a writer to replace an observed empty sidecar with live non-empty content before unlink. | **Resolved (U3).** Separate metadata-check + name-based unlink is prohibited. U3 must prove a safe API that deletes the same identity whose emptiness was observed or fail closed by leaving transient sidecars in place. Added a replacement-race test requirement. |
| E2 | P1 | The capability-safe handle ended before `configure_sqlite_wal` and `open_sqlite_instance`; path-based engine open could still follow a swapped intermediate directory outside the root. | **Resolved (U7/U6).** U7 adds a sixth mandatory proof obligation for a capability- or identity-bound SQLite/Cozo open through WAL/sidecar creation. U6 carries the proven boundary through all persistent open paths and adds an engine-open swap test. A path-only engine API forces U7 BLOCKED. |
| E3 | P2 | Durable Stage memory still described the obsolete U1-U5 manifest and dependency chain. | **Resolved.** The handoff now lists U1-U7, the current eight-item shipment manifest including `059-F`, and the authoritative U7 to U6 to U2-U5 dependency chain. |

**Findings summary:** P0 = 0, P1 = 2 (E1-E2, resolved in-artifact), P2 = 1 (E3,
resolved). Shipment `051-S` remains queued with the same seven tasks and sequencing
`050-S -> 051-S -> 049-S`. Principles III/IV remain **NOT-PASSED (provisional)**
until U7 proves all six obligations and U6 lands. Gate: **PASS** (advisory).

<!-- plan-review-attempt: 6 -->
<!-- plan-review-verdict: PASS -->
* **Superseded by attempt 7 below.** Attempt 6's seven-task U1-U7 structure
  exceeded the task-scenario heuristic and made U7's BLOCKED path unschedulable.
* Sidecar cleanup now fails closed on identity ambiguity instead of relying on a
  directory-relative check/use sequence.
* The capability boundary now includes WAL configuration and the actual SQLite/Cozo
  engine open; no guarded-then-path-reopen claim remains.
* The durable Stage handoff matches the current U1-U7 DAG and `051-S` manifest.

### Report-only staging-review addendum — 2026-08-25 (PR #107 pass 5: schedulable feasibility and bounded integrations)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists). The
current-head review identified four security-DAG defects plus one duplicate
rollback finding. All are resolved in the active plan and backlog.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| F1 | P1 | U7 depended on U1 even though U1 required U7's possibly-failing Rust 1.75 proof, so U7 could never record BLOCKED. | **Resolved.** U7 now has no dependency and runs candidate discovery in an isolated harness. U8 depends on U7; U1 depends on both proofs and adopts only proven versions. |
| F2 | P1 | U7 combined six cross-platform and engine scenarios, violating the fewer-than-four-scenarios task heuristic. | **Resolved.** U7 owns three root/API/MSRV scenarios. New U8 (`059.008-T`) owns three engine-boundary feasibility scenarios. |
| F3 | P1 | U6 combined five opener, permission, and engine scenarios. | **Resolved.** U6 owns three contained opener/permission scenarios. New U9 (`059.009-T`) owns three production engine-integration scenarios. U2–U5 depend on U6 and U9. |
| F4 | P1 | The pip-hardening plan rollback restored the blanket `pip` grant on an approval prompt. | **Resolved in the sibling 050-S plan.** Rollback keeps `pip` denied/manual and permits only a separately reviewed exact anchored entry. |
| F5 | P1 | Task `057.001-T` repeated the insecure rollback. | **Resolved.** Its acceptance and implementation notes now prohibit restoring blanket approval. |

**Authority at pass 5 (superseded by pass 6 below):** nine-task U1–U9 DAG:
U7 → U8 → U1 → U6 → U9 → U2 → U3/U4 → U5, with explicit
fan-in dependencies recorded in backlog. Shipment `051-S` contains `059-F` plus
`059.001-T` through `059.009-T`. Principles III/IV remain NOT-PASSED until
U7/U8 PASS and U6/U9 land.

<!-- plan-review-attempt: 7 -->
<!-- plan-review-verdict: PASS -->


### Report-only staging-review addendum - 2026-08-25 (PR #107 pass 6: U5 test-scenario split)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists). Current-head
Copilot review flagged that U5 (`059.005-T`) still carried seven test deltas (a)-(g),
violating the fewer-than-four-test-scenarios task heuristic. U5 is split into three
bounded test tasks with no new cycles.

| # | Severity | Finding | Disposition |
|---|----------|---------|-------------|
| G1 | P1 | U5 (`059.005-T`) defined seven independently countable test deltas (a)-(g), exceeding the fewer-than-four-scenarios heuristic. | **Resolved.** U5 keeps at most three cross-platform deltas: (a) sidecar-type matrix fail-closed refusal, (b) per-platform fail-closed signal, (c) junction-variant refusal. New U10 (`059.010-T`) owns (d) Windows retained-handle engine non-interference and (e) broader non-name-surrogate reparse regression (at most two scenarios). New U11 (`059.011-T`) owns (f) the deterministic `should_refuse_reparse` predicate unit test, (g) the Windows literal-equality assertion, and the structural single-source proof (at most three scenarios). |
| G2 | P2 | Shipment `051-S`, feature `059-F` DoD, sibling task gating prose, the dependency graph, and durable Stage memory still described a nine-task U1-U9 DAG. | **Resolved.** `051-S` manifest is 12 items (`059-F` plus `059.001-T` through `059.011-T`); `059-F`, the sibling tasks, the Dependency Graph, structured `.backlogit/memories.json`, and the durable handoff now describe the eleven-task U1-U11 DAG. |

**Split dependency wiring:** U5 and U10 depend on U3, U4, U6, and U9 (identical gating);
U11 depends on U2 and is therefore transitively feasibility- and integration-gated.
Nothing depends on U5, U10, or U11; all three are terminal test units, so the split is
acyclic. Wherever earlier passes of this plan use the collective shorthand `U2-U5`, read
it under the current authority as `U2-U5 plus U10, with U11 gated after U2`.

**Authority at pass 6 (superseded by pass 7 below):** eleven-task U1-U11 DAG:
U7 -> U8 -> U1 -> U6 -> U9 -> U2 -> U3/U4 -> U5, with U10 sharing U5's U3+U4+U6+U9 gating
and U11 gated after U2; explicit fan-in dependencies are recorded in backlog. Shipment
`051-S` contains `059-F` plus `059.001-T` through `059.011-T` (12 items). Principles
III/IV remain NOT-PASSED until U7/U8 PASS and U6/U9 land.

### Report-only staging-review addendum - 2026-08-25 (PR #107 pass 7: U2-before-U6 dependency rewire)

**Gate: ADVISORY** (report-only; backlog `059-F`/`051-S` already exists). Current-head
Copilot review flagged that U6's acceptance needs U2's leaf no-follow/permission
primitives, yet U2 depended on U6 and U9 — an inverted build order. The dependency
direction is corrected with no new cycles and all feasibility gates preserved.

| # | Severity | Finding | Disposition |
|---|----------|---------|-------------|
| H1 | P1 | U2 (`059.002-T`) depended on U6 and U9 while U6's acceptance needs U2's `open_no_follow`/`capture_perms`/`set_readonly_via_handle` primitives, inverting the build order. | **Resolved.** U2 now depends only on U1 and is scheduled BEFORE U6. U6 (`059.006-T`) depends on U2 and COMPOSES its leaf primitives instead of duplicating them. U9 still follows U6. U3/U4 depend on U2+U6+U9; U5/U10 depend on U3+U4+U6+U9; U11 depends on U2. Frontmatter, unit definitions, Requirements Trace, Dependency Graph, Risks, deliberation, `.backlogit/memories.json`, and the durable handoff updated. |
| H2 | P2 | Requirements Trace, implementation narrative, and rollback/runtime prose still assigned the Windows handle-mode / non-interference delta (d) validation to U5 after the pass-6 split moved it to U10. | **Resolved.** All live ownership of the Windows handle-mode validation and delta (d)/(e) breadth is corrected to U10; U5 retains only the sidecar-matrix deltas (a)-(c). |

**Current authority:** eleven-task U1-U11 DAG:
U7 -> U8 -> U1 -> U2 -> U6 -> U9 -> U3/U4 -> U5/U10, with U11 after U2. U2 depends only
on U1; U6 composes U2 and depends on U1+U2+U7+U8; U9 depends on U6+U8; U3/U4 depend on
U2+U6+U9; U5 and U10 depend on U3+U4+U6+U9 (U10 owns the Windows handle-mode /
non-interference deltas d-e); U11 depends on U2. Explicit fan-in dependencies are
recorded in backlog. Shipment `051-S` contains `059-F` plus `059.001-T` through
`059.011-T` (12 items). Principles III/IV remain NOT-PASSED until U7/U8 PASS and U6/U9
land. Wherever earlier passes use the shorthand `U2-U5`, read it under this authority
as: U2 is a leaf primitive preceding U6; U3-U5 plus U10 are gated on U6+U9; U11 is
gated after U2.

### AUTHORITATIVE re-deliberation replan - 2026-08-29 (U8 BLOCKED: scoped permission-mutation path)

**Gate: DECISION** — supersedes the engine-boundary portions of every prior section for the
purpose of unblocking `059-F`. Source:
`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`
(Option B chosen). U7 (`059.007-T`) recorded PASS; U8 (`059.008-T`) recorded **BLOCKED** with
source-verified, compile-checked evidence that cozo 0.7 `SqliteStorage` re-opens SQLite by a
**bare path** on every pool-empty `transact()` for the `DataStore` lifetime, and
`DbInstance::new`'s `path: impl AsRef<Path>` structurally rejects a handle/capability object
(type-system fact). No safe capability-/identity-bound engine open exists without forking cozo
(out of scope, Principle VI).

**Chosen path (Option B + evidence-based `049-S` decouple):** land the *permission-mutation*
containment (feasible per U7) and accept + document the *engine-open* residual (infeasible per
U8), tracking full closure via upstream cozo (Option A) as a later separate shipment.

**Rescoped authoritative DAG (engine-open binding removed from the near-term critical path):**

```text
U7(done) ─▶ U1 ─▶ U2 ─▶ U6 ─▶ U3/U4 ─▶ U5/U10
                    └────────────────▶ U11 (after U2)
sign-off 059.014-T ─▶ U1          (implementation gated on operator sign-off)
DEFERRED (later separate shipment): U8(evidence, terminal) ; U9 ; U12(059.012-T) ; upstream cozo 059.013-T
```

Dependency changes from the prior authority (each enacted in backlog by this Stage pass):

* `059.001-T` (U1): deps `[059.007-T, 059.014-T]` — drop `059.008-T`; add the sign-off gate.
* `059.006-T` (U6): deps `[059.001-T, 059.002-T, 059.007-T]` — drop `059.008-T`. U6 contains the
  `chmod` paths (leaf via U2 + intermediate directory via the cap-std root-relative walk); it does
  **not** require the engine binding.
* `059.003-T` (U3), `059.004-T` (U4): deps `[059.002-T, 059.006-T]` — drop `059.009-T`.
* `059.005-T` (U5), `059.010-T` (U10): deps `[059.003-T, 059.004-T, 059.006-T]` — drop `059.009-T`.
  U10's Windows retained-handle non-interference test now asserts non-interference between the
  retained **permission** handle and the engine open (unchanged value; no longer U9-gated).
* `059.002-T` (U2), `059.011-T` (U11): unchanged.
* `059.009-T` (U9): reclassified **deferred/upstream**; deps `[059.013-T, 059.006-T]`; excluded
  from `059-F` near-term completion; later separate shipment. Nothing feasible depends on it.
* New `059.013-T` (Option A: upstream cozo capability-open request / maintained-fork evaluation),
  parent `059-F`, later separate shipment.
* New `059.014-T` (operator sign-off gate on the accepted engine-open residual), parent `059-F`.

**Amended `059-F` Definition of Done (engine-boundary portion):**

* The two originally-reported permission-mutation TOCTOUs (`5905CDEE`, `E86A6E56`) are closed by
  identity-bound, no-follow, root-relative handles; no path-based `chmod`; no `chmod` can be
  redirected outside the workspace (leaf **and** intermediate-directory containment of the
  permission paths).
* **U8 PASS is NO LONGER a completion gate.** Its BLOCKED evidence is an accepted decided input.
  **U9 (engine-open binding) is removed** from the DoD and deferred to Option A.
* Principles III/IV are **PASSED for the permission-mutation threat only**; they remain
  **NOT-PASSED for the engine-open leaf and intermediate-directory redirection**, which is the named,
  signed-off accepted residual. The original fail-closed DoD is **not** claimed as satisfied.
* Implementation (U1 onward) does not begin until `059.014-T` (operator sign-off) is `done`.
* Everything else in the prior DoD (test-first, `#![forbid(unsafe_code)]`, MSRV 1.75 continuous
  CID check, clippy pedantic, cargo audit, no duplicate crate copy, exact-permission and sidecar
  invariants, deterministic `should_refuse_reparse` predicate) is retained unchanged for the
  feasible units.

**`051-S` transition (SUPERSEDED / ENACTED — 2026-08-30 Stage convergence, PR #114):**
~~Stage does not mutate the active `051-S` manifest or close it. Ship, on its next cycle and only
after `059.014-T` sign-off, either re-scopes `051-S`'s manifest to the feasible task set (`059-F` +
U1/U2/U6/U3/U4/U5/U10/U11) as the owner, or closes `051-S` (feasibility complete; engine binding
infeasible/accepted) and Stage assembles a fresh implementation shipment.~~ **Enacted outcome:**
`051-S` was **safely closed and is now `archived`**; its non-terminal members were removed with
status-preserving `return-blocked` and handed off; the Ship-created successor `054-S` was deleted
and is absent, but that deletion was an unapproved destructive P-005 violation, not compliant
remediation. **Ship did not, and cannot, re-scope `051-S` or create a successor shipment** —
under fail-closed P-010 both are Stage-only. **Stage exclusively** normalizes (`blocked → queued`)
the feasible units and assembles any successor shipment (Step 5.5). Those two Stage acts are on
**different clocks**: the `blocked → queued` normalization of `059-F` + U1/U2/U6/U3/U4/U5/U10/U11
is **already completed and Stage-ratified** (2026-08-30) while `059.014-T` is still `queued` — it
was never gated on sign-off, because the gate functions as the `blocks` edge `059.001-T ←
059.014-T`, so U1 stays unexecutable regardless of the units' intake status. Only
**successor-shipment assembly and the implementation that follows it (U1 onward)** wait for
`059.014-T` to be `done`. Ratifying the completed normalization grants no early execution
authority and does not retroactively legalize the Ship mutation that produced it. `049-S` is
decoupled from `051-S` (sequencing-only edge, no
code coupling, no schedule gain) so bug `7BF1961D` proceeds independently.

**`051-S` closure timing (superseded precondition; recorded 2026-08-30).** The original PR #113
requirement that `051-S` be resolved only *after* `059.014-T` sign-off is **explicitly superseded**
by `docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`
§ *Supersession of the PR #113 `051-S` Closure-Timing Requirement*. Evidence-based basis: `051-S`
was an **evidence** shipment whose post-return manifest was `[059.007-T]` (already `done`);
the safe-close archived only that delivered member plus the shipment record, returned `059-F` and
`059.008-T` status-preservingly via `return-blocked` (both stayed `blocked`, neither archived),
accepted no residual risk, and began no implementation. The sequencing mismatch is recorded
honestly: the closure ran before the precondition was superseded. Current rule:
**evidence-shipment closure may precede sign-off; `059.014-T` gates successor-shipment assembly
and implementation only.** This is a shipment-lifecycle timing supersession, **not** a retroactive
security sign-off — `059.014-T` remains `queued` and the Accepted-Residual-Risk Record is unsigned.

**Status-normalization + assembly ownership (Stage-owned; added 2026-08-30).** Following Copilot
review comment `3888455427` on PR #114 and
`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`: the
`blocked → queued` normalization of the feasible units (`059-F` + U1/U2/U6/U3/U4/U5/U10/U11) is a
Stage `update backlog items` operation, not a Ship operation. Ship only identifies the non-terminal
manifest members, `return-blocked`s them (status-preserving), safe-closes the evidence shipment, and
hands off the rescoped scope; **Stage** performs the status normalization, rewires the feasible DAG,
and assembles any successor shipment under Step 5.5. The prior Ship-performed normalization remains
a P-010 violation and is not retroactively legalized — Stage affirmed the resulting `queued`
disposition only after independent review. A fourth distinct Ship P-010 was subsequently recorded:
Ship's post-return mutation of `059.008-T`'s `blocked_reason` planning field; Stage independently
ratified the current terminal blocked reason as semantically correct without legalizing the mutation
(see the *Historical Ship role-boundary violation record* in
`docs/decisions/2026-08-29-store-toctou-engine-boundary-redeliberation-deliberation.md`).

**Successor-shipment assembly path (Step 5.5 Mode R; added 2026-08-30).** The ten near-term items
were harvested in an earlier session and already exist in the queue, so Step 5.5's default Mode H
scope guard (harvest-only IDs) cannot admit them. The durable Mode R authorization — covering
feature `059-F`, exact `handoff_ids` `059-F, 059.001-T, 059.002-T, 059.003-T, 059.004-T,
059.005-T, 059.006-T, 059.010-T, 059.011-T, 059.014-T`, with the parent-first assembly order and
the exclusion of `059.008-T`/`059.009-T`/`059.012-T`/`059.013-T` and the terminal external
prerequisite `059.007-T` — is recorded in
`docs/decisions/2026-08-30-stage-ratify-059-f-normalization-ownership-deliberation.md`
§ *Mode R Authorization for Successor-Shipment Assembly*. After `059.014-T` is `done`, Stage enters
Step 5.5 directly under Mode R citing that section, logs Steps 1–5 as not applicable, re-validates
the exact set, and assembles. No stash entry or synthetic harvest may be manufactured, and no
shipment is assembled while the gate is open.
