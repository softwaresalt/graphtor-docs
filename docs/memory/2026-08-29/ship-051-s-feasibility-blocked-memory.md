---
title: "Ship 051-S execution memory — U7 PASS / U8 BLOCKED feasibility outcome"
date: "2026-08-29"
shipment: "051-S"
feature: "059-F"
agent: "Ship"
status: "blocked (feasibility gate)"
---

## Update — PR #111 current-HEAD review remediation (this pass, HEAD `58e4204` base)

Four review findings against the original checkpoint below were remediated
in this pass:

1. **P2 Constitution/Test-first** (`059.008-T` acceptance promised a
   compiled test-first evidence harness; only source-tracing was persisted):
   built and ran a compiled, test-first negative-proof harness (`trybuild`,
   isolated/throwaway under `target/`, deleted after capture) that proves
   `cozo::DbInstance::new`'s `path: impl AsRef<Path>` parameter structurally
   rejects a capability/handle object (`std::fs::File` fails to compile with
   `E0277`). Full test-first chronology (red: fixtures missing → green:
   fixtures authored, baseline passes, negative fixture fails to compile
   exactly as expected) and the scenario-by-scenario reconciliation are
   persisted in `.backlogit/queue/059.008-T.md`'s `feasibility-evidence`
   section.
2. **P3 traceability** (generic `blocked_reason` boilerplate didn't
   distinguish direct vs. transitive U8 dependents): corrected all 9 returned
   tasks' `blocked_reason` to accurately name the direct dependency
   (`059.001-T`/U1, `059.006-T`/U6, `059.009-T`/U9 each depend on `059.008-T`
   directly) versus the transitive path for the rest.
3. **P2 security advisory** (option (b) leaf-only narrowing under-specified):
   the Recommendation section below now requires a new Stage `deliberate`
   pass, an amended feature DoD/scope, and an explicit accepted-residual-risk
   record with compensating controls before option (b) may be started, and
   states plainly it is not a substitute for the original fail-closed DoD.
4. **P3 forward-progress**: the Next Steps section below now names the
   concrete downstream shipment (`049-S`) that is blocked by `051-S`
   remaining open, and states explicitly that `051-S` must not be closed,
   archived, or have its dependency dropped as a workaround.
5. **Additional correction (surfaced by ongoing Copilot shadow review, not
   one of the original 4 findings)**: Copilot correctly identified that
   U7's originally-recorded root bootstrap (`Dir::open_ambient_dir` alone)
   does not refuse a symlinked workspace root. Verified empirically and
   corrected: the root must be opened via `cap_primitives::fs::
   open_dir_nofollow` relative to an ambiently-opened parent, not via
   `Dir::open_ambient_dir` on the root path directly. U7 remains PASS with
   the corrected construction; full evidence is in
   `.backlogit/archive/059.007-T.md`.

---

## Task IDs touched

- `051-S` (shipment) — claimed, remains `active` (manifest narrowed to `059-F`,
  `059.007-T`, `059.008-T` after `return-blocked` detached the other 9 items).
- `059-F` (feature) — moved `queued` → `blocked` with a feature-level summary
  comment.
- `059.007-T` (U7, root/API/MSRV feasibility) — moved `queued` → `active` →
  `done`, PASS evidence recorded via `backlogit comment add`.
- `059.008-T` (U8, engine-open feasibility) — moved `queued` → `active` →
  `blocked`, BLOCKED evidence recorded via `backlogit comment add`.
- `059.001-T`, `059.002-T`, `059.003-T`, `059.004-T`, `059.005-T`, `059.006-T`,
  `059.009-T`, `059.010-T`, `059.011-T` — each returned blocked from the
  shipment via `backlogit shipment return-blocked --shipment 051-S --item
  <id> --reason "..."` (transitively gated on U8 BLOCKED per the approved
  plan dependency graph: `U1` never adopts `cap-std`/`libc`/`windows-sys`
  until both `U7` and `U8` PASS).

## Files modified

- `.backlogit/queue/051-S.md`, `.backlogit/queue/059-F.md`, and the 9 returned
  task files — status/comment updates only (backlog state, no source code).
- `.backlogit/archive/059.007-T.md` (new) — U7 archived on `done`.
- `.backlogit/hooks_queue.jsonl` — backlogit-managed event log.
- `docs/memory/2026-08-29/ship-051-s-feasibility-blocked-memory.md` (this
  file).
- **No files under `src/`, `Cargo.toml`, or `Cargo.lock` were changed.**
  `.gitignore`'s pre-existing operator modification (`+.backlogit/checkpoints/`,
  `+.backlogit/runtime/`) and `docs/scratch/` (untracked) were left untouched,
  unstaged, and byte-identical throughout.

