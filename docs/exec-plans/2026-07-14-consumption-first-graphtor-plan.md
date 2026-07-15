---
title: "Consumption-First Graphtor Context Server — Implementation Plan"
description: "Phased plan to make serve read-only + zero-config by default (Phase 1) and install consumption-first with opt-in ingestion (Phase 2)."
source: "docs/decisions/2026-07-14-consumption-first-graphtor-deliberation.md"
stash_ids:
  - "79B5A7BC"
  - "B333B9B8"
phases:
  - "Phase 1 — 79B5A7BC: serve read-only auto-discovery + content-derived mode"
  - "Phase 2 — B333B9B8: consumption-first install + opt-in ingestion"
tags:
  - "serve"
  - "install"
  - "read-only"
  - "mode-detection"
---

## Problem Frame

graphtor must operate in two roles, but the code treats every workspace as a
potential sync engine. In technical terms:

* `serve` (`cmd_serve`, `src/main.rs:2383`) resolves databases via
  `discover_db_files` (`src/main.rs:244`), which only reads `config.sources` or
  a single explicit `--db-path`. It never scans `.graphtor/` for dropped `.db`
  files. `open_serve_databases` (`src/main.rs:2344`) opens each database
  read-write AND read-only and `cmd_serve` spawns `spawn_background_sync`
  (a WRITE path) whenever `sources.yaml` has sources (`src/main.rs:2447`).
* `install()` (`src/workspace/install.rs:34`) always scaffolds
  `.graphtor/{bin,data,cache,config,logs}` (`GRAPHTOR_SUBDIRS`,
  `src/workspace/paths.rs:17`) and `cmd_install` (`src/main.rs:3002`) writes a
  template `sources.yaml` and an ingestion-first post-install message.

Target: read-only serve with zero-config auto-discovery is the DEFAULT; the
write/sync posture is enabled only when the workspace has resolvable real
generation sources (the dev workspace being the primary such case). Install
becomes consumption-first with an opt-in ingestion scaffold.

This plan implements the locked decisions from the deliberation: content-derived
mode (fail read-only), serve-discovery Option C (auto-discovery + optional
explicit entries), and install Option I1 (consumption-first default + opt-in
ingestion).

## Requirements Trace

| # | Requirement (from decision) | Implementation action | Phase |
|---|---|---|---|
| R1 | `serve` auto-discovers `*.db` in `.graphtor/` root | Add a root-scan discovery step feeding `discover_db_files` | P1 / T1 |
| R2 | Mode is content-derived; stale/empty `sources.yaml` never enables sync | Add resolvable-source classification; default read-only | P1 / T2 |
| R3 | No-real-source dbs are served read-only and never background-synced | Gate rw-store open + `spawn_background_sync` on resolved sources | P1 / T3 |
| R4 | v4 pre-sync gate applies to auto-discovered read-only dbs | Reuse `needs_v4_migration` gate for discovered dbs | P1 / T4 |
| R5 | `status` + MCP list-sources span multiple discovered dbs | Synthesize source metadata from stored `doc_sources` | P1 / T5 |
| R6 | Optional explicit read-only db entry in `sources.yaml` | Parse a `read_only`/database entry kind | P1 / T6 |
| R7 | Optional `--read-only` override escape hatch | Add serve CLI flag forcing consumption posture | P1 / T7 |
| R8 | Docs: pipeline + dev-workspace exception + read-only serve | Phase-1 docs in product-specs/design-docs | P1 / T8 |
| R9 | `install` default creates only `.graphtor/` root + minimal serve `.mcp.json` | Consumption-first install path | P2 / T1 |
| R10 | Ingestion scaffold is opt-in (`install --with-ingestion`) | New flag creates full layout + binary + `sources.yaml` | P2 / T2 |
| R11 | Binary resolution: PATH command default, `.graphtor/bin` when scaffolded | Precedence in `managed_server_value` caller | P2 / T3 |
| R12 | `doctor` tolerates the minimal consumption layout | Make doctor checks layout-aware | P2 / T4 |
| R13 | uninstall/upgrade parity for both footprints (never delete user dbs) | Update `uninstall()` / `upgrade()` + tests | P2 / T5 |
| R14 | Backward compat for existing full installs + idempotency | Detect existing layout; additive-only opt-in | P2 / T6 |
| R15 | Post-install message + separate ingestion-setup docs | Consumption-first message + ingestion doc section | P2 / T7 |

## Implementation Units

Each unit follows the 2-hour rule (< 3 files, < 5 functions, < 4 test
scenarios), width isolation (single domain), and produces a verifiable outcome.

