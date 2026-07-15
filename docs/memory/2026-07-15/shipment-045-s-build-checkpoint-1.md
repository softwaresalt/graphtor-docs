---
type: session-memory
shipment_id: 045-S
branch: feat/045-s-consumption-first-graphtor
timestamp: 2026-07-15T15:50:00-07:00
phase: Phase 1 build loop (in progress)
---

# Shipment 045-S — Session Checkpoint 1 (mid-build)

## Scope

Shipment 045-S: "Consumption-first graphtor: read-only serve auto-discovery
+ minimal install" — 27 manifest items (050-F + 14 tasks, 051-F + 11 tasks),
25 executable tasks. Plan: `docs/exec-plans/2026-07-14-consumption-first-graphtor-plan.md`.

## Completed so far (5 of 25 tasks)

| Task | Backlog ID | Commit(s) | Status |
|---|---|---|---|
| Intake gate | — | e9ae6e7 (bundled w/ P1-T0 by staging mistake) | done |
| P1-T0 | 050.009-T | e9ae6e7 | done — engine-readonly proof PASSED |
| P1-T1 | 050.002-T | af64750 | done |
| P1-T2 | 050.004-T | a1f4362 | done |
| P1-T3 | 050.001-T | 8b200fe | done |

Backlog housekeeping commits: 2f6f2d3.

## Key outcome: P1-T0 feasibility gate PASSED

The hard root-gate (engine/filesystem-enforced read-only open for Cozo's
SQLite backend) is **feasible and proven**. Mechanism: `EngineReadonlyGuard`
in `src/db/store.rs` marks the db file + any existing `-wal`/`-shm`/
`-journal` sidecars filesystem-readonly before Cozo opens them. SQLite's
documented open-path behaviour silently retries a `SQLITE_OPEN_READWRITE`
request as read-only when the file cannot be opened for writing, so every
connection Cozo opens (initial + later pool refills) becomes genuinely
engine-enforced read-only — proven directly by calling
`cozo::DbInstance::run_script(..., ScriptMutability::Mutable)` and observing
a real engine-level rejection, bypassing our own `DataStore::mutate` guard
entirely. `open_engine_readonly()` is the new public constructor;
`open_sqlite()` self-heals a stale lock from a crashed session via
`clear_stale_readonly_lock()`. A transient `-shm`/`-wal` bookkeeping
artifact SQLite's WAL-reader machinery creates is cleaned up on `Drop` so
the workspace shows no persistent trace once the session ends. **No
shipment-blocking feasibility stop condition was triggered.**

## Design decisions made during build (not fully spelled out in the plan)

1. `serve_discovery::discover_served_databases` needed a DUAL-root
   parameter (`scan_root` for `.graphtor/` auto-discovery, `candidate_root`
   for existing candidates) because an explicit `--db-path` can legitimately
   live outside `.graphtor/` (see
   `tests/explicit_db_target_no_registry_test.rs`). Auto-discovery itself
   stays strictly scoped to `.graphtor/`.
2. Found and fixed a real pre-existing gap in `cmd_serve`: a true
   zero-config workspace (no `sources.yaml`, no `--db-path`, no `--config`)
   previously hard-errored with "config file not found" BEFORE
   auto-discovery ever ran — defeating the shipment's primary use case. Now
   falls through to `.graphtor/` auto-discovery when there's no explicit
   `--config` override.
3. Found and fixed a phantom-default gap: `discover_db_files` falls back to
   `base_db_path` when a resolved `sources.yaml` has zero sources (existing
   behaviour, unchanged, needed by `sync`). For `serve`, a `ReadOnly`
   candidate from that fallback that does NOT exist on disk is now excluded
   before `open_engine_readonly` is attempted (which requires the file to
   exist) — reports "no databases found to serve" instead of erroring.
4. `tracing_subscriber::fmt()` in this codebase does not call
   `.with_ansi(false)`, so `info!` logs are ANSI-colourised even when piped
   to a non-TTY subprocess. New integration tests that assert on `info!`
   log content must strip ANSI codes first (see `strip_ansi()` helper in
   `tests/serve_posture_gating_test.rs`). Plain `eprintln!` output is
   unaffected. Recorded as a repository-scope memory.

## Crate-structure facts recorded as memories (for future sessions)

- `src/cli/` and `src/workspace/` are BINARY-crate-only modules (`mod cli;`
  / `mod workspace;` in `main.rs`), not part of the `graphtor_core` library.
  Use `graphtor_core::...` imports there, and `cargo test --bin
  graphtor-docs` (not `--lib`) to run their tests.
- Commit scope convention: use `db`, `workspace` (not in the documented
  scope table in `commit-message.instructions.md`, but confirmed by
  extensive git history precedent).
- This backlogit deployment auto-archives an item the moment
  `backlogit_move_item(status="done")` is called — moves
  `.backlogit/queue/{id}.md` straight to `.backlogit/archive/{id}.md`. The
  `shipment-reconcile` skill's `pre-archived` classification already
  anticipates this.

## Process note (non-blocking)

A `git add` staging mistake bundled the P1-T0 source diff into a commit
titled `chore(harness): claim shipment 045-S...`. Code is correctly
committed; only the commit message doesn't fully describe its contents. Not
rewriting history per policy (branch may still be amended safely since
unpushed, but leaving as-is per "avoid history rewrites" discipline).

## Remaining work (20 of 25 tasks + full pipeline)

Dependency order still to implement: P1-RF1 → RF2 → RF3 → RF4 → RF5 → P1-T6
→ P1-T4 → P1-T5 → P1-T7 → P1-T8 → P2-T3 → P2-T1 → P2-T2a → P2-T2b → P2-T4 →
P2-T6 → P2-T5a → P2-T5b → P2-T5c → P2-T7a → P2-T7b.

Then: full quality gate pass, adversarial multi-model review (3+
reviewers), PR creation + Copilot hosted review loop, CI fix loop, P-014
gate, merge, post-merge closure (shipment-reconcile safe-close, operational
closure, docs, compound-refresh, compact-context), and the carry-forward
git-stash handoff report (stash@{0} / 0b694d99, NOT to be touched — Ship
role boundary forbids creating a new stash/intake item for it; will report
exact handoff instead per operator's own fallback instruction).

## Quality gate status (as of this checkpoint)

All 4 gates green after every completed task: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings -D clippy::pedantic`, `cargo test
--all-targets` (full suite, ~500+ tests across lib+bin+all integration
files), `cargo audit` (pre-existing suppressed advisories only, matching
`.github/workflows/ci.yml`'s documented allowlist).

## Next step

Implement P1-RF1 (050.010-T): `Source::as_local()`/`is_ingestible()`
accessors + widen `id()` to `pub`, refactor 3 sites in
`src/config/validation.rs` + 3 colocated tests in `src/config/source.rs`.
