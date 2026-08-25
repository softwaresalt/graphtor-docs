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
| Preserve exact-permission and sidecar-content tests | U3/U4 keep capture/restore exact (handle-bound) and empty-only sidecar cleanup; U5 re-verifies. |
| Test-first swap-resistance + platform behavior | U3/U4 add test-first swap-resistance cases; U5 adds the cross-platform matrix and Windows handle-mode validation. |

## Implementation Units

### U1 — Add platform-gated no-follow flag dependencies (config)

* **Changes**: In `Cargo.toml`, add `libc` under `[target.'cfg(unix)'.dependencies]`
  (for `O_NOFOLLOW`) and `windows-sys` under `[target.'cfg(windows)'.dependencies]`,
  pinned to an already-present lock version to avoid adding a new copy:
  `windows-sys = { version = "0.61", features = ["Win32_Storage_FileSystem"] }`.
  That feature exposes every constant the design needs:
  `FILE_FLAG_OPEN_REPARSE_POINT`, `FILE_FLAG_BACKUP_SEMANTICS`,
  `FILE_ATTRIBUTE_REPARSE_POINT`, `FILE_READ_ATTRIBUTES`, `FILE_WRITE_ATTRIBUTES`,
  and `FILE_SHARE_READ|WRITE|DELETE`. Add a justification comment (both crates are
  already transitive; Principle VI).
* **Files**: `Cargo.toml` (and `Cargo.lock` refresh).
* **Tests**: pinned-MSRV verification is **required** for these new direct
  dependencies — run `cargo +1.75.0 check --all-targets` (equivalently
  `rustup run 1.75.0 cargo check --all-targets`) and confirm it succeeds, so the
  added `libc`/`windows-sys` edges and their feature set are proven to build on
  the declared MSRV (1.75), not merely on the host toolchain. Also run
  `cargo build` / `cargo check --all-targets` on the host platform. **Duplicate
  impact must be verified by an explicit before/after comparison:** capture
  `cargo tree -d` (and `cargo tree -i windows-sys` / `cargo tree -i libc`) output
  **before** the dependency edit and **after**, then diff the two captures to
  prove no new `windows-sys`/`libc` version (no second copy) was introduced — a
  post-edit-only snapshot is insufficient. `cargo audit` clean.
