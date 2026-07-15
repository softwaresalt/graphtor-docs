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
| R1 | `serve` auto-discovers `*.db` in `.graphtor/` root | Add a root-scan discovery step feeding the new `serve_discovery` module; it does NOT feed or modify `discover_db_files` | P1 / T1 |
| R2 | Mode is content-derived; stale/empty `sources.yaml` never enables sync | Add resolvable-source classification; default read-only | P1 / T2 |
| R3 | No-real-source dbs are served read-only and never background-synced | Gate rw-store open + pass ONLY filtered `Generation` source groups to `spawn_background_sync` | P1 / T2+T3 |
| R4 | v4 pre-sync gate applies to auto-discovered read-only dbs | Engine-enforced read-only open + no-write proof; reuse `needs_v4_migration` gate | P1 / T4 |
| R5 | `status` + MCP list-sources span multiple discovered dbs | Synthesize source metadata from stored `doc_sources` | P1 / T5 |
| R6 | Optional explicit read-only db entry in `sources.yaml` | Parse an additive `read_only`/database entry kind; workspace-contained (out-of-root paths rejected) | P1 / T6 |
| R7 | Optional `--read-only` override escape hatch | Add serve CLI flag forcing consumption posture | P1 / T7 |
| R8 | Docs: pipeline + dev-workspace exception + read-only serve | Phase-1 docs in product-specs/design-docs | P1 / T8 |
| R9 | `install` default creates only `.graphtor/` root + minimal serve `.mcp.json` | Consumption-first install path | P2 / T1 |
| R10 | Ingestion scaffold is opt-in (`install --with-ingestion`) | New flag (T2a) + full-layout scaffold with managed marker (T2b) | P2 / T2a+T2b |
| R11 | Binary resolution: PATH command default, `.graphtor/bin` when scaffolded | Shared writer ladder + provenance marker + atomic write (Phase-2 root) | P2 / T3 |
| R12 | `doctor` tolerates the minimal consumption layout | Make doctor checks layout-aware | P2 / T4 |
| R13 | uninstall/upgrade parity for both footprints (never delete user dbs) | Footprint-safe uninstall (T5a) + managed MCP-entry removal (T5b) + upgrade parity (T5c) | P2 / T5a+T5b+T5c |
| R14 | Backward compat for existing full installs + idempotency | Detect existing layout; additive-only opt-in | P2 / T6 |
| R15 | Post-install message + separate ingestion-setup docs | Message contract (T7a) + ingestion-setup docs section (T7b) | P2 / T7a+T7b |

## Implementation Units

Each unit follows the 2-hour rule (< 3 files, < 5 functions, < 4 test
scenarios), width isolation (single domain), and produces a verifiable outcome.
Phase 1 has 8 units (P1-T1..P1-T8); Phase 2 has 11 units after the review-thread
splits (P2-T1, P2-T2a, P2-T2b, P2-T3, P2-T4, P2-T5a, P2-T5b, P2-T5c, P2-T6,
P2-T7a, P2-T7b). With the two covering features (050-F, 051-F) this is 21 items
in shipment 045-S. Backlog IDs: P1-T1..T8 = 050.002/050.004/050.001/050.003/
050.005/050.006/050.007/050.008-T; P2-T3/T1/T2a/T2b/T4/T6/T5a/T5b/T5c/T7a/T7b =
051.004/051.001/051.002/051.008/051.003/051.006/051.005/051.009/051.010/051.007/
051.011-T.

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
  promotes ONLY the candidate db whose resolved (canonicalized) path equals that
  source's resolved target db → `Generation` (a resolvable source NEVER promotes
  an unrelated co-resident dropped db, which stays `ReadOnly`);
  (3) absent/empty/stale/unresolvable sources → `ReadOnly`. The
  preflight then runs only for `Generation` dbs. Expose a PURE function that
  returns BOTH the `Vec<(PathBuf, ServeMode)>` classification AND the **filtered
  per-database `Generation` source groups** (the subset of source groups whose
  resolved target db is `Generation`), so preflight and background sync receive
  ONLY `Generation` sources and never the full `SourceConfig` (review thread 2).
