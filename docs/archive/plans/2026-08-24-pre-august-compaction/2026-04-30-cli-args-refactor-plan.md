---
title: "CLI Args Refactor: Naming, Lock Semantics, and Data Root Placement"
task_id: 014.005-T
feature_id: 014-F
shipment_id: 006-S
created: 2026-04-30
status: reviewed
---

## Problem Frame

The CLI arguments defined in `src/cli/mod.rs` and consumed in `src/main.rs` have four interrelated design issues identified during the 005-S Copilot PR review:

1. **Naming mismatch**: The `--data-dir` flag (field `data_dir`) is documented as pointing to a database *file* (`GRAPHTOR_DB_PATH` env var, value_name `FILE`), but the field name implies a directory.
2. **`data_root` placement**: `src/main.rs:139` derives `data_root` as `db_path.parent()`, meaning Git clones are placed in the same directory as the database file (`.graphtor/`). This is unexpected — clones should go in `.graphtor/data/` or a dedicated directory.
3. **Concurrent fresh install race**: `cmd_install()` (main.rs:280-287) only acquires the workspace lock if `.graphtor/` already exists. Two concurrent `install` calls can both see the directory as absent and proceed to create it.
4. **`--force` double-duty**: `cmd_upgrade()` (main.rs:375) passes `args.force` to `WorkspaceLock::acquire()`, so `--force` both force-upgrades *and* force-unlocks — two distinct semantic operations conflated into one flag.

## Requirements Trace

| Requirement | Source | Implementation Action |
|---|---|---|
| Fix naming: `data_dir` → `db_path` | Copilot review comment on cli/mod.rs:47 | Rename field, update env var docs, update all consumers |
| Separate data root from DB path | Copilot review comment on main.rs:141 | Add explicit `--data-root` flag or derive from workspace dir |
| Concurrent install safety | Copilot review comment on main.rs:287 | Always acquire lock (create .graphtor dir first, then lock, then scaffold) |
| Separate force-upgrade from force-unlock | Copilot review comment on main.rs:376 | Add `--force-unlock` or separate concerns |

## Implementation Units

### Unit 1: Rename `data_dir` → `db_path` and fix documentation

**Changes:**
- `src/cli/mod.rs`: Rename field `data_dir` to `db_path`, keep `--data-dir` as deprecated alias via `#[arg(alias = "data-dir")]`, primary long name becomes `--db-path`
- `src/main.rs:76-79`: Update field access from `cli.data_dir` to `cli.db_path`
- Update doc comment to clarify it points to the database file

**Files:** `src/cli/mod.rs`, `src/main.rs`  
**Tests:** Existing binary tests (CLI parsing) should still pass; add a test for `--db-path` and `--data-dir` alias  
**Posture:** Test-first — write a CLI parsing test for the new flag name, verify it fails, then rename

### Unit 2: Fix `data_root` derivation

**Changes:**
- `src/main.rs:139`: Change `data_root` derivation from `db_path.parent()` to `workspace_dir.join("data")` (the standard `.graphtor/data/` subdirectory already created by `install`)
- Add a new `--data-root` optional flag to `SyncArgs` for override (not global — only sync needs it)
- When `--data-root` is provided, use it; otherwise default to `.graphtor/data/`

**Files:** `src/cli/mod.rs` (add `SyncArgs::data_root`), `src/main.rs` (update derivation)  
**Tests:** Add unit test that verifies default `data_root` resolves to `.graphtor/data/` not `.graphtor/`  
**Posture:** Test-first

### Unit 3: Concurrent fresh install safety

**Changes:**
- `src/main.rs:277-287`: Restructure `cmd_install()` to:
  1. Create `.graphtor/` directory (idempotent mkdir)
  2. Immediately acquire workspace lock
  3. Proceed with scaffold creation
- This ensures two concurrent installs both try to lock, and one wins

**Files:** `src/main.rs`  
**Tests:** Existing `workspace::install::tests::install_is_idempotent` covers the happy path; add a test that two sequential lock acquisitions on a fresh dir work correctly (second blocks/fails)  
**Posture:** Characterization-first — verify current behavior, then fix

### Unit 4: Separate `--force` from `--force-unlock` in upgrade

**Changes:**
- `src/cli/mod.rs`: Add `--force-unlock` flag to `UpgradeArgs` (and `UninstallArgs` for consistency)
- `src/main.rs:375`: Pass `args.force_unlock` (not `args.force`) to `WorkspaceLock::acquire()`
- `args.force` remains for the upgrade-specific "replace even if same version" semantics
- Update `InstallArgs` to also gain `--force-unlock` for symmetry

**Files:** `src/cli/mod.rs`, `src/main.rs`  
**Tests:** Add CLI parsing tests verifying `--force` and `--force-unlock` are independent  
**Posture:** Test-first

