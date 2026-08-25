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
| Intermediate-directory (parent) swap containment, not just final component (PR #107 review, 2026-08-25) | U6 adds a candidate `cap-std` beneath-root directory-handle-relative walk (feasibility/MSRV proven by U7 before use); U2's `open_no_follow` is invoked through it; U2–U5 depend on U6 so full-path containment lands before they complete. |
| Prove the capability design is achievable with safe APIs under MSRV 1.75 before building on it (PR #107 review, 2026-08-25) | U7 (059.007-T) is a bounded test-first feasibility/evidence gate proving the root-handle bootstrap (explicit threat model), `cap_std` Dir/File conversion + File boundary, intermediate-swap-refusing walk, and in-bounds leaf refusal, or returning BLOCKED; U7 gates U6, which gates U2–U5. |

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
  already transitive; Principle VI). **Also add `cap-std`** as a workspace-level
  *candidate* dependency for the **U6** containment-safe beneath-root directory walk,
  pinned to the newest version that builds under MSRV 1.75. Its MSRV-1.75 build **and**
  the required root/intermediate/leaf beneath-root semantics are **authoritatively
  proven in U7 (`059.007-T`), not assumed here** — U1 only declares/pins the candidate
  (the Tests row runs a first-pass `cargo +1.75.0 check --all-targets`). `cap-std`
  layers on the already-transitive
  `rustix` (1.1.4 in `Cargo.lock`) on Unix and wraps the Windows handle-relative
  primitives safely, so the supply-chain delta is the `cap-std`/`cap-primitives`
  family only; carry the same Principle VI justification comment. (If U7 records
  `cap-std` BLOCKED because no MSRV-1.75-compatible release delivers the needed
  semantics, do not keep it; the documented Unix-only `rustix` fallback + a separate
  safe Windows design is pursued only after its own feasibility evidence, recorded in
  the PR.)
* **Files**: `Cargo.toml` (and `Cargo.lock` refresh).
* **Tests**: pinned-MSRV verification is **required** for these new direct
  dependencies — run `cargo +1.75.0 check --all-targets` (equivalently
  `rustup run 1.75.0 cargo check --all-targets`) and confirm it succeeds, so the
  added `libc`/`windows-sys` edges and their feature set are proven to build on
  the declared MSRV (1.75), not merely on the host toolchain. Also run
  `cargo build` / `cargo check --all-targets` on the host platform. **Duplicate
  impact must be verified by an explicit before/after comparison:** capture
  `cargo tree -d` (and `cargo tree -i windows-sys` / `cargo tree -i libc` /
  `cargo tree -i cap-std`) output **before** the dependency edit and **after**, then
  diff the two captures to prove no new `windows-sys`/`libc`/`cap-std` version (no
  second copy) was introduced — a post-edit-only snapshot is insufficient. The pinned
  MSRV check MUST also cover `cap-std` (the U6 dependency) so the added edge is proven
  to build on 1.75 (the authoritative cap-std feasibility/semantics proof is **U7**).
  **Continuous MSRV evidence is required:** a dedicated Rust 1.75 CI check (a
  `cargo +1.75.0 check --all-targets` job) MUST be added during implementation, or an
  equivalent already-present repository MSRV gate MUST be explicitly identified as
  continuously covering these new deps — a one-off local `+1.75.0` run is not
  sufficient by itself. (Do not alter the workflow during Stage; this is an
  implementation-time obligation.) `cargo audit` clean.
* **Posture**: config-first. **Domain**: config.
* **Acceptance**: Both platform deps **and `cap-std`** declared, pinned, and
  justified; workspace compiles on the host **and** under the pinned MSRV toolchain
  via `cargo +1.75.0 check --all-targets` (including the `cap-std` edge); a continuous
  Rust 1.75 CI check is added (or an equivalent repository MSRV gate identified) so the
  MSRV holds beyond a single local run; the before/after `cargo tree -d` diff shows no
  new duplicate crate copy; no new `cargo audit` advisory. (Authoritative cap-std
  feasibility/semantics evidence is produced by U7, which U6 consumes.)

### U7 — cap-std beneath-root feasibility & MSRV-1.75 evidence gate (test-first; gates U6→U2–U5)

* **Why (added 2026-08-25, PR #107 review)**: the `cap-std` beneath-root design is a
  *candidate*, not a proven mechanism. Nothing downstream may assume it delivers the
  exact root/intermediate/leaf containment semantics or compiles under MSRV 1.75 until
  this gate records a PASS. This removes the prior overclaim.
* **Changes**: author an evidence harness (throwaway example / narrowly-scoped `tests/`
  file / temporary gated module) **test-first**, then minimal proof code, proving with
  **safe APIs only** under `#![forbid(unsafe_code)]` and MSRV 1.75:
  1. **Atomic workspace-root directory-handle bootstrap** with no-follow/no-reparse
     semantics, retained for the `DataStore` lifetime — **Unix**: `OpenOptions` with
     read access + `custom_flags(O_DIRECTORY | O_NOFOLLOW)` (a symlinked root fails
     closed); **Windows**: `OpenOptions` directory handle with
     `custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)`,
     explicit `access_mode` (≥ `FILE_READ_ATTRIBUTES`) and `share_mode`
     (`FILE_SHARE_READ|WRITE|DELETE`), then an explicit post-open
     `FILE_ATTRIBUTE_REPARSE_POINT` attribute rejection of the root. **Threat model
     (state explicitly)**: an attacker may create/write/swap entries **inside** the
     workspace root but **not** its trusted parent; the root handle is bootstrapped
     once from the trusted parent and retained, so the root identity cannot be
     redirected post-bootstrap.
  2. **Conversion to/from `cap_std::fs::Dir`/`cap_std::fs::File`** (e.g.
     `Dir::from_std_file`, `File` into/from std) — or whichever safe capability API is
     selected — round-tripping the retained root handle into the capability authority
     with **no in-crate `unsafe`**.
  3. A **component walk** that prevents escape (`..`/absolute/out-of-root) **and
     refuses an intermediate symlink/junction swap** of a parent directory.
  4. A **final, in-bounds leaf** symlink/reparse is **refused** (fail-closed), not
     merely prevented from escaping.
  5. The `cap-std` candidate **compiles under Rust 1.75** (`cargo +1.75.0 check` /
     `rustup run 1.75.0 cargo check` on the harness), **and the
     `cap_std::fs::File` vs `std::fs::File` boundary is to be decided and proven here
     (PENDING U7 PASS)** — either `into_std` conversion or explicit capability-file
     helper signatures, with the exact chosen APIs recorded on PASS. The boundary is
     **not decided until U7 records PASS**; on PASS the recorded evidence is what U6/U2
     carry in, so no ambiguity is handed to them.
* **Outcome contract**: **PASS** → record the evidence (API names, pinned versions,
  test results, chosen File boundary, threat model) in this plan/PR and **unblock U6**.
  **BLOCKED** (any obligation fails under safe-API / MSRV-1.75 constraints) → record
  the specific failing obligation, keep U6/U2–U5 gated, and **Constitution
  Principles III/IV remain NOT-PASSED**. No vague `unsafe` and no path-based
  `chmod`/re-canonicalization is an acceptable substitute for the proof.
* **Files**: a throwaway/gated feasibility harness (superseded by U6's real
  `open_beneath`).
* **Posture**: test-first (feasibility spike). **Domain**: code (bounded evidence).
  **Backlog**: `059.007-T` (depends on `059.001-T`; gates `059.006-T`).
* **Acceptance**: all five obligations demonstrated with safe APIs (or a recorded
  BLOCKED naming the failing obligation); `cargo +1.75.0 check` succeeds on the harness
  with `cap-std` pinned; the File-vs-std boundary and threat model are recorded; no
  in-crate `unsafe`; no `.unwrap()`/`.expect()`; result recorded in the plan/PR and, on
  BLOCKED, III/IV marked NOT-PASSED with U6/U2–U5 gated.

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
        reparse file (see U5 delta (e)) and for the junction refusal path (U4).
  * **Deterministic refusal predicate (target-independent, Linux-testable; single
    source of truth)**: factor the reparse-bit refusal into a pure predicate
    `pub(crate) fn should_refuse_reparse(file_attributes: u32) -> bool` that tests a
    **module-private literal bit constant** `const REPARSE_ATTR: u32 = 0x0000_0400`
    (the numeric value of `FILE_ATTRIBUTE_REPARSE_POINT`; **not** `pub`/`pub(crate)`,
    **not** re-exported) — i.e. `file_attributes & REPARSE_ATTR != 0`. `REPARSE_ATTR`
    is referenced **only** inside `should_refuse_reparse`, which is the single source of
    truth for the reparse-bit decision. The predicate and its literal compile and
    unit-test on **Linux CI** (no `windows-sys`, no filesystem, no privilege). The
    **production Windows refusal branch MUST call
    `should_refuse_reparse(file_attributes())`** and MUST NOT inline a separate
    reparse-bit mask or duplicate the literal. To prevent a **decorative unused helper**
    or a **duplicated inline mask**, acceptance **structurally requires** (i) the
    literal `0x0000_0400`/`REPARSE_ATTR` appears exactly once in the module — inside the
    predicate — and (ii) a `#[cfg(windows)]` test drives the production refusal branch
    through `should_refuse_reparse`, so the helper cannot be dead code and no inline
    mask can diverge from it. A **`#[cfg(windows)]` assertion/test MUST also prove
    `0x0000_0400 == windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT`**
    so the cross-platform literal can never drift from the real Win32 constant (see U5
    deltas (f)/(g)).
  * `capture_perms(&File) -> Result<fs::Permissions>` via `File::metadata()`.
  * `set_readonly_via_handle(&File, bool) -> Result<()>` via `File::set_permissions`
    (fchmod on Unix; `SetFileInformationByHandle(FileBasicInfo)` on the Windows
    handle). Preserve exact perms (never a coarse writable/readonly boolean that
    would widen Unix mode bits).
  * Every helper returns `Result` (no `.unwrap()`/`.expect()`); all error paths map
    to `GraphtorError` with a traceable message.
  * **Scope note (doc-comment)**: the final-component handle enforces **no-follow /
    no-reparse** on the leaf; **full-path (intermediate-directory) containment is
    provided by U6's beneath-root directory-handle-relative walk** (U2's
    `open_no_follow` is invoked through the U6 opener, never on a raw absolute path).
    A bare `O_NOFOLLOW`/`OPEN_REPARSE_POINT` guards only the final component, so U2
    alone does **not** close an intermediate parent-directory swap after
    `validate_path`; U6 closes it via a safe capability API (`cap-std`) rather than an
    in-crate `unsafe` `openat`/`O_PATH`. This unit therefore depends on U6 and MUST
    NOT be considered complete on its own. The `cap_std::fs::File` vs `std::fs::File`
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

### U6 — Containment-safe directory-handle-relative opener (beneath-root walk; gates U2–U5) (code, test-first)

* **Why (added 2026-08-25)**: final-component `O_NOFOLLOW` /
  `FILE_FLAG_OPEN_REPARSE_POINT` (U2) does **not** prevent an **intermediate-directory
  swap**: after `validate_path`/canonicalization, a workspace writer can replace a
  parent directory of the db/sidecar with a symlink/junction and redirect the
  permission mutation outside the workspace. Constitution III/IV cannot be claimed
  complete while this race remains, so U2–U5 are gated on this unit. **U6 is itself
  gated on U7 (`059.007-T`)** — the test-first feasibility/evidence gate that must
  record a PASS before U6 begins; if U7 is BLOCKED, U6 stays blocked and III/IV remain
  NOT-PASSED.
* **Candidate design (cross-platform, safe API — pending the U7 feasibility gate, not
  yet proven)**: a **`cap-std` capability-based `cap_std::fs::Dir` beneath-root walk**.
  `cap-std` is the **leading candidate** for a *safe* (no in-crate `unsafe`) capability
  filesystem: it resolves paths component-by-component **relative to an open directory
  handle** and is **intended to refuse** symlink/reparse/`..`/absolute escapes on
  **both** platforms — Unix via `openat`/`openat2`
  (`RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`) over the already-transitive `rustix`
  (1.1.4), Windows via handle-relative `NtCreateFile` wrapped safely. Whether `cap-std`
  actually delivers the exact root/intermediate/leaf semantics **and** compiles under
  Rust 1.75 is **the specific claim U7 must prove or return BLOCKED** — it is not
  assumed here. Because any `unsafe` FFI lives inside the dependency,
  `#![forbid(unsafe_code)]` in this crate is preserved (Principle I unchanged;
  `cap-std` is a *justified safe dependency/API*).
* **Changes**: Add an internal opener that (1) bootstraps the caller-validated
  workspace root **once** into a retained directory-handle authority (per U7's
  no-follow/no-reparse bootstrap from the trusted parent) and converts it to a
  `cap_std::fs::Dir` via the File boundary (**PENDING U7 PASS**; the exact APIs come
  from U7's recorded evidence), and (2) re-expresses U2's
  `open_no_follow(path)` as `open_beneath(root: &Dir, relative: &Path) ->
  Result<File, GraphtorError>` that resolves the db (index 0) and each
  `-wal/-shm/-journal` sidecar **relative to that Dir handle**, so every intermediate
  component is opened via handle-relative resolution and a post-validation swap of a
  parent directory is refused. Retain, on the returned final-component handle, the
  Unix `O_NOFOLLOW` `ELOOP` leaf refusal and the Windows broader
  `FILE_ATTRIBUTE_REPARSE_POINT` fail-closed refusal from U2. This same retained root
  `Dir` authority is the handle through which **U3's transient-sidecar cleanup** runs
  its relative `symlink_metadata`/`remove_file`, so an intermediate-directory swap
  cannot redirect a sidecar deletion either. **Fail closed** (return
  the existing refusal `GraphtorError`) when the root `Dir` cannot be opened, any
  component escapes containment, or the leaf is a reparse/symlink. **No** path-based
  `chmod` fallback; **no** in-crate `unsafe`.
* **Platform-asymmetry fallback (documented, reachable only if U7 records `cap-std`
  BLOCKED under MSRV 1.75)**: Unix — a hand-rolled *safe* `rustix` `openat` component
  walk (`OFlags::NOFOLLOW | OFlags::DIRECTORY` per intermediate component; `openat2`
  `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS` where the kernel supports it); Windows — a
  separately-specified safe handle-relative design. This fallback is **not** a silent
  substitute — it must clear its own feasibility evidence (a follow-on U7-style gate)
  before adoption, and is never a vague `unsafe` or path-based fallback. `cap-std` is
  **preferred** to avoid maintaining two hand-rolled walks; the final dependency
  decision (cap-std vs fallback) is recorded in the PR with the U7 evidence.
* **Files**: `src/db/store.rs` (+ colocated `#[cfg(test)]`; a single narrowly-scoped
  `tests/` file only if a full workspace-root `Dir` + engine open is required).
* **Tests (test-first)**: (a) `open_beneath` round-trips exact `Permissions` on a real
  regular db/sidecar beneath the root `Dir`; (b) **intermediate-directory swap
  resistance** — after the root `Dir` is established, replace an intermediate parent
  directory with a symlink/junction to an external target; `open_beneath` **refuses**
  (returns the refusal error) and the external target is neither opened nor
  permission-mutated (assert the refusal, not merely an unchanged bit); (c) the leaf
  symlink/reparse still fails closed (Unix `ELOOP` / Windows
  `FILE_ATTRIBUTE_REPARSE_POINT`). Skip **gracefully** (skip, not fail) where
  unprivileged CI cannot create the intermediate link; add a **Windows junction
  variant** so the Windows refusal executes where symlink creation is denied; report
  executed-vs-skipped.
* **Posture**: test-first. **Domain**: code (incl. doc-comment updates). **Backlog**:
  `059.006-T` (depends on `059.001-T` **and `059.007-T`/U7**; gates
  `059.002-T`/`059.003-T`/`059.004-T`/`059.005-T`).
* **Acceptance**: U7 recorded a PASS before this unit starts (the File boundary —
  **PENDING U7 PASS** until U7 records the exact APIs — and the root-handle bootstrap
  are carried in from U7's recorded evidence); `open_beneath` compiles under
  `#![forbid(unsafe_code)]`; passes
  `clippy::pedantic -D warnings`; no `.unwrap()`/`.expect()`; intermediate-directory
  swap is refused fail-closed (or the case skips gracefully with executed/skipped
  reported); the leaf no-follow/reparse refusal breadth from U2 is preserved.
  **MSRV re-verification on the integrated implementation (not only the U7 harness):**
  rerun `cargo +1.75.0 check --all-targets` (equivalently `rustup run 1.75.0 cargo
  check --all-targets`) against the **real, integrated `src/db/store.rs`
  `open_beneath` implementation** with the `cap-std` edge, and record the successful
  result — the U7 feasibility-harness evidence is **necessary but not sufficient**,
  because the production integration in `store.rs` must independently compile under the
  pinned MSRV toolchain.

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
    creates during the read cycle) have **no** per-entry retained no-follow handle at
    `Drop`, but their empty-only cleanup MUST still be performed **relative to the
    retained U6 workspace-root `Dir` handle**: probe emptiness via a
    directory-handle-relative `symlink_metadata` (a planted link reports
    non-zero/unknown length → not removed) and unlink via a directory-handle-relative
    `remove_file` (unlinking the **link itself, never the target**), so an
    intermediate-directory swap between the probe and the unlink cannot redirect the
    deletion outside the workspace. This is fully contained through the root `Dir`
    handle — do **not** describe or accept it as a path-based residual.
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
  stands as the recorded behavior; report executed-vs-skipped. **(f)
  deterministic normal-CI predicate unit test (REQUIRED, always executes on Linux
  CI):** extract the Windows reparse-bit refusal decision into a **pure,
  target-independent safe predicate** —
  `pub(crate) fn should_refuse_reparse(file_attributes: u32) -> bool` returning
  `file_attributes & REPARSE_ATTR != 0` against a **module-private** literal bit
  constant `const REPARSE_ATTR: u32 = 0x0000_0400` (the numeric value of
  `FILE_ATTRIBUTE_REPARSE_POINT`; not `pub`/`pub(crate)`, referenced only inside the
  predicate) — and test it with **fabricated attribute-bit
  inputs** (reparse bit `0x0000_0400` set → refuse; clear → allow; combined with
  unrelated attributes such as `FILE_ATTRIBUTE_READONLY`/`FILE_ATTRIBUTE_HIDDEN` →
  still refuse). Because the predicate and its literal are target-independent, this
  test **compiles and runs on Linux CI** with **no filesystem, no privilege, no
  reparse fixture, and no `windows-sys`**, so it runs deterministically on every
  normal-CI job and pins the broader fail-closed policy even when the real fixture in
  delta (e) skips. **(g) Windows-only literal-equality assertion (REQUIRED,
  `#[cfg(windows)]`):** assert `0x0000_0400 ==
  windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT` (via
  `const_assert` or a `#[cfg(windows)]` `#[test]`), proving the cross-platform literal
  can never drift from the real Win32 constant, **and structurally proving the
  production Windows refusal branch calls `should_refuse_reparse`** — a
  `#[cfg(windows)]` test drives the production branch through the predicate so it
  cannot be a decorative unused helper, and the module contains exactly one occurrence
  of the `0x0000_0400`/`REPARSE_ATTR` literal (inside the predicate) so no duplicated
  inline mask can exist. Delta (e)'s real reparse-file
  fixture is thereby **optional integration coverage** layered on top of the
  always-on (f) unit test, with explicit executed/skipped reporting.
* **Files**: `src/db/store.rs` tests (and/or a single narrowly-scoped `tests/`
  integration file only if a full engine open is required for assertion (d)).
* **Tests**: the delta matrix above (skips gracefully when the platform refuses
  unprivileged symlink/reparse creation; junction variant runs where symlink
  creation is denied).
* **Posture**: test-first / characterization. **Domain**: tests.
* **Acceptance**: Suite passes on the host platform; the **deterministic
  `should_refuse_reparse` predicate unit test (delta (f)) executes on every normal-CI
  job** (no filesystem/privilege) and proves the broader fail-closed policy; **the
  `#[cfg(windows)]` delta (g) assertion proves the production Windows refusal branch
  calls `should_refuse_reparse` (single source of truth) with the `REPARSE_ATTR`
  literal defined module-private and occurring exactly once, so there is no decorative
  unused helper and no duplicated inline mask**; the
  filesystem deltas skip (not fail) where symlink or non-redirecting-reparse creation
  is unprivileged, with the junction variant covering the Windows name-surrogate
  refusal and delta (e) covering the broader non-name-surrogate refusal where
  possible; the Windows handle-mode decision (Option A vs C) is recorded in the PR;
  whether each filesystem-dependent Windows refusal test executed or skipped is
  reported; the intermediate-directory swap delta from U6 is referenced (not
  re-implemented).

## Dependency Graph

```text
U1 (deps: libc + windows-sys + cap-std) ─▶ U7 (feasibility/evidence gate) ─▶ U6 (beneath-root walk) ─▶ U2 (helper) ─┬─▶ U3 (guard lock/Drop) ─┐
                                                       │                            │                              └─▶ U4 (clear_stale)      ─┴─▶ U5 (matrix)
                                                       │                            └────────────▶ U3, U4, U5 (each gated on U6)
                                                       └▶ (BLOCKED ⇒ U6–U5 do not start; III/IV NOT-PASSED)
```

* U7 depends on U1 (needs the candidate `cap-std` pin + platform flag constants to
  compile the feasibility probes). **U7 is the gate**: it must record a PASS before U6
  starts. A U7 **BLOCKED** halts the chain and returns to Stage — U6–U5 do not begin
  and Principles III/IV remain NOT-PASSED. No vague `unsafe` or path-based fallback.
* U6 depends on U1 **and U7** (needs the U7-proven safe root-handle bootstrap, the
  cap-std Dir/File conversion, and the decided File-vs-std boundary).
* U2 depends on U1 **and U6** (final-component helper is invoked through the
  beneath-root opener).
* U3 depends on U2 **and U6**; U4 depends on U2 **and U6** (both consume the
  identity-bound, containment-checked handle).
* U5 depends on U3, U4 **and U6** (validates both paths plus the intermediate-dir
  swap delta).
* Every code unit (U2–U5) is gated on U6, which is itself gated on the U7 evidence
  PASS, so none can complete without proven full-path containment.
* No cycles.

## Decisions and Rationale

* **Retained handle over re-check (Option A)** — only a handle bound to the file
  identity removes the check→use gap; a repeated path re-check (Option B) still races.
* **Beneath-root walk via a safe capability API (candidate: `cap-std`) for
  intermediate-directory containment (U6, gated on U7)** — a final-component
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
  test-first before U6 starts. If U7 records BLOCKED, the documented fallback is a
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
| Windows retained handle blocks Cozo/SQLite db/WAL open | Open attribute-only + full share mode; U5 validates; Option C fallback documented. |
| Accidental `unsafe` via raw handles | Safe std only; CI `#![forbid(unsafe_code)]` enforces. |
| Regressing exact-permission / sidecar-content behavior | Capture/restore stays exact (handle-bound); all existing tests retained (U3/U4/U5). |
| Dangling-symlink self-heal edge case | Explicit absent-vs-link disambiguation before the no-follow open (U4). |
| Dependency friction (Principle VI) | Both crates already transitive; platform-gated with justification comment (U1). |
| `cap-std` MSRV incompatibility or missing beneath-root semantics (U6) | **U7 proves feasibility test-first before U6 starts** (`cargo +1.75.0 check --all-targets` on the candidate pin + the five proof obligations: safe root-handle bootstrap, Dir/File conversion, intermediate-swap refusal, in-bounds leaf-reparse refusal, decided File boundary). If any obligation fails, U7 returns **BLOCKED** to Stage; the documented fallback (safe `rustix` `openat` walk + separate safe Windows design) requires its own evidence gate. No vague `unsafe`/path-based fallback. |
| `cap-std` adds a new crate family to the graph | Layers on the already-transitive `rustix`; before/after `cargo tree -d` in U1 proves no unexpected duplicate; `cargo audit` clean. |
| U7 feasibility gate slips or returns BLOCKED | U6–U5 are dependency-gated on the U7 evidence PASS and do not start; Principles III/IV remain **NOT-PASSED** and the shipment cannot claim intermediate-directory containment until U7 PASSES and U6 lands. |
| Intermediate-dir swap still open if U6 slips | U2–U5 are dependency-gated on U6 (itself gated on U7); Constitution III/IV completeness is claimed only with U7 PASS **and** U6 landed. |

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Safety-First Rust | PASS | Entirely safe std (`OpenOptionsExt::custom_flags`, `File::set_permissions`, `File::metadata`, Windows `access_mode`/`share_mode`); `#![forbid(unsafe_code)]` preserved; all new helpers return `Result` with no `.unwrap()`/`.expect()`; each code unit gates on `clippy::pedantic -D warnings`. |
| II. Test-First Development | PASS | U2/U3/U4 author NEW failing tests before implementation; U5 characterization; all existing tests re-run unchanged. |
| III. Workspace Isolation / IV. CLI Containment | **NOT-PASSED (provisional; gated on U7 PASS + U6)** | Full-path containment (intermediate directories, not only the final component) depends on U6's beneath-root directory-handle-relative walk, which in turn depends on the **U7 feasibility/evidence gate proving** — test-first, safe APIs only, Rust 1.75 — a safe atomic workspace-root directory-handle bootstrap (Unix `O_DIRECTORY\|O_NOFOLLOW` read; Windows `FILE_FLAG_BACKUP_SEMANTICS\|FILE_FLAG_OPEN_REPARSE_POINT` + attribute rejection + share/access flags), safe `cap_std::fs::Dir/File` conversion, intermediate symlink/junction swap refusal, and in-bounds leaf reparse/symlink refusal. Until U7 records PASS **and** U6 lands, III/IV are **NOT claimed complete**: final-component `O_NOFOLLOW`/`OPEN_REPARSE_POINT` (U2) alone leaves the parent-directory swap-after-`validate_path` race raised in PR #107 review **open**. Threat model (explicit): an attacker may write/swap **inside** the workspace root but not its trusted parent; the root handle is bootstrapped once from the trusted parent and retained for the `DataStore` lifetime. If U7 returns **BLOCKED**, these principles stay NOT-PASSED and no vague `unsafe`/path-based fallback is adopted. On the leaf, the explicit Windows reparse-attribute refusal adopts the **intentionally broader any-reparse-point fail-closed policy** (refuse ANY `FILE_ATTRIBUTE_REPARSE_POINT` entry — see U2 decision — because a precise name-surrogate test needs `unsafe` `DeviceIoControl` precluded at MSRV 1.75, and DB paths expect plain files); Unix `O_NOFOLLOW` refuses the leaf name-surrogate. No path-based `chmod` fallback; fail-closed on link/reparse/containment-escape/unobtainable handle. `#![forbid(unsafe_code)]` preserved (any `unsafe` FFI lives inside the proven-safe `cap-std`/`rustix`, contingent on U7). |
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
  mistake could block engine opens. Mitigated by fail-closed design and U5 validation.

**Requires plan hardening: yes**

## Runtime Verification and Closure

* **Changed runtime surface**: the read-only serve path (`open_engine_readonly` →
  `EngineReadonlyGuard`) and the write-mode open self-heal (`open_sqlite` →
  `clear_stale_readonly_lock`). No CLI/API signature change.
* **Runtime verification (Ship)**: run the full `cargo test` store suite on the host
  platform; where possible validate on Windows that a real read-only serve + subsequent
  write-mode open succeed (engine open not blocked by the retained handle), that a
  planted symlinked sidecar is refused on both open paths, and that a planted
  **intermediate-directory** symlink/junction (parent-dir swap) is refused by the U6
  beneath-root walk.
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
   the workspace; a linked/reparse entry **or an intermediate parent-directory swap**
   is refused, never followed (final-component no-follow + U6 beneath-root walk).
5. **`#![forbid(unsafe_code)]`** — zero `unsafe`.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

| ProposedAction | change_kind | targets | ActionRisk | rollback | approval | ActionResult |
|---|---|---|---|---|---|---|
| Add `libc` + `windows-sys` as platform-gated direct deps (U1) | config change | `Cargo.toml`, `Cargo.lock` | moderate | revert Cargo.toml/lock; both already transitive | not required (non-destructive, justified) | planned |
| Add `cap-std` as a direct dep for the beneath-root walk (U1/U6) | config change | `Cargo.toml`, `Cargo.lock` | moderate | revert Cargo.toml/lock; falls back to Unix-only `rustix` walk + separate Windows design | not required (non-destructive, MSRV-verified, justified) | planned |
| Resolve guarded entries relative to a retained workspace-root `Dir` handle (U6) | local code change | `src/db/store.rs` | high | revert to final-component-only `open_no_follow`; the intermediate-dir race re-opens (documented regression) | prefer approval if `cap-std` MSRV/Windows behavior forces the fallback design | planned |
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
  swap after `validate_path` refused by the beneath-root walk (U6)**; dangling-symlink
  fail-closed; stale-lock self-heal; exact-mode and non-empty-sidecar preservation;
  Windows retained handle does not block the engine db/WAL open (U5).
* **Blocked-path handling**: if the no-follow handle cannot be opened/retained, refuse
  the open (existing refusal error) — never mutate via a re-resolved path.
* **Rollback trigger**: any failing store test, a `cargo audit` advisory from U1, a
  Windows engine-open blocked by the retained handle, or a `cap-std` MSRV-1.75 /
  Windows-behavior incompatibility surfaced in U1/U6. **Rollback procedure**: for the
  Windows-open case, switch that platform to Option C (identity-verified re-open) and
  re-run U5; for a `cap-std` MSRV/Windows incompatibility, switch U6 to the documented
  Unix-only `rustix` `openat` walk plus the separate safe Windows design (recording the
  decision in the PR); for a broader failure, revert the guard/clear-stale changes (the
  functions are self-contained) while keeping U1's deps. **Owner**: Ship agent.
  **Validation window**: the Ship runtime-verification + CI pass for the release PR.
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
| A1 | P2 | Windows `FILE_ATTRIBUTE_REPARSE_POINT` breadth was ambiguous vs the narrower Unix `O_NOFOLLOW` name-surrogate class (symlink/junction). A blanket attribute check also refuses non-redirecting reparse points (OneDrive/dedup/HSM/WSL/app-alias). | **Resolved (U2/U4/U5).** Explicitly **adopted the broader fail-closed policy** — refuse ANY reparse-point entry — and justified it: a precise name-surrogate test needs the reparse tag via `unsafe` `DeviceIoControl(FSCTL_GET_REPARSE_POINT)` (precluded by `#![forbid(unsafe_code)]` at MSRV 1.75), and `FileType::is_symlink()` is path-based (re-introduces TOCTOU). Added U5 delta (e) regression for a legitimate non-redirecting reparse file (refused; skips unprivileged). Intentional Unix/Windows breadth asymmetry documented. |
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
| B2 | P2 | The U5 Windows non-name-surrogate reparse **integration fixture may always skip** on unprivileged CI, leaving the broader fail-closed policy unproven in normal CI. | **Resolved (U5 delta (f), NEW).** Requires a **deterministic normal-CI predicate unit test** — extract the reparse-bit refusal into a pure `should_refuse_reparse(file_attributes: u32) -> bool` and test it with **fabricated attribute-bit inputs** (no filesystem/privilege), so the broader policy is pinned on every CI run; the real reparse-file fixture (delta (e)) remains **optional integration coverage** with explicit executed/skipped reporting. `059.005-T` acceptance updated to match. Safe Rust 1.75 / `#![forbid(unsafe_code)]` preserved. |

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
| C3 | P2 | The Windows reparse policy test risked being **target-gated** (needing `windows-sys`), so it could not run deterministically on Linux CI. | **Resolved (U2/U5).** Defined a **target-independent literal bit constant `0x0000_0400`** and a pure predicate `should_refuse_reparse(u32)` compiled/tested on Linux CI; production Windows code MUST call that predicate; added a `#[cfg(windows)]` assertion that `0x0000_0400 == FILE_ATTRIBUTE_REPARSE_POINT`. `059.002-T`/`059.005-T` updated. |
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
| D1 | P2 | `should_refuse_reparse` was described as the decision point but nothing **structurally** prevented a decorative unused helper or a duplicated inline reparse-bit mask in the production Windows branch. | **Resolved (U2/U5).** The reparse-bit literal is now a **module-private** `const REPARSE_ATTR` (not `pub`/`pub(crate)`, referenced only inside the predicate); the predicate is the **single source of truth**; the production Windows refusal branch MUST call `should_refuse_reparse(file_attributes())`. Acceptance **structurally requires** the literal occurs exactly once (inside the predicate) and a `#[cfg(windows)]` test drives the production branch through the predicate, so the helper cannot be dead code and no inline mask can diverge. `059.002-T`/`059.005-T` updated. |
| D2 | P2 | U6 acceptance leaned only on U7's throwaway feasibility harness for MSRV evidence, so the **integrated** `src/db/store.rs` was never re-verified under Rust 1.75. | **Resolved (U6).** U6 acceptance now **reruns `cargo +1.75.0 check --all-targets` against the actual integrated `src/db/store.rs` `open_beneath` implementation** with the `cap-std` edge; U7 harness evidence is explicitly **necessary but not sufficient**. `059.006-T` updated. |
| D3 | P2 | The `cap_std::fs::File` vs `std::fs::File` boundary was worded as **already decided/inherited** in several places, understating that it is unproven until U7 records evidence. | **Resolved (U7/U6/U2 + deliberation).** The boundary is now marked **`PENDING U7 PASS`** everywhere it is referenced (U7 obligation 5, U7 acceptance, U6 mechanics/acceptance, U2 scope note, plan C4, deliberation); "decided"/"inherited"/"no late ambiguity" wording that implied it was already settled is removed. It is decided and recorded only on U7 PASS. |
| D4 | P3 | Plan-review **attempt 3**'s "all four findings / no unresolved P0/P1" statement could be mistaken for the final authority even though attempt 4 reopened the unit. | **Resolved.** Attempt 3 is now explicitly marked **superseded by attempt 4** (overclaim C1; III/IV `NOT-PASSED (provisional)`), pointing readers to attempt 4 for the authoritative status. |

**Findings summary:** P0 = 0, P1 = 0, P2 = 3 (D1–D3), P3 = 1 (D4) — all resolved
in-artifact this pass. No manifest/dependency/shipment change; `051-S` remains queued
with sequencing `050-S → 051-S → 049-S`. Honest Constitution status unchanged: **III/IV
NOT-PASSED (provisional; gated on U7 PASS + U6)**. Gate: **PASS** (advisory).

<!-- plan-review-attempt: 5 -->
<!-- plan-review-verdict: PASS -->
* PR #107 pass-3 final clarity fixes applied in-artifact and tasks: `should_refuse_reparse`
  made an enforceable single source of truth (module-private literal, no decorative helper /
  no duplicated inline mask), U6 MSRV re-verified on the integrated `src/db/store.rs`, the
  File-vs-std boundary marked `PENDING U7 PASS` everywhere, and attempt 3 marked superseded
  by attempt 4.
* Honest posture unchanged: Principles III/IV remain **NOT-PASSED (provisional)** until U7
  records PASS and U6 lands.
