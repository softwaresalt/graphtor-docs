---
title: "Source Registry Normalization and Duplicate-Intake Preflight — Implementation Plan"
type: impl-plan
source: docs/decisions/2026-05-23-source-registry-normalization-deliberation.md
stash_id: 4BEEF41A
created: 2026-05-23
---

# Source Registry Normalization and Duplicate-Intake Preflight — Implementation Plan

## Problem Frame

The config module (`src/config/`) loads a single `sources.yaml` from
`.graphtor/config/sources.yaml`. The operator wants to split sources across
multiple domain-specific files following a `*.sources.yaml` naming pattern,
enforce the `database` field across all files, and detect duplicate file/path
intake before sync begins.

Affected modules: `src/config/mod.rs` (loading), `src/config/source.rs` (schema),
`src/config/validation.rs` (validation), `src/cli/mod.rs` (sync command preflight).

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| Multiple source files (`*.sources.yaml`) | Multi-file glob discovery in config loader |
| Consistent schema with `database` field | Schema enforcement in multi-file mode |
| Detect duplicate file paths across DBs | Cross-file path overlap validation |
| Alert operator before ingestion | CLI preflight report with override prompt |
| `*.sources.yaml` naming pattern | Glob pattern in config directory |
| Backward-compatible with single file | Fallback to `sources.yaml` when no `*.sources.yaml` found |

## Implementation Units

### Unit 1: Multi-File Config Discovery

**Posture**: test-first

Extend the config loading module to discover all `*.sources.yaml` files in
`.graphtor/config/` via glob. Sort alphabetically for deterministic load order.
Fall back to `sources.yaml` (without prefix) if no `*.sources.yaml` files exist.

* **Files**: `src/config/mod.rs`
* **Changes**: New `discover_source_files()` function; update `load()` to use it
* **Tests**: Unit tests for discovery with 0, 1, N source files; backward-compat test
* **Verifiable outcome**: `cargo test` passes

### Unit 2: Schema Enforcement — Required `database` Field

**Posture**: test-first

When multiple source files are loaded, enforce that every source entry declares
an explicit `database` field. In single-file mode, retain the current default
(`graph.db` when omitted). Add a new validation rule in `validation.rs`.

* **Files**: `src/config/validation.rs`, `src/config/source.rs`
* **Changes**: New validation rule; conditional enforcement based on multi-file mode flag
* **Tests**: Unit test for rejection when `database` is missing in multi-file mode
* **Verifiable outcome**: `cargo test` passes

### Unit 3: Cross-File Duplicate Path Detection

**Posture**: test-first

After all source files are loaded and merged, compare resolved paths across
sources targeting different databases. Flag entries where the same file path
(after glob expansion or literal path) appears in sources routed to different
databases.

* **Files**: `src/config/validation.rs`
* **Changes**: New `detect_path_overlaps()` function; integration with `validate()`
* **Tests**: Unit tests with overlapping paths, non-overlapping paths, same-db duplicates (allowed)
* **Verifiable outcome**: `cargo test` passes; overlaps detected correctly

### Unit 4: Duplicate Intake Report Structure

**Posture**: test-first

Define a `DuplicateIntakeReport` struct that captures: overlapping paths,
source IDs involved, target databases for each, and a human-readable summary.
This feeds into the CLI preflight display.

* **Files**: `src/config/validation.rs` (or new `src/config/preflight.rs`)
* **Changes**: New struct + Display impl + builder
* **Tests**: Unit tests for report formatting
* **Verifiable outcome**: `cargo test` passes

### Unit 5: CLI Preflight Check in Sync Command

**Posture**: test-first

Before ingestion begins, the `sync` command calls the cross-file validation.
If duplicates are detected, display the report and prompt the operator:
- Default: abort sync
- `--force` flag: proceed despite duplicates
- Interactive: ask for confirmation

* **Files**: `src/cli/mod.rs` (sync handler)
* **Changes**: Preflight check before ingestion loop; `--force` flag addition
* **Tests**: Integration test for preflight abort; integration test for `--force` override
* **Verifiable outcome**: `cargo test` passes; manual CLI verification

### Unit 6: Documentation and Migration Guide

**Posture**: docs-only

Document the multi-file source convention, naming pattern requirements,
schema differences between single-file and multi-file mode, and migration
steps from `sources.yaml` to `*.sources.yaml`.

