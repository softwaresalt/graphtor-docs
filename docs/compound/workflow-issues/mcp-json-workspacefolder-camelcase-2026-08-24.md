---
title: "VS Code MCP config env interpolation requires exact camelCase ${workspaceFolder}"
description: "${workspace_folder} (snake_case) is not a recognized VS Code predefined variable in .mcp.json env blocks and passes through literally, silently breaking workspace-relative env vars for MCP servers"
problem_type: "config_error"
category: "workflow-issues"
component: ".mcp.json"
root_cause: "The repository-root .mcp.json used the snake_case token ${workspace_folder} for the BACKLOGIT_WORKSPACE and ENGRAM_WORKSPACE env entries; VS Code's MCP configuration reference only recognizes the exact camelCase ${workspaceFolder} predefined variable, so the snake_case token is never substituted and is passed through to the child process literally"
resolution_type: "config_change"
severity: "medium"
message: "BACKLOGIT_WORKSPACE=${workspace_folder} (literal, unsubstituted)"
file_path: ".mcp.json"
citations:
  - "PR #106: chore/stage-049-S, Ship commit 1af5239"
  - "docs/memory/2026-08-24/049-s-ship-pr-106-lifecycle-memory.md"
tags:
  - "mcp"
  - "vscode"
  - "workspaceFolder"
  - "env-interpolation"
  - "config"
---

## Problem

The repository-root `.mcp.json` (agent-harness dev tooling, not a
`graphtor-docs` product artifact) configured the `backlogit` and `engram` MCP
server entries with:

```json
"env": {"BACKLOGIT_WORKSPACE": "${workspace_folder}"}
```

Any consumer relying on `BACKLOGIT_WORKSPACE`/`ENGRAM_WORKSPACE` resolving to
an absolute workspace path would instead receive the literal, unsubstituted
string `${workspace_folder}`, since VS Code (and MCP clients following its
config conventions) never recognizes that token.

## Root Cause

VS Code's MCP configuration reference documents predefined variables in
**exact camelCase**: `${workspaceFolder}`. There is no snake_case
`${workspace_folder}` variant. Unlike some templating systems, an
unrecognized `${...}` token is not an error — it is passed through to the
`env`/`args`/`command` value literally, so the failure is silent: no error is
raised, but the environment variable's actual runtime value is wrong.

## Resolution

Correct the token casing:

```diff
- "env": {"BACKLOGIT_WORKSPACE": "${workspace_folder}"},
+ "env": {"BACKLOGIT_WORKSPACE": "${workspaceFolder}"},
```

Applied to both the `backlogit` and `engram` entries only; other server
entries (`context7`, `tavily`, `github`) did not use this variable at all and
were left untouched. Validated with a direct JSON round-trip
(`Get-Content .mcp.json -Raw | ConvertFrom-Json`) and against VS Code's own
MCP configuration reference, which explicitly warns: "Make sure to use the
exact casing (`${workspaceFolder}`)."

## Prevention

1. When authoring or reviewing `.mcp.json` / `.vscode/mcp.json` env, args, or
   cwd values, use the exact camelCase `${workspaceFolder}` token — never a
   snake_case or otherwise differently-cased variant.
2. Because an unrecognized `${...}` token fails silently (passed through
   literally, no error), this class of defect is easy to miss in review
   unless someone actually inspects the resolved runtime environment.
   Grep repository config for `${workspace_folder}` or similar snake_case
   variants as a quick sanity check when reviewing MCP config changes.
3. This repository's own `graphtor-docs` product generator
   (`src/workspace/mcp_config.rs::resolve_command`) does not use
   `${workspaceFolder}`-style interpolation at all for the `.mcp.json` files
   it generates for consumers — it resolves an absolute, canonicalized path
   directly. The interpolation convention documented here applies only to
   this repository's own dev-tooling root `.mcp.json` (consumed by the local
   VS Code / Copilot CLI agent harness), not to files the `graphtor-docs`
   binary itself generates.
