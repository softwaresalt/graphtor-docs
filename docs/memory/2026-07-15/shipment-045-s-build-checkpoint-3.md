---
title: "Shipment 045-S build checkpoint 3 — Phase 2 install/doctor family complete"
description: "Session memory checkpoint after P2-T3, P2-T1, P2-T2a, P2-T2b, P2-T4 (19 of 25 tasks done)"
date: "2026-07-15"
status: "current"
---

## Summary

Continuing shipment 045-S. This checkpoint follows
`shipment-045-s-build-checkpoint-2.md` (Phase 1 complete, 14/25). Since then,
completed 5 more Phase-2 tasks: P2-T3, P2-T1, P2-T2a, P2-T2b, P2-T4.
**19 of 25 implementation tasks done.** Remaining: P2-T5a, P2-T5b, P2-T5c,
P2-T6, P2-T7a, P2-T7b, then the full pipeline (quality gates, adversarial
review, PR lifecycle, CI, merge, post-merge closure).

## Branch / commit state

* Branch: `feat/045-s-consumption-first-graphtor`
* HEAD at commit `25d4f9b` ("feat(doctor): tolerate the minimal consumption
  layout (P2-T4)")
* Commits since last checkpoint, in order: `49cc1a0` (P2-T3 mcp writer),
  `202be4e` (P2-T1 install_minimal), `ff82d26` (P2-T2a --with-ingestion flag),
  `1082d10` (P2-T2b full-scaffold orchestration), `25d4f9b` (P2-T4 doctor).
* All quality gates green at every commit: fmt, clippy pedantic, full test
  suite (currently 338 lib + 137 bin + all integration tests), cargo audit
  (matches the 6-item baseline in `audit.toml` exactly).
* Carry-forward stash reconfirmed intact after every commit: `stash@{0}`
  resolves to exactly `0b694d9955d8ad6acfb4a9d6194874dd061933de`.

## Key implementation decisions this checkpoint

### P2-T3 (051.004-T) — shared `.mcp.json` writer

`src/workspace/mcp_config.rs` rewritten: `resolve_command()` implements the
binary-resolution ladder (absolute canonical path when
`.graphtor/bin/graphtor-docs[.exe]` exists, else bare `"graphtor-docs"` PATH
command). `managed_server_value()` now writes an `x-graphtor-managed: true`
provenance marker into every entry. `generate_mcp_config()` replaced the old
insert-and-overwrite with a locked four-way dispatch on the `graphtor-docs`
key: absent→insert, marked→refresh-in-place (no-op if identical),
unmarked-exact-legacy-shape→migrate-in-place (add marker), unmarked-other→
fail closed with `GraphtorError::Config`, file untouched. `write_json` is now
atomic (temp file + rename). **Important fix**: additively extended
`prune_managed_server`'s removal recognition to ALSO check the marker
(`is_managed_for_removal`), because an absolute Windows path uses backslashes
and the bare command has no path at all — neither matches the old
forward-slash `MANAGED_COMMAND_MARKER` substring check, which would have
silently broken write-then-remove round-trips. This is additive only; the
substring fallback is untouched so P2-T5b's own exact-match legacy-removal
scope remains fully intact and meaningful.

### P2-T1 (051.001-T) — consumption-first install default

Added `install_minimal()` in `src/workspace/install.rs` (creates ONLY
`.graphtor/`, no subdirs/binary/sources.yaml) as a SIBLING to the preserved
`install()` (unchanged, full scaffold). `cmd_install` in `main.rs` switched
its default call from `install()` to `install_minimal()`. **Real finding**:
this made `install()`/`InstallResult`/`add_gitignore_entry` genuinely
unreachable from any production call site (confirmed `upgrade()` only calls
`installed_binary_path()` directly, never `install()`) — temporarily
annotated with explicit, comment-documented `#[allow(dead_code)]` referencing
P2-T2a/P2-T2b as the tasks that restore reachability (both since removed, see
below). Strengthened `install()`'s own tests to assert `.created`/
`.binary_path` rather than just papering over the dead-code lint.

### P2-T2a (051.002-T) — `--with-ingestion` flag + routing only

Added `InstallArgs::with_ingestion: bool`. `cmd_install` branches: absent →
`install_minimal()` (P2-T1 default, unchanged); present → calls the preserved
`install()` directly (routing only, per this task's explicit narrow scope —
NO sources.yaml/gitignore/mcp-config orchestration yet, that's P2-T2b).
Updated the `Install` clap doc comment (rendered as `--help`) to describe the
new default + opt-in, replacing the stale "always creates the full scaffold"
claim; added a help-text assertion test using the `Cli::command()` +
`find_subcommand_mut("install")` + `render_long_help()` idiom (existing
precedent in `cli/mod.rs`). Removed the P2-T1 `#[allow(dead_code)]` on
`install()`/`InstallResult` since they're reachable again via this branch.

### P2-T2b (051.008-T) — full-path orchestration

Extracted the with-ingestion branch into a new `cmd_install_full()` helper
(also resolved a `clippy::too_many_lines` finding on `cmd_install` — chose
extraction over `#[allow(clippy::too_many_lines)]` for readability, though
the latter has precedent on `cmd_serve`). `cmd_install_full` now calls
`init_sources_yaml`, `add_gitignore_entry` (guarded by `!args.no_gitignore`),
and `generate_mcp_config`, in that order — guarded so the minimal default
never runs them. Because `install()` copies the binary BEFORE
`generate_mcp_config` runs, the shared P2-T3 writer's ladder naturally
produces the absolute-path + marker shape with zero extra plumbing. Removed
the P2-T1 `#[allow(dead_code)]` on `add_gitignore_entry`.

### P2-T4 (051.003-T) — doctor tolerates minimal layout

Added `detect_footprint()`/`WorkspaceFootprint` (Full/Minimal) to
`src/workspace/doctor.rs`: Full when ANY of the 5 ingestion subdirs exists
(conservative toward Full for partial/in-transition states), Minimal
otherwise. `check_subdirs`/`check_sources_yaml` and the inline binary/
database checks in `run_doctor` now downgrade "missing" (not "exists but
invalid") to `Severity::Pass` with an informational message when Minimal;
Full-layout behavior is byte-for-byte unchanged (verified via a dedicated
`doctor_on_full_layout_matches_pre_existing_behavior` test). A sources.yaml
that EXISTS but has invalid YAML still Fails regardless of footprint — only
the "absent" branch is downgraded.

## Carry-forward stash — reconfirmed intact (again)

`git stash list` still shows exactly one entry; `git rev-parse "stash@{0}"`
still resolves to `0b694d9955d8ad6acfb4a9d6194874dd061933de`. Untouched
throughout. Will remain so for the rest of this session.

## Next steps (6 implementation tasks remain, then the pipeline)

1. **P2-T6 (051.006-T)** — backward-compat detection + idempotency (likely
   builds on `detect_footprint`; re-running install against an existing full
   or minimal layout must stay additive-only and idempotent)
2. **P2-T5a (051.005-T)** — footprint-safe uninstall + approval-set
   enumeration. **STRICT-SAFETY FLAG**: this is classified `destructive` in
   the plan's Risky Actions table (PA-3) and requires EXPLICIT OPERATOR
   APPROVAL AT EXECUTION TIME per Constitution Principle VII — must pause
   and request approval when implementing/exercising the actual
   destructive-deletion path, not just note it in a doc. The core concern:
   `uninstall()` currently does `fs::remove_dir_all(&workspace_dir)`
   unconditionally, which would DELETE an operator-dropped `.db` file in a
   minimal/consumption workspace — this task must make deletion
   footprint-safe (delete only graphtor-created artifacts, never a
   user-dropped db).
3. **P2-T5b (051.009-T)** — managed `.mcp.json` entry removal by provenance
   marker (exact-match legacy predicate, tightening the substring fallback
   I added in P2-T3 without removing it)
4. **P2-T5c (051.010-T)** — minimal/full upgrade parity
5. **P2-T7a (051.007-T)** — consumption-first post-install message contract
   (locks the EXACT wording with its own message-assertion test; my P2-T1
   placeholder message ["installation complete." + drop-a-db hint] should be
   revisited/finalized here, not treated as already-locked)
6. **P2-T7b (051.011-T)** — separate ingestion-setup docs section

After all 25 tasks: final full quality gate pass, adversarial multi-model
review (3+ reviewers), implementation PR + Copilot review loop (GraphQL
botIds, patient poll/classify/fix/reply/resolve within cycle limits), CI fix
loop, P-014 defense-in-depth readiness query with full thread pagination,
merge via merge-commit only (pre-authorized per operator instruction),
post-merge closure (shipment-reconcile pre → safe-close → post, operational
closure, compound-refresh/compact-context, backlog comments/commit-tracking),
and the final completion report including the carry-forward stash handoff
for the next Stage session.

## Open decisions / reminders carried forward

* PA-3 (uninstall deletion, P2-T5a) requires explicit operator approval AT
  EXECUTION TIME — distinct from the operator's blanket pre-authorization for
  the 045-S implementation PR's merge. Must actually pause for this, not just
  log it.
* Closure-PR merge remains fail-closed per the operator's explicit
  instruction — prepare but do not merge without a separate approval signal.
* Do NOT touch `stash@{0}` under any circumstances; report handoff only.
* `backlogit_docs_lint` (MCP tool) throws a Windows path error; use the CLI
  `backlogit docs lint` instead (confirmed working). The whole repo
  (437 violations, pre-existing, unrelated to this shipment) fails the
  docline `source`/`doc_type` frontmatter schema — already flagged via
  `backlogit_append_comment` on 050.008-T; do not attempt to fix repo-wide.
