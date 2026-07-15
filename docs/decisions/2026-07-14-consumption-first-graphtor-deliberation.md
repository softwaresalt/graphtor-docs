---
title: "Consumption-First Graphtor Context Server"
description: "Make graphtor a read-only, zero-config context server by default; keep generation/sync as an opt-in path for authoring workspaces."
topic: "Read-only serve auto-discovery + content-derived mode + consumption-first install"
depth: "deep"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-07-14-consumption-first-graphtor-plan.md"
stash_ids:
  - "79B5A7BC"
  - "B333B9B8"
tags:
  - "serve"
  - "install"
  - "read-only"
  - "mode-detection"
  - "consumption-first"
---

## Problem Frame

graphtor is used in two very different roles today, but the code treats every
workspace as a potential sync engine:

* **Generation role** — an authoring/build workspace (e.g. this repo,
  `C:\Source\GitHub\graphtor`) has real docline sources declared in
  `sources.yaml`. `sync` GENERATES a `.db` from that content (write path).
* **Consumption role** — an agent workspace has only `.db` files dropped into
  `.graphtor/`. It should `serve` those databases READ-ONLY and must NEVER
  sync (a sync there is only ever accidental data corruption risk).

Two gaps block the consumption role:

1. **Serve does not auto-discover dropped `.db` files.** `discover_db_files`
   (`src/main.rs:244`) only iterates `config.sources` or a single explicit
   `--db-path`; it never scans `.graphtor/` for dropped databases. Worse,
   `open_serve_databases` (`src/main.rs:2344`) opens every db read-write AND
   read-only, and `cmd_serve` (`src/main.rs:2447`) spawns a background
   incremental **write** sync whenever `sources.yaml` has sources — the
   accidental-resync risk in a consumer workspace.
2. **Install scaffolds ingestion machinery by default.** `install()`
   (`src/workspace/install.rs:34`) creates all of
   `.graphtor/{bin,data,cache,config,logs}` (`GRAPHTOR_SUBDIRS`,
   `src/workspace/paths.rs:17`), copies the binary, and `cmd_install`
   (`src/main.rs:3002`) writes a template `sources.yaml` plus an
   ingestion-oriented post-install message. A consumer workspace needs only the
   `.graphtor/` root and a minimal serve `.mcp.json`.

The two are coupled: **B333B9B8 (consumption-first install) depends on 79B5A7BC
(serve auto-discovery)** — a minimal install is meaningless until `serve`
auto-discovers dropped databases. They share the "consumption-first graphtor"
theme and must ship together, phased.

### Who cares and why

* **Agent workspaces / operators** — want to drop a `.db` into `.graphtor/` and
  have `serve` expose it read-only with zero config, zero sync risk.
* **docline authors / the graphtor dev workspace** — must retain the full
  generation/sync/write path with no regression.

### Constraints

* Default-safe: an ambiguous or stale/empty `sources.yaml` in a consumer
  workspace MUST NOT enable sync (fail read-only).
* Backward compatibility: existing full installs and the sources-driven
  multi-db serve path in the dev workspace must keep working.
* Security/containment: auto-discovery scans only the `.graphtor/` root; skip
  lock/index/tmp files, `.graphtor/models`, and generated artifacts; enforce
  path containment.
* Schema v4 pre-sync gate (`SCHEMA_VERSION = 4`, `src/db/schema.rs:39`) still
  applies to served read-only dbs (pre-v4 dbs are refused, operator must sync).
* `#![forbid(unsafe_code)]`, `Result<T, E>` propagation, clippy pedantic — all
  per the constitution.

### Success criteria

* In a consumer workspace with a minimal `.mcp.json` (`serve` only) and `.db`
  files in `.graphtor/`, the MCP tools + `status` query them read-only and
  `serve` never writes/resyncs.
* In the graphtor dev workspace (real sources), `sync` still works and it both
  generates and serves.
