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
| R1 | `serve` auto-discovers `*.db` in `.graphtor/` root | Add a root-scan discovery step feeding the new `serve_discovery` module; the served set is a UNION that PRESERVES the existing `discover_db_files` candidates (configured targets incl. not-yet-created, explicit `--db-path`) plus the root-scan entries; the auto-discovered subset does NOT feed or modify `discover_db_files` | P1 / T1 |
| R2 | Mode is content-derived; stale/empty `sources.yaml` never enables sync | Add resolvable-source classification; default read-only | P1 / T2 |
| R3 | No-real-source dbs are served read-only and never background-synced | Gate rw-store open + pass ONLY filtered `Generation` source groups to `spawn_background_sync` | P1 / T2+T3 |
| R4 | v4 pre-sync gate applies to auto-discovered read-only dbs | Engine-enforced read-only open + no-write proof; reuse `needs_v4_migration` gate | P1 / T4 |
| R5 | `status` + MCP list-sources span multiple discovered dbs | Synthesize source metadata from stored `doc_sources` | P1 / T5 |
| R6 | Optional explicit read-only db entry in `sources.yaml` | Parse an additive `type: database` variant, workspace-contained (out-of-root paths rejected), after the P1-RF1..P1-RF5 variant-safe consumer pre-refactors make the additive variant compile-safe | P1 / T6 |
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
Phase 1 has 9 primary units (P1-T0..P1-T8) plus 5 `Source`-variant compatibility
pre-refactor units (P1-RF1..P1-RF5; RF1..RF4 added after the PR #88 third-pass
review, RF5 added after the fourth-pass irrefutable-consumer audit) = 14
Phase-1 units; Phase 2 has 11 units after the review-thread
splits (P2-T1, P2-T2a, P2-T2b, P2-T3, P2-T4, P2-T5a, P2-T5b, P2-T5c, P2-T6,
P2-T7a, P2-T7b). With the two covering features (050-F, 051-F) this is 27 items
in shipment 045-S. Backlog IDs: P1-T0 = 050.009-T; P1-T1..T8 =
050.002/050.004/050.001/050.003/050.005/050.006/050.007/050.008-T;
P1-RF1..RF5 = 050.010/050.011/050.012/050.013/050.014-T;
P2-T3/T1/T2a/T2b/T4/T6/T5a/T5b/T5c/T7a/T7b =
051.004/051.001/051.002/051.008/051.003/051.006/051.005/051.009/051.010/051.007/
051.011-T.

### Phase 1 — 79B5A7BC (covering feature: read-only serve auto-discovery + content-derived mode)

> **Review-driven architecture note (applies to all Phase-1 code units):** the
> serve/status discovery + posture logic lives in a NEW cohesive module
> `src/workspace/serve_discovery.rs` (library code: `Result<_, GraphtorError>`,
> no `unwrap`/`expect`, `thiserror`). It owns: (a) the `.graphtor/` root scan +
> filter + containment, (b) resolvable-source classification, (c) explicit-entry
> merge, (d) canonical-path dedup, and (e) UNION assembly that PRESERVES the
> existing serve candidate inputs — the configured source target db paths
> (including not-yet-created generation targets) and the explicit `--db-path`
> candidate returned by `discover_db_files` — so the served set is a strict
> superset of today's candidates, not a replacement (review thread 1 / PR #88
> comments 3588875971, 3588876019). It CONSUMES the OUTPUT of `discover_db_files`
> but MUST NOT modify the shared `discover_db_files` (`src/main.rs:244`) or
> `split_plan_by_database` (`src/main.rs:273`), which feed the SYNC/write path —
> injecting discovery there would pull dropped consumption dbs into background
> sync (the INV-1 hazard). Posture is **per-db** (`ServeMode::ReadOnly | Generation`), not
> per-workspace, so a workspace may hold both source-backed and dropped
> read-only dbs.

**P1-T0 — Engine/filesystem read-only open feasibility proof (Phase-1 gate)** (code, test-first)
* Changes: PROVE an ENGINE/FILESYSTEM-level read-only open primitive for the
  Cozo SQLite backend BEFORE any other Phase-1 work. This is the Phase-1 ROOT
  GATE: P1-T1 depends on it, so the whole Phase-1 chain — and shipment 045-S — is
  gated on this proof (review thread 3). `CozoDB`'s public SQLite backend opens a
  WRITE-capable `DbInstance` and ignores the options string
  (`src/db/store.rs:366-373,383-390`); the current `open_sqlite_readonly` guards
  only at the `DataStore` mutate boundary (`src/db/store.rs:105-134`), NOT at the
  engine/filesystem level. Implement + prove a real engine-level read-only open
  (SQLite `immutable=1`/`mode=ro` URI or an equivalent `SQLITE_OPEN_READONLY`
  connection) that cannot create or mutate the db or its WAL/SHM/journal/lock
  sidecars, verified by an automated before/after no-write proof. Scope is the
  primitive + proof ONLY; v4 gate parity and ATTACH/extension/single-file serve
  hardening move to P1-T4 (which consumes this primitive).
* Files: `src/db/store.rs` (engine-level read-only open path) + a v3/v4 fixture
  builder if a suitable db fixture is absent (≤ 2 files).
* Tests: engine/filesystem no-write proof — capture the served `.db` (size,
  mtime, content hash) and assert NO creation/mutation of the db or its
  `-wal`/`-shm`/`-journal`/lock sidecars before AND after a query+search+semantic
  read cycle; an attempted write is rejected at the ENGINE boundary (not the
  `DataStore` mutate-guard); the open is proven to use immutable/`mode=ro` (or
  `SQLITE_OPEN_READONLY`).
* Posture: test-first. No upstream dependency — this is the Phase-1 root.
* **Feasibility stop condition (NON-NEGOTIABLE):** if the Cozo SQLite backend
  cannot be opened with a PROVEN engine/filesystem-level no-write guarantee
  (immutable/`mode=ro` or equivalent, verified by the before/after no-write
  proof), then P1-T0 (050.009-T) **and** shipment 045-S become **BLOCKED** and
  Dark Mode HALTS before Phase 2 / minimal install. Do NOT silently degrade, do
  NOT claim INV-1 on the `DataStore` mutate-guard alone, and do NOT merge a
  feature-disable fallback. Resolve by proving engine read-only, or escalate for
  an explicit decision.