### Phase 1 — 79B5A7BC (covering feature: read-only serve auto-discovery + content-derived mode)

> **Review-driven architecture note (applies to all Phase-1 code units):** the
> serve/status discovery + posture logic lives in a NEW cohesive module
> `src/workspace/serve_discovery.rs` (library code: `Result<_, GraphtorError>`,
> no `unwrap`/`expect`, `thiserror`). It owns: (a) the `.graphtor/` root scan +
> filter + containment, (b) resolvable-source classification, (c) explicit-entry
> merge, and (d) canonical-path dedup. It MUST NOT modify the shared
> `discover_db_files` (`src/main.rs:244`) or `split_plan_by_database`
> (`src/main.rs:273`), which feed the SYNC/write path — injecting discovery
> there would pull dropped consumption dbs into background sync (the INV-1
> hazard). Posture is **per-db** (`ServeMode::ReadOnly | Generation`), not
> per-workspace, so a workspace may hold both source-backed and dropped
> read-only dbs.

**P1-T1 — Serve/status-scoped `.graphtor/*.db` discovery with containment** (code, test-first)
* Changes: add a serve/status-scoped discovery helper in the new
  `serve_discovery` module that scans ONLY the `.graphtor/` root for `*.db`.
  Containment mechanism: canonicalize the root and each candidate, normalize the
  Windows `\\?\` UNC prefix, assert the canonical candidate is prefixed by the
  canonical root, and reject `..`, POSIX symlinks, AND Windows junctions/reparse
  points. Skip-list (locked): `*.lock`, index/tmp files, `.graphtor/models`, and
  generated artifacts; served set = `*.db` by extension minus skip-list. Do NOT
  touch `discover_db_files`/`split_plan_by_database`.
* Files: `src/workspace/serve_discovery.rs` (new), `src/workspace/mod.rs` (wire).
* Tests: discovers a dropped `.db`; skips `*.lock`/models/non-`.db`; rejects
  `..`/symlink/junction escape (Windows junction case).
* Posture: test-first.

**P1-T2 — Content-derived posture classification (three-way, fail-safe)** (code, test-first)
* Changes: in `serve_discovery`, classify each candidate db into a per-db
  `ServeMode` using a THREE-WAY outcome that runs BEFORE the write-path
  `run_duplicate_intake_preflight` (`src/main.rs:2398-2406`): (1) malformed/
  unparseable `sources.yaml` → fail-closed hard `Err(GraphtorError)` (unchanged
  behaviour); (2) a `local` source whose path exists AND has ≥1 ingestible file
  → `Generation`; (3) absent/empty/stale/unresolvable sources → `ReadOnly`. The
  preflight then runs only for `Generation` dbs. Expose a PURE classification
  function returning `Vec<(PathBuf, ServeMode)>` for unit-testability.
* Files: `src/workspace/serve_discovery.rs`, `src/main.rs` (call ordering in
  `cmd_serve` so classification precedes the preflight).
* Tests: real non-empty source → Generation; existing-but-empty path → ReadOnly;
  stale `sources.yaml`, paths absent → ReadOnly; malformed yaml → hard error.
* Posture: test-first. Depends on P1-T1.

**P1-T3 — Per-db posture threaded through serve open + sync gating** (code, characterization-first)
* Changes: change `open_serve_databases` (`src/main.rs:2344`) to accept the
  `Vec<(PathBuf, ServeMode)>` classification instead of a bare `Vec<PathBuf>`.
  For `ReadOnly` dbs: skip `DataStore::open_sqlite` (rw) and `ensure_schema()`
  (a write), skip the exclusive `acquire_database_lock` write lock (use a shared/
  read lock or none), and open ONLY the read-only store. For `Generation` dbs:
  keep the current rw+lock path. `ServeOpenedDatabases` carries per-db posture so
  `spawn_background_sync` receives ONLY `Generation` rw stores. Handle the
  zero-discovered-db case explicitly (clear "no databases found to serve" exit,
  no `graph.db` write-fallback, replace the `unreachable!` at ~2490 with a
  handled error).
* Files: `src/main.rs` (`open_serve_databases`, `cmd_serve`).
* Tests: consumption db → no rw store, no write lock, no sync spawn; MIXED
  workspace (real-source + dropped db) → source-backed gets rw+sync while
  co-resident dropped db stays read-only and RO read paths work; empty
  `.graphtor/` → graceful "nothing to serve".
* Posture: characterization-first (lock current dev + mixed behaviour, then gate).
  Depends on P1-T2.

**P1-T4 — v4 pre-sync gate parity via read-only store** (code, test-first)
* Changes: evaluate `needs_v4_migration` on the READ-ONLY store for discovered
  read-only dbs (no write transaction), keeping the existing refusal message
  (`open_serve_databases:2363`). Ensure the read-only open for auto-discovered
  (untrusted) dbs is hardened: disable loadable extensions and disallow `ATTACH`,
  constraining the open to the single file. Sequence AFTER P1-T3 (same function,
  shared change surface).
* Files: `src/main.rs` (`open_serve_databases`), read-only open helper.
* Tests: pre-v4 discovered db → refusal exit + message; v4 db → served; crafted
  db attempting `ATTACH`/extension-load → refused/inert.
* Posture: test-first. **Precondition**: confirm a pre-v4 (v3) fixture db or a
  programmatic v3-schema builder exists; if absent, add a v3 fixture builder as
  the first step of this unit. Depends on P1-T3.

**P1-T5 — Expose discovered read-only dbs via status + MCP list-sources** (code, test-first)
* Changes: make `status` (`discover_status_db_paths` → currently
  `discover_db_files:2525`) resolve the SAME shared `serve_discovery` set as
  `serve`, so `status` and `serve` report an identical db set. `status` and the
  MCP list-sources tool report all discovered read-only dbs, synthesizing source
  metadata from each db's stored `doc_sources`. Preserve the existing JSON-RPC/
  MCP output contract on both surfaces.
* Files: `src/main.rs` (status path), MCP tool surface module.
* Tests: multi-db discovery reflected identically in status and list-sources;
  metadata synthesized from stored `doc_sources`.
* Posture: test-first. **Fixtures**: add a temp-sqlite v4 fixture builder with
  populated `doc_sources`. Depends on P1-T1.

**P1-T6 — Optional explicit read-only db entry in `sources.yaml`** (code, test-first)
* Changes: support an explicit read-only database entry (e.g. `read_only: true`
  or a `type: database` kind) for named/aliased/external dbs, merged through
  `serve_discovery` with CANONICAL-path dedup against auto-discovery (same
  underlying file via different path forms collapses to one served store).
  `SourceConfig` schema change MUST be additive: `#[serde(default)]` on new
  fields (check for `deny_unknown_fields`) so existing `sources.yaml` (dev
  workspace) still deserializes.
