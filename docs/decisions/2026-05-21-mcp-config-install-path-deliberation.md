---
title: "MCP Configuration Install Path Fix"
description: "Lightweight deliberation on removing the non-functional Editor::Copilot MCP config path from install defaults while preserving legacy uninstall cleanup."
topic: "MCP config install defaults"
depth: "lightweight"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/exec-plans/2026-05-21-mcp-config-install-path-plan.md"
tags:
  - "mcp"
  - "install"
  - "cli"
  - "editor-config"
  - "copilot"
---

## Problem Frame

The `graphtor-docs install` command generates MCP client configuration files for
editor integration. When invoked without an explicit `--editor` flag, it defaults
to writing configs for ALL supported editors: VS Code (`.vscode/mcp.json`),
Cursor (`.cursor/mcp.json`), and GitHub Copilot (`.github/copilot/mcp.json`).

The issue: `.github/copilot/mcp.json` is **not a recognized configuration path**
for any current tool:

- **VS Code** recognizes `.vscode/mcp.json` ✓
- **Cursor** recognizes `.cursor/mcp.json` ✓
- **GitHub Copilot CLI** does not read `.github/copilot/mcp.json` — it uses
  `~/.config/github-copilot/` or environment-specific paths
- **VS Code with Copilot extension** uses `.vscode/mcp.json` (shared with VS Code)

Writing to `.github/copilot/mcp.json` creates a stale, unrecognized file that
confuses operators who assume it must be working since it was auto-generated.

## Scope

- **In scope:** Fix the default behavior of `generate_mcp_configs` and the
  `Editor::Copilot` variant to either remove the non-functional path, replace it
  with a correct path, or make it opt-in rather than default.
- **Out of scope:** Adding new editor targets, restructuring the install flow.

## Options

### Option A: Remove `Editor::Copilot` entirely

Remove the `.github/copilot/mcp.json` target. Only VS Code and Cursor remain as
defaults. The Copilot variant added no real value since the path is unrecognized.

- **Pros:** Simplest fix, eliminates dead code, no false config file generation
- **Cons:** If a future Copilot CLI version adopts this path, we'd need to re-add
- **Effort:** Low (single file, ~10 lines removed)

### Option B: Make `Editor::Copilot` opt-in only

Keep the code but exclude it from the "all editors" default set. Only write
`.github/copilot/mcp.json` when explicitly requested via `--editor copilot`.

- **Pros:** Preserves extensibility, no false defaults, operators can opt in
- **Cons:** Slightly more complex (need a "default set" vs "all set" distinction)
- **Effort:** Low (add `is_default()` method or filter on default set)

### Option C: Replace path with correct Copilot CLI config location

Research the actual Copilot CLI config path and write there instead.

- **Pros:** Correct behavior for Copilot CLI users
- **Cons:** Copilot CLI config paths are user-global (`~/.config/...`), not
  project-local — breaks the project-local convention of all other configs.
  Path varies by platform. May conflict with user's existing Copilot config.
- **Effort:** Medium (platform detection, user-dir resolution)

## Chosen Direction

**Option A: Remove `Editor::Copilot` entirely.**

Rationale:
1. The path is non-functional in all current environments.
2. No evidence that any tool will adopt `.github/copilot/mcp.json` in the future.
3. VS Code + Cursor cover the actual user base.
4. The removal is additive-safe — it can be re-introduced with a correct path later.
5. Simplest fix with zero risk of breaking working configurations.

## Rejected Alternatives

- **Option B** adds unnecessary complexity for a path nobody uses.
- **Option C** introduces platform-specific user-directory logic for a config
  target that doesn't demonstrably work — premature and risky.

## Unresolved Questions

None. The fix is straightforward.

## Risks and Mitigations

- **Risk:** A user explicitly passed `--editor copilot` expecting it to work.
  **Mitigation:** Add a deprecation warning or remove the variant with a clear
  changelog entry.

## Success Criteria

1. `graphtor-docs install` no longer writes `.github/copilot/mcp.json` by default
2. The `Editor::Copilot` variant and its config path are removed
3. `graphtor-docs uninstall` still cleans up any pre-existing `.github/copilot/mcp.json`
4. Existing tests pass; removed variant's test coverage is updated