**P1-T1 — Serve/status-scoped `.graphtor/*.db` discovery + candidate union with containment** (code, test-first)
* Changes: add a serve/status-scoped discovery helper in the new
  `serve_discovery` module that scans the `.graphtor/` root for `*.db` AND
  assembles the served set as a UNION preserving the existing serve candidate
  inputs (review thread 1 / PR #88 comments 3588875971, 3588876019): (a) the
  configured source TARGET db paths from `discover_db_files`
  (`src/main.rs:244-255`) INCLUDING generation targets not yet on disk; (b) the
  explicit `--db-path` / no-config candidate (`src/main.rs:2408-2417`); (c)
  workspace-contained `type: database` entries (merged by P1-T6); and (d) newly
  auto-discovered EXISTING `*.db` files in the `.graphtor/` root. Only the (d)
  root-scan subset is force-classified read-only and MUST NEVER feed back into
  `discover_db_files`/`split_plan_by_database`; the zero-db exit fires ONLY when
  the FULL union is empty.
  Containment mechanism: canonicalize the root and each candidate, normalize the
  Windows `\\?\` UNC prefix, assert the canonical candidate is prefixed by the
  canonical root, and reject `..`, POSIX symlinks, AND Windows junctions/reparse
  points. Skip-list (locked): `*.lock`, index/tmp files, `.graphtor/models`, and
  generated artifacts; served set = `*.db` by extension minus skip-list. Do NOT
  touch `discover_db_files`/`split_plan_by_database`.
* Files: `src/workspace/serve_discovery.rs` (new), `src/workspace/mod.rs` (wire).
* Tests: served set = canonical-deduped union of the existing `discover_db_files`
  candidates + the auto-discovered root `*.db`; **regression — a fresh
  source-backed workspace whose configured target db does not exist yet still
  reaches serve (not the zero-db exit); regression — an explicit `--db-path`
  candidate is served even with no root-scan hit**; discovers a dropped `.db`;
  skips `*.lock`/models/non-`.db`; rejects `..`/symlink/junction escape (Windows
  junction case); auto-discovered entries never reach `discover_db_files`/sync.
* Posture: test-first. Depends on P1-T0.

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
  proven in P1-T0 (050.009-T) — this unit's INV-1 claim is the **gating** invariant, not the
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

**P1-T4 — v4 gate parity + ATTACH/extension/single-file serve hardening** (code, test-first)
* Changes: consume the engine-enforced read-only open primitive PROVEN in P1-T0
  and apply the serve-side hardening for EVERY `ReadOnly`-classified db
  (auto-discovered AND explicit workspace-contained entries from P1-T6): (1) keep
  v4 pre-sync gate parity — evaluate `needs_v4_migration` on the read-only store
  with NO write transaction, preserving the refusal message
  (`open_serve_databases:2363`); and (2) harden the RO open — disable loadable
  extensions, disallow `ATTACH`, and constrain to the single file — identically
  for all `ReadOnly` entries. This unit does NOT re-prove the engine no-write
  guarantee (that is P1-T0); it builds v4 parity + hardening on top of the proven
  primitive (review threads 1, 3).
* Files: `src/db/store.rs` (single-file/`ATTACH`/extension hardening on the RO
  open), `src/main.rs` (`open_serve_databases` v4 gate on the RO store).
* Tests: pre-v4 (v3) db → refusal exit + message; v4 db → served; the v4 gate
  (`needs_v4_migration`) is evaluated on the read-only store with no write
  transaction; a crafted auto-discovered db and an explicit workspace-contained
  entry attempting `ATTACH`/extension-load → refused/inert, RO open constrained to
  the single file.
* Posture: test-first. **Precondition**: confirm a pre-v4 (v3) fixture db or a
  programmatic v3-schema builder exists (may be shared with P1-T0); if absent, add
  a v3 fixture builder as the first step. Depends on P1-T3 and P1-T6 (hardening
  applies to explicit entries; review thread 4); transitively gated by P1-T0 via
  P1-T1.
* **Feasibility dependency:** the engine/filesystem-level no-write PROOF is owned
  by P1-T0. If that proof fails, P1-T0 and shipment 045-S are BLOCKED upstream and
  this unit never runs — do NOT re-open a write-capable handle or claim INV-1 on
  the `DataStore` mutate-guard alone here (see P1-T0 feasibility stop condition).

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
* Changes: support an explicit read-only database entry via a NEW, DISTINCT
  `type: database` source variant (NOT a `read_only: true` flag — the distinct
  variant avoids overloading LocalSource's ingestion-only fields; review thread
  5). LOCKED serialized contract: `Source::Database(DatabaseSource { id: String
  (required, unique alias/name), path: PathBuf (required, workspace-contained) })`
  — no other fields in Phase 1 (the variant IS read-only; no `read_only` field).
  Adding `Database(DatabaseSource)` to the existing internally-tagged `Source`
  enum (`#[serde(tag = "type", rename_all = "lowercase")]`,
  `src/config/source.rs:51-56`) is additive at the serde level AND compile-safe
  for consumers because P1-RF1..P1-RF5 first route every irrefutable/non-exhaustive
  `Source::Local` consumer — all 18 production sites (config validation,
  acquire/plan, acquire/mod, pipeline/mod, sync/mod, main.rs) plus every breaking
  test binding (src/config/source.rs + src/main.rs colocated tests and external
  tests/config_test.rs & tests/pipeline_format_test.rs) — through variant-safe
  accessors (review threads 2/2b / PR #88 comments 3588876057, 3588876095) —
  existing `type: local` entries parse unchanged and `git`/`url` stay rejected;
  adding the variant then only extends the `src/config/source.rs` accessor match
  arms — `id()` returns the required `DatabaseSource.id` alias, `formats()`/`include()`/
  `exclude()` return empty, `database()` returns `None` (local-target-only; it must
  NOT return the served path because it feeds the ingestion/generation write path),
  a DISTINCT `served_db_path()` accessor returns `Some(DatabaseSource.path)` for the
  read-only served path, `as_local()` → `None`,
  `is_ingestible()` → `false` — and breaks no consumer (build OR test). Merged through
  `serve_discovery` with CANONICAL-path dedup against auto-discovery (same
  underlying file collapses to one served store; the explicit entry's `id` is the
  served alias/name). Phase-1 explicit entries MUST remain **workspace-contained**:
  each `path` is canonicalized and validated to stay within the same authorized
  root as auto-discovery (`validate_path`, `src/path/security.rs:143`). Out-of-
  root/external paths are REJECTED, not served — external-path support is
  explicitly OUT of Phase-1 scope and MUST NOT broaden authorized roots (review
  thread 3). Because LocalSource is UNTOUCHED, every existing `sources.yaml`
  round-trips with zero risk (the original `deny_unknown_fields` concern is moot).
* Files: `sources.yaml` schema/parse module, `src/workspace/serve_discovery.rs`.
* Tests: explicit workspace-contained read-only entry served read-only + never
  synced; **mixed local+database config through `serve` (database served
  read-only via `served_db_path`, local source keeps generation posture),
  `sync` (`cmd_sync` ingests only the local source; the database entry is ignored
  via the P1-RF2/P1-RF3 gates), AND generation (the database entry is EXCLUDED
  from `discover_db_files`/`split_plan_by_database` via the P1-RF4 gate, never
  opened read-write)**; explicit entry + auto-discovery for the same file
  collapse to one store; an entry using `..`/POSIX symlink/Windows
  junction/outside-root path → rejected with a path-violation error; a pre-change
  `sources.yaml` round-trips (backward-compat parse); the additive variant
  compiles with only `src/config/source.rs` match-arm changes (clippy-pedantic
  clean).
* Posture: test-first. Depends on P1-T3 and P1-RF5 (050.014-T, the last
  variant-safe pre-refactor) — the variant is added only after every
  `Source::Local` consumer (production AND test) is variant-safe (review threads 2/2b, 5). Ingestion
  AND generation filtering is owned by P1-RF2/P1-RF3/P1-RF4 (acquisition plan loop +
  sync path + `discover_db_files`/`split_plan_by_database`), so this unit does NOT touch
  `cmd_sync`/main.rs and stays at 2 source files.

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

### Phase 1 — `Source`-variant compatibility pre-refactors (P1-RF1..P1-RF5)

> **Added after the PR #88 third-pass review (comments 3588876057, 3588876095);
> P1-RF5 and the exact-count correction added after the fourth-pass
> irrefutable-consumer audit.**
> The `Source` enum today has ONLY `Local(LocalSource)`
> (`src/config/source.rs:53-56`), so EVERY consumer destructures it irrefutably
> (`let Source::Local(...) = ...`) or matches it non-exhaustively. Adding
> `Database(DatabaseSource)` in P1-T6 would break compilation at **18 production
> sites** (3 in config validation, 2 in acquire/plan, 1 in acquire/mod, 1 in
> pipeline/mod, 2 in sync/mod, 9 in main.rs) AND at **7 breaking test consumers**
> (3 in src/config/source.rs colocated tests, 1 in src/main.rs colocated tests, 1
> in tests/config_test.rs, 2 in tests/pipeline_format_test.rs). These five
> pre-refactors route every consumer — production and test — through variant-safe
> accessors FIRST; each is a pure structural refactor with NO behaviour change
> while only `Local` exists (independently green), each ≤ 2 files, chained
> P1-RF1 → P1-RF2 → P1-RF3 → P1-RF4 → P1-RF5 → P1-T6. "Filter/ignore `Database` by design"
> is the emergent effect of these gates: once the variant lands, non-ingestible
> sources are skipped by every acquisition/sync consumer with no separate
> top-level filter needed.

**P1-RF1 — Variant-safe `Source` accessors + config-validation refactor** (050.010-T; code, test-first)
* Changes: add `as_local(&self) -> Option<&LocalSource>` (declared `pub`) and
  `is_ingestible(&self) -> bool` to `Source` in `src/config/source.rs` (today always
  `Some`/`true`), AND widen the existing `Source::id()` accessor (`src/config/source.rs:60`)
  from `pub(crate)` to `pub` so the bin crate (src/main.rs, P1-RF4) and external
  integration tests (P1-RF5) — SEPARATE crates from the library — can call
  `id()`/`as_local()` across the crate boundary; `is_ingestible()` MAY remain
  `pub(crate)` (library-only, used by RF3's src/sync/mod.rs); refactor the THREE
  irrefutable bindings in
  `src/config/validation.rs` (`validate` ~L67, `detect_with_context` ~L210,
  `intake_key` ~L302) and the THREE colocated `#[cfg(test)]` bindings in
  `src/config/source.rs` (~L162/L188/L197) to route through `as_local()` /
  existing accessors. NO variant is added here.
* Files: `src/config/source.rs`, `src/config/validation.rs`.
* Tests: `as_local()`/`is_ingestible()` unit behaviour; `validate`,
  `detect_with_context`, and `intake_key` produce identical results for local-only
  configs; the src/config/source.rs colocated tests stay green through `as_local()`;
  cross-crate visibility compiles (the bin crate and external tests call the now-public
  `id()`/`as_local()`); clean under clippy pedantic (refutable `Option` let-else — no irrefutable-pattern lint).
* Posture: test-first. Depends on P1-T3.

**P1-RF2 — Acquire plan/dispatch variant-safe refactor** (050.011-T; code, test-first)
* Changes: filter the acquisition PLAN LOOP (`plan`,
  `src/acquire/plan.rs:47-55` — it calls `resolve_source_dir` for EVERY configured
  source, so it MUST gate on `as_local()` BEFORE `resolve_source_dir`) and route
  `validate_sources` (`src/acquire/plan.rs:90`),
  `resolve_source_dir` (`src/acquire/plan.rs:135`), and `execute_scan_local`
  (`src/acquire/mod.rs:143`) through `as_local()` so acquisition ignores
  non-ingestible sources by design. Without the plan-loop filter, a mixed
  local/database config would FAIL planning once P1-T6 lands (PR #88 thread
  PRRT_kwDORiB5E86RNyUU); `resolve_source_dir` fail-closes on a non-local source as
  defence-in-depth.
* Files: `src/acquire/plan.rs`, `src/acquire/mod.rs`.
* Tests: `plan`/`validate_sources`/`resolve_source_dir`/`execute_scan_local`
  identical for local-only configs; a mixed local/database config plans ONLY the
  local source (the database entry is filtered, not failed — exercised once P1-T6
  adds the variant); existing acquire tests green.
* Posture: test-first. Depends on P1-RF1.

**P1-RF3 — Pipeline record + sync-cycle variant-safe refactor** (050.012-T; code, test-first)
* Changes: route `build_source_record` (`src/pipeline/mod.rs:709`), `sync_source`
  (`src/sync/mod.rs:227`), and `build_new_state` (`src/sync/mod.rs:1059`) through
  `as_local()`/`is_ingestible()` so the per-source sync path ignores non-ingestible
  sources by design.
* Files: `src/pipeline/mod.rs`, `src/sync/mod.rs`.
* Tests: `build_source_record`/`sync_source`/`build_new_state` identical for local
  sources; existing pipeline/sync tests green.
* Posture: test-first. Depends on P1-RF2.

**P1-RF4 — `src/main.rs` variant-safe refactor (v4-migration + sync/prewarm surface)** (050.013-T; code, test-first)
* Changes: route ALL nine `src/main.rs` production consumers through the centralized
  accessors — id-only sites `source_id` (~L655) and the six `=> local.id...` match
  arms (`guard_no_embed_before_v4_rebuild` ~L1280, `collect_snapshot_candidates`
  ~L1410, `freeze_v4_migration_input` ~L1486, `run_incremental_sync` ~L2070,
  `cmd_prewarm` ~L3382, `prewarm_sync_source` ~L3435) via `Source::id()`;
  `changed_source_fields` (~L661) and `collect_candidate_md_files` (~L1310) via
  `as_local()` (defensive empty/skip when non-local); plus the one colocated
  `#[cfg(test)]` binding (~L4323) via `as_local()`. ALSO gate the GENERATION
    db-discovery/splitting against non-ingestible sources with TWO separate guards
    (the two functions iterate DIFFERENT collections): `discover_db_files` (~L244)
    gates its `source_config.sources` loop on `as_local()`, and
    `split_plan_by_database` (~L273) — which seeds keys from the now-filtered
    `discover_db_files` but then iterates `plan.sources` (~L284-290) where
    `or_insert_with` (~L286-288) can re-introduce a db key — also gates that
    `plan.sources` loop on `as_local()`, belt-and-suspenders atop RF2's (050.011-T)
    filtered `AcquisitionPlan` invariant, so a non-ingestible `Database` source is
    EXCLUDED from generation discovery AND splitting and never opened read-write
    (PR #88 threads PRRT_kwDORiB5E86RNyT4 / PRRT_kwDORiB5E86RO5GB). This is a
  SHRINKING exclusion filter that PRESERVES the P0-1 invariant (the sync/write set is
  never ENLARGED) and is distinct from the P1-T1 constraint forbidding auto-discovery
  feedback INTO those functions. Uniform mechanical
  accessor-routing in a single file/domain. src/main.rs
  is the BIN crate (separate from the library), so it calls the now-public
  `Source::id()`/`Source::as_local()` widened in P1-RF1 across the crate boundary.
* Files: `src/main.rs`.
* Tests: id-extraction, `changed_source_fields`, and `collect_candidate_md_files`
  identical for local sources; `discover_db_files` (gating its `source_config.sources`
  loop) and `split_plan_by_database` (gating its `plan.sources` loop atop RF2's
  filtered `AcquisitionPlan`) enumerate the same generation db set for a local-only
  config, and a non-local (future `Database`) source is excluded from BOTH (exercised once P1-T6 adds the
  variant); the filter skips a `Database` source ENTIRELY (it must not fall through
  `resolve_source_db_path`'s `database()==None` default to `base_db_path`); for a
  database-only config the empty-set + `graph.db` write-fallback remains owned by
  P1-T3 and consumption-mode classification by P1-T2, and the served entry reaches
  the serve union via the P1-T6 merge (P1-T1), not `discover_db_files`; the main.rs colocated migration test stays green; after
  this unit ZERO `Source::Local` consumers remain in `src/main.rs` (production and
  colocated tests) — only the EXTERNAL integration tests (P1-RF5) remain.
* Posture: test-first. Depends on P1-RF3.

**P1-RF5 — External integration-test variant-safe pre-refactor** (050.014-T; code/test, test-first)
* Changes: route the irrefutable `let Source::Local(...)` bindings in the EXTERNAL
  integration tests — `tests/config_test.rs:31` and `tests/pipeline_format_test.rs:223,243`
  (the ONLY external test files with breaking consumers; every other `tests/*`
  occurrence merely CONSTRUCTS `Source::Local(LocalSource { .. })`, which an additive
  variant does not break) — through `as_local()`. NO variant is added here.
* Files: `tests/config_test.rs`, `tests/pipeline_format_test.rs`.
* Tests: both files compile and pass identically under `cargo test --all-targets`;
  after this unit ZERO irrefutable `Source::Local` bindings remain across `src/` AND
  `tests/`, so P1-T6 keeps `cargo build` / clippy / `cargo test` green with only
  `src/config/source.rs` arm updates.
* Posture: test-first. Depends on P1-RF4. This is the pre-refactor chain tail; P1-T6
  depends on it.

### Phase 2 — B333B9B8 (covering feature: consumption-first install + opt-in ingestion)

**P2-T1 — Consumption-first `install` default** (code, test-first)
* Changes: add a NEW consumption-first path `install_minimal()` that creates only
  the `.graphtor/` root + a minimal serve `.mcp.json` written via the shared P2-T3
  writer (PATH command `graphtor-docs`, args `["serve"]`, with the managed
  provenance marker and atomic write); it writes NO `sources.yaml`, creates NONE
  of `config/bin/cache/data/logs`, and does NOT create/update `.gitignore` (review
  thread 8: the minimal install has no managed `.gitignore` side effect; the
  current `cmd_install` manages it at `src/main.rs:3021-3024`, so the minimal path
  skips it). **Backward-compat (review thread 8 follow-on):** PRESERVE the existing
  full-scaffold `install()` (`src/workspace/install.rs:34`) UNCHANGED so internal
  callers (upgrade, `--with-ingestion`, `init`) keep working and the existing
  upgrade tests stay green — do NOT gut `install()`; ADD a sibling
  `install_minimal()`. `cmd_install` DEFAULT routes to `install_minimal()`;
  `--with-ingestion` routes to the preserved `install()`. Represent the minimal
  "no binary" case via `InstallResult.binary_path: Option<PathBuf>` (full
  `install()` → `Some`, `install_minimal()` → `None`) or a dedicated minimal
  result; update `cmd_install` message + uninstall to handle `None`. Upgrade needs
  NO change (still calls the preserved full `install()`/`installed_binary_path`).
  This unit CALLS the shared writer (P2-T3) and asserts the resulting minimal
  install — it does not re-implement the marker/atomic-write (review thread 7).
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install:3002`).
* Tests: fresh default install (`install_minimal()`) creates only `.graphtor/` +
  minimal `.mcp.json` (marker present, no `.exe`); no `sources.yaml`; no ingestion
  subdirs; NO `.gitignore` created/modified; the minimal "no binary" case is
  representable; the existing upgrade test (`upgrade_succeeds_after_install`,
  `src/workspace/upgrade.rs:94-101`, which calls `install()` then `upgrade()`)
  REMAINS GREEN because `install()` is preserved.
* Posture: test-first. Depends on P2-T3.

**P2-T2a — `install --with-ingestion` CLI flag + plumbing** (code, test-first)
* Changes: add `--with-ingestion` to `InstallArgs` (`src/cli/mod.rs:274`) and
  thread it through `cmd_install` to select the install PATH — the consumption-
  first default routes to `install_minimal()` (P2-T1), `--with-ingestion` routes
  to the PRESERVED full `install()` (invoked by the P2-T2b scaffold path). Because
  this selector changes the DEFAULT to `install_minimal()`, also update the
  command-level `Install` doc comment clap renders as `install --help`
  (`src/cli/mod.rs:115-120`), which currently promises the default creates
  `bin/data/cache/config/logs` and copies the binary — behaviour that no longer
  occurs on the default path (PR #88 thread PRRT_kwDORiB5E86RNyVA). Flag +
  plumbing + help-text update ONLY; scaffold creation is P2-T2b (review thread 9 split — the original
  P2-T2 listed three files, violating the `< 3 files` rule).
* Files: `src/cli/mod.rs`, `src/main.rs` (`cmd_install`).
* Tests: flag parsed and threaded to the install-path selector; absent flag
  selects the consumption-first default (`install_minimal()`); present flag
  selects the preserved full `install()`; no scaffold behaviour implemented here;
  `install --help` describes the minimal default + `--with-ingestion` opt-in and no
  longer claims the default creates the full scaffold or copies the binary
  (help-text assertion).
* Posture: test-first. Depends on P2-T1.

**P2-T2b — Opt-in full-ingestion scaffold + managed marker** (code, test-first)
* Changes: when `--with-ingestion` is set (routed by P2-T2a), OWN the full-path
  install orchestration. Invoke the PRESERVED full-scaffold `install()` (already
  creates config + data/cache/logs + bin/binary copy; kept intact by P2-T1) AND
  own the post-install orchestration currently UNCONDITIONAL in `cmd_install`
  (`src/main.rs:3017-3028`): write the template `sources.yaml`
  (`init_sources_yaml`), manage the `.gitignore` block (unless `--no-gitignore`;
  thread 8), and write `.mcp.json` via the shared P2-T3 writer so the managed
  server entry uses the pinned, cwd-independent ABSOLUTE
  `<canonical_project_root>/.graphtor/bin/graphtor-docs[.exe]` command (the relative
  `.graphtor/bin/...` string is reserved ONLY for legacy exact-match recognition)
  AND carries the managed marker
  (review thread 13). These `sources.yaml`/`.gitignore`/`.mcp.json` calls MUST be
  guarded to the `--with-ingestion` path so the consumption-first DEFAULT
  (`install_minimal()`, P2-T1) never runs them. P2-T2a stays routing-only (flag +
  branch selection, no scaffold behaviour); this split resolves the boundary
  conflict where neither a routing-only T2a nor a one-file T2b could own the
  `cmd_install` orchestration (PR #88 comments 3588876203, 3588876245). The
  graphtor dev workspace uses this full path.
* Files: `src/workspace/install.rs`, `src/main.rs` (`cmd_install` full-path
  orchestration; still < 3 files).
* Tests: `--with-ingestion` → full layout + `sources.yaml` + copied binary +
  `.mcp.json` pinned cwd-independent ABSOLUTE `<canonical_project_root>/.graphtor/bin/graphtor-docs[.exe]` command WITH managed marker + managed `.gitignore`; the
  full-path `sources.yaml`/`.gitignore`/`.mcp.json` orchestration runs ONLY on
  `--with-ingestion`; default (no flag) routes to `install_minimal()` and creates
  none of these (no `sources.yaml`, no `.gitignore`, minimal serve `.mcp.json`);
  P2-T2a implements no scaffold behaviour.
* Posture: test-first. Depends on P2-T2a and P2-T3.

**P2-T3 — Shared `.mcp.json` writer: resolution ladder + managed marker + atomic write** (code, test-first) — Phase-2 root
* Changes: make the MCP config writer (`generate_mcp_config`/`managed_server_value`,
  `src/workspace/mcp_config.rs:84-140`) the shared foundation both install paths
  consume: (a) a binary-resolution LADDER that LOCKS a single cwd-independent
  model — the ACTUAL ABSOLUTE project-root path
  `<project_root>/.graphtor/bin/graphtor-docs` (+ platform ext), computed from the
  canonical root at install time, when a managed binary exists (a bare
  workspace-relative `.graphtor/bin/graphtor-docs` string does NOT resolve when the
  MCP client launches from a different cwd — PR #88 thread PRRT_kwDORiB5E86RNyUs);
  else the bare `graphtor-docs` PATH command (no `.exe`; Windows resolves via
  `PATHEXT`); (b) a managed-entry PROVENANCE MARKER in the server entry so
  `uninstall` (P2-T5b) can identify the managed entry; (c) ATOMIC temp-file +
  rename writes with stable key ordering (Principle IX); (d) a LOCKED fixed-key
  collision contract for the `graphtor-docs` key (review thread E / comment
  3585938864) — the current writer does `servers.insert("graphtor-docs", ...)`
  (`mcp_config.rs:131-134`), overwriting an unmarked user entry with that key.
  Replace it with a FOUR-way decision (review thread E / comment 3585938864,
  extended by PR #88 comments 3588876135, 3588876172): key ABSENT → insert a
  marked managed entry atomically; PRESENT AND carrying the provenance marker →
  UPDATE atomically in place; PRESENT, UNMARKED, but EXACTLY matching the legacy
  generated shape (exact NORMALIZED command equality to the HISTORICAL RELATIVE
  string `.graphtor/bin/graphtor-docs`
  OR `.graphtor/bin/graphtor-docs.exe` written by the pre-marker writer, AND args == `["serve"]` AND stdio
  transport) → MIGRATE IN PLACE atomically by ADDING the provenance marker and
  refreshing the managed value to the new absolute-path + marker shape (this is the current release's OWN pre-marker entry
  — R14 backward-compat for existing installs / reinstall / `--with-ingestion` —
  NOT a user collision); PRESENT, UNMARKED, and any OTHER shape (user-authored) →
  FAIL CLOSED with a `GraphtorError::Config` collision error, leaving the file
  byte-for-byte unchanged (never overwrite). The legacy shape uses EXACT equality
  (the SAME predicate as P2-T5b removal), never CONTAINS, and recognises only the
  HISTORICAL RELATIVE pre-marker shape; the marker (not the command string) is the
  forward-looking managed identity, so the going-forward value is absolute. The current writer hardcodes a RELATIVE `.graphtor/bin/...`
  (`mcp_config.rs:86`) and always creates it; restructured as the Phase-2 ROOT so
  binary resolution + marker + atomic write + collision contract exist BEFORE the
  minimal install (P2-T1) claims a working install (review thread 7).
* Files: `src/workspace/mcp_config.rs`.
* Tests: ladder pure-function (managed binary → ACTUAL ABSOLUTE project-root
  path+ext, cwd-independent; none → bare PATH
  command, no `.exe`); managed marker present; atomic write with stable ordering;
  collision — absent key → created; marked entry → updated in place; **UNMARKED
  entry EXACTLY matching the historical relative legacy shape → migrated in place (marker
  added, managed value refreshed to absolute+marker), NOT a collision (R14 backward-compat)**;
  UNMARKED user `graphtor-docs` key of any OTHER shape → install fails closed and
  the user entry is preserved byte-for-byte; unrelated user servers preserved.
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
  root. The footprint rewrite MUST PRESERVE the existing public
  `uninstall(project_root, keep_config)` contract: `keep_config == true` retains
  `.graphtor/config` (existing behaviour + test, `uninstall.rs:34-77,112-124`) —
  the new artifact allowlist branches on `keep_config` to skip `config/` deletion
  while still retaining user-dropped `*.db` (PR #88 thread PRRT_kwDORiB5E86RNyVa);
  do NOT drop the `keep_config` parameter during the rewrite.
  `.gitignore` parity (thread 8): a minimal uninstall MUST NOT remove a
  `.gitignore` it never created; only a full-footprint uninstall touches the
  managed `.gitignore` block. The PA-3 approval prompt enumerates the deletion
  set. Managed `.mcp.json` removal is P2-T5b; upgrade parity is P2-T5c (review
  thread 10 split — the original P2-T5 spanned four files).
* Files: `src/workspace/uninstall.rs`, `src/main.rs` (`cmd_uninstall` prompt).
* Tests: minimal → no user-db deletion and no `.gitignore` removal; full +
  `keep_config == false` → removes graphtor subdirs + managed `.gitignore` while a
  dropped `.db` SURVIVES; full + `keep_config == true` → removes managed subdirs but
  PRESERVES `.graphtor/config` while the dropped `.db` still SURVIVES;
  no symlink-follow; PA-3 prompt enumerates the deletion set.
* Posture: test-first. Depends on P2-T1 and P2-T2b.

**P2-T5b — Managed `.mcp.json` entry removal by provenance marker** (code, test-first)
* Changes: add managed-entry removal to the MCP config module — `uninstall`
  removes the managed server entry matched by (a) the P2-T3 provenance marker
  (primary) OR (b) a NARROW LEGACY MATCH for entries generated by the current
  pre-marker writer, which have no provenance field (review thread F / comment
  3585938909; tightened by PR #88 comments 3588876275, 3588876318). LEGACY MATCH
  (EXACT equality): the shape emitted by `managed_server_value`
  (`mcp_config.rs:273-279`) — command EQUALS the normalized value
  `.graphtor/bin/graphtor-docs` OR `.graphtor/bin/graphtor-docs.exe` (exact string
  equality after path normalization; NEVER a CONTAINS/substring test) AND args ==
  `["serve"]` AND transport == `"stdio"`. Remove/migrate ONLY marker-matched or
  exact-legacy-matched entries; leave all other unmarked/user entries untouched;
  atomic rewrite.
* Files: `src/workspace/mcp_config.rs`, `src/workspace/uninstall.rs`.
* Tests: a MARKED managed entry is removed while a user-authored entry SURVIVES;
  an UNMARKED LEGACY entry (exact generated shape) is ALSO removed so upgraders
  do not retain a stale registration; **a user command with an extra PREFIX or
  SUFFIX around the legacy path (e.g. `/opt/tools/.graphtor/bin/graphtor-docs` or
  `.graphtor/bin/graphtor-docs-wrapper`) SURVIVES even when args are `["serve"]`
  and transport is `"stdio"` (exact-equality, not CONTAINS)**; a user entry that
  references the binary path but with different args is PRESERVED; a user-only
  `.mcp.json` is unchanged; rewrite is atomic.
* Posture: test-first. Depends on P2-T5a and P2-T3.

**P2-T5c — Minimal/full upgrade parity** (code, test-first)
* Changes: `upgrade()` (`src/workspace/upgrade.rs:43`) of a minimal (no-bin)
  install treats missing bin/subdirs as a no-op success; a full install upgrade
  replaces the binary as today; idempotent. Purely ADDITIVE: because P2-T1
  preserves the full-scaffold `install()`, the existing full-install upgrade path
  and its tests stay green throughout — this unit only ADDS the minimal no-op case
  (review thread 8 follow-on), not a deferred fix for a broken upgrade.
* Files: `src/workspace/upgrade.rs`.
* Tests: minimal upgrade → no-op success; full upgrade → replaces binary (existing
  behaviour, still green); repeat idempotent.
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

Phase 1 internal (P1-T0 is the root gate):
  P1-T0 ──> P1-T1 ──> P1-T2 ──> P1-T3 ──> {P1-T7, P1-T8}
  P1-T1 ──> P1-T5
  P1-T3 ──> P1-RF1 ──> P1-RF2 ──> P1-RF3 ──> P1-RF4 ──> P1-RF5 ──> P1-T6 ──> P1-T4
                                    (the five variant-safe pre-refactors gate P1-T6;
                                     P1-T6 also depends on P1-T3; P1-T4 also depends
                                     on P1-T3 and hardens the RO primitive proven in
                                     P1-T0)

Phase 2 internal (P2-T3 is the root):
  P2-T3 ──> P2-T1 ──> {P2-T2a, P2-T4, P2-T6, P2-T7a}
  P2-T2a ──> P2-T2b                (P2-T2b also depends on P2-T3)
  {P2-T1, P2-T2b} ──> P2-T5a ──> P2-T5b   (P2-T5b also depends on P2-T3)
  {P2-T1, P2-T2b} ──> P2-T5c
  {P2-T7a, P2-T2b} ──> P2-T7b
```

No cycles. Suggested execution order (matches shipment 045-S): P1-T0 → P1-T1 →
P1-T2 → P1-T3 → P1-RF1 → P1-RF2 → P1-RF3 → P1-RF4 → P1-RF5 → P1-T6 → P1-T4 → P1-T5 →
P1-T7 → P1-T8 → P2-T3 → P2-T1 → P2-T2a → P2-T2b → P2-T4 → P2-T6 → P2-T5a →
P2-T5b → P2-T5c → P2-T7a → P2-T7b. Notes: P1-T0
proves the engine read-only primitive first (de-risks the shipment's primary
invariant before any dependent work); the five `Source`-variant pre-refactors
(P1-RF1..P1-RF5) run after P1-T3 and BEFORE P1-T6 so every `Source::Local`
consumer — production AND test — is variant-safe before the additive
`type: database` variant is added (review threads 2/2b); P1-T3 and P1-T4
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
| Engine/filesystem write to a served read-only db (Cozo exposes no RO flag) | Engine-enforced read-only open (immutable/`mode=ro`) + before/after no-write verification, PROVEN first as the Phase-1 root gate; fail closed AND block 045-S if unattainable (P1-T0; hardening P1-T4) |
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
| Adding `Source::Database` breaks 18 production + 7 test `Source::Local` consumers | Variant-safe accessor pre-refactors (P1-RF1..P1-RF5) route every consumer — production AND test, internal AND external — through `Source::id()`/`as_local()`/`is_ingestible()` BEFORE the variant is added; each independently green (PR #88 threads 2/2b + fourth-pass audit) |
| Root-scan discovery drops existing serve candidates (fresh generation target / explicit `--db-path`) | Union preserves `discover_db_files` candidates incl. not-yet-created targets + explicit `--db-path`; zero-db exit only when the full union is empty (P1-T1; PR #88 thread 1) |
| Reinstall/`--with-ingestion` fails on the release's own pre-marker `.mcp.json` entry | Four-way collision matrix migrates the exact legacy shape in place by adding the marker (R14); fail-closed only for other unmarked entries (P2-T3; PR #88 thread 3) |
| `CONTAINS` legacy match deletes a look-alike user `.mcp.json` command | Exact normalized command equality to `.graphtor/bin/graphtor-docs`[.exe] (never CONTAINS) + prefix/suffix preservation test (P2-T3, P2-T5b; PR #88 threads 3, 5) |
| Install orchestration split leaves T2a or T2b unable to meet acceptance | T2b owns the full-path `cmd_install` orchestration (adds `src/main.rs`, < 3 files); T2a stays routing-only (P2-T2b; PR #88 thread 4) |

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
  mutate-guard alone (P1-T0 engine-level proof — the Phase-1 root gate; P1-T3
  gating). Read-only is the fail-safe default on any ambiguity. If engine
  read-only cannot be PROVEN, this is a hard STOP CONDITION (not a silent
  degrade): P1-T0 (050.009-T) and shipment 045-S are BLOCKED and Dark Mode halts
  before Phase 2 / minimal install — no INV-1 claim on the mutate-guard alone and
  no feature-disable fallback merge (see P1-T0 feasibility stop condition).
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
  and Phase-2 docs in lockstep with the behaviour change (informs P1-T8, P2-T7b (051.011-T)).
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
  serve/status-scoped `serve_discovery` module and explicitly forbids ENLARGING
  `discover_db_files`/`split_plan_by_database` with auto-discovered dbs; a
  characterization test asserts sync does not enlarge its db set from dropped dbs.
  P1-RF4's non-ingestible `as_local()` EXCLUSION filter on those same functions is
  compatible: it can only SHRINK the generation set (dropping `type: database`
  read-only entries), never enlarge it, so it upholds the same invariant.
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
  install copies no binary.** *Resolution*: P2-T1 preserves the full `install()`
  and adds a sibling `install_minimal()`; `binary_path` becomes `Option<PathBuf>`
  (full → `Some`, minimal → `None`) and callers handle `None` (see second-review
  thread D).
* **P1-6 (Rust): `SourceConfig` serde change must be additive.** *Resolution*:
  P1-T6 adds a DISTINCT additive `type: database` variant (LocalSource untouched)
  and a pre-change round-trip parse test (see second-review thread C for the
  locked contract).
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
* Atomic config writes + stable key ordering (P2-T3 (051.004-T)) — Principle IX.
* Positive posture log line — Principle V (Constitution Check).
* P2-T2 file-count split: binary resolution moved to its own unit P2-T3.
* Pre-v4/v4 fixture availability precondition (P1-T0, P1-T4, P1-T5).

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

* **Thread 1 (INV-1, security)** — an engine/filesystem-level read-only open
  (immutable/`mode=ro`) with automated before/after no-write verification and a
  fail-closed fallback is required, because `open_sqlite_readonly` reuses a
  write-capable `DbInstance` and Cozo's SQLite backend exposes no read-only flag
  (`src/db/store.rs:107-110,384`). P1-T3's INV-1 claim is scoped to gating only.
  (The second review split the PROOF into the P1-T0 root gate — see below.)
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
051.008/051.009/051.010/051.011-T); shipment 045-S was 21 items after the first
review; the dependency DAG was re-validated acyclic.

### Second-Review Remediation (PR #88 second review / threads A–F)

The 7 planning/backlog threads from the second review were remediated in-place
(planning/backlog only; no code, no build):

* **A (decision lines 234, 257)** — both authoritative Decision summaries now
  state generation requires a `local` source whose path exists AND resolves to at
  least one ingestible file; an existing-but-empty source stays read-only (aligns
  with the locked "resolvable real source" definition).
* **B (050.003-T)** — the engine/filesystem read-only feasibility PROOF is split
  into a new Phase-1 ROOT task **P1-T0 (050.009-T)**; P1-T1 depends on it so the
  whole Phase-1 chain and shipment 045-S are gated on the proof (≤ 2 files, ≤ 3
  scenarios). P1-T4 (050.003-T) refocuses on v4 gate parity +
  ATTACH/extension/single-file hardening, consuming P1-T0's primitive.
* **C (050.006-T)** — the explicit read-only db entry is a DISTINCT additive
  `type: database` variant `{ id, path }` (not `read_only: true`); required
  fields, alias/name semantics, workspace-contained `validate_path` validation,
  and backward-compat are locked with a fixture.
* **D (051.001/002/008/010-T)** — the consumption-first install PRESERVES the
  full-scaffold `install()` and adds a sibling `install_minimal()`, so existing
  upgrade tests stay green and no task leaves the suite red; T2a/T2b/T5c updated.
* **E (051.004-T)** — the `.mcp.json` writer locks a fixed-key collision
  contract: absent → create marked entry; marked → update in place; unmarked
  user `graphtor-docs` → fail closed (never overwrite), with an exact test.
* **F (051.009-T)** — uninstall keeps a NARROW legacy-removal match (old managed
  shape: `.graphtor/bin/graphtor-docs[.exe]` command + `["serve"]` args + `stdio`
  transport) so unmarked pre-change entries are removed while user entries are
  preserved.

Backlog impact (second review): Phase 1 grew from 8 to 9 task units (1 new:
050.009-T); shipment 045-S is now 22 items; the dependency DAG was re-validated
acyclic and the manifest is topologically ordered (P1-T0 first).

### Third-Review Remediation (PR #88 third pass / 12 threads)

The 12 Copilot threads from the third review were remediated in-place
(planning/backlog only; no code, no build), grouped into 7 fixes:

* **Serve candidate union (3588875971 plan, 3588876019 050.002-T)** — P1-T1's
  served set is now a UNION preserving the existing `discover_db_files` candidates
  (configured targets incl. not-yet-created generation targets, explicit
  `--db-path`) plus root-scan entries; only auto-discovered entries are kept out of
  `discover_db_files`/sync; the zero-db exit fires only when the full union is
  empty; regression tests for a missing generation target and an explicit
  `--db-path` were added.
* **`Source::Database` consumer refactors (3588876057 plan, 3588876095 050.006-T;
  fourth-pass exact-count audit)** — the additive variant breaks **18 production**
  irrefutable/non-exhaustive `Source::Local` consumers (3 config validation, 2
  acquire/plan, 1 acquire/mod, 1 pipeline/mod, 2 sync/mod, 9 main.rs) AND **7
  breaking test consumers** (3 src/config/source.rs colocated, 1 src/main.rs
  colocated, 1 tests/config_test.rs, 2 tests/pipeline_format_test.rs), so FIVE
  dependency-ordered variant-safe pre-refactors P1-RF1..P1-RF5
  (050.010 → 050.011 → 050.012 → 050.013 → 050.014-T, each ≤ 2 files, independently
  green) route every consumer through `Source::id()`/`as_local()`/`is_ingestible()`
  BEFORE P1-T6 adds the variant; acquisition, sync, AND generation db-discovery/splitting
  ignore `Database` by design (per-consumer gates: RF2 acquisition plan loop, RF3 sync,
  RF4 `discover_db_files`/`split_plan_by_database`); `database()` stays local-target-only and a distinct
  `served_db_path()` exposes the read-only path; P1-T6 adds the variant + a mixed local+database serve+sync+generation
  test and keeps `cargo build`/clippy/`cargo test` green.
* **Writer collision migration (3588876135 plan, 3588876172 051.004-T)** — the
  P2-T3 collision matrix is now FOUR-way: absent → create marked; marked → update
  in place; UNMARKED but exact HISTORICAL RELATIVE legacy shape → migrate in place by adding the marker
  (R14 backward-compat for the release's own pre-marker entry); any other unmarked
  entry → fail closed. The going-forward managed command is the ACTUAL ABSOLUTE
  project-root path (cwd-independent; PR #88 thread PRRT_kwDORiB5E86RNyUs) with the
  marker as forward-looking identity; legacy recognition uses exact normalized
  equality to the historical relative string, `["serve"]` args, stdio
  transport; tests for both new cases.
* **T2b full-path scaffold (3588876203 plan, 3588876245 051.008-T)** — P2-T2b adds
  `src/main.rs` (still < 3 files) and owns the full-path `cmd_install` orchestration
  (`sources.yaml`, managed `.gitignore`, `.mcp.json` writer calls), guarded to
  `--with-ingestion`; P2-T2a stays routing-only.
* **Legacy removal exact match (3588876275 plan, 3588876318 051.009-T)** — P2-T5b's
  legacy predicate is EXACT normalized equality to `.graphtor/bin/graphtor-docs` or
  `.graphtor/bin/graphtor-docs.exe` (never CONTAINS); a prefix/suffix look-alike
  command with `["serve"]` args + stdio survives (new preservation test). This
  supersedes the imprecise "narrow match" wording in second-review thread F and
  aligns with the P2-T3 case-3 migration predicate (extends thread E to four-way).
* **050-F summary (3588876360)** — the feature summary now reads P1-T0..P1-T8 plus
  the five P1-RF1..P1-RF5 compatibility sub-units.
* **051-F summary (3588876398)** — the feature summary now lists the actual 11
  Phase-2 units (P2-T1, P2-T2a, P2-T2b, P2-T3, P2-T4, P2-T5a, P2-T5b, P2-T5c, P2-T6,
  P2-T7a, P2-T7b).

Backlog impact (third review): Phase 1 grew from 9 to 13 task units (4 new
variant-safe pre-refactors: 050.010/050.011/050.012/050.013-T); the dependency
DAG was re-validated acyclic and the manifest stays topologically ordered
(P1-T0 first).

Backlog impact (fourth-pass irrefutable-consumer audit): the earlier
"10 external sites" figure was an undercount — a full source audit confirms **18
production** irrefutable/non-exhaustive `Source::Local` consumers plus **7
breaking test consumers**. P1-RF1 (050.010-T) expands to cover
`src/config/validation.rs:302` (`intake_key`) and the three `src/config/source.rs`
colocated test bindings; P1-RF4 (050.013-T) expands to cover all nine `src/main.rs`
production sites and its one colocated test; and a new tail unit **P1-RF5
(050.014-T)** pre-refactors the external integration tests (`tests/config_test.rs`,
`tests/pipeline_format_test.rs`). Phase 1 is now **14 task units**, shipment 045-S is
**27 items**, P1-T6 (050.006-T) now depends on P1-RF5, and the DAG stays
acyclic/topological (P1-T3 → P1-RF1 → P1-RF2 → P1-RF3 → P1-RF4 → P1-RF5 → P1-T6,
P1-T0 first).
