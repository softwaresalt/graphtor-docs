---
type: session-memory
timestamp: 2026-05-06T12:39:00-07:00
agent: Ship
shipment: 021-S
feature: 030-F
pr: "https://github.com/softwaresalt/graphtor-docs/pull/36"
status: shipped
---

## Summary

Shipped 021-S (CLI JSON-RPC 2.0 Output Mode & Manifest Command) via PR #36.

## What Was Done

- Implemented `src/cli/jsonrpc.rs` — JSON-RPC 2.0 `wrap_success` / `wrap_error` helpers
- Added `OutputFormat` enum and global `--json` flag to `src/cli/mod.rs`
- Added `Manifest` subcommand variant
- Added `DocServer::list_tools()` in `src/mcp/server.rs` to expose MCP tool definitions
- Added `list_mcp_tools()` public API in `src/mcp/mod.rs`
- Threaded `OutputFormat` through all 8 `cmd_*` functions in `src/main.rs`
- Implemented `cmd_manifest()` producing human table or JSON-RPC tools list
- Addressed 5 Copilot review findings (PR #36 review cycle 1)
- Archived all 030-F tasks and feature to done / shipped state

## Key Decisions

- `wrap_success` uses `match to_value()` — never `unwrap_or(Null)` — to avoid masking serialization failures
- `cmd_manifest` fails fast (exit 1 + error envelope) if any tool can't be serialized
- `list_tools()` sorts alphabetically; doc comment says "mirrors the *set* of tools" not ordering
- PR body written to file to avoid PowerShell backtick interpretation
- GraphQL queries assigned to `$query` variable before `gh api graphql` to avoid `$variable` expansion

## Compound Learnings Written

- `docs/compound/cli-jsonrpc-output-pattern-2026-05-06.md`

## Changed Files

- `src/cli/jsonrpc.rs` (new)
- `src/cli/mod.rs`
- `src/mcp/server.rs`
- `src/mcp/mod.rs`
- `src/main.rs`
- `.backlogit/archive/030-F.md` + 030.001-T through 030.005-T

## Next Steps

- Queue is now clear of 021-S. Check for new queued shipments.
- `docs/archive/plans/2026-08-24-pre-august-compaction/2026-05-05-source-agnostic-bootstrap-plan.md` exists untracked — may need staging.