* Files: `src/workspace/serve_discovery.rs`, `src/main.rs` (call ordering in
  `cmd_serve` so classification precedes the preflight).
* Tests: real non-empty source → Generation; existing-but-empty path → ReadOnly;
  stale `sources.yaml`, paths absent → ReadOnly; malformed yaml → hard error;
  source-backed db + co-resident dropped db → only the source's target db is
  Generation while the dropped db stays ReadOnly; **mixed valid+stale sources
  targeting DIFFERENT dbs → only the valid db's `Generation` source group is
  returned**.
* Posture: test-first. Depends on P1-T1.

**P1-T3 — Per-db posture threaded through serve open + sync gating** (code, characterization-first)
* Changes: change `open_serve_databases` (`src/main.rs:2344`) to accept the
  `Vec<(PathBuf, ServeMode)>` classification instead of a bare `Vec<PathBuf>`.
  For `ReadOnly` dbs: skip `DataStore::open_sqlite` (rw) and `ensure_schema()`
  (a write), skip the exclusive `acquire_database_lock` write lock, and open ONLY
  the read-only store (the *engine-enforced* no-write guarantee is established and
  proven in P1-T4 — this unit's INV-1 claim is the **gating** invariant, not the
  filesystem no-write proof; review thread 1). For `Generation` dbs: keep the
  current rw+lock path. `ServeOpenedDatabases` carries per-db posture, and ONLY
  the **filtered `Generation` source groups** from P1-T2 (never the full
  `SourceConfig`) are passed to `run_duplicate_intake_preflight` and
  `spawn_background_sync`, which internally re-splits its `SourceConfig` via
  `split_plan_by_database` (`src/main.rs:2268`) and would otherwise re-schedule
  stale/read-only targets (review thread 2). Handle the zero-discovered-db case
  explicitly (clear "no databases found to serve" exit, no `graph.db`
  write-fallback, replace the `unreachable!` at ~2490 with a handled error). Emit
  a positive startup log of the resolved per-db posture and discovered-db count
  (review thread 14; Constitution V).
* Files: `src/main.rs` (`open_serve_databases`, `cmd_serve`).
* Tests: consumption db → no rw store, no write lock, no sync spawn; MIXED
  workspace (real-source + dropped db) → source-backed gets rw+sync while
  co-resident dropped db stays read-only and RO read paths work; **mixed
  valid+stale sources targeting different dbs → only the `Generation` db's source
  group reaches `spawn_background_sync`**; empty `.graphtor/` → graceful
  "nothing to serve"; **a tested positive startup log reports resolved per-db
  posture + discovered-db count**.
* Posture: characterization-first (lock current dev + mixed behaviour, then gate).
  Depends on P1-T2.

**P1-T4 — Engine-enforced read-only open + no-write proof + v4 gate parity** (code, test-first)
* Changes: establish an ENGINE/FILESYSTEM-level read-only open for EVERY
  `ReadOnly`-classified db (auto-discovered AND explicit workspace-contained
  entries from P1-T6) plus automated no-write verification, and keep the v4
  pre-sync gate. The read-only open MUST use a backend/open mode that cannot
  create or mutate the db or its WAL/SHM/journal/lock sidecars (SQLite
  `immutable=1`/`mode=ro` URI or an equivalent `SQLITE_OPEN_READONLY` connection).
  `CozoDB`'s public SQLite backend exposes NO read-only connection flag and
  ignores the options string (`src/db/store.rs:107-110,384`), so the FIRST
  test-first step confirms engine-level read-only feasibility for the Cozo SQLite
  backend; if it cannot be PROVEN, the **feasibility stop condition below**
  applies (fail closed AND block — never a write-capable handle claiming INV-1 on
  the `DataStore` mutate-guard alone, and never a silent degrade; review thread
  1). Evaluate `needs_v4_migration` on the read-only store (no
  write transaction), keeping the refusal message (`open_serve_databases:2363`),
  and harden the RO open (disable loadable extensions, disallow `ATTACH`,
  constrain to the single file).
