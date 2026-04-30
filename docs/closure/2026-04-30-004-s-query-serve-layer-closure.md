---
feature: 004-S / 008-F / 009-F
mode: post-merge
status: READY WITH CONDITIONS
date: 2026-04-30
pr: "#10"
merged_commit: 6804796
---

# Operational Closure — 004-S Query & Serve Layer

## Change Summary

Shipped in **PR #10** (merged at `6804796`). Implements two features as part of
shipment 004-S — Query & Serve Layer:

**008-F: Incremental Sync**

- `src/sync/state.rs` — `.sync_state.json` read/write with SHA-256-keyed per-source state
- `src/sync/git_diff.rs` — tree-to-tree diff via `git2` crate (added/modified/deleted files)
- `src/sync/mtime_diff.rs` — mtime-based change detection for local directory sources
- `src/sync/reingest.rs` — surgical delete-then-reload pipeline per changed file
- `src/sync/mod.rs` — `sync_source()` orchestrator tying detection → reingest → state persist

**009-F: MCP Plugin Server**

- `src/mcp/server.rs` — `DocServer` with `search_local_docs` and `traverse_doc_links` tools
- `src/mcp/format.rs` — structured markdown formatters for LLM-consumable output
- `src/mcp/mod.rs` — module root exporting `DocServer`
- `src/main.rs` — async STDIO MCP server entry point (tokio, rmcp 1.5)

Key review fix: `reingest.rs` strip_prefix fallback now returns `GraphtorError::Pipeline`
instead of silently writing absolute paths to the database.

## Invariants to Preserve

| Invariant | Owner | Risk if Broken |
|-----------|-------|----------------|
| All `path` fields in `doc_chunks`, `doc_edges`, `doc_code` are source-root-relative (forward slashes) | `reingest.rs::reingest_file` | Future diffs and deletes will miss records |
| Chunk IDs are stable SHA-256 of `content + source_path` | `pipeline/chunker.rs` | MCP graph traversal returns wrong or missing nodes |
| MCP server binds to STDIO only — no network socket | `main.rs` | Privacy violation (docs leave machine) |
| `GRAPHTOR_DB_PATH` must resolve within `current_dir()` | `db/store.rs::open_sqlite` | PathViolation on startup if absolute path outside cwd |
| Sync state is stored at workspace-relative `.sync_state.json` | `sync/state.rs` | Full re-ingest on every run if path drifts |
| Canary: `strip_prefix(source_root)` returns `Err` for files outside root | `sync/reingest.rs` | Absolute paths in DB if validation bypassed |

## Pre-Deploy Audits

This is a local developer tool compiled from source — "deployment" means building
and wiring the binary into an MCP client configuration.

| Audit | Check |
|-------|-------|
| Database directory exists | Create `.graphtor/` before first run (`mkdir .graphtor`) |
| `sources.yaml` is present and valid | Verify at least one source entry before running sync |
| Rust toolchain ≥ 1.85 | `rustc --version` |
| `.sync_state.json` not corrupted | Delete to force full re-ingest if state appears stale |
| `GRAPHTOR_DB_PATH` is relative or within cwd | Absolute paths outside cwd will fail with `PathViolation` |

## Deployment Path

Single Rust binary — no services, no containers, no cloud:

```powershell
# Build release binary
cargo build --release

# Wire into Claude Desktop (example)
# Edit ~/Library/Application\ Support/Claude/claude_desktop_config.json:
# {
#   "mcpServers": {
#     "graphtor-docs": {
#       "command": "/path/to/graphtor-docs",
#       "args": []
#     }
#   }
# }
```

## Post-Deploy Checks

After first successful MCP server connection:

1. **Smoke test sync**: Run `graphtor-docs sync` — should complete without error and create `.sync_state.json`
2. **Search test**: Call `search_local_docs` with `{ "query": "overview" }` — should return results with chunk IDs
3. **Traversal test**: Take a `chunk_id` from search results and call `traverse_doc_links` — should return linked chunks
4. **Path invariant check**: Inspect a `doc_chunks` row in CozoDB — `path` field must be relative (no leading `/` or drive letter)
5. **State persistence**: Run sync twice — second run should report "no changes detected"

## Known Conditions (READY WITH CONDITIONS)

