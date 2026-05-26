---
type: session-memory
timestamp: 2026-04-30T18:15:00-07:00
shipment: 005-S
pr: 11
merge_commit: a6ebb3f
branch: feat/cli-workspace-distribution
status: shipped
---

## Session Summary: CLI & Workspace Distribution (005-S)

### Outcome

PR #11 merged to `main` at `a6ebb3f`. All 17 backlog items (010-F, 011-F, 15 tasks) moved to `done`. Shipment 005-S closed.

### What Was Built

- **`src/cli/mod.rs`** — `clap` 4 derive-based CLI: `Cli` struct, `Command` enum (8 subcommands), all `*Args` structs
- **`src/workspace/`** — 10 modules covering the full workspace lifecycle:
  - `paths.rs` — `find_workspace_dir`, `GRAPHTOR_DIR`, `GRAPHTOR_SUBDIRS`
  - `lock.rs` — advisory file-based lock with timestamp stale detection (no `unsafe`)
  - `gitignore.rs` — `add_gitignore_entry` / `remove_gitignore_entry`
  - `mcp_config.rs` — MCP config file generation for VS Code, Cursor, Windsurf
  - `install.rs` — workspace scaffold + binary copy + `.gitignore` + MCP configs
  - `init.rs` — `sources.yaml` template generation
  - `doctor.rs` — health checks: binary, subdirs, sources.yaml, database, disk
  - `upgrade.rs` — binary replacement with `--force` support
  - `uninstall.rs` — full or config-preserving workspace removal
  - `mod.rs` — re-exports
- **`src/main.rs`** — rewritten from 46-line MCP server to ~400-line CLI dispatcher

### Key Technical Decisions

1. **Binary crate isolation**: `mod cli; mod workspace;` live in the binary, not the library. They use `graphtor_core::GraphtorError` (not `crate::error::GraphtorError`).
2. **Stale lock detection**: Uses file modification timestamp (not PID liveness) to avoid `unsafe`. A lock older than 3600s is auto-overwritten. `force=true` always overwrites.
3. **Idempotency**: `install`, `upgrade`, and `uninstall` are all idempotent and safe to re-run.
4. **clippy 1.93 vs 1.95**: Local Windows passes 1.93, but CI runs 1.95 on Linux. The `map_unwrap_or` lint only fires on 1.95 — `.map(f).unwrap_or(d)` must be `.map_or(d, f)`.
5. **DB path consistency**: `doctor` checks `.graphtor/graph.db` (not `.graphtor/data/graph.db`), matching the CLI default.

### Copilot Review Findings

**Fixed (P1) — commit 3e32a24:**
- `lock.rs`: `--force-unlock` → `--force` in error message
- `init.rs`: removed false interactive/`--non-interactive` mode claim from module doc
- `doctor.rs`: DB path check changed from `.graphtor/data/graph.db` to `.graphtor/graph.db`
- `cli/mod.rs`: removed false "checks schema migrations" / "triggers re-index" from Upgrade doc

**Deferred to 006-S (Hardening & Code Quality):**
- `upgrade.rs`: file-size-based equality check should use hash or mtime
- `uninstall.rs`: `entries.flatten()` silently drops `read_dir` errors
- `lock.rs:88`: `fs::write` is not atomic w.r.t. creation (should use `O_CREAT|O_EXCL`)
- `lock.rs:376`: `--force` also force-unlocks — should be separate concerns
- `main.rs:141`: `data_root` defaulting to `db_path.parent()` places Git clones under DB dir
- `main.rs:287`: workspace lock not acquired on fresh install (race window)
- `cli/mod.rs:47`: `--data-dir` named as a file path but used as a directory
- `mcp_config.rs:131`: `unwrap_or_default()` silently swallows I/O errors

### Test Results

- 99 lib unit tests: ✅
- 21 binary unit tests: ✅
- Integration + doc tests: ✅
- CI (Linux, clippy 1.95): ✅

### Next

Open shipment: **006-S** — Hardening & Code Quality (includes deferred P2/P3 findings above plus existing hardening tasks).