* Files: `src/db/store.rs` (read-only open strategy), `src/main.rs`
  (`open_serve_databases` v4 gate on the RO store).
* Tests: engine/filesystem no-write proof — capture the served `.db` (size,
  mtime, content hash) and assert NO creation/mutation of `-wal`/`-shm`/
  `-journal`/lock sidecars before AND after a serve+query+search+semantic read
  cycle; attempted write/mutation rejected at the ENGINE boundary; fail-closed
  when engine read-only is unavailable; pre-v4 db → refusal exit + message; v4 db
  → served; a crafted auto-discovered db and an explicit workspace-contained
  entry attempting `ATTACH`/extension-load → refused/inert.
* Posture: test-first. **Precondition**: confirm a pre-v4 (v3) fixture db or a
  programmatic v3-schema builder exists; if absent, add a v3 fixture builder as
  the first step of this unit. Depends on P1-T3 and P1-T6 (hardening applies to
  explicit entries; review thread 4).
* **Feasibility stop condition (NON-NEGOTIABLE):** the engine/filesystem-level
  no-write guarantee for the Cozo SQLite backend is a hard precondition of this
  unit. If Cozo cannot be opened with a PROVEN engine/filesystem-level no-write
  guarantee (immutable/`mode=ro` or equivalent, verified by the before/after
  no-write proof), then P1-T4 (050.003-T) **and** shipment 045-S become
  **BLOCKED** and Dark Mode HALTS before Phase 2 / minimal install. Do NOT
  silently degrade, do NOT claim INV-1 on the `DataStore` mutate-guard alone, and
  do NOT merge a feature-disable fallback. This is a STOP CONDITION, not a
  temp-copy or scope expansion — resolve by proving engine read-only, or escalate
  for an explicit decision.

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

**P1-T6 — Optional explicit read-only db entry (workspace-contained)** (code, test-first)
* Changes: support an explicit read-only database entry (e.g. `read_only: true`
  or a `type: database` kind) for NAMED/ALIASED dbs, merged through
  `serve_discovery` with CANONICAL-path dedup against auto-discovery (same
  underlying file via different path forms collapses to one served store).
  Phase-1 explicit entries MUST remain **workspace-contained**: each entry's path
  is canonicalized and validated to stay within the same authorized root as
  auto-discovery (`validate_path`, `src/path/security.rs:143`). Out-of-root/
  external paths are REJECTED, not served — external-path support is explicitly
  OUT of Phase-1 scope and MUST NOT broaden authorized roots (review thread 3).
  `SourceConfig` schema change MUST be additive: `#[serde(default)]` on new
  fields (check for `deny_unknown_fields`) so existing `sources.yaml` (dev
  workspace) still deserializes.
* Files: `sources.yaml` schema/parse module, `src/workspace/serve_discovery.rs`.
* Tests: explicit workspace-contained read-only entry served read-only + never
  synced; explicit entry + auto-discovery for the same file collapse to one
  store; an entry using `..`/POSIX symlink/Windows junction/outside-root path →
  rejected with a path-violation error; a pre-change `sources.yaml` round-trips
  (backward-compat parse).