* **Posture**: config-first. **Domain**: config.
* **Acceptance**: Both platform deps declared, pinned to an existing lock version,
  and justified; workspace compiles on the host **and** under the pinned MSRV
  toolchain via `cargo +1.75.0 check --all-targets`; the before/after `cargo tree
  -d` diff shows no new duplicate crate copy; no new `cargo audit` advisory.

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
      `MetadataExt::file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT` and **return
      the existing refusal error when the reparse bit is set** — fail-closed is an
      explicit code step on Windows, not a byproduct of the open. Attribute-level
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
        reparse file (see U5 delta (e)) and for the junction refusal path (U4).
  * `capture_perms(&File) -> Result<fs::Permissions>` via `File::metadata()`.
  * `set_readonly_via_handle(&File, bool) -> Result<()>` via `File::set_permissions`
    (fchmod on Unix; `SetFileInformationByHandle(FileBasicInfo)` on the Windows
    handle). Preserve exact perms (never a coarse writable/readonly boolean that
    would widen Unix mode bits).
  * Every helper returns `Result` (no `.unwrap()`/`.expect()`); all error paths map
    to `GraphtorError` with a traceable message.
  * **Scope note (doc-comment)**: identity binding covers the **final path
    component only**; parent-directory containment continues to rely on the caller's
    `validate_path`/canonicalization (a parent-dir swap is out of scope for the
    file-swap threat model). `O_NOFOLLOW`/`OPEN_REPARSE_POINT` do not guard
    intermediate components, and `#![forbid(unsafe_code)]` + MSRV 1.75 preclude an
    `openat`/`O_PATH` directory-handle approach.
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
    creates during the read cycle) have **no** retained handle at `Drop`. Their
    empty-only cleanup therefore stays path-level but link-safe: check emptiness via
    `symlink_metadata` (a planted link reports non-zero/unknown length → not removed)
    and rely on `remove_file` unlinking the **link itself, never the target**.
    Document this residual (contained) behavior in the doc-comment; do not describe it
    as fully handle-bound.
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
  sidecar_on_drop`, `..._never_removes_a_non_empty_transient_sidecar_on_drop`,
  `..._never_removes_a_non_empty_shm_sidecar_on_drop`.
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

### U5 — Cross-platform swap-resistance deltas + Windows handle-mode validation (tests)

* **Changes**: Add colocated tests covering only the **deltas** U3/U4 do not already
  assert (reference, do not re-implement, the U3/U4 cases): (a) the full sidecar-type
  matrix (`-wal`/`-shm`/`-journal`) fail-closed refusal on both open paths; (b) the
  platform fail-closed signal is asserted correctly per platform (Unix `O_NOFOLLOW` →
  `ELOOP` open failure; Windows reparse open → explicit `FILE_ATTRIBUTE_REPARSE_POINT`
  refusal); (c) reuse the `try_symlink_file` unprivileged-skip pattern and add a
  Windows junction variant so the Windows refusal path executes on unprivileged CI
  where possible; (d) validate the Windows retained handle
  (`FILE_READ_ATTRIBUTES|FILE_WRITE_ATTRIBUTES`, full share mode) does **not** block
  the engine's own db/WAL open — scoped to that single assertion (Option A vs the
  documented Option C fallback decision — record the outcome in the PR); (e)
  **broader-fail-closed-policy regression (Windows):** create a legitimate
  **non-redirecting** reparse file — one whose reparse tag is NOT a name
  surrogate (e.g. a non-name-surrogate reparse point creatable without the
  symlink privilege where feasible) — and assert the guarded `open_no_follow`
  path **refuses it** (consistent with the adopted broader policy in U2), so the
  intentional Windows/Unix breadth asymmetry is pinned by a test rather than left
  implicit. If a non-redirecting reparse file cannot be created on unprivileged
  CI, the test skips (not fails) and the doc-comment's accepted-breadth statement
  stands as the recorded behavior; report executed-vs-skipped.
* **Files**: `src/db/store.rs` tests (and/or a single narrowly-scoped `tests/`
  integration file only if a full engine open is required for assertion (d)).
* **Tests**: the delta matrix above (skips gracefully when the platform refuses
  unprivileged symlink/reparse creation; junction variant runs where symlink
  creation is denied).
* **Posture**: test-first / characterization. **Domain**: tests.
* **Acceptance**: Suite passes on the host platform; skips (not fails) where symlink
  or non-redirecting-reparse creation is unprivileged, with the junction variant
  covering the Windows name-surrogate refusal and delta (e) covering the broader
  non-name-surrogate refusal where possible; the Windows handle-mode decision
  (Option A vs C) is recorded in the PR; whether each Windows refusal test executed
  or skipped is reported.

## Dependency Graph

```text
U1 (deps) ─▶ U2 (helper) ─┬─▶ U3 (guard lock/Drop) ─┐
                          └─▶ U4 (clear_stale)      ─┴─▶ U5 (cross-platform matrix)
