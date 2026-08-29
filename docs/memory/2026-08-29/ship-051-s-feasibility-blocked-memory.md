---
title: "Ship 051-S execution memory — U7 PASS / U8 BLOCKED feasibility outcome"
date: "2026-08-29"
shipment: "051-S"
feature: "059-F"
agent: "Ship"
status: "blocked (feasibility gate)"
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

1. **U7 (`059.007-T`) — PASS.** Built an isolated, throwaway `cap-std`
   feasibility harness at `target/feasibility/u7-cap-std-harness/` (git-ignored,
   own `[workspace]` table to escape the root workspace, no `tempfile`
   dev-dependency to avoid an unrelated `getrandom 0.4.3` `edition2024` MSRV
   trap under the pinned toolchain). Proved, under both stable `rustc` and
   `rustc +1.75.0`:
   - Scenario 1: `cap-std 4.0.3` (newest stable release) compiles and its
     tests pass under Rust 1.75 (cap-primitives 4.0.3, rustix 1.1.4,
     io-lifetimes 2.0.4/3.0.1, io-extras 0.19.0, windows-sys 0.59.0/0.60.2/
     0.61.2 all resolve cleanly).
   - Scenario 2: `Dir::open_ambient_dir(root, ambient_authority())` bootstraps
     the workspace-root handle; `cap_std::fs::File::into_std()` is the
     selected `File` boundary (U2's `open_no_follow` keeps its existing
     `std::fs::File` return type regardless of ambient-path vs.
     capability-root-relative origin).
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
  intermediate-directory containment), explicitly documenting the
  intermediate-directory swap as an accepted residual risk.
- (c) File an upstream `cozo` feature request (or maintain a fork) for a
  capability-/handle-bound SQLite open.

## Open questions

- Should `059-F`'s scope be split so a leaf-only fix (option b above) can ship
  independently of the engine-boundary question? This is a Stage decision.
- Is the `cozo` maintainer likely to accept a capability-open feature request
  in a reasonable timeframe? Not investigated in this session.

## Next steps

1. Push this branch and open a PR carrying the backlog-state + memory-only
   diff for review.
2. Once reviewed, the operator/Stage decides whether to re-run `deliberate`
   for `059-F`'s engine-boundary question, split scope, or shelve the feature.
3. This shipment (`051-S`) remains `active` (not archived/closed) pending that
   decision — it was NOT force-closed as "done" because its covering feature
   did not reach its Definition of Done.