* A stale/empty `sources.yaml` in a consumer workspace does NOT enable sync.
* Pre-v4 dbs are gated with a clear message.
* `install` in a fresh consumer workspace creates only `.graphtor/` + a minimal
  serve `.mcp.json`; ingestion scaffold is opt-in; docs cover both paths.

### Out of scope

* Changing the docline authoring format or the `sync` acquisition/embedding
  pipeline internals.
* Remote/networked serving, auth, or multi-tenant hosting.
* New MCP query tools beyond exposing already-discovered read-only dbs.

## Research Findings

Grounded in the current source (verified this session):

* `discover_db_files(base_db_path, source_config)` (`src/main.rs:244`) returns
  db paths from `config.sources` or falls back to the single `base_db_path`. No
  directory scan of `.graphtor/`.
* `cmd_serve` (`src/main.rs:2383`) is fail-closed on a malformed registry and on
  a missing explicit `--config`; it computes `db_paths` via `discover_db_files`,
  opens them through `open_serve_databases`, then conditionally spawns
  `spawn_background_sync` only when `source_config.sources` is non-empty
  (`src/main.rs:2447-2462`). The "no sources" branch already skips sync — the
  hook point for content-derived mode is precisely this source-resolution step.
* `open_serve_databases` (`src/main.rs:2344`) already opens a read-only store
  via `DataStore::open_sqlite_readonly` for every db, and gates each on
  `needs_v4_migration`. It also opens a read-write store + acquires a write lock
  unconditionally — the change surface for a read-only classification.
* `managed_server_value` (`src/workspace/mcp_config.rs:273`) already emits a
  minimal `{command, args:["serve"], transport:"stdio"}` with NO env — the
  minimal-install `.mcp.json` need is largely satisfied; the remaining question
  is the `command` value (PATH `graphtor-docs` vs `.graphtor/bin/...`).
* `install()` unconditionally creates `GRAPHTOR_SUBDIRS`
  (`["bin","data","cache","config","logs"]`) and copies the binary.
  `cmd_install` writes `sources.yaml` via `init_sources_yaml` and prints an
  ingestion-first "next steps" message (`src/main.rs:3079-3082`).
* `run_doctor` (`src/workspace/doctor.rs:124`) asserts the full layout: binary
  present, `config/sources.yaml`, and `graph.db`. It will emit failures against
  a minimal consumption layout unless taught to tolerate it.
* `InstallArgs` (`src/cli/mod.rs:274`) currently exposes only `no_gitignore` and
  `force_unlock` — a new opt-in flag slots in cleanly.
* `cmd_uninstall` / `uninstall()` (`src/main.rs:3194`, `src/workspace/uninstall.rs:34`)
  and `cmd_upgrade` / `upgrade()` (`src/main.rs:3147`, `src/workspace/upgrade.rs:43`)
  exist and must stay consistent with whichever footprint was created.

Prior learnings (`docs/compound/`): no directly relevant serve/install entry
(confidence: low). `keep-docs-synchronized-with-implementation.md` applies to
the documentation tasks — keep docs in lockstep with the behaviour change.

## Options Evaluated

### Mode-detection axis

#### Option M1: Content-derived mode (RECOMMENDED)

A workspace is in generation/write mode ONLY when it has RESOLVABLE ingestion
sources with real content (a `sources.yaml` whose `local` source paths exist).
Otherwise it is read-only consumption, even if a stale/empty `sources.yaml`
exists. Default-safe: on any ambiguity, fail read-only. An explicit override
flag exists only as an escape hatch.

* **Pros**: No hardcoded paths; the dev workspace qualifies naturally (real
  sources); consumer workspaces are safe by default; a stale `sources.yaml`
  cannot re-enable sync; aligns with existing `cmd_serve` source-resolution
  branch points.
* **Cons**: "Resolvable source" needs a precise, testable definition; edge
  cases (source path exists but empty) must be specified.
* **Effort**: medium. **Fit**: excellent.

#### Option M2: Hardcoded dev-workspace path / env flag for mode