```

* U2 depends on U1 (needs the platform flag constants).
* U3 depends on U2; U4 depends on U2 (both consume the no-follow helper).
* U5 depends on U3 and U4 (validates both paths together).
* No cycles.

## Decisions and Rationale

* **Retained handle over re-check (Option A)** — only a handle bound to the file
  identity removes the check→use gap; a repeated path re-check (Option B) still races.
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
| Windows retained handle blocks Cozo/SQLite db/WAL open | Open attribute-only + full share mode; U5 validates; Option C fallback documented. |
| Accidental `unsafe` via raw handles | Safe std only; CI `#![forbid(unsafe_code)]` enforces. |
| Regressing exact-permission / sidecar-content behavior | Capture/restore stays exact (handle-bound); all existing tests retained (U3/U4/U5). |
| Dangling-symlink self-heal edge case | Explicit absent-vs-link disambiguation before the no-follow open (U4). |
| Dependency friction (Principle VI) | Both crates already transitive; platform-gated with justification comment (U1). |

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Safety-First Rust | PASS | Entirely safe std (`OpenOptionsExt::custom_flags`, `File::set_permissions`, `File::metadata`, Windows `access_mode`/`share_mode`); `#![forbid(unsafe_code)]` preserved; all new helpers return `Result` with no `.unwrap()`/`.expect()`; each code unit gates on `clippy::pedantic -D warnings`. |
| II. Test-First Development | PASS | U2/U3/U4 author NEW failing tests before implementation; U5 characterization; all existing tests re-run unchanged. |
| III. Workspace Isolation / IV. CLI Containment | PASS (core intent) | Every permission mutation bound to a no-follow, identity-bound handle; explicit Windows reparse-attribute refusal adopting a **broader fail-closed policy** (refuse ANY `FILE_ATTRIBUTE_REPARSE_POINT` entry — see U2 decision — because a precise name-surrogate test needs `unsafe` `DeviceIoControl` precluded at MSRV 1.75, and DB paths expect plain files); no path-based `chmod` fallback; fail-closed on link/reparse/unobtainable handle; previously-uncovered main db (index 0) swap now covered. Final-component-only scope stated (U2); parent-dir containment relies on caller `validate_path`. |
| V. Structured Observability | PASS | Each fail-closed refusal returns a traceable `GraphtorError`, not a silent early return. |
| VI. Single Responsibility | PASS | `libc`/`windows-sys` are platform-gated, pinned to an already-transitive lock version, justified; no speculative abstraction (helpers consumed by both paths). |
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
  (dependency)**. Adds two platform-gated direct dependencies (`libc`, `windows-sys`).
* **high runtime, rollout, or rollback risk** — **PRESENT (moderate)**. The guard is on
  the read-only serve hot path and the write-mode self-heal path; a Windows handle-mode
  mistake could block engine opens. Mitigated by fail-closed design and U5 validation.

**Requires plan hardening: yes**

## Runtime Verification and Closure

* **Changed runtime surface**: the read-only serve path (`open_engine_readonly` →
  `EngineReadonlyGuard`) and the write-mode open self-heal (`open_sqlite` →
  `clear_stale_readonly_lock`). No CLI/API signature change.
* **Runtime verification (Ship)**: run the full `cargo test` store suite on the host
  platform; where possible validate on Windows that a real read-only serve + subsequent
  write-mode open succeed (engine open not blocked by the retained handle) and that a
  planted symlinked sidecar is refused on both open paths.
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
   only empty transient sidecars this session created are cleaned up.
3. **Byte-identical db/sidecars** after a read cycle.
4. **Fail-closed containment** — no permission mutation ever reaches a target outside
   the workspace; a linked/reparse entry is refused, never followed.
5. **`#![forbid(unsafe_code)]`** — zero `unsafe`.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

| ProposedAction | change_kind | targets | ActionRisk | rollback | approval | ActionResult |
|---|---|---|---|---|---|---|
| Add `libc` + `windows-sys` as platform-gated direct deps (U1) | config change | `Cargo.toml`, `Cargo.lock` | moderate | revert Cargo.toml/lock; both already transitive | not required (non-destructive, justified) | planned |
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
  swap-between-check-and-clear on the write path (U4); dangling-symlink fail-closed;
  stale-lock self-heal; exact-mode and non-empty-sidecar preservation; Windows retained
  handle does not block the engine db/WAL open (U5).