* Files: `sources.yaml` schema/parse module, `src/workspace/serve_discovery.rs`.
* Tests: explicit read-only entry served read-only + never synced; explicit
  entry + auto-discovery for the same file collapse to one store; a pre-change
  `sources.yaml` round-trips (backward-compat parse).
* Posture: test-first. Depends on P1-T2.

**P1-T7 — Optional `--read-only` serve override flag** (config/CLI, test-first)
* Changes: add a `--read-only` escape-hatch flag to `serve` that forces
  `ReadOnly` posture for all dbs regardless of resolved sources. No force-sync
  flag in this phase.
* Files: `src/cli/mod.rs` (serve args), `src/main.rs` (`cmd_serve`).
* Tests: `--read-only` forces read-only even with real sources.
* Posture: test-first. Depends on P1-T3.

**P1-T8 — Phase-1 documentation** (docs)
* Changes: document the docline → graphtor → agent pipeline, the dev-workspace
  generation exception, `.graphtor/` layout, read-only serve auto-discovery
  behaviour, and the **operator trust boundary** (only drop `.db` files from
  trusted sources into `.graphtor/`; discovered dbs are served as authoritative
  agent context) in `docs/product-specs/` and/or `docs/design-docs/`.
* Files: `docs/product-specs/*.md`, `docs/design-docs/*.md`.
* Tests: n/a (docs); `backlogit_docs_lint` clean.
* Posture: docs. Depends on P1-T3 (document actual behaviour).

### Phase 2 — B333B9B8 (covering feature: consumption-first install + opt-in ingestion)