The following issues were deferred to backlog feature **013-F** and do not block
operation, but callers should be aware:

| ID | Issue | Impact | Backlog |
|----|-------|--------|---------|
| 013.001-T | `source_id` filter in `search_local_docs` is broken — `SearchResult` has no `source_id` field, so `path.starts_with(source_id)` never matches | `source_id` param silently ignored; all sources returned | 013.003-T |
| 013.002-T | `build_new_state` swallows `git2::Repository::open` and `scan_mtimes` failures | State may regress to `last_commit=None`, causing full re-ingest on next run | 013-F |
| 013.003-T | `scan_mtimes` drops directory walk errors via `filter_map(Result::ok)` | Permission or IO failures during mtime scan are silent | 013-F |
| 013.004-T | `embed_text` called during reingest but output discarded | Unnecessary CPU cost; no embeddings stored | 013-F |
| 013.005-T | `sync_source` doc claims `GraphtorError::Sync` wrapping; implementation does not construct this variant | Documentation mismatch only, no runtime impact | 013-F |

## Healthy Signals

- MCP server starts with `tracing::info!` logs and no panics
- `search_local_docs` returns results in structured markdown with `chunk_id` fields
- `traverse_doc_links` returns 2–5 related chunks from a valid starting chunk
- `.sync_state.json` updated after each `sync` run with new `last_sync` timestamp
- Subsequent sync runs complete faster and report fewer changed files than the initial run

## Failure Signals

| Signal | Likely Cause | Action |
|--------|-------------|--------|
| `PathViolation` on startup | `GRAPHTOR_DB_PATH` is absolute outside cwd | Use relative path or unset the env var |
| `database schema` error | CozoDB schema version mismatch | Delete `.graphtor/graph.db` and re-sync |
| `strip_prefix` / `Pipeline` error during sync | File appears outside the configured source root | Check `sources.yaml` source path configuration |
| `search_local_docs` returns empty results | Sync has not run, or no Markdown files indexed | Run sync first |
| `traverse_doc_links` returns empty | No outgoing links from this chunk, or graph edges not populated | Normal for leaf documents |
| MCP server exits immediately | STDIO transport error | Check MCP client configuration; run binary directly to see stderr |

## Monitoring Plan

No cloud monitoring — this is a local tool. Developer observation:

- **On first run**: Check stderr for panics or schema errors
- **After sync**: Verify `.sync_state.json` updated and contains expected `last_sync` epoch value
- **After MCP tool calls**: Check that `chunk_id` fields appear in search responses (format.rs regression guard)
- **Ongoing**: Watch `files_errored` count in `sync_source` logs — if consistently > 0, diagnose with `RUST_LOG=debug`

## Rollback Trigger

| Trigger | Description |
|---------|-------------|
| PathViolation on all sources | Indicates DB path validation regression |
| MCP binary panics on startup | Critical runtime error in initialization path |
| DB corruption after sync | Schema or write failure corrupted CozoDB state |

## Rollback Procedure

Since this is a local compiled binary with no deployed infrastructure:

1. Revert to the previous binary (`git checkout <prior_sha> && cargo build --release`)
2. If DB is corrupted: delete `.graphtor/graph.db` and `.sync_state.json`, re-sync from scratch
3. The source documentation repositories are unaffected — re-sync is always safe

## Validation Window

- **Duration**: First 2–3 sync-and-search cycles in the developer's actual workflow
- **Owner**: `softwaresalt`
- **Focus areas**: path invariant (relative paths in DB), source_id filter (known broken — expect no filtering), state persistence across syncs

## Follow-Up

| Item | Action | Artifact |
|------|--------|----------|
| 013-F tasks queued | Fix source_id filter, build_new_state error handling, scan_mtimes propagation, embed_text removal | `.backlogit/queue/013-F.md` |
| `rust-mcp-server.instructions.md` references rmcp 0.8.1 | Update to rmcp 1.5 API | Separate chore task |
| Compound learnings | rmcp 1.5 serve_server pattern, clippy::map_unwrap_or CI discrepancy, cargo test with candle | `docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`, `docs/compound/workflow-issues/clippy-pedantic-map-unwrap-or-ci-vs-local-2026-04-30.md`, `docs/compound/best-practices/cargo-test-candle-ml-codegen-slow-2026-04-30.md` |
