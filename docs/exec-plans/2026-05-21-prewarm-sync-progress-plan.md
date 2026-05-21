---
title: "Pre-warm Sync Mode — Progress Reporting and Backlogit Telemetry"
source: "docs/decisions/2026-05-21-prewarm-sync-progress-reporting.md"
feature_id: "037-F"
date: 2026-05-21
---

## Objective

Add a dedicated `graphtor prewarm` CLI subcommand that syncs all configured sources with
file-level progress output to stderr and emits backlogit-consumable JSONL telemetry to stdout
on completion. Builds on 036-F's `SyncMetrics` and `SyncStatus` infrastructure.

## Constitution Check

- **I. Safety-First Rust**: All new code uses `Result<T, E>` propagation; no unsafe.
- **II. Test-First**: Each unit has test harness before implementation.
- **III. Workspace Isolation**: Sync operates within configured workspace root.
- **VI. Single Responsibility**: Progress reporting is additive; telemetry format is a new concern.

## Implementation Units

### Unit 1: Progress callback in sync_source

**Scope**: `src/sync/mod.rs`

1. Add an optional progress callback parameter to `sync_source`:
   ```rust
   pub fn sync_source(
       store: &DataStore,
       source: &Source,
       source_dir: &Path,
       state_path: &Path,
       root: &Path,
       model: Option<&EmbeddingModel>,
       on_progress: Option<&mut dyn FnMut(&Path, usize, usize)>,
   ) -> Result<SyncMetrics, GraphtorError>
   ```
2. Invoke `on_progress` at each file reingest boundary, passing the file path,
   1-based file index, and total file count for the current source.
3. Existing callers pass `None` — zero behavioral change to current code paths.
4. Unit test: mock callback confirms invocation count matches file count in fixture.

**Files touched**: `src/sync/mod.rs`, `src/sync/reingest.rs` (if file loop lives there),
all existing `sync_source` call sites (add `, None` argument)
**Risk**: Low — additive parameter with `Option` defaulting to no-op.
**Acceptance Criteria**:
- `sync_source` signature extended with `on_progress` parameter
- Existing call sites compile with `None`
- Unit test verifies callback invocation count matches expected file count

### Unit 2: `prewarm` CLI subcommand with stderr progress

**Scope**: `src/cli/mod.rs`, new `src/cli/prewarm.rs`

1. Add `Prewarm` variant to the clap-derived `Commands` enum in `src/cli/mod.rs`.
2. Create `src/cli/prewarm.rs` implementing the prewarm handler:
   - Load workspace config and resolve source list.
   - For each source, call `sync_source` with a progress closure that writes to stderr:
     ```text
     [syncing] source_id: file_name (file_idx/total_files) [overall_pct%]
     ```
   - Track overall progress across all sources (sum of files across sources).
3. On completion, print a summary line to stderr with total files synced, chunks created,
   and duration.
4. Integration test: run `graphtor prewarm` against a test fixture directory, verify stderr
   contains progress lines and exit code is 0.

**Files touched**: `src/cli/mod.rs`, new `src/cli/prewarm.rs`
**Risk**: Low — new subcommand; no change to existing serve/sync behavior.
**Acceptance Criteria**:
- `graphtor prewarm` executes, syncs all sources, and exits with code 0
- stderr output contains file-level progress lines with percentage
- No output to stdout during progress (stdout reserved for telemetry)

### Unit 3: JSONL telemetry output for backlogit consumption

**Scope**: `src/cli/prewarm.rs`

1. After all sources complete, emit a single JSONL record to stdout:
   ```json
   {"event_type":"sync_complete","timestamp":"2026-05-21T10:30:00Z","payload":{"files_total":47,"files_synced":45,"files_deleted":2,"chunks_created":312,"chunks_deleted":8,"duration_ms":4200,"errors":0,"sources_count":3}}
   ```
2. The record structure follows backlogit's telemetry ingest expectations:
   - `event_type`: string discriminator
   - `timestamp`: ISO-8601
   - `payload`: typed JSON object (extends `SyncMetrics` with `sources_count`)
3. When `--quiet` flag is passed, suppress stderr progress but still emit the JSONL record.
4. Unit test: capture stdout from prewarm handler, parse as JSON, validate schema fields.
5. Integration test: pipe stdout to a JSON parser, confirm well-formed JSONL.

**Files touched**: `src/cli/prewarm.rs`
**Risk**: Low — stdout is only written once at completion; no streaming concerns.
**Acceptance Criteria**:
- stdout contains exactly one JSON line on successful completion
- JSON record contains all `SyncMetrics` fields plus `event_type`, `timestamp`, `sources_count`
- `--quiet` flag suppresses stderr progress but preserves stdout telemetry
- `cargo test` includes a test that parses the JSON output and validates field presence

## Dependency Order

Unit 1 → Unit 2 → Unit 3

Unit 2 depends on the progress callback from Unit 1. Unit 3 extends Unit 2's handler with
telemetry output.

## Acceptance Criteria (Plan-Level)

- `cargo test` passes with new tests for all three units
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` clean
- `graphtor prewarm` syncs all configured sources and exits cleanly
- stderr shows file-level progress with percentages during sync
- stdout emits valid JSONL telemetry record on completion
- Existing `graphtor serve` and `graphtor sync` behavior unchanged
- No breaking changes to `sync_source` callers (all pass `None` for callback)
