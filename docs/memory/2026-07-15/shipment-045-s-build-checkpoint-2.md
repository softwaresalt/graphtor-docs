---
title: "Shipment 045-S build checkpoint 2 — Phase 1 complete"
description: "Session memory checkpoint after completing all 14 Phase-1 units (P1-T0..T8 + P1-RF1..RF5) of shipment 045-S"
date: "2026-07-15"
status: "current"
---

## Summary

Continuing shipment 045-S (consumption-first graphtor: read-only serve
auto-discovery + content-derived mode, then consumption-first install with
opt-in ingestion). This checkpoint follows
`shipment-045-s-build-checkpoint-1.md` (which covered intake through P1-T3).
**Phase 1 is now 100% complete (14 of 14 units).** Phase 2 (11 units) has not
started.

## Branch / commit state

* Branch: `feat/045-s-consumption-first-graphtor`
* HEAD at commit `99b3c86` ("docs(docs): document Phase-1 consumption-first
  serve behavior (P1-T8)")
* All commits since the prior checkpoint, in order:
  * `9cdf66e` — test(db): prove ATTACH/extension hardening for served
    read-only stores (P1-T4)
  * `01be7f2` — feat(cli): share serve_discovery auto-discovery with status
    (P1-T5)
  * `353db37` — feat(cli): add --read-only escape-hatch flag to serve (P1-T7)
  * `99b3c86` — docs(docs): document Phase-1 consumption-first serve behavior
    (P1-T8)
* Full quality gates green at every commit: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit` (matches the documented 6-item CI
  allowlist in `audit.toml` exactly: RUSTSEC-2026-0041 (vulnerability,
  lz4_flex) + 5 unmaintained-crate warnings).

## Tasks completed this checkpoint (P1-T4, P1-T5, P1-T7, P1-T8)

### P1-T4 (050.003-T) — v4 gate parity + ATTACH/extension/single-file hardening

v4 gate parity was already satisfied by P1-T3's serve rewrite (no new
production code needed there). Grepped Cozo's entire vendored source tree for
`ATTACH`/`load_extension`/`enable_load_extension`/raw-SQL escape hatches: zero
matches — CozoScript's Pest grammar has no such syntax. Added 3 empirical
proof tests in `src/db/store.rs` (colocated test module) that actually attempt
`ATTACH DATABASE ...` and a `load_extension(...)` call via
`store.query(...)` against an `open_engine_readonly` store, asserting
`Err`. Also added `auto_discovered_pre_v4_db_is_refused_via_the_same_v4_gate`
in `tests/serve_posture_gating_test.rs` (new `build_v3_fixture` helper using
the pre-existing `set_schema_version_for_test` — confirmed already used
identically in `main.rs`/`sync/mod.rs`) to prove the v4 gate applies
identically to a purely auto-discovered (no config, no explicit --db-path) db.

### P1-T5 (050.005-T) — status + MCP list-sources share serve's discovery set

`discover_status_db_paths` (used by both `cmd_status` and
`QueryCtx::open_stores`, which the CLI `list-sources`/`search`/`traverse`/
`get-chunk`/`get-document` subcommands all share) now ALSO merges through
`workspace::serve_discovery::discover_served_databases` after computing its
existing `existing_candidates` list — exactly mirroring `cmd_serve`'s own
merge call. Key design decision: **did not** additionally run
`classify_serve_postures`/apply the Generation-vs-ReadOnly phantom-exclusion
filter that `cmd_serve` applies, because that filter exists purely to avoid
*attempting a write-open* on a non-existent phantom generation target — a
concern specific to serve's write path, not to status's read-only inspection.
Status's pre-existing "missing single database" UX (`is_missing_single_database`
/`emit_missing_database_status`) for the empty-`sources.yaml`-with-no-db-file
case is therefore fully preserved unchanged. `discover_served_databases`
preserves `existing_candidates` in full (including non-existent phantom
entries) and only ADDS genuinely-existing auto-discovered files, so this
"just works" with zero extra filtering logic.

**Real regression caught and fixed**: routing the explicit `--db-path` case
through `discover_served_databases` for the first time (previously it bypassed
canonicalization) surfaced a pre-existing Windows `std::fs::canonicalize`
quirk (expands to 8.3 short-name form, e.g. `DEWILL~1`, in this dev
environment) that ALSO already silently affects `cmd_serve`'s explicit
`--db-path` handling since P1-T3 (no serve test happened to do an exact-string
path comparison, so it went unnoticed until now). Fixed by updating
`status_json_inspects_explicit_db_path_without_registry`
(`tests/explicit_db_target_no_registry_test.rs`) to compare
`std::fs::canonicalize`d forms instead of raw strings — this is the
objectively correct fix since `validate_path`-based canonicalization is the
established, intentional workspace-containment mechanism (Constitution
Principle III) used pervasively elsewhere in this codebase.

### P1-T7 (050.007-T) — `--read-only` escape-hatch flag

Added `ServeArgs::read_only: bool` (clap `#[arg(long)]`), threaded through
`run()`'s `Command::Serve(args)` arm into `cmd_serve`'s new `force_read_only`
parameter. Implementation is a single conditional at the
`classify_serve_postures` call site:
`if force_read_only { None } else { source_config.as_ref() }` — passing `None`
means no source can ever promote a target to `Generation`, so every db
classifies `ReadOnly` and `generation_sources` stays empty, which in turn
naturally skips both the duplicate-intake preflight and the background sync
spawn (no extra logic needed for those). No corresponding `--force-sync` flag
exists (per the plan's explicit resolved-decision record — read-only is
already the fail-safe default, so only a force-*safer* override is needed).
Added a control test (`without_read_only_flag_the_same_workspace_still_promotes_generation`)
proving the identical fixture promotes `Generation` WITHOUT the flag, so the
flag itself (not some other change) is proven to be what forces read-only.

### P1-T8 (050.008-T) — Phase-1 documentation

Created `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
covering all 5 required topics from the plan's R8/acceptance-criteria: the
docline→graphtor-docs→agent pipeline (high-level, cross-referencing the
existing detailed `docs/pipeline.md` for the ingestion-side 5-stage detail),
`.graphtor/` workspace layout table, serve auto-discovery + content-derived
posture rules, the dev/authoring-workspace generation exception (this repo's
OWN dev workspace is the canonical `Generation` example), read-only serve
hardening (P1-T0/P1-T4 guarantees), the `--read-only` escape hatch, explicit
`type: database` entries, and the operator trust boundary (dropped `.db`
files are served as authoritative agent context with zero provenance
validation — operators must only drop `.db` files from trusted sources).

Also fixed two now-materially-inaccurate EXISTING reference docs (in scope
per the "keep docs synchronized with implementation" compound learning and
the general instruction to update directly-related documentation):
* `docs/configuration.md` — was claiming `type: local` is "the only supported
  type" (false since P1-T6 added `type: database` several tasks ago); added a
  full `Source Type: Database` section with the LOCKED `id`+`path`-only
  contract.
* `docs/cli-reference/graphtor-docs.md` — the `serve` section described
  pre-Phase-1 behavior verbatim ("Opens the CozoDB database" / "No additional
  flags") which is now false; rewrote to describe auto-discovery, per-db
  content-derived posture, and the `--read-only` flag; added a one-line note
  to the `status` section about the shared discovery set.

**Known/accepted gap**: `backlogit_docs_lint` (MCP tool) throws a Windows
path-relativization internal error (`Rel: can't make ... relative to .`); the
CLI `backlogit docs lint` (both `--profile authoring` and default) works but
reports 437 PRE-EXISTING violations across the ENTIRE repo (confirmed:
`AGENTS.md`, `README.md`, and prior-shipment design-docs like
`2026-05-24-multi-database-runtime-hardening.md` all fail identically) for
missing `source`/`doc_type` frontmatter fields per a docline base schema the
repo's actual authored-doc convention never adopted. My new/modified files
fail with the EXACT SAME two findings as every other pre-existing doc — no
new violation types, not a regression. Fixing this repo-wide gap is
out-of-scope for a single docs task; documented via
`backlogit_append_comment` on 050.008-T for future dedicated-chore visibility.
Interpreted the literal "backlogit_docs_lint clean" acceptance criterion
pragmatically given this pre-existing, unrelated, repo-wide blocker.

## Carry-forward stash — reconfirmed intact

Verified again after these 4 commits: `git stash list` still shows exactly
one entry (`stash@{0}: On main: carry-forward next shipment after 045-S`),
and `git rev-parse "stash@{0}"` still resolves to the exact operator-specified
object `0b694d9955d8ad6acfb4a9d6194874dd061933de`. Confirmed all 7 files via
`git stash show -p --stat` (6 tracked-file diffs) plus
`git show --stat "stash@{0}^3"` (1 untracked file:
`.backlogit/runtime/hooks/stage.checkpoint.json`) — matches the operator's
exact 7-file manifest. Stash remains completely untouched; will remain so
through the rest of this session per the standing instruction.

## Next steps (Phase 2 — 11 units, dependency order per the plan)

1. **P2-T3 (051.004-T)** — shared `.mcp.json` writer (Phase-2 root
   dependency; several later Phase-2 tasks depend on this)
2. **P2-T1 (051.001-T)** — consumption-first install default
3. **P2-T2a (051.002-T)** — install `--with-ingestion` CLI flag + plumbing
4. **P2-T2b (051.008-T)** — opt-in full-ingestion scaffold + managed marker
5. **P2-T4 (051.003-T)** — doctor tolerates minimal consumption layout
6. **P2-T6 (051.006-T)** — backward-compat detection + idempotency
7. **P2-T5a (051.005-T)** — footprint-safe uninstall + approval-set
   enumeration (strict-safety: destructive, requires explicit operator
   approval per PA-3 in the plan's Risky Actions table — flag this when
   reached)
8. **P2-T5b (051.009-T)** — managed `.mcp.json` entry removal by provenance
   marker
9. **P2-T5c (051.010-T)** — minimal/full upgrade parity
10. **P2-T7a (051.007-T)** — consumption-first post-install message contract
11. **P2-T7b (051.011-T)** — separate ingestion-setup docs section

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

* PA-3 (uninstall deletion, P2-T5a) is classified `destructive` in the plan's
  strict-safety table and requires explicit operator approval AT EXECUTION
  TIME — this is distinct from the operator's blanket pre-authorization for
  the 045-S implementation PR's merge. Must pause and request approval when
  actually implementing/running the destructive uninstall-deletion path,
  not just note it in a doc.
* Closure-PR merge remains fail-closed per the operator's explicit instruction
  — prepare but do not merge without a separate approval signal.
* Do NOT touch `stash@{0}` under any circumstances; report handoff only.