**P2-T1 — Consumption-first `install` default** (code, test-first)
* Changes: default `install` creates only the `.graphtor/` root + a minimal
  serve `.mcp.json` (PATH command `graphtor-docs`, args `["serve"]`); do NOT
  write `sources.yaml`, do NOT create `config/bin/cache/data/logs`. Make
  `InstallResult.binary_path` an `Option<PathBuf>` (or add an `InstallKind`) so
  a consumption install with no copied binary is representable; update callers
  (`cmd_install` message, upgrade, uninstall) to handle `None`. Config/`.mcp.json`
  writes use temp-file + rename (atomic) with stable key ordering.
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install:3002`).
* Tests: fresh install creates only `.graphtor/` + minimal `.mcp.json`; no
  `sources.yaml`; no ingestion subdirs; `binary_path` is `None`.
* Posture: test-first. Depends on Phase 1 (serve must auto-discover first).

**P2-T2 — Opt-in `install --with-ingestion` full scaffold** (code, test-first)
* Changes: add `--with-ingestion` to `InstallArgs` (`src/cli/mod.rs:274`);
  when set, create the full generation scaffold (config + `sources.yaml` +
  data/cache/logs + bin/binary copy). The graphtor dev workspace uses this path.
* Files: `src/cli/mod.rs`, `src/main.rs` (`cmd_install`),
  `src/workspace/install.rs`.
* Tests: `--with-ingestion` creates full layout + `sources.yaml` + copied
  binary; default (no flag) does not.
* Posture: test-first. Depends on P2-T1.

**P2-T3 — Binary resolution precedence in `.mcp.json`** (code, test-first)
* Changes: `.mcp.json` references the PATH command `graphtor-docs` by default and
  `.graphtor/bin/graphtor-docs` only when the bin scaffold exists. Do NOT append
  `.exe` for the bare PATH command (Windows resolves via `PATHEXT`); append the
  platform ext only for the `.graphtor/bin` path. Prefer an absolute pinned path
  when the resolved binary location is known at install time; fall back to bare
  PATH otherwise (documented binary-hijack trade-off).
* Files: `src/workspace/mcp_config.rs` (`managed_server_value`), `src/main.rs`.
* Tests: minimal install → PATH command value (no `.exe`); `--with-ingestion`
  → bin path with platform ext.
* Posture: test-first. Depends on P2-T2.

**P2-T4 — `doctor` tolerates the minimal consumption layout** (code, test-first)
* Changes: add a footprint-detection helper; make `run_doctor`
  (`src/workspace/doctor.rs:124`) layout-aware — when a consumption layout is
  detected, downgrade the missing `config/sources.yaml`, `bin/`, and `graph.db`
  checks to informational instead of Fail/Warn. Full-layout behaviour unchanged.
* Files: `src/workspace/doctor.rs`.
* Tests: doctor on minimal layout → no Fail/Warn; doctor on full layout →
  unchanged.
* Posture: test-first. Depends on P2-T1.

**P2-T5 — uninstall/upgrade parity + user-data preservation** (code, test-first)
* Changes: `uninstall()` (`src/workspace/uninstall.rs:34`) removes ONLY
  graphtor-CREATED artifacts (known subdirs + the managed `.mcp.json` entry) and
  MUST NEVER delete user-dropped `*.db` files in the `.graphtor/` root; do not
  follow symlinks out of the root. When user-dropped dbs are present, the
  operator-approval prompt (PA-3) enumerates the exact deletion set and preserves
  (or per-file confirms) dropped dbs. `upgrade()`
  (`src/workspace/upgrade.rs:43`) of a consumption install has no bin to replace
  and must treat missing bin/subdirs as a no-op, not an error.
* Files: `src/workspace/uninstall.rs`, `src/workspace/upgrade.rs`.
* Tests: uninstall removes minimal + full graphtor artifacts but a user-dropped
  `.db` in `.graphtor/` SURVIVES; upgrade of a minimal (no-bin) install succeeds.
* Posture: test-first. Depends on P2-T1, P2-T2.

**P2-T6 — Backward-compat detection + idempotency** (code, test-first)
* Changes: detect an existing full install and preserve it (opt-in is additive,
  never removes); re-running default install on a full layout is a safe no-op.
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install`).
* Tests: default install over existing full layout preserves it; repeat install
  idempotent.
* Posture: test-first. Depends on P2-T1.