* Posture: test-first. Depends on P1-T3 (behaviour owned by T3; review thread 5).

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
* Changes: default `install` creates only the `.graphtor/` root + a minimal serve
  `.mcp.json` written via the shared P2-T3 writer (PATH command `graphtor-docs`,
  args `["serve"]`, with the managed provenance marker and atomic write); do NOT
  write `sources.yaml`, do NOT create `config/bin/cache/data/logs`, and do NOT
  create/update `.gitignore` (review thread 8: the minimal consumption install has
  no managed `.gitignore` side effect; the current `cmd_install` always manages it
  at `src/main.rs:3021-3024`, so the minimal path must skip it). Make
  `InstallResult.binary_path` an `Option<PathBuf>` (or add an `InstallKind`);
  update callers (`cmd_install` message, upgrade, uninstall) to handle `None`.
  This unit CALLS the shared writer (P2-T3) and asserts the resulting minimal
  install — it does not re-implement the marker/atomic-write (review thread 7).
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install:3002`).
* Tests: fresh install creates only `.graphtor/` + minimal `.mcp.json` (marker
  present, no `.exe`); no `sources.yaml`; no ingestion subdirs; NO `.gitignore`
  created/modified; `binary_path` is `None`.
* Posture: test-first. Depends on P2-T3.

**P2-T2a — `install --with-ingestion` CLI flag + plumbing** (code, test-first)
* Changes: add `--with-ingestion` to `InstallArgs` (`src/cli/mod.rs:274`) and
  thread it through `cmd_install` to select the install kind. Flag + plumbing
  ONLY; scaffold creation is P2-T2b (review thread 9 split — the original P2-T2
  listed three files, violating the `< 3 files` rule).
* Files: `src/cli/mod.rs`, `src/main.rs` (`cmd_install`).
* Tests: flag parsed and threaded to `install()`; absent flag selects the
  consumption-first default; no scaffold behaviour implemented here.
* Posture: test-first. Depends on P2-T1.

**P2-T2b — Opt-in full-ingestion scaffold + managed marker** (code, test-first)
* Changes: when `--with-ingestion` is set, create the full generation scaffold
  (config + `sources.yaml` + data/cache/logs + bin/binary copy) and write
  `.mcp.json` via the shared P2-T3 writer so the managed server entry uses the
  pinned `.graphtor/bin` path AND carries the managed marker (review thread 13);
  retain the existing managed `.gitignore` behaviour (unless `--no-gitignore`;
  thread 8). The graphtor dev workspace uses this path.
* Files: `src/workspace/install.rs`.
* Tests: `--with-ingestion` → full layout + `sources.yaml` + copied binary +
  `.mcp.json` pinned bin path WITH managed marker + managed `.gitignore`; default
  → none of these and no `.gitignore`.
* Posture: test-first. Depends on P2-T2a and P2-T3.

**P2-T3 — Shared `.mcp.json` writer: resolution ladder + managed marker + atomic write** (code, test-first) — Phase-2 root
* Changes: make the MCP config writer (`generate_mcp_config`/`managed_server_value`,
  `src/workspace/mcp_config.rs:84-97`) the shared foundation both install paths
  consume: (a) a binary-resolution LADDER — pinned absolute
  `.graphtor/bin/graphtor-docs` (+ platform ext) when a managed binary exists,
  else the bare `graphtor-docs` PATH command (no `.exe`; Windows resolves via
  `PATHEXT`); (b) a managed-entry PROVENANCE MARKER in the server entry so
  `uninstall` (P2-T5b) can identify the managed entry; (c) ATOMIC temp-file +
  rename writes with stable key ordering (Principle IX). The current writer
  hardcodes `.graphtor/bin/...` (`mcp_config.rs:86`) and always creates it;
  restructured as the Phase-2 ROOT so binary resolution + marker + atomic write
  exist BEFORE the minimal install (P2-T1) claims a working install (review
  thread 7).
* Files: `src/workspace/mcp_config.rs`.
* Tests: ladder pure-function (managed binary → pinned path+ext; none → bare PATH
  command, no `.exe`); managed marker present; atomic write with stable ordering;
  user-authored `.mcp.json` entries preserved.
* Posture: test-first. Depends on Phase 1 (050-F).

**P2-T4 — `doctor` tolerates the minimal consumption layout** (code, test-first)
* Changes: add a footprint-detection helper; make `run_doctor`
  (`src/workspace/doctor.rs:124`) layout-aware — when a consumption layout is
  detected, downgrade the missing `config/sources.yaml`, `bin/`, and `graph.db`
  checks to informational instead of Fail/Warn. Full-layout behaviour unchanged.
* Files: `src/workspace/doctor.rs`.
* Tests: doctor on minimal layout → no Fail/Warn; doctor on full layout →
  unchanged.
* Posture: test-first. Depends on P2-T1.

**P2-T5a — Footprint-safe uninstall + approval-set enumeration** (code, test-first)
* Changes: `uninstall()` (`src/workspace/uninstall.rs:34`) removes ONLY
  graphtor-created filesystem artifacts (full-install subdirs) and MUST NEVER
  delete user-dropped `*.db` in the `.graphtor/` root; no symlink-follow out of
  root. `.gitignore` parity (thread 8): a minimal uninstall MUST NOT remove a
  `.gitignore` it never created; only a full-footprint uninstall touches the
  managed `.gitignore` block. The PA-3 approval prompt enumerates the deletion
  set. Managed `.mcp.json` removal is P2-T5b; upgrade parity is P2-T5c (review
  thread 10 split — the original P2-T5 spanned four files).
* Files: `src/workspace/uninstall.rs`, `src/main.rs` (`cmd_uninstall` prompt).
* Tests: minimal → no user-db deletion and no `.gitignore` removal; full →
  removes graphtor subdirs + managed `.gitignore` while a dropped `.db` SURVIVES;
  no symlink-follow; PA-3 prompt enumerates the deletion set.
* Posture: test-first. Depends on P2-T1 and P2-T2b.

**P2-T5b — Managed `.mcp.json` entry removal by provenance marker** (code, test-first)
* Changes: add managed-entry removal to the MCP config module — `uninstall`
  removes ONLY the managed server entry matched by the P2-T3 provenance marker
  and leaves user-authored entries untouched; atomic rewrite.
* Files: `src/workspace/mcp_config.rs`, `src/workspace/uninstall.rs`.
* Tests: managed entry removed while a user-authored entry SURVIVES; a user-only
  `.mcp.json` is unchanged; rewrite is atomic.
* Posture: test-first. Depends on P2-T5a and P2-T3.

**P2-T5c — Minimal/full upgrade parity** (code, test-first)
* Changes: `upgrade()` (`src/workspace/upgrade.rs:43`) of a minimal (no-bin)
  install treats missing bin/subdirs as a no-op success; a full install upgrade
  replaces the binary; idempotent.
* Files: `src/workspace/upgrade.rs`.
* Tests: minimal upgrade → no-op success; full upgrade → replaces binary; repeat
  idempotent.
* Posture: test-first. Depends on P2-T1 and P2-T2b.

**P2-T6 — Backward-compat detection + idempotency** (code, test-first)
* Changes: detect an existing full install and preserve it (opt-in is additive,
  never removes); re-running default install on a full layout is a safe no-op.
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install`).
* Tests: default install over existing full layout preserves it; repeat install
  idempotent.
