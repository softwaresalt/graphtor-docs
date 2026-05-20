---
title: "MCP Configuration Install Path Fix"
source: "docs/decisions/2026-05-21-mcp-config-install-path-deliberation.md"
feature_id: "035-F"
shipment_id: "026-S"
stash_id: "831A0320"
date: 2026-05-21
---

## Problem Frame

`graphtor-docs install` writes MCP client configuration to `.github/copilot/mcp.json`
by default. This path is not recognized by any current editor or CLI tool. The
`Editor::Copilot` variant and its config path must be removed. The default set
should include only VS Code (`.vscode/mcp.json`) and Cursor (`.cursor/mcp.json`).

### Affected Files

- `src/workspace/mcp_config.rs` — enum `Editor`, `config_path()`, `from_str()`,
  `generate_mcp_configs()`, `remove_mcp_configs()`, tests
- `src/cli/mod.rs` — `InstallArgs.editor` help text (minor doc update)
- `tests/` — any integration tests referencing copilot editor target

## Requirements Trace

| Requirement | Implementation Action |
|---|---|
| Remove non-functional `.github/copilot/mcp.json` generation | Remove `Editor::Copilot` variant |
| Stop writing to that path by default | Remove from `all_editors` array |
| Update CLI help text | Remove "copilot" from supported editor list in `--editor` docs |
| Preserve uninstall cleanup for pre-existing files | Keep `.github/copilot/mcp.json` in uninstall's cleanup list as a legacy path |
| Update tests | Remove or update tests that reference `Editor::Copilot` |

## Implementation Units

### Unit 1: Remove `Editor::Copilot` variant and update config logic

**Domain:** Code  
**Files:** `src/workspace/mcp_config.rs`  
**Changes:**
1. Remove `Editor::Copilot` from the `Editor` enum
2. Remove the `"copilot" | "github-copilot"` arm from `Editor::from_str()`
3. Remove `Self::Copilot => ".github/copilot/mcp.json"` from `config_path()`
4. Update `all_editors` array in `generate_mcp_configs()` to only contain `[Editor::VsCode, Editor::Cursor]`
5. In `remove_mcp_configs()`: keep `.github/copilot/mcp.json` as a **legacy cleanup path** (hardcoded string, not enum variant) so existing installs get cleaned up on uninstall

**Tests to update:** Existing tests only use `Editor::VsCode` — no test changes needed for the positive path. Add a test verifying that `from_str("copilot")` returns `None`.

**Verification:** `cargo build`, `cargo test`  
**Posture:** Test-first (write the `from_str("copilot")` → None test, then remove the variant)

### Unit 2: Update CLI help text

**Domain:** Code (docs in code)  
**Files:** `src/cli/mod.rs`  
**Changes:**
1. Update the `--editor` arg documentation from "Supported: `vscode`, `cursor`, `copilot`" to "Supported: `vscode`, `cursor`"
2. Update "Defaults to all supported editors" (still correct after removal)

**Verification:** `cargo build` (compile-time doc strings)  
**Posture:** Direct edit

### Unit 3: Add legacy cleanup path to uninstall

**Domain:** Code  
**Files:** `src/workspace/mcp_config.rs` (already in Unit 1 scope — merge if single-session)  
**Changes:**
1. In `remove_mcp_configs()`, add `.github/copilot/mcp.json` as a hardcoded legacy path to check and remove, independent of the `Editor` enum

**Verification:** Add a test that places a file at `.github/copilot/mcp.json` containing "graphtor-docs" and verifies `remove_mcp_configs` removes it.  
**Posture:** Test-first

## Dependency Graph

```
Unit 1 (remove variant) → Unit 2 (CLI help)
Unit 1 → Unit 3 (legacy cleanup)
```

Units 2 and 3 can run in parallel after Unit 1.

## Decisions and Rationale

- **Keep legacy path in uninstall**: Operators who already installed may have the stale
  `.github/copilot/mcp.json`. The uninstall path should clean it up even though the enum
  variant is gone. This is defense-in-depth.
- **No deprecation warning**: The path never worked, so there's no "upgrade path" to
  communicate. Silent removal is appropriate.

## Risks and Caveats

- **Risk:** A future VS Code or Copilot update adopts `.github/copilot/mcp.json`.
  **Mitigation:** Re-add the variant in a future PR if evidence emerges. The removal is
  easily reversible.
- **Risk:** Breaking compilation if other code references `Editor::Copilot`.
  **Mitigation:** Grep confirmed no references outside `mcp_config.rs` itself.

## Plan Hardening Signals

| Signal | Present? | Justification |
|---|---|---|
| Public API, schema, or contract change | No | Internal enum, not exposed via public API |
| Security, auth, permission, or compliance-sensitive | No | Config file placement only |
| Migration, backfill, destructive data/config action | Marginal | Removes a file on uninstall, but this is existing behavior scoped to graphtor-managed files only |
| External integration, operator checkpoint | No | No external systems affected |
| High runtime, rollout, or rollback risk | No | Simple code removal |

**Requires plan hardening: no**

## Runtime Verification and Closure

- **Unit 1** changes the install runtime surface (CLI). Verify by running `graphtor-docs install`
  in a temp directory and confirming only `.vscode/mcp.json` and `.cursor/mcp.json` are created.
- **Unit 3** changes the uninstall runtime surface. Verify by placing a legacy file and running
  `graphtor-docs uninstall --confirm`, confirming the legacy file is removed.
- No monitoring or rollback needed — this is a local CLI tool, not a service.
