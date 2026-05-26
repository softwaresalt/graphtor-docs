---
type: session-memory
timestamp: 2026-05-06T00:00:00Z
shipment: 020-S
pr: "https://github.com/softwaresalt/graphtor-docs/pull/35"
branch: feat/zero-config-adoption
status: shipped
---

# Session Memory — 020-S Zero-Config Adoption & Composite Research Tool

---

## What Was Built

### 029.001-T — Auto-Discovery When No sources.yaml Exists

Added two helpers to `src/main.rs`:

- **`build_workspace_source_config(cwd)`** — creates a `SourceConfig` for the
  workspace root using `include: ["**/*.md", "**/*.markdown"]`, `formats: ["md",
  "markdown"]`, and standard exclusions (`.graphtor/**`, `.git/**`, `target/**`).
  Include globs filter early so non-Markdown files are never scanned.

- **`load_source_config(cwd, config_override)`** — unified config resolution:
  1. Explicit override exists → load it
  2. Explicit override missing → `Ok(None)` (caller treats as fatal)
  3. No override, default path exists → load it
  4. No override, default path missing → auto-discover workspace

Updated `cmd_sync` to use `load_source_config`; explicit missing path → `eprintln!`
+ exit code 2. Updated `cmd_serve` to fail fast (same exit 2) on explicit missing
path rather than silently skipping background sync.

### 029.002-T — Background Auto-Sync on `serve`

Extracted `spawn_background_sync(source_config, db_path, cwd, store, model) ->
Arc<Mutex<SyncStatus>>` from `cmd_serve`. The function:
- Sets `SyncStatus::Syncing`
- Runs `acquire_execute` + per-source `sync_source` inside `spawn_blocking`
- Sets `Done { files, chunks }` or `Error(msg)` when complete
- Returns the `Arc<Mutex<SyncStatus>>` for injection into `DocServer`

The `cmd_serve` function passes this handle via `DocServer::with_sync_status()`.

### 029.003-T — `research_topic` MCP Tool

New types in `src/mcp/server.rs`:
- **`SyncStatus`** enum: `Idle`, `Syncing`, `Done { files, chunks }`, `Error(String)` — `pub` and re-exported via `mcp::SyncStatus`
- **`ResearchTopicParams`**: `query`, `top_k: Option<u32>` (default 5, max 20), `max_depth: Option<u32>` (default 1, max 3)

Tool behavior:
- `search_k = top_k.min(20)`, `seed_k = search_k.min(3)` for traversal seeds
- Prefers semantic search for seeds when model is loaded; falls back to text search
- Multi-hop graph traversal via `DataStore::traverse`
- Global dedup via `HashSet<String>` across all seed traversals
- Returns structured markdown via `format_research_results`

Added `format_research_results(query, initial, related)` to `src/mcp/format.rs`.
Query string is sanitized (trim + newline removal) before embedding in heading.

---

## Key Design Decisions

| Decision | Rationale |
|---|---|
| `std::sync::Mutex` (not tokio) for `SyncStatus` | Critical section is tiny (single status write); no async code holds the lock |
| `seed_k = search_k.min(3)` | Prevents traversal explosion; 3 seeds × depth-1 is already ample context |
| `max_depth` default 1, max 3 | Depth 2+ on large graphs causes output explosion; 1 is the right default |
| `include` globs in auto-discovery | Filters at glob stage, not just at `formats` allow-list — avoids scanning all file types |
| `cmd_serve` fail-fast on explicit missing config | Consistent with `cmd_sync` behavior; silent skip would be confusing |

---

## Copilot Review Findings (all fixed)

1. **Empty `include` in auto-discovery** — fixed with `["**/*.md", "**/*.markdown"]`
2. **`markdown` extension missing from formats** — added alongside `md`
3. **Doc comment said "exactly" mirrors cmd_sync_incremental** — clarified
4. **`cmd_serve` silently swallows explicit missing config** — now fails fast (exit 2)
5. **Raw query embedded in Markdown heading** — sanitized with trim + newline replace

---

## Tests Added (14 new)

| Module | Test | Coverage |
|---|---|---|
| `mcp::format` | `format_research_results_empty_initial_shows_no_results` | empty state |
| `mcp::format` | `format_research_results_includes_chunk_id_and_path` | normal results |
| `mcp::format` | `format_research_results_includes_related_context` | related section |
| `mcp::server` | `sync_status_default_is_idle` | `Default` impl |
| `mcp::server` | `get_status_includes_auto_sync_field` | status output |
| `mcp::server` | `with_sync_status_done_appears_in_get_status` | builder + status |
| `mcp::server` | `research_topic_empty_query_returns_error` | validation |
| `mcp::server` | `research_topic_returns_ok_on_empty_store` | happy path |
| `mcp::server` | `research_topic_top_k_clamped_to_twenty` | clamping |
| `main::tests` | `build_workspace_source_config_produces_local_source` | include/formats |
| `main::tests` | `load_source_config_returns_auto_discovery_when_default_missing` | auto-discovery |
| `main::tests` | `load_source_config_returns_none_for_explicit_missing_override` | explicit missing |

---

## Compile Time Note

`cargo test` required ~35 minutes on this machine due to full Candle/fastembed
recompile triggered by `cargo fmt` changing file timestamps. Subsequent incremental
builds are fast (~50s for clippy, ~1.5m for check). The large compile is a
one-time cost per format-triggered rebuild.

---

## Files Changed

- `src/main.rs` — +296 lines: helpers, cmd_sync/cmd_serve updates, 3 tests
- `src/mcp/server.rs` — +282 lines: SyncStatus, ResearchTopicParams, research_topic tool, 8 tests
- `src/mcp/format.rs` — +113 lines: format_research_results, 3 tests
- `src/mcp/mod.rs` — re-exports SyncStatus