## Decisions and rationale

1. **U7 (`059.007-T`) — PASS (corrected in this pass; see the Update section
   above).** Built an isolated, throwaway `cap-std` feasibility harness at
   `target/feasibility/u7-cap-std-harness/` (git-ignored, own `[workspace]`
   table to escape the root workspace, no `tempfile` dev-dependency to avoid
   an unrelated `getrandom 0.4.3` `edition2024` MSRV trap under the pinned
   toolchain). Proved, under both stable `rustc` and `rustc +1.75.0`:
   - Scenario 1: `cap-std 4.0.3` (newest stable release) compiles and its
     tests pass under Rust 1.75 (cap-primitives 4.0.3, rustix 1.1.4,
     io-lifetimes 2.0.4/3.0.1, io-extras 0.19.0, windows-sys 0.59.0/0.60.2/
     0.61.2 all resolve cleanly).
   - Scenario 2 (**corrected**): the originally-recorded bootstrap,
     `Dir::open_ambient_dir(root, ambient_authority())` alone, does **not**
     provide a no-follow/no-reparse root open — a Copilot review correctly
     flagged this, and it was confirmed empirically in a follow-up harness: a
     symlinked root was **followed**, not refused. The corrected
     construction ambiently opens the workspace root's **parent** only
     (`cap_primitives::fs::open_ambient_dir`), then opens the root itself
     relative to that parent handle via `cap_primitives::fs::
     open_dir_nofollow` (public in `cap-primitives` 4.0.3, but not exposed as
     a `cap_std::fs::Dir` method — both crates are needed as direct
     dependencies). Re-verified: a symlinked root is refused
     (`ERROR_STOPPED_ON_SYMLINK`/os error 681 on Windows); an ordinary root
     still succeeds. The trusted-parent threat model is now precise: only
     the immediate parent of the root is trusted (the same single
     ambient-authority step any `std::fs` ambient call already makes); the
     root itself is verified, not assumed. `cap_std::fs::File::into_std()`
     remains the selected `File` boundary. Full corrected evidence, including
     the explicit U1/U2/U6 implementation note that **both** `cap-std` and
     `cap-primitives` must be adopted, is in `.backlogit/archive/059.007-T.md`.
   - Scenario 3: table-driven refusal matrix — absolute path, `..` escape,
     intermediate-directory symlink swap, and in-bounds leaf symlink were all
     refused. The Windows leaf case required an **explicit post-open**
     `FILE_ATTRIBUTE_REPARSE_POINT` check via `MetadataExt::file_attributes()`
     (the open itself succeeds on a reparse point on Windows), confirming
     U2's planned `should_refuse_reparse` predicate is load-bearing, not
     optional.
   - Harness deleted after evidence capture (`target/` is git-ignored and the
     plan explicitly frames U7's harness as throwaway).

2. **U8 (`059.008-T`) — BLOCKED.** Traced the actual engine-open code path
   this crate depends on: `cozo` 0.7 (`storage-sqlite` feature) →
   `cozo-core/src/storage/sqlite.rs`. Two decisive facts, read directly from
   `cozo`'s source:
   - `new_cozo_sqlite(path)` (reached via `DbInstance::new("sqlite", path,
     options)`) calls `sqlite::Connection::open_thread_safe(&path)` — a bare
     path, and the `options` JSON argument is never forwarded to the
     underlying `sqlite3_open_v2` flags (matches the pre-existing comment in
     `src/db/store.rs::configure_sqlite_wal`: "Cozo's SQLite backend ignores
     the `options` string").
   - **`SqliteStorage::transact()` re-opens by path on every transaction**
     when its internal connection pool is empty (`self.pool.lock().unwrap()
     .pop() == None` → `Connection::open_thread_safe(&self.name)`, where
     `self.name` is the original `PathBuf` captured at construction). This
     means even a hypothetical one-time capability-bound open at
     `DbInstance::new` could not close the gap — every later pool-empty
     transaction re-resolves the bare path with zero capability involvement,
     for the entire lifetime of the `DataStore`.
   - The underlying `sqlite` crate (`stainless-steel/sqlite` 0.32) does expose
     `OpenFlags::with_uri()` (`SQLITE_OPEN_URI`), which is the only plausible
     hook for a same-identity trick (e.g. Linux `file:/proc/self/fd/N?...`),
     but `cozo` never sets it and gives callers no way to inject it. The
     `/proc/self/fd` trick has no Windows equivalent short of raw Win32
     `OpenFileById`/`FILE_ID_DESCRIPTOR` — unsafe FFI, incompatible with
     `#![forbid(unsafe_code)]` absent a vetted safe wrapper crate (none found).
   - **Conclusion**: no capability- or same-identity-bound SQLite/Cozo
     engine-open is reachable from safe Rust without forking `cozo` (out of
     scope; Constitution Principle VI). This is a hard, source-verified
     blocker, not a time-boxed guess.

3. **Downstream gating is the plan's own designed contingency, not a defect.**
   The approved plan (`docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md`)
   already states: *"Either BLOCKED result keeps Principles III/IV
   NOT-PASSED"* and *"BLOCKED keeps U1/U2/U6/U9/U2-U5/U10/U11 gated... no
   dependency or path-based fallback is added."* Ship's role boundary forbids
   re-triaging or re-grouping backlog structure (that is Stage's job), so the
   9 downstream tasks were returned blocked with an evidence-citing reason
   rather than re-sequenced or worked around.

4. **No PR was opened for a code change** — because none exists. `U1` (the
   only unit that would touch `Cargo.toml`) never runs while `U8` is
   BLOCKED, so `src/db/store.rs` and `Cargo.toml`/`Cargo.lock` are untouched.
   The committable artifact for this session is the backlog state transition
   (shipment/feature/task status + evidence comments) plus this memory
   checkpoint. A PR carrying only that traceability record is opened so the
   BLOCKED conclusion is reviewable before the shipment is considered
   resolved.

## Recommendation for Stage (next step, not performed by Ship)

Re-deliberate the engine-boundary approach before any further `059-F` work
resumes. Options surfaced for that deliberation:

- (a) Option C (identity-verified re-open) applied **per `transact()` call**,
  not just at initial open — since `cozo` reopens repeatedly, a one-shot
  Option A/C decision at construction time is insufficient by itself.
- (b) Narrow scope to a leaf-only no-follow fix (U2's primitives applied
  directly to `EngineReadonlyGuard`/`clear_stale_readonly_lock` without full
  intermediate-directory containment). **This option is NOT a drop-in
  substitute for the original plan and must NOT be presented as satisfying
  the original Constitution Principles III/IV fail-closed Definition of
  Done.** Choosing it requires, at minimum, all of the following before any
  implementation begins:
  1. a **new Stage `deliberate` pass** documenting why the narrower scope is
     acceptable, superseding or amending
     `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`;
  2. an **amended feature Definition of Done / scope** for `059-F` (or a
     replacement feature) that explicitly states Principles III/IV are met
     only for the leaf (final-component) threat and NOT for the
     intermediate-directory threat — the current DoD's fail-closed wording
     cannot simply carry over unchanged;
  3. an explicit **accepted-residual-risk record** naming the
     intermediate-directory symlink/junction swap as a known, accepted gap,
     with compensating controls (e.g., workspace-root permission hardening,
     monitoring/alerting on unexpected reparse points beneath the workspace,
     or a documented operational mitigation) — not silence;
  4. sign-off that this residual risk is acceptable for the threat model in
     `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
     (or successor) before implementation starts.
  Without all four, option (b) must not be started — it would silently
  narrow a security fix below its originally-approved bar.
- (c) File an upstream `cozo` feature request (or maintain a fork) for a
  capability-/handle-bound SQLite open.

## Open questions

- Should `059-F`'s scope be split so a leaf-only fix (option b above) can ship
  independently of the engine-boundary question? This is a Stage decision,
  and per option (b) above it requires a new deliberation pass, amended DoD,
  and an explicit residual-risk record before it may proceed — it is not a
  simple scope trim.
- Is the `cozo` maintainer likely to accept a capability-open feature request
  in a reasonable timeframe? Not investigated in this session.

## Next steps

1. Push this branch and open a PR carrying the backlog-state + memory-only
   diff for review.
2. Once reviewed, the operator/Stage decides whether to re-run `deliberate`
   for `059-F`'s engine-boundary question, split scope, or shelve the feature.
3. **This shipment (`051-S`) intentionally remains `active` (not archived,
   closed, or dependency-dropped) and, by design, BLOCKS downstream work
   until Stage re-deliberates and selects a safe replacement path.**
   Concretely, shipment `049-S` ("Fix MCP serve initialize-handshake
   regression") already declares a `blocks` dependency on `051-S`
   (`049-S → 051-S`) and cannot proceed while `051-S` remains unresolved.
   `051-S` must NOT be treated as safe to close, archive, or have its
   dependency silently dropped merely because its own feature work is
   blocked — that would incorrectly unblock `049-S` (and any other future
   dependent) without the underlying security question actually being
   resolved. It stays open, visibly blocked, until Stage's re-deliberation
   produces a decision per the Recommendation section above.