**P2-T7 — Post-install message + separate ingestion-setup docs** (docs)
* Changes: consumption-first post-install message ("drop a `.db` into
  `.graphtor/` to serve it read-only"; link to ingestion docs). Add a distinct
  ingestion/sync setup docs section (create scaffold, author `sources.yaml`,
  run sync, consume read-only downstream).
* Files: `src/main.rs` (`cmd_install` message strings), `docs/product-specs/` or
  `docs/cli-reference/`.
* Tests: explicit test asserting the consumption-first post-install message
  (locks the user-facing contract); `backlogit_docs_lint` clean.
* Posture: docs + a message-assertion test. Depends on P2-T1.

## Dependency Graph

```text
Phase 1 (feature) ──blocks──> Phase 2 (feature)

Phase 1 internal:
  P1-T1 ──> P1-T2 ──> P1-T3 ──> {P1-T7, P1-T8}
  P1-T1 ──> P1-T4
  P1-T1 ──> P1-T5
  P1-T2 ──> P1-T6

Phase 2 internal:
  P2-T1 ──> {P2-T2, P2-T4, P2-T6, P2-T7}
  P2-T2 ──> P2-T3
  {P2-T1, P2-T2} ──> P2-T5
```

No cycles. Suggested execution order: P1-T1 → P1-T2 → P1-T3 → P1-T4 → P1-T5 →
P1-T6 → P1-T7 → P1-T8 → P2-T1 → P2-T2 → P2-T3 → P2-T4 → P2-T6 → P2-T5 → P2-T7.
Note: P1-T3 and P1-T4 both edit `open_serve_databases` — sequence T3 then T4 (do
not interleave) to avoid churn on the shared change surface.

## Decisions and Rationale

* **Content-derived mode over path/env mode** — role must derive from real
  content so a stale/empty `sources.yaml` can never re-enable a background write
  sync in a consumer workspace. Hooks onto the existing `cmd_serve`
  "has sources / no sources" split (`src/main.rs:2447-2467`).
* **Serve discovery Option C** — auto-discovery gives zero-config UX; optional
  explicit entries cover named/external dbs; a strict superset with no
  regression to the sources-driven dev path.
* **Install Option I1** — consumption-first default keeps the footprint minimal
  and sync-free; ingestion is one opt-in flag away; `managed_server_value`
  already emits the minimal `.mcp.json` shape.
* **Characterization-first for P1-T3** — the dev-workspace multi-db serve/sync
  behaviour must be locked by tests before gating the write path, to prevent a
  generation regression.

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Accidental background sync in a consumer workspace | Content-derived mode, fail read-only; gate `spawn_background_sync` + rw-store open behind resolved real sources (P1-T3) |
| Regression in dev-workspace generation/serve | Characterization tests before refactor (P1-T3); preserve sources-driven path |
| Auto-discovery picks up generated/non-db artifacts | Explicit filter + `.graphtor/` root-only scan + containment (P1-T1) |
| Pre-v4 db served ungated | Reuse `needs_v4_migration` gate for discovered dbs (P1-T4) |
| doctor/uninstall/upgrade break on minimal layout | Layout-aware doctor (P2-T3); footprint-aware uninstall/upgrade (P2-T4) |
| Existing full installs disrupted | Backward-compat detection + idempotency (P2-T5) |
| Path traversal in discovery | Resolve within `.graphtor/` root; reject `..`/symlink escapes (P1-T1) |

## Plan Hardening Signals (REQUIRED)

* **Public API, schema, or contract change** — PRESENT. New serve
  auto-discovery behaviour, a new `sources.yaml` read-only entry kind, a
  `--read-only` flag, an `install --with-ingestion` flag, and a changed default
  install footprint are user-facing contract changes.
* **Security, auth, permission, or compliance-sensitive behavior** — PRESENT.
  Filesystem discovery/containment within `.graphtor/` (Principles III/IV);
  read-only vs read-write posture is a data-safety boundary.
* **Migration, backfill, destructive data/config action, or irreversible step**
  — PRESENT. Changed install footprint + uninstall cleanup touch on-disk layout;
  the v4 pre-sync gate interacts with served dbs.
* **External integration, operator checkpoint, or external dependency** —
  PRESENT (minor). `.mcp.json` is consumed by external MCP clients; binary
  resolution via PATH vs `.graphtor/bin`.
* **High runtime, rollout, or rollback risk** — PRESENT. Accidental background
  write sync in a consumer workspace is the central hazard the change must
  eliminate without regressing the dev workspace.

Conclude: **Requires plan hardening: yes**

## Runtime Verification and Closure

Runtime-affecting surfaces: the `serve` CLI/MCP server (read/write posture,
discovery), `status`, and `install`/`uninstall`/`upgrade`/`doctor` CLI commands.

* **P1 (serve)** — verify in a scratch consumer workspace: drop a v4 `.db` into
  `.graphtor/`, run `serve` with a minimal `.mcp.json`; confirm query/search/
  semantic work, `status`/list-sources report the db, and NO background sync /
  NO writes occur (watch for absence of `background sync task spawned`). Verify a
  stale/empty `sources.yaml` does not enable sync. Verify the dev workspace
  (real sources) still generates and serves. Verify a pre-v4 db is refused.
* **P2 (install)** — verify `install` in a fresh consumer workspace creates only
  `.graphtor/` + minimal `.mcp.json`; `install --with-ingestion` creates the full
  scaffold; `doctor` passes on the minimal layout; `uninstall` cleans both
  footprints; re-install is idempotent.
* **Closure** — no external monitoring system; record a manual verification
  checklist in the closure artifact (`docs/closure/`). Rollback trigger: any
  observed write/sync in a consumption workspace → revert the gating change.
  Owner: single-developer operator. Validation window: manual, at merge.

Seed detail for `plan-harden`: the primary safety invariant to tighten is
"a consumer workspace never writes to or syncs a served db"; harden the
verification, rollback trigger, and characterization-test coverage around that
invariant and around backward compatibility for existing full installs.

## Plan Hardening

**Hardening required: YES.** Five hardening signals are present (contract change,
security/containment, on-disk layout/migration, external `.mcp.json` consumer,
high runtime/rollback risk). The dominant hazard is an accidental background
write sync in a consumption workspace.

### Protected Invariants

* **INV-1 (primary)** — A consumption workspace (no resolvable real sources)
  MUST NEVER open a read-write store or spawn `spawn_background_sync` for a
  served db. Read-only is the fail-safe default on any ambiguity.
* **INV-2** — The graphtor dev/authoring workspace (real sources) retains full
  generate-and-serve behaviour with no regression.
* **INV-3** — A stale or empty `sources.yaml` in a consumption workspace does
  NOT enable sync.
* **INV-4** — Auto-discovery scans only the `.graphtor/` root and stays within
  it (no `..`/symlink escape); only `*.db` files, minus the skip list, are
  served.
* **INV-5** — The v4 pre-sync gate still refuses pre-v4 dbs (auto-discovered or
  explicit) with a clear "run sync" message.
* **INV-6** — Existing full installs are preserved; the opt-in scaffold is
  additive-only and re-running install is idempotent.

### Learnings and Instructions Consulted

* `docs/compound/keep-docs-synchronized-with-implementation.md` — keep Phase-1
  and Phase-2 docs in lockstep with the behaviour change (informs P1-T8, P2-T6).
* `docs/compound/cli-jsonrpc-output-pattern-2026-05-06.md` and
  `mcp-formatter-source-verification-2026-05-06.md` — preserve the existing
  JSON-RPC/MCP output contracts when extending `status`/list-sources (P1-T5).
* `.github/instructions/constitution.instructions.md` — Principles III/IV
  (workspace isolation, CLI containment) govern the discovery scan (P1-T1).
* `.github/instructions/strict-safety.instructions.md` — risky actions below use
  `ProposedAction`/`ActionRisk`/`ActionResult` vocabulary.
* No relevant prior serve/install incident in `docs/compound/` (confidence low).

### Risky Actions (strict-safety classification)

| ID | ProposedAction | Targets | ActionRisk | Approval | ActionResult |
|---|---|---|---|---|---|
| PA-1 | Gate rw-store open + background sync behind resolved real sources | `cmd_serve`, `open_serve_databases` (`src/main.rs`) | high | prefer approval (changes serve read/write posture) | planned |
| PA-2 | Change default `install` footprint (drop `config/bin/cache/data/logs` + `sources.yaml`) | `install()`, `cmd_install` | high | prefer approval (on-disk layout + backward compat) | planned |
| PA-3 | Make `uninstall` clean up both footprints | `uninstall()` | destructive | **approval required** (deletes on-disk dirs) | planned |
| PA-4 | Add filesystem discovery scan of `.graphtor/` root | new discovery helper | moderate | standard (read-only scan, containment-gated) | planned |
| PA-5 | Add `--read-only` serve flag and `--with-ingestion` install flag | `src/cli/mod.rs` | moderate | standard (additive CLI contract) | planned |

All risky actions are implemented and executed by the **Ship** agent, not Stage.
These entries travel forward so the reviewer and Ship see the approval posture.
PA-3 (uninstall deletion) requires explicit operator approval at execution time
per Constitution Principle VII.

### Deepened Runtime Verification

Environment prechecks before verification:
* Use a disposable scratch workspace UNDER the repo tree (e.g. a `tempfile`
  tempdir rooted at `target/`) for consumer-mode tests — never mutate the dev
  repo's real `.graphtor/` and never write outside the cwd tree (Principle IV).
* Confirm a v4 `.db` fixture and a pre-v4 `.db` fixture are available.

Target scenarios (each must pass before the invariant is considered proven):
1. Consumer workspace + dropped v4 db + minimal `.mcp.json` → query/search/
   semantic succeed; log shows NO `background sync task spawned`; no file mtime
   change on the served db (INV-1).
2. Consumer workspace + stale/empty `sources.yaml` present → still read-only, no
   sync (INV-3).
3. Dev-style workspace with real sources → sync still spawns and generates
   (INV-2, characterization guard).
4. Pre-v4 dropped db → serve refuses with the v4 message (INV-5).
5. Path-escape/symlink db reference → rejected (INV-4).
6. `install` default → only `.graphtor/` + minimal `.mcp.json`; `--with-ingestion`
   → full scaffold; `doctor` OK on minimal; `uninstall` cleans both; re-install
   idempotent (INV-6).

Blocked-path handling: if any consumer-mode scenario shows a write/sync, HALT and
do not proceed to closure — this is a direct INV-1 violation.

### Deepened Operational Closure

* **Monitoring signals** (manual, no external system): absence of the
  `background sync task spawned` log line and unchanged served-db file mtime in
  consumer mode are the health signals. Presence of either in consumer mode is a
  failure signal.
* **Rollback trigger**: any observed write or sync against a consumption-mode
  served db, OR any regression in the dev-workspace generation path.
* **Rollback procedure**: revert the gating commit(s) for Phase 1 (PA-1) and,
  if install already changed, the install-footprint commit (PA-2); the changes
  are additive/gated and independently revertible per phase.
* **Owner**: single-developer operator (@softwaresalt).
* **Validation window**: manual verification at merge; re-check the six target
  scenarios above.
* **Human checkpoints**: PA-3 uninstall deletion requires operator approval at
  execution; PA-1/PA-2 posture changes preferred for approval before merge.

### Unresolved Operator Decisions (carry into review)

* ~~Precise "resolvable real source" semantics~~ — **RESOLVED in plan review**:
  requires path existence AND ≥1 ingestible file; malformed config stays
  fail-closed. See decision doc Unresolved Questions and P1-T2.
* `--read-only` naming and whether a symmetric force-sync flag is needed
  (lean: `--read-only` only this phase).
* Whether Phase 1 and Phase 2 ship as one PR or two sequential PRs (dependency
  requires Phase 1 first regardless).

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Safety-First Rust | COMPLIANT | New library code (`serve_discovery`) uses `Result<_, GraphtorError>`, no `unwrap`/`expect`; `#![forbid(unsafe_code)]` retained; clippy pedantic clean is a per-task gate; new helpers `#[must_use]`. |
| II. Test-First | COMPLIANT | Every code unit is test-first or characterization-first with explicit scenarios; docs unit P2-T7 adds a message-assertion test. Fixture preconditions (v3/v4 dbs) called out. |
| III. Workspace Isolation | COMPLIANT | Discovery scans only `.graphtor/` root with canonicalize + prefix containment (P1-T1). |
| IV. CLI Containment | COMPLIANT | Scratch test workspaces rooted under `target/`; no writes outside cwd; uninstall never follows symlinks out of `.graphtor/`. |
| V. Structured Observability | COMPLIANT | Positive serve-start posture log line (resolved posture + discovered-db count) added so read-only selection is affirmatively observable, not inferred from an absent line. |
| VI. Single Responsibility | COMPLIANT | No new dependencies; reuses existing `SourceConfig`, `DataStore`, tokio. |
| VII. Destructive Approval | COMPLIANT | Uninstall deletion is PA-3 `destructive`, operator-approval-required, and scoped to graphtor-created artifacts only (never user dbs). |
| VIII. Safety Modes | COMPLIANT | Plan hardened; risky actions classified with strict-safety vocabulary. |
| IX. Git-Friendly Persistence | COMPLIANT | Generated `.mcp.json`/`sources.yaml` writes are atomic (temp-file + rename) with stable key ordering. |
| X. Context Efficiency | N/A | No agent data-access surface changes. |
| XI. Merge Commit Preservation | N/A (Ship-owned) | Merge strategy enforced by Ship at merge time. |

## Plan Review

**Attempt 1 — Gate: FAIL** (multi-persona: Constitution Reviewer, Scope Boundary
Auditor, Architecture Strategist, Security Lens Reviewer, Rust Reviewer;
Learnings Researcher folded in — compound library has no relevant serve/install
prior art, confidence low). Plan hardening was required and present; strict-safety
risky actions were classified. FAIL was driven by convergent P0/P1 findings, all
resolved in this revision (Attempt 2).

<!-- plan-review-attempt: 1 -->
<!-- plan-review-attempt: 2 -->

### Findings and Resolutions

**P0 — resolved**

* **P0-1 (Rust, Architecture): `discover_db_files` is a shared sync/write
  chokepoint.** It is called by `split_plan_by_database` on the sync path;
  folding auto-discovery into it would pull dropped consumption dbs into
  background writes (INV-1 hazard). *Resolution*: P1-T1 now adds a NEW
  serve/status-scoped `serve_discovery` module and explicitly forbids modifying
  `discover_db_files`/`split_plan_by_database`; a characterization test asserts
  sync does not enlarge its db set from dropped dbs.
* **P0-2 (Rust): `run_duplicate_intake_preflight` fires before classification.**
  A stale non-empty `sources.yaml` would trip the write-path guard in consumer
  mode. *Resolution*: P1-T2 moves content-derived classification BEFORE the
  preflight (`src/main.rs:2398-2406`); the preflight runs only for `Generation`
  dbs.

**P1 — resolved**

* **P1-1 (Architecture, Rust): no single source of truth for per-db posture;
  `open_serve_databases` takes a bare `Vec<PathBuf>`; rw-open + `ensure_schema()`
  (write) + write-lock happen unconditionally before the v4 gate.** *Resolution*:
  P1-T3 threads a per-db `Vec<(PathBuf, ServeMode)>` classification through
  `open_serve_databases`; `ReadOnly` dbs skip rw-open/`ensure_schema`/write-lock
  and open read-only only; `ServeOpenedDatabases` carries posture so
  `spawn_background_sync` receives only `Generation` rw stores.
* **P1-2 (Architecture): per-workspace vs per-db model.** The dev workspace can
  hold both real-source and dropped read-only dbs. *Resolution*: plan now commits
  to a per-DB posture model; P1-T3 characterization covers the MIXED workspace.
* **P1-3 (Constitution, Security): "path-exists = resolvable" contradicts
  INV-1/INV-3.** *Resolution*: locked stricter definition (path exists AND ≥1
  ingestible file; malformed config stays fail-closed) in the decision doc and
  P1-T2 three-way outcome.
* **P1-4 (Constitution, Security): uninstall could delete user-dropped dbs.** The
  consumption footprint IS the `.graphtor/` drop location. *Resolution*: P2-T5
  scopes deletion to graphtor-created artifacts only, never user `*.db`, no
  symlink-follow out of root, approval prompt enumerates the deletion set; test
  asserts a dropped `.db` survives uninstall.
* **P1-5 (Rust): `InstallResult.binary_path` is non-optional but a consumption
  install copies no binary.** *Resolution*: P2-T1 makes it `Option<PathBuf>`
  (or `InstallKind`) and updates callers.
* **P1-6 (Rust): `SourceConfig` serde change must be additive.** *Resolution*:
  P1-T6 uses `#[serde(default)]`, checks `deny_unknown_fields`, and adds a
  pre-change round-trip parse test.
* **P1-7 (Constitution IV): scratch workspace OUTSIDE the repo tree.**
  *Resolution*: runtime verification now roots scratch under `target/`.

**P2 — addressed in plan (verify during build)**

* Constitution Check section added (was missing) — **done above**.
* Containment mechanism specified (canonicalize + Windows junction/UNC
  normalization) in P1-T1.
* Canonical-path dedup for discovery + explicit entries in P1-T6.
* Hardened read-only open (no `ATTACH`/extension load) for untrusted discovered
  dbs in P1-T4.
* `status`/`serve` use the SAME shared discovery set (P1-T5).
* Zero-discovered-db handling + remove `unreachable!`/`graph.db` write-fallback
  (P1-T3).
* Malformed `sources.yaml` classified fail-closed (P1-T2).
* `doctor` footprint-detection helper (P2-T4).
* Atomic config writes + stable key ordering (P2-T1) — Principle IX.
* Positive posture log line — Principle V (Constitution Check).
* P2-T2 file-count split: binary resolution moved to its own unit P2-T3.
* Pre-v4/v4 fixture availability precondition (P1-T4, P1-T5).

**P3 — advisory (carried as build-time guidance)**

* Binary-hijack surface via bare PATH command: prefer absolute pinned path when
  known (P2-T3, documented trade-off).
* Operator trust boundary for dropped dbs documented in P1-T8.
* `#[must_use]` + iterator combinators + `Vec::with_capacity` on discovery
  helpers (Constitution Check note).
* P1-T7 (`--read-only`) kept as a separate small unit (merge optional).

### Runtime Verification and Closure Readiness

Verification scenarios (six) and closure (rollback trigger, owner, validation
window) are present and were deepened by plan-harden. Adequate for harvest.

**Attempt 2 — Gate: PASS.** All P0/P1 findings resolved in this revision; P2
items folded into the affected units or the Constitution Check; P3 items carried
as advisory build-time guidance. Plan is cleared for harvest.