## Dependency Graph

```
Unit 1 (rename db_path) ─── independent
Unit 2 (data_root)      ─── depends on Unit 1 (uses the renamed field)
Unit 3 (install lock)   ─── independent
Unit 4 (force-unlock)   ─── independent
```

Recommended execution order: Unit 1 → Unit 2, then Unit 3 and Unit 4 (parallel-safe).

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Keep `--data-dir` as deprecated alias | Avoid breaking existing scripts/configs that use the old flag name |
| Default `data_root` to `.graphtor/data/` not cwd | The `data/` subdirectory already exists from `install`; keeping clones inside `.graphtor/` maintains workspace containment |
| Add `--data-root` to `SyncArgs` not globally | Only the sync command uses `data_root` for clone placement; other commands don't need it |
| Separate `--force-unlock` flag | Principle of least surprise — upgrading the binary shouldn't implicitly break someone else's lock |
| Create `.graphtor/` before locking in install | Minimal change that closes the race window; the directory creation itself is idempotent (mkdir with exist_ok) |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Breaking change for `--data-dir` users | Deprecated alias preserves backward compat; emit deprecation warning on stderr |
| Tests may depend on old field name | Search all tests for `data_dir` references before renaming |
| `--force-unlock` proliferation across commands | Only add to commands that actually acquire locks (upgrade, uninstall, install) |
| `data_root` change affects existing synced repos | Repos already cloned to old location won't be found at new path; sync will re-clone. Document in release notes. |

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | **YES** | CLI flag renaming changes the user-facing interface |
| Security, auth, permission, compliance | No | No security implications |
| Migration, backfill, destructive data/config | **Partial** | Existing clones at old data_root location may need manual move or re-sync |
| External integration, operator checkpoint | No | No external dependencies |
| High runtime, rollout, or rollback risk | No | Backward-compat alias mitigates rollout risk |

**Requires plan hardening: yes** — the CLI flag rename is a public interface change requiring deprecation handling and backward compatibility.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification | Closure |
|---|---|---|---|
| Unit 1 | CLI help text, env var | `graphtor-docs --help` shows `--db-path`; `--data-dir` still works with deprecation warning | Release notes mention rename |
| Unit 2 | Sync clone placement | `graphtor-docs sync` places clones in `.graphtor/data/` | Document data_root change |
| Unit 3 | Install concurrency | Two rapid `graphtor-docs install` calls don't corrupt workspace | N/A (internal robustness) |
| Unit 4 | CLI help text | `graphtor-docs upgrade --help` shows both `--force` and `--force-unlock` | Release notes mention new flag |

## Plan Hardening

### Hardening Required: Yes

**Risk triggers:**
1. CLI flag rename (`--data-dir` → `--db-path`) changes the public user-facing contract
2. `data_root` default change causes existing clones to be "orphaned" at the old location

**Protected invariants:**
- Existing `GRAPHTOR_DB_PATH` env var MUST continue to work without modification
- Existing `--data-dir` flag MUST continue to be accepted (deprecated, not removed)
- Existing database files MUST remain accessible at their current path
- The system MUST NOT delete or move existing cloned repositories

### Learnings and Instructions Consulted

- No directly relevant compound learnings for CLI deprecation patterns
- `keep-docs-synchronized-with-implementation.md` — reminder to update all docs referencing the old flag name
- Rust `clap` docs: `#[arg(alias = "...")]` provides transparent backward compatibility without warning; for deprecation warnings, manual detection in `run()` is needed

### Proposed Actions and Risk Classification

| ProposedAction | ActionRisk | Approval Needed? |
|---|---|---|
| Rename `--data-dir` to `--db-path` with alias | LOW — alias preserves backward compat | No |
| Change `data_root` default from `db_path.parent()` to `.graphtor/data/` | MEDIUM — orphans existing clones | No (re-sync is non-destructive) |
| Add `--force-unlock` flag to upgrade/uninstall | LOW — purely additive | No |
| Restructure install to lock before scaffold | LOW — internal robustness, no user-visible change | No |

### Deepened Verification

**Unit 1 (flag rename):**
- Pre-check: grep entire codebase for `data_dir` field references (tests, docs, configs)
- Test: `graphtor-docs --db-path ./test.db sync --help` parses correctly
- Test: `graphtor-docs --data-dir ./test.db sync --help` still works (alias)
- Deprecation warning: when `--data-dir` is used, emit `eprintln!("warning: --data-dir is deprecated; use --db-path")` before proceeding
- Verify `GRAPHTOR_DB_PATH` env var still maps to the renamed field