* **Pros**: trivially simple.
* **Cons**: brittle; breaks if the repo moves; a stale env var silently
  re-enables sync; rejected by the operator's stated intent.
* **Effort**: low. **Fit**: poor.

### Serve-discovery axis

#### Option A: Auto-discovery of `*.db` in `.graphtor/` top level

* **Pros**: true zero-config; matches the desired UX (drop a `.db`, serve it).
* **Cons**: no way to name/alias/point at an external db; discovery filtering
  rules must exclude generated artifacts.
* **Effort**: medium. **Fit**: good but incomplete alone.

#### Option B: Explicit read-only db entry in `sources.yaml`

* **Pros**: explicit, supports named/aliased dbs.
* **Cons**: not zero-config; a consumer workspace shouldn't need `sources.yaml`
  at all; contradicts the consumption-first install goal.
* **Effort**: medium. **Fit**: partial.

#### Option C: Combination — auto-discovery default + optional explicit entries (RECOMMENDED)

Zero-config auto-discovery of `.graphtor/*.db` is the default posture. An
optional explicit read-only db entry (in `sources.yaml`) covers named or
aliased, **workspace-contained** databases for advanced/generation workspaces.
Out-of-root/external databases are explicitly OUT of Phase-1 scope (deferred,
no committed timeline).

* **Pros**: zero-config for the common consumer case AND an explicit escape
  hatch for named/aliased workspace-contained dbs; superset of A and B; no
  regression for the sources-driven dev path.
* **Cons**: two code paths to test (discovery + explicit).
* **Effort**: medium-high. **Fit**: best.

### Install axis

#### Option I1: Consumption-first default + opt-in ingestion scaffold (RECOMMENDED)

Default `install` creates only the `.graphtor/` root + a minimal serve
`.mcp.json` (PATH command `graphtor-docs`, args `["serve"]`); NO `sources.yaml`,
NO `config/bin/cache/data/logs`. Ingestion/generation scaffold becomes opt-in
via a new `install --with-ingestion` flag (creates the full layout, copies the
binary, writes `sources.yaml`). The graphtor dev workspace uses the ingestion
path.

* **Pros**: minimal footprint for consumers; ingestion remains one flag away;
  the dev workspace keeps full capability; `managed_server_value` already emits
  the minimal shape.
* **Cons**: uninstall/upgrade/doctor must tolerate both footprints; PATH-vs-bin
  binary resolution precedence must be decided.
* **Effort**: medium-high. **Fit**: best.

#### Option I2: Keep full scaffold, add a `--minimal` post-hoc prune

* **Pros**: smaller diff to `install()`.
* **Cons**: still writes ingestion files then removes them; awkward; doesn't
  match "consumption-first default".
* **Effort**: medium. **Fit**: poor.

## Trade-off Comparison

| Criterion | M1 content-derived | M2 hardcoded | A discovery | B explicit | C combination | I1 opt-in | I2 prune |
|---|---|---|---|---|---|---|---|
| Zero-config UX | n/a | n/a | high | low | high | high | medium |
| Safety (no accidental sync) | high | low | medium | medium | high | high | medium |
| Backward compat (dev path) | high | medium | medium | high | high | high | high |
| Named/aliased db support (workspace-contained) | n/a | n/a | none | yes | yes | n/a | n/a |
| Complexity | medium | low | medium | medium | med-high | med-high | medium |
| Operator intent alignment | full | rejected | partial | partial | full | full | partial |

## Decision

**Locked decisions (with rationale):**

1. **Mode detection is CONTENT-DERIVED (Option M1), default-safe.** A workspace
   is in generation/write mode only when it has resolvable ingestion sources
   with real content (a `sources.yaml` with a declared `local` source whose
   path exists AND resolves to at least one ingestible file; an
   existing-but-empty source remains read-only). Otherwise it is read-only
   consumption — even when a stale or empty
   `sources.yaml` is present. On ambiguity, **fail read-only**. An explicit
   override flag (e.g. `--read-only` / `--allow-sync`) is added ONLY as an
   escape hatch, never as the primary control. *Rationale*: role must be derived
   from real content, not a path or a lingering config file, so a consumer
   workspace can never accidentally re-enable a background write sync. This maps
   directly onto the existing `cmd_serve` source-resolution branch
   (`src/main.rs:2447-2467`), which already has a "has sources / no sources"
   split.

