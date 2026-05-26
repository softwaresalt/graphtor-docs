---
date: 2026-05-26
slug: 031-s-runtime-verification
shipment: 031-S
surface: cli
mode: manual
status: PASS
merge_commit: 295518da2bc131ec3c3a40915fe0282ea2e6f5ed
owner: copilot
---

# Runtime Verification — 031-S Source registry normalization and duplicate-intake preflight

## Verification Target

Verify the shipped config and CLI runtime surfaces for source registry normalization:

* multi-file `*.sources.yaml` discovery remains deterministic
* multi-file mode rejects sources missing an explicit `database` field
* duplicate local intake across different databases is blocked
* `sync --force` warns and continues instead of blocking
* workspace containment rejects local paths that escape the workspace root

## Preconditions

* PR `#60` merged at `295518da2bc131ec3c3a40915fe0282ea2e6f5ed`
* Shipment `031-S` scope:
  * `040-F`
  * `040.001-T`
  * `040.002-T`
  * `040.003-T`
  * `040.004-T`
  * `040.005-T`
  * `040.006-T`
* Closure verification ran from isolated worktree `tmp/post-merge-031-S`
  on branch `post-merge/040-source-registry-normalization`

## Commands Attempted

```text
cargo test --test sync_source_registry_test
cargo test multi_file_mode_rejects_source_without_database
cargo test detect_with_context_overlapping_local_globs_different_dbs_is_conflict
cargo test detect_with_context_local_path_escaping_workspace_returns_path_violation
```

## Expected Behavior

* discovery loads `*.sources.yaml` files in deterministic order
* multi-file configs fail fast when any source omits `database`
* overlapping local intake across different databases yields a duplicate report
* `sync` exits with code `2` without `--force`
* `sync --force` emits a warning and proceeds
* local source roots outside the workspace are rejected with a path violation

## Observed Behavior

All targeted runtime verification commands passed on the merged default-branch
code in the closure worktree:

* `cargo test --test sync_source_registry_test`
  * passed `sync_preflight_blocks_on_cross_db_duplicates_without_force`
  * passed `sync_force_flag_proceeds_past_cross_db_duplicates`
* `cargo test multi_file_mode_rejects_source_without_database`
  * passed `config::tests::multi_file_mode_rejects_source_without_database`
* `cargo test detect_with_context_overlapping_local_globs_different_dbs_is_conflict`
  * passed `config::validation::tests::detect_with_context_overlapping_local_globs_different_dbs_is_conflict`
* `cargo test detect_with_context_local_path_escaping_workspace_returns_path_violation`
  * passed `config::validation::tests::detect_with_context_local_path_escaping_workspace_returns_path_violation`

## Evidence

* the integration test confirms the default `sync` preflight blocks cross-database
  duplicate intake and that `--force` downgrades the block to a warning
* the config unit test confirms multi-file schema enforcement still requires
  explicit database names
* the validation unit test confirms duplicate overlap detection still catches
  conflicting local intake across database targets
* the path-violation unit test confirms workspace containment remains enforced

## Verdict

**PASS**

## Recommended Next Action

Carry this verification result into post-merge closure, publish the closure PR,
and stop at the operator approval gate for the closure merge.