* **Blocked-path handling**: if the no-follow handle cannot be opened/retained, refuse
  the open (existing refusal error) — never mutate via a re-resolved path.
* **Rollback trigger**: any failing store test, a `cargo audit` advisory from U1, or a
  Windows engine-open blocked by the retained handle. **Rollback procedure**: for the
  Windows-open case, switch that platform to Option C (identity-verified re-open) and
  re-run U5; for a broader failure, revert the guard/clear-stale changes (the functions
  are self-contained) while keeping U1's deps. **Owner**: Ship agent. **Validation
  window**: the Ship runtime-verification + CI pass for the release PR.
* **Unresolved operator decision blocking safe execution**: none. The Windows
  handle-mode choice is resolvable in-implementation via U5 test evidence.

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
| F3 | P2 | Rust | Windows handle needs explicit `access_mode` + `share_mode`; unspecified share mode risks sharing violations against the engine's own open. | **Fixed (U2/U5).** Full share mode (`READ|WRITE|DELETE`) specified; U5 validates the engine db/WAL open is not blocked. |
| F4 | P2 | Correctness | Transient sidecars are created by the engine, not the guard, so cleanup **cannot** be handle-bound; the plan implied otherwise. | **Fixed (U3).** Cleanup documented as path-level but link-safe (`symlink_metadata` emptiness + `remove_file` unlinks the link, not the target). |
| F5 | P2 | Correctness, Scope | U4 absent-vs-dangling disambiguation keyed on `is_symlink()` would miss Windows **junctions**. | **Fixed (U4).** Disambiguation keys off the reparse attribute (matching `is_reparse_point` breadth). |
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
| A1 | P2 | Windows `FILE_ATTRIBUTE_REPARSE_POINT` breadth was ambiguous vs the narrower Unix `O_NOFOLLOW` name-surrogate class (symlink/junction). A blanket attribute check also refuses non-redirecting reparse points (OneDrive/dedup/HSM/WSL/app-alias). | **Resolved (U2/U4/U5).** Explicitly **adopted the broader fail-closed policy** — refuse ANY reparse-point entry — and justified it: a precise name-surrogate test needs the reparse tag via `unsafe` `DeviceIoControl(FSCTL_GET_REPARSE_POINT)` (precluded by `#![forbid(unsafe_code)]` at MSRV 1.75), and `FileType::is_symlink()` is path-based (re-introduces TOCTOU). Added U5 delta (e) regression for a legitimate non-redirecting reparse file (refused; skips unprivileged). Intentional Unix/Windows breadth asymmetry documented. |
| A2 | P2 | New direct deps (`libc`, `windows-sys`) were verified only on the host toolchain, not the declared MSRV. | **Resolved (U1).** Acceptance now **requires** explicit pinned-MSRV verification `cargo +1.75.0 check --all-targets` (or `rustup run 1.75.0 …`). |
| A3 | P2 | Unix `open_no_follow` specified only `custom_flags(libc::O_NOFOLLOW)` without an access mode; `O_NOFOLLOW` is a modifier and needs an access mode. | **Resolved (U2).** Unix `OpenOptions` now **explicitly sets `.read(true)`** alongside `O_NOFOLLOW` (least-privilege; sufficient for metadata + fchmod restore; avoids denying an already-read-only file). |
| A4 | P2 | Duplicate-crate impact was asserted with a post-edit-only `cargo tree -d` snapshot. | **Resolved (U1).** Acceptance now **requires a before/after `cargo tree -d` (and `-i windows-sys`/`-i libc`) comparison** proving no new version copy. |

**Findings summary:** P0 = 0, P1 = 0, P2 = 4 (all resolved in-artifact this pass),
P3 = 0. Task acceptance criteria for `059.001-T`/`059.002-T`/`059.004-T` updated
to match. Gate: **PASS** (advisory). No manifest change; shipment `051-S` remains
queued.

<!-- plan-review-attempt: 2 -->
<!-- plan-review-verdict: PASS -->