2. **Serve discovery is COMBINATION (Option C).** Zero-config auto-discovery of
   `*.db` at the top level of `.graphtor/` is the default; optional explicit
   read-only db entries in `sources.yaml` cover named/aliased,
   **workspace-contained** databases only (each entry is canonicalized and
   validated to stay within the same authorized root as auto-discovery;
   out-of-root/external paths are REJECTED, not served, and MUST NOT broaden
   authorized roots — external-path support is explicitly DEFERRED out of
   Phase-1 scope). Discovered no-real-source dbs are served **read-only**: they are
   **never** background-synced, and generation/resolution **must not perform any
   write-side v4 prune (or other in-place write migration) on them**. The
   read-only posture does NOT relax the pre-v4 serve refusal gate — a pre-v4
   read-only db is still refused at serve time (operator must sync); it is simply
   never written to or pruned in place. Source-backed dbs (a `local`
   source whose path exists AND resolves to at least one ingestible file) keep
   read-write + background sync on the generation side. *Rationale*: C is a
   strict superset of A and B — it delivers the
   zero-config consumer UX while preserving the explicit (workspace-contained)
   and sources-driven generation paths with no regression. Confirmed over the
   operator's initial lean toward C.

3. **Install is CONSUMPTION-FIRST by default with opt-in ingestion (Option
   I1).** Default `install` creates only the `.graphtor/` root + a minimal serve
   `.mcp.json` referencing the PATH command `graphtor-docs` with args
   `["serve"]`; it writes NO `sources.yaml` and creates NONE of
   `config/bin/cache/data/logs`. A new `install --with-ingestion` (and/or the
   existing `init`) opt-in creates the full generation scaffold (config +
   `sources.yaml` + data/cache/logs + bin/binary copy). The graphtor dev
   workspace uses the ingestion path. Binary resolution precedence: prefer the
   managed `.graphtor/bin/graphtor-docs` binary copy when the ingestion scaffold
   created it — serialized in `.mcp.json` as its cwd-independent ABSOLUTE
   `<canonical_project_root>/.graphtor/bin/graphtor-docs` path (see the locked
   `.mcp.json` command value below) — otherwise reference the PATH command. `uninstall`, `upgrade`, and `doctor`
   are updated to tolerate both footprints; behaviour stays idempotent and
   backward-compatible with existing full installs. *Rationale*: consumers get a
   minimal, sync-free footprint; authors are one flag away from the full scaffold.

**Design posture summary**: read-only serve is the DEFAULT; the write/sync
posture is enabled only by the presence of real generation sources (the dev
workspace being the primary such case).

## Rejected Alternatives

* **M2 (hardcoded path/env mode)** — brittle and unsafe; a stale env var or
  moved repo silently changes posture. Contradicts the content-derived,
  fail-safe requirement.
* **A alone** — no support for named/aliased dbs.
* **B alone** — forces `sources.yaml` on consumers, contradicting
  consumption-first install.
* **I2 (full scaffold then prune)** — writes ingestion files just to remove
  them; not a genuine consumption-first default.

## Resolved Questions (locked post plan-review and PR #88 review)

* **Precise "resolvable real source" definition** — **LOCKED (post-review)**: a
  source is resolvable for generation mode only when its declared `local` path
  exists AND resolves to at least one ingestible file (non-empty resolvable
  content). Mere path existence is NOT sufficient — an existing-but-empty or
  incidental directory (e.g. `.`, `./docs`) classifies as CONSUMPTION
  (read-only). A malformed/unparseable `sources.yaml` remains a fail-closed hard
  error (never silently downgraded). This closes the INV-1/INV-3 bypass raised
  in plan review.
