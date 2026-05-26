---
type: session-memory
shipment: 006-S
date: 2026-04-30
branch: feat/hardening-code-quality
pr: 12
status: pr-open-awaiting-merge
---

## Shipment 006-S — Hardening & Code Quality

Execution of shipment 006-S from branch `feat/hardening-code-quality`.
PR #12: https://github.com/softwaresalt/graphtor-docs/pull/12

### Tasks Completed (11/12)

| Task | Title | Commit |
|---|---|---|
| 013.001-T | `is_client_error()` + PathViolation → `invalid_params` MCP error | `90239b4` |
| 013.002-T | Remove `pub use mcp::DocServer` from lib.rs | `ca063ff` |
| 013.003-T | Directory-boundary source_id prefix filter | `90239b4` |
| 013.004-T | Safe u32-to-usize cast with platform comment | `90239b4` |
| 013.005-T | Tokio features: `full` → `rt-multi-thread,macros` | `47fb4df` |
| 013.006-T | Document format.rs placement (kept co-located) | `dbb2b31` |
| 013.007-T | Branch-protection guardrails spike + implementation | `41b979f` |
| 014.001-T | Atomic lock creation with `O_CREAT\|O_EXCL` | `0807a38` |
| 014.002-T | Handle `read_dir` errors in `uninstall()` | `c926c28` |
| 014.003-T | mtime-based equality check in `upgrade()` | `1a6ec76` |
| 014.004-T | Propagate I/O errors in `remove_mcp_configs()` | `fe189cb` |
| 014.005-T | CLI args refactor (db_path, data_root, force_unlock) | `f44c905` |
| **013.008-T** | **BLOCKED — upstream releases** | skipped |

### Quality Gates

- `cargo check`: ✅ clean
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`: ✅ clean
- `cargo fmt --all -- --check`: ✅ clean
- `cargo test`: ✅ 155 tests, 0 failures (up from ~120; 8 new tests added)

### CI

- GitHub Actions `CI/build`: ✅ SUCCESS (2m36s) at 2026-05-01T02:40:17Z

### Key Technical Decisions

1. **`is_client_error()` instead of `to_mcp_error()`**: Kept rmcp coupling out of `src/error/`
   by adding a pure boolean method, with the error-code dispatch staying in `src/mcp/server.rs`.

2. **format.rs kept in `src/mcp/`**: `SearchResult`/`TraversalResult` have no non-MCP consumers
   currently. Documented the decision in module doc comment rather than moving prematurely.

3. **Tokio minimal features**: Grep of src/ confirmed only `rt-multi-thread` + `macros` needed
   (rmcp's STDIO transport handles AsyncRead/Write internally via its own tokio dependency).

4. **Branch protection guardrails** (013.007-T):
   - Created `.githooks/pre-push` hook that blocks `git push` to `main`
   - Created `.github/workflows/protect-main.yml` CI alert on direct pushes
   - Updated `copilot-instructions.md` with explicit "never push directly to main" rule
   - Recommended disabling admin bypass in GitHub branch protection settings (manual step)

5. **Atomic lock** (014.001-T): `OpenOptions::new().create_new(true)` provides `O_CREAT|O_EXCL`
   semantics. Force path still uses `fs::write` (intentional — force means "take regardless").
   Lock error message updated to reference `--force-unlock` not `--force`.

6. **data_root fix** (014.005-T Unit 2): `db_path.parent()` resolved to `.graphtor/` (same dir
   as the db file), so Git clones landed alongside config/bin/etc. Fixed to `.graphtor/data/`.
   Existing clones at old location will be re-synced on next `graphtor-docs sync`.

7. **CLI flag deprecation** (014.005-T Unit 1): `--data-dir` alias kept via `#[arg(alias)]` with
   hide-from-help and stderr deprecation warning when detected via `std::env::args()` scan.

### Outstanding

- **013.008-T**: Permanently blocked on cozo (RUSTSEC-2026-0041) and git2 (RUSTSEC-2026-0008)
  upstream releases. Remains in backlog, not archived.
- **Post-merge closure**: Shipment archival, compound learnings, and docs update pending
  user-approved merge of PR #12.
- **Branch protection admin bypass**: Manual step in GitHub repo settings — agents cannot
  configure this programmatically without admin API access.