**Unit 2 (data_root):**
- Pre-check: if `.graphtor/` contains cloned repos at the top level (not in `data/`), warn the user during sync that repos should be moved
- Test: fresh `install` + `sync` places clones in `.graphtor/data/{source}/`
- Test: `--data-root /custom/path` overrides the default
- Blocked-path: if `.graphtor/data/` doesn't exist (older install), create it before placing clones

**Unit 3 (install lock):**
- Test: `cmd_install` on a path where `.graphtor/` doesn't exist succeeds and creates both dir and lock
- Test: second concurrent `cmd_install` on same path either blocks or fails gracefully
- Rollback: if scaffold creation fails after lock, release lock on error (already handled by Drop)

**Unit 4 (force-unlock):**
- Test: `--force` alone does NOT override the lock
- Test: `--force-unlock` alone overrides the lock but does NOT force-upgrade
- Test: `--force --force-unlock` does both

### Rollback Procedure

All changes are backward-compatible. Rollback path:
1. Revert the commit (git revert) — old `--data-dir` name returns, old `data_root` derivation returns
2. Users who adopted `--db-path` would need to switch back (unlikely in the short window)
3. No data migration needed — rollback doesn't move files

### Monitoring and Validation Window

- After merge: verify CI green, `cargo test` covers all new flag combinations
- First 7 days: watch for issues with existing MCP config files that reference `GRAPHTOR_DB_PATH` (should be unaffected since env var name is unchanged)
- Deprecation removal: `--data-dir` alias should remain for at least 2 minor versions before removal

### Unresolved Operator Decisions

None — all decisions are self-contained and backward-compatible. No operator checkpoint required before execution.

## Plan Review

**Gate Decision: PASS**

Reviewed by: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor, Learnings Researcher  
Date: 2026-04-30

### Gate Rationale

The plan is well-structured with clear implementation units, proper dependency sequencing, hardening signals correctly identified, and rollback procedure documented. No P0 or P1 findings. Plan hardening is present and satisfies the identified risk triggers. The plan may proceed to harvest.

### Findings

#### P2 — Moderate Gaps (record as backlog follow-up)

**P2-1: Deprecation warning implementation detail missing (Rust Reviewer)**

The plan states "emit deprecation warning on stderr when `--data-dir` is used" but clap's `alias` attribute doesn't expose which alias was used at runtime. The implementation will need manual detection logic — either by checking `std::env::args()` for `--data-dir` or by using clap's `ArgMatches` API to detect the alias.

*Recommendation:* Add a note in Unit 1 that the deprecation warning requires `ArgMatches` inspection or raw args scanning, not just the `alias` attribute.

**P2-2: Unit 2 data_root scope interaction with `cmd_serve` (Scope Boundary Auditor)**

The plan addresses `data_root` in `cmd_sync` but doesn't mention `cmd_serve` (main.rs:88). The serve command also opens the database — does it also need awareness of the new data_root? Currently `cmd_serve` only receives `db_path` and `cwd`, so it's likely fine, but the plan should explicitly state that serve is out of scope.

*Recommendation:* Add a sentence to Unit 2 noting that `cmd_serve` doesn't use `data_root` and is unaffected.

**P2-3: `--force-unlock` discoverability concern (Scope Boundary Auditor)**

Adding `--force-unlock` to three commands (install, upgrade, uninstall) increases CLI surface area. Users may not discover the flag when they need it because the common error path (lock conflict) currently tells them to use `--force`.

*Recommendation:* Update the lock conflict error message (lock.rs:64-68) to reference `--force-unlock` instead of `--force` as part of Unit 4.

#### P3 — Advisory (minor improvements)

**P3-1: Consider `#[arg(hide = true)]` for deprecated alias (Rust Reviewer)**

Using `#[arg(alias = "data-dir")]` shows the alias in help text. If the intent is deprecation, consider `#[arg(alias = "data-dir", hide = true)]` to hide the old name from `--help` while still accepting it. This signals users toward the new name.

**P3-2: Compound learning opportunity (Learnings Researcher)**

No prior learnings conflict with this plan. After implementation, consider recording a compound learning for "clap CLI flag deprecation pattern in Rust" — the alias + manual deprecation warning approach would be reusable.

### Plan Hardening Assessment

Plan hardening is present and adequate:
- ✅ Risk triggers identified (CLI rename, data_root change)
- ✅ Protected invariants stated
- ✅ ProposedAction / ActionRisk classification present
- ✅ Rollback procedure documented
- ✅ Monitoring and validation window defined

### Constitution Compliance

- ✅ All units satisfy 2-hour rule (2 files, ≤4 functions, ≤3 test scenarios each)
- ✅ Width isolation maintained (all units are code changes, not docs + code mixed)
- ✅ Test-first posture specified for 3 of 4 units
- ✅ Error handling uses existing `GraphtorError` patterns
- ✅ No `unsafe` code introduced
- ✅ No new dependencies required