* Posture: test-first. Depends on P2-T1.

**P2-T7a — Consumption-first post-install message contract** (code, test-first)
* Changes: emit the consumption-first post-install message from `cmd_install`
  ("drop a `.db` into `.graphtor/` to serve it read-only"; link to the
  ingestion-setup docs) and lock it with a message-assertion test. Runtime
  message/code ONLY; docs are P2-T7b (review thread 11 split — width isolation
  forbids mixing code and docs in one task).
* Files: `src/main.rs` (`cmd_install` message strings).
* Tests: explicit assertion of the message content + docs link; the minimal path
  prints this message (not the ingestion-oriented one).
* Posture: test-first. Depends on P2-T1.

**P2-T7b — Separate ingestion-setup docs section** (docs)
* Changes: add a distinct ingestion/sync setup docs section (create scaffold via
  `--with-ingestion`, author `sources.yaml`, run `sync`, consume read-only
  downstream). Keep default install docs focused on the consumption/serve path.
* Files: `docs/product-specs/*.md` or `docs/cli-reference/*.md`.
* Tests: n/a (docs); `backlogit_docs_lint` clean; cross-links from the P2-T7a
  message.
* Posture: docs. Depends on P2-T7a and P2-T2b.

## Dependency Graph

```text
Phase 1 (feature) ──blocks──> Phase 2 (feature)

Phase 1 internal:
  P1-T1 ──> P1-T2 ──> P1-T3 ──> {P1-T7, P1-T8}
  P1-T1 ──> P1-T5
  P1-T3 ──> P1-T6 ──> P1-T4        (P1-T4 also depends on P1-T3)

Phase 2 internal (P2-T3 is the root):
  P2-T3 ──> P2-T1 ──> {P2-T2a, P2-T4, P2-T6, P2-T7a}
  P2-T2a ──> P2-T2b                (P2-T2b also depends on P2-T3)
  {P2-T1, P2-T2b} ──> P2-T5a ──> P2-T5b   (P2-T5b also depends on P2-T3)
  {P2-T1, P2-T2b} ──> P2-T5c
  {P2-T7a, P2-T2b} ──> P2-T7b
```