* **Override flag naming/semantics** — **LOCKED (PR #88 thread 12)**:
  `--read-only` is the chosen flag — the primary safety escape hatch that forces
  consumption (read-only) posture regardless of resolved sources. A symmetric
  `--force-sync` flag is intentionally ABSENT in this phase (deferred, no
  committed timeline); read-only is the fail-safe default, so a force-consumption
  override is the only escape hatch required. See plan P1-T7.
* **`.mcp.json` command value on Windows** — **LOCKED (PR #88 thread 12)**: the
  binary-resolution ladder is fixed and cwd-independent — reference the pinned
  ABSOLUTE `<canonical_project_root>/.graphtor/bin/graphtor-docs` (+ platform ext),
  computed from the canonical project root at install time so it resolves regardless
  of the MCP client's launch cwd, when the ingestion scaffold created a managed
  binary, otherwise the bare `graphtor-docs` PATH command with NO `.exe` (Windows
  resolves via `PATHEXT`); append the platform ext ONLY on the pinned bin path. The
  workspace-relative `.graphtor/bin/graphtor-docs` string is reserved ONLY for legacy
  exact-match recognition, never the going-forward managed value (PR #88 thread
  PRRT_kwDORiB5E86RNyUs). See plan P2-T3.
* **Discovery filter list** — **LOCKED (plan P1-T1)**: skip `*.lock`, index/tmp
  files, `.graphtor/models`, and generated artifacts; the served set is `*.db` by
  extension in the `.graphtor/` root minus that skip-list, with canonicalize +
  containment.
* **Explicit read-only entry path scope (workspace-contained vs external)** —
  **LOCKED (adversarial follow-up)**: Phase-1 explicit read-only entries are
  restricted to WORKSPACE-CONTAINED named/aliased databases — each entry is
  canonicalized and validated (`validate_path`) to stay within the same
  authorized root as auto-discovery; out-of-root/external paths are REJECTED and
  MUST NOT broaden authorized roots. External/out-of-root database support is
  explicitly DEFERRED out of Phase-1 scope (no committed timeline). See plan
  P1-T6.

All questions above are now decided; none remain open (consistent with
`decision_status: decided`).

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Accidental background sync in a consumer workspace | Data corruption / silent writes | Content-derived mode, fail read-only; never spawn `spawn_background_sync` for no-real-source dbs; gate write-store open behind resolved sources |
| Regression in dev-workspace generation/serve | Broken build/test of powerbi.db/postgresql.db | Preserve sources-driven path unchanged; add characterization tests for the dev-workspace multi-db serve before refactor |
| Auto-discovery picks up non-db / generated artifacts | Serve errors or serving junk | Explicit discovery filter (extension + skip list) with unit tests; only scan `.graphtor/` root, not recursive |
| Pre-v4 db served without gating | Stale/incorrect data | Keep the existing v4 gate in `open_serve_databases`; apply it to auto-discovered read-only dbs too |
| doctor/uninstall/upgrade break on minimal layout | Confusing failures | Make doctor tolerant of the consumption layout; uninstall/upgrade parity tests for both footprints |
| Backward compat for existing full installs | Existing users disrupted | Idempotent install; detect existing full layout and preserve it; opt-in flag only adds, never removes |
| Path containment / traversal in discovery | Security (Principle III/IV) | Resolve within `.graphtor/` root only; reject symlink/`..` escapes |

## Phasing

* **Phase 1 = 79B5A7BC** — serve read-only auto-discovery + content-derived
  mode + background-sync gating + v4 gate parity + status/list-sources across
  discovered dbs + Phase-1 docs.
* **Phase 2 = B333B9B8** — consumption-first install default + opt-in ingestion
  scaffold + doctor/uninstall/upgrade parity + separate ingestion-setup docs.
  Depends on Phase 1.
