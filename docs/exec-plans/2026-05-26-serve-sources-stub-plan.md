---
title: "Serve Sources Stub Auto-Generation - Implementation Plan"
type: impl-plan
source: docs/decisions/2026-05-26-serve-sources-stub-deliberation.md
stash_id: 25F91517
created: 2026-05-26
---

## Problem Frame

When an existing `.db` is detected under `.graphtor/data/` but no
`.graphtor/config/sources.yaml` (or `*.sources.yaml`) exists, auto-generate
a stub file containing `sources: []` so that `serve` (and all other commands)
skip workspace auto-discovery and background sync.

### Affected Files

- `src/config/mod.rs`
- `src/main.rs`
- `tests/`

## Constitution Check

| Principle | Compliance |
|-----------|-----------|
| I. Safety-First Rust | Result-based error handling; no unwrap |
| II. Test-First | Unit + integration tests written first |
| III. Workspace Isolation | Stub written within `.graphtor/` only |
| IV. CLI Containment | No external writes |
| VI. Single Responsibility | Minimal new helper; no new deps |

## Implementation Units

### Unit 1: Stub generation helper (src/config/mod.rs)

**Files**: `src/config/mod.rs`
**Approach**: Add a function `ensure_sources_stub(config_dir, data_dir) -> Result<Option<PathBuf>>`
that:
1. Checks if any `*.sources.yaml` or `sources.yaml` exists in `config_dir` - if yes, returns `None`.
2. Checks if `data_dir` contains at least one `*.db` file - if not, returns `None`.
3. Creates `config_dir` (`mkdir_all`) and writes `sources: []\n` to `config_dir/sources.yaml`.
4. Returns `Ok(Some(path))` pointing to the generated stub.

**Acceptance**:
- Stub generated when DB exists and no config present.
- No-op when config already exists.
- No-op when no DB exists.

### Unit 2: Call-site integration (src/main.rs)

**Files**: `src/main.rs`
**Approach**: In `load_source_config`, before the
`build_workspace_source_config` fallback, call `ensure_sources_stub`.
If it returns `Some(path)`, parse and return the stub config (which is
`SourceConfig { sources: vec![] }`). This naturally causes
`spawn_background_sync` to be skipped since the
`!source_config.sources.is_empty()` guard already exists.

**Acceptance**:
- `serve` on imported-DB workspace prints info log about stub generation.
- No background sync is spawned.
- `status --db-path` still works.

### Unit 3: Unit tests (src/config/mod.rs #[cfg(test)])

**Files**: `src/config/mod.rs`
**Approach**: Test `ensure_sources_stub` in isolation:
- temp dir with a `.db` file and no config -> stub created.
- temp dir with existing `sources.yaml` -> no-op.
- temp dir with no `.db` -> no-op.

### Unit 4: Integration test (tests/)

**Files**: `tests/serve_sources_stub.rs`
**Approach**: Set up a temp workspace with a pre-populated `.db`
(copied from test fixtures or opened and closed via `DataStore`). Run
`load_source_config` and verify the returned config has empty sources.
Verify the stub file was created on disk.

## Dependency Graph

```text
Unit 3 (tests first)
  -> Unit 1 (stub helper)
    -> Unit 2 (call-site)
      -> Unit 4 (integration test)
```

## Estimated Effort

4 tasks x ~1.5 hours each = ~6 hours total (well within the 2-hour-per-task rule).

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | No | Internal config-loading behavior only |
| Security, auth, permission, or compliance-sensitive | No | Local workspace config only |
| Migration, backfill, destructive data/config action | No | Additive stub file in workspace config |
| External integration, operator checkpoint | No | No external service or approval path |
| High runtime, rollout, or rollback risk | No | Scoped serve-safety behavior |

**Requires plan hardening: no**