No cycles. Suggested execution order (matches shipment 045-S): P1-T1 → P1-T2 →
P1-T3 → P1-T6 → P1-T4 → P1-T5 → P1-T7 → P1-T8 → P2-T3 → P2-T1 → P2-T2a → P2-T2b →
P2-T4 → P2-T6 → P2-T5a → P2-T5b → P2-T5c → P2-T7a → P2-T7b. Notes: P1-T3 and P1-T4
both edit the read-only open / `open_serve_databases` — sequence T3 then T4. P1-T6
introduces the explicit read-only entry that P1-T4's hardening exercises, so T6
precedes T4 (review threads 4–6). P2-T3 (shared `.mcp.json` writer) is the Phase-2
root so the marker/atomic-write exist before the minimal install (review thread 7).

## Decisions and Rationale

* **Content-derived mode over path/env mode** — role must derive from real
  content so a stale/empty `sources.yaml` can never re-enable a background write
  sync in a consumer workspace. Hooks onto the existing `cmd_serve`
  "has sources / no sources" split (`src/main.rs:2447-2467`).
* **Serve discovery Option C** — auto-discovery gives zero-config UX; optional
  explicit entries cover named/aliased workspace-contained dbs (external/
  out-of-root support deferred out of Phase-1 scope); a strict superset with no
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
| Engine/filesystem write to a served read-only db (Cozo exposes no RO flag) | Engine-enforced read-only open (immutable/`mode=ro`) + before/after no-write verification; fail closed if unattainable (P1-T4) |
| Full `SourceConfig` re-split re-schedules stale/read-only targets | Return + pass ONLY filtered `Generation` source groups to preflight/sync (P1-T2, P1-T3) |
| Regression in dev-workspace generation/serve | Characterization tests before refactor (P1-T3); preserve sources-driven path |
| Auto-discovery picks up generated/non-db artifacts | Explicit filter + `.graphtor/` root-only scan + containment (P1-T1) |
| Pre-v4 db served ungated | Reuse `needs_v4_migration` gate for discovered dbs (P1-T4) |
| Explicit read-only entry escapes the workspace root | Canonicalize + `validate_path` containment; reject out-of-root; no authorized-root broadening (P1-T6) |
| Ambiguous `.gitignore` behaviour on install | Minimal install does not manage `.gitignore`; full `--with-ingestion` retains it; uninstall parity (P2-T1, P2-T2b, P2-T5a) |
| doctor breaks on minimal layout | Layout-aware doctor (P2-T4) |
| uninstall/upgrade break on minimal layout | Footprint-safe uninstall (P2-T5a) + managed MCP-entry removal (P2-T5b) + upgrade parity (P2-T5c) |
| Existing full installs disrupted | Backward-compat detection + idempotency (P2-T6) |
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
  MUST NEVER open a read-write store, mutate the served db or its WAL/SHM/journal
  sidecars, or spawn `spawn_background_sync` for a served db. The read-only open is
  ENGINE-enforced (immutable/`mode=ro`) with automated before/after no-write
  verification; if the Cozo SQLite backend cannot guarantee engine read-only, the
  served read-only path fails closed rather than relying on the `DataStore`
  mutate-guard alone (P1-T3 gating; P1-T4 engine-level proof). Read-only is the
  fail-safe default on any ambiguity. If engine read-only cannot be PROVEN, this
  is a hard STOP CONDITION (not a silent degrade): P1-T4 (050.003-T) and shipment
  045-S are BLOCKED and Dark Mode halts before Phase 2 / minimal install — no
  INV-1 claim on the mutate-guard alone and no feature-disable fallback merge (see
  P1-T4 feasibility stop condition).
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
* **INV-7** — Only `Generation`-classified source groups reach preflight and
  `spawn_background_sync`; the full `SourceConfig` is never handed to the sync
  path, so a stale/read-only source can never be re-split into a background write.

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
| PA-1 | Gate rw-store open + background sync behind resolved real sources; engine-enforced read-only open with no-write proof | `cmd_serve`, `open_serve_databases`, `DataStore::open_sqlite_readonly` (`src/main.rs`, `src/db/store.rs`) | high | prefer approval (changes serve read/write posture) | planned |
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
* **Post-merge observation window**: 7 days after merge (or until the next
  release, whichever comes first), owner @softwaresalt. Manual observation is
  acceptable for this local CLI/MCP server: within the window, on the first
  `serve` in the dev workspace AND in at least one scratch consumer workspace,
  confirm no `background sync task spawned` log line appears in consumption mode
  and the served-db file mtime is unchanged; on the first `install`/`uninstall`
  exercised, confirm the default footprint stays minimal and no user-dropped
  `*.db` is deleted.