* **Files**: `docs/` (new guide or update to existing docs)
* **Changes**: New documentation file
* **Tests**: N/A (documentation)
* **Verifiable outcome**: Document exists and is accurate

## Dependency Graph

```text
Unit 1 (multi-file discovery)
  ↓
Unit 2 (schema enforcement) — depends on Unit 1
  ↓
Unit 3 (duplicate detection) — depends on Unit 1
  ↓
Unit 4 (report struct) — depends on Unit 3
  ↓
Unit 5 (CLI preflight) — depends on Units 1, 3, 4
  ↓
Unit 6 (documentation) — depends on all above
```

Units 2 and 3 can proceed in parallel after Unit 1.
Unit 4 depends on Unit 3.
Unit 5 integrates everything.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Glob `*.sources.yaml` (not `sources.d/`) | Matches operator's mental model of domain-grouped files |
| Alphabetical load order | Deterministic; no hidden priority; documented |
| Require `database` only in multi-file mode | Backward-compatible; single-file users don't break |
| Block-by-default on duplicates | Safety-first; `--force` provides escape hatch |
| Same-database duplicates allowed | Not a cross-contamination risk; may be intentional |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| Glob expansion may be expensive for large include patterns | Only compare literal paths and top-level directories; defer full glob expansion to sync time |
| Backward-compat regression | Explicit fallback path tested; single `sources.yaml` always works |
| `--force` flag could mask real problems | Log a warning even in force mode; include in sync summary |

## Plan Hardening Signals

* Public API, schema, or contract change: **YES** — config loading behavior changes; new CLI flag
* Security, auth, permission, or compliance: **No**
* Migration, destructive data/config action: **No** (backward-compatible)
* External integration or operator checkpoint: **YES** — interactive operator prompt in CLI
* High runtime, rollout, or rollback risk: **No** (additive feature)

**Requires plan hardening: yes**

## Plan Hardening

### Risk Triggers

| Signal | Present | Detail |
|---|---|---|
| Public API change | YES | Config loading behavior changes; new `--force` CLI flag |
| Security/auth/compliance | No | — |
| Migration/destructive action | No | Backward-compatible; existing config still works |
| External integration/checkpoint | YES | Interactive operator prompt blocks sync by default |
| High runtime/rollback risk | No | Additive feature |

### Protected Invariants

1. **Backward compatibility**: Single `sources.yaml` MUST continue to work unchanged
2. **No silent data duplication**: Duplicate paths detected MUST surface to operator before ingestion
3. **Deterministic loading**: File discovery order must be reproducible (alphabetical sort)
4. **Schema consistency**: Multi-file mode enforces `database` field without breaking single-file mode

### Risky Actions (strict-safety vocabulary)

| ProposedAction | ActionRisk | Approval | Rollback |
|---|---|---|---|
| Change config loading to glob `*.sources.yaml` | moderate | Agent (test-verified) | Revert to single-file loading path |
| Block sync by default on duplicate detection | moderate | Operator (by design) | `--force` flag provides immediate escape |
| Require `database` field in multi-file mode | low | Agent (test-verified) | Remove validation rule |

### Reinforced Verification Plan

* Backward-compatibility test MUST verify existing `sources.yaml` works unchanged
* Multi-file tests MUST cover: 0 files (fallback), 1 file, N files, overlapping paths
* Interactive prompt MUST respect non-interactive environments (CI): default to abort, not hang
* `--force` flag MUST still log a warning (not silent override)

### Rollback Strategy

* Config discovery is additive — single `sources.yaml` fallback always available
* No database schema changes; no data migration
* If preflight check causes workflow disruption, `--force` provides immediate relief

## Runtime Verification and Closure

### Changed runtime surfaces

* CLI `sync` command — new preflight check, new `--force` flag
* Config loading — new file discovery behavior
* CLI output — duplicate intake report formatting

### Verification expectations

* Run `sync` with overlapping sources across databases — verify report displays
* Run `sync --force` with overlaps — verify override works
* Run `sync` with single `sources.yaml` — verify backward compatibility
* Run `sync` with multiple `*.sources.yaml` files — verify all are discovered

### Closure expectations

* Document the multi-file convention in operational docs
* Note the breaking change boundary (multi-file requires `database` field)
* Rollback trigger: if config loading regression breaks existing users, revert discovery to single-file only