* **Post-merge rollback triggers** (concrete): (a) any consumption-mode `serve`
  emits `background sync task spawned` OR a served-db mtime changes during the
  window → revert the Phase-1 gating commit(s) (PA-1); (b) a default `install`
  writes `sources.yaml`/`config`/`bin` OR an `uninstall` deletes a user-dropped
  `*.db` → revert the Phase-2 install/uninstall commit(s) (PA-2/PA-3).
* **Human checkpoints**: PA-3 uninstall deletion requires operator approval at
  execution; PA-1/PA-2 posture changes preferred for approval before merge.

### Unresolved Operator Decisions (carry into review)

* ~~Precise "resolvable real source" semantics~~ — **RESOLVED in plan review**:
  requires path existence AND ≥1 ingestible file; malformed config stays
  fail-closed. See decision doc Unresolved Questions and P1-T2.
* ~~`--read-only` naming and whether a symmetric force-sync flag is needed~~ —
  **RESOLVED**: `--read-only` is the chosen flag name — the primary safety
  escape hatch that forces consumption (read-only) posture regardless of resolved
  sources. A symmetric `--force-sync` flag is intentionally ABSENT in this phase
  (deferred with no committed timeline); read-only is the fail-safe default, so a
  force-consumption override is the only escape hatch required. See P1-T7.
* ~~Whether Phase 1 and Phase 2 ship as one PR or two sequential PRs~~ —
  **RESOLVED**: ship shipment 045-S as one bounded release unit / one PR
  containing both Phase 1 and Phase 2. Phase 2 is a consumption-first install
  layered on Phase 1, the shipment already encodes the Phase 1 → Phase 2
  dependency, and P-001 permits one in-flight bundled release unit. Phase 1
  still lands before Phase 2 within the single PR.
* ~~Explicit read-only entry path scope (workspace-contained vs external)~~ —
  **RESOLVED (plan review thread 3)**: Phase-1 explicit entries are
  workspace-contained (canonicalize + `validate_path`); out-of-root/external
  paths are rejected and MUST NOT broaden authorized roots. External-path support
  is deferred (no committed timeline). See P1-T6.
* ~~Whether the minimal install manages `.gitignore`~~ — **RESOLVED (thread 8)**:
  the consumption-first minimal install does NOT create/update `.gitignore`; the
  full `--with-ingestion` install retains the existing managed `.gitignore`
  behaviour, and uninstall parity is tested. See P2-T1 / P2-T2b / P2-T5a.

## Constitution Check

| Principle | Status | Notes |
|---|---|---|
| I. Safety-First Rust | COMPLIANT | New library code (`serve_discovery`) uses `Result<_, GraphtorError>`, no `unwrap`/`expect`; `#![forbid(unsafe_code)]` retained; clippy pedantic clean is a per-task gate; new helpers `#[must_use]`. |
| II. Test-First | COMPLIANT | Every code unit is test-first or characterization-first with explicit scenarios; docs unit P2-T7 adds a message-assertion test. Fixture preconditions (v3/v4 dbs) called out. |
| III. Workspace Isolation | COMPLIANT | Discovery scans only `.graphtor/` root with canonicalize + prefix containment (P1-T1); explicit read-only entries stay workspace-contained via `validate_path` (P1-T6). |
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

### Review-Thread Remediation (PR #88 / shipment 045-S)

The 14 Copilot review threads on the staging PR were remediated in-place
(planning/backlog only; no code, no build). Summary:

* **Thread 1 (INV-1, security)** — P1-T4 now requires an engine/filesystem-level
  read-only open (immutable/`mode=ro`) with automated before/after no-write
  verification and a fail-closed fallback, because `open_sqlite_readonly` reuses a
  write-capable `DbInstance` and Cozo's SQLite backend exposes no read-only flag
  (`src/db/store.rs:107-110,384`). P1-T3's INV-1 claim is scoped to gating only.
* **Thread 2** — P1-T2 returns filtered `Generation` source groups; P1-T3 passes
  ONLY those to preflight/`spawn_background_sync` (never the full `SourceConfig`,
  which re-splits at `src/main.rs:2268`). New INV-7.
* **Thread 3** — P1-T6 explicit entries are workspace-contained (`validate_path`);
  external/out-of-root paths rejected; authorized roots not broadened.
* **Threads 4–6** — P1-T6 depends on P1-T3; P1-T4 depends on P1-T3 + P1-T6; the
  045-S manifest runs T6 before T4.
* **Thread 7** — P2-T3 (shared `.mcp.json` writer: ladder + marker + atomic write)
  is the Phase-2 root; P2-T1 depends on and consumes it.
* **Thread 8** — minimal install does not manage `.gitignore`; full
  `--with-ingestion` retains it; uninstall parity tested (P2-T1/T2b/T5a).
* **Threads 9–11** — oversized tasks split: P2-T2 → P2-T2a (flag) + P2-T2b
  (scaffold); P2-T5 → P2-T5a (uninstall) + P2-T5b (MCP-entry removal) + P2-T5c
  (upgrade); P2-T7 → P2-T7a (message) + P2-T7b (docs). Each < 3 files, one domain.
* **Thread 12** — deliberation "Unresolved Questions" locked/resolved
  (`--read-only`, Windows PATH/pinned command ladder, discovery filter).
* **Thread 13** — P2-T2b asserts the full install writes the managed marker via
  the shared P2-T3 writer.
* **Thread 14** — P1-T3 adds a tested positive startup log (resolved per-db
  posture + discovered-db count).

Backlog impact: Phase 2 grew from 7 to 11 task units (4 new:
051.008/051.009/051.010/051.011-T); shipment 045-S is 21 items; the dependency
DAG was re-validated acyclic.
