---
type: session-memory
agent: ship
date: 2026-08-24
branch: chore/stage-049-S
base_head: 65cff5240f31cbea51fcc32e7178582cc8f7a9b9
pr: 106
feature: 056-F
shipment: 049-S
scope: pr-106-lifecycle-handoff
---

# Ship PR #106 lifecycle handoff — 056-F / 049-S staging (MCP serve OS error 232)

Bounded Ship session scoped strictly to making PR #106 ready for
operator-approved merge. Per explicit operator instruction, this session does
**not** claim shipment 049-S, does not implement any of its eight evidence
tasks, does not merge the PR, does not checkout `main`, and does not begin
post-merge closure.

## Starting state verified

* Branch `chore/stage-049-S`, single worktree at `C:/Source/GitHub/graphtor`
  (no parallel worktree — P-016 topology OK).
* Remote `origin/chore/stage-049-S` HEAD: `65cff5240f31cbea51fcc32e7178582cc8f7a9b9`,
  matching PR #106 `headRefOid` at session start.
* Local unpushed Stage correction commit `178c54cf90894883a734ac911d5d75cc6ec96357`
  verified by direct diff inspection: four one-line backlog task corrections
  (`056.013-T`, `056.017-T`, `056.018-T`, `056.028-T`), one plan clippy-gate
  line gaining `--all-targets`, a memory changed-file count fix (8 to 9), and
  one new Stage memory file. The diff matches Stage's reported six-comment
  review correction exactly.
* Tool availability: `backlogit` and `github` MCP surfaces reachable and
  returned real data. Feature `056-F` has 28 tasks. Shipment `049-S` has
  exactly `056.020-T, 056.022-T, 056.023-T, 056.021-T, 056.001-T, 056.002-T,
  056.003-T, 056.019-T`, `status: queued`, not yet claimed.

## `.mcp.json` validation (Ship-owned config correction)

* Pre-existing uncommitted change, not authored by Stage or this session:
  `${workspace_folder}` corrected to `${workspaceFolder}` for the
  `BACKLOGIT_WORKSPACE` and `ENGRAM_WORKSPACE` entries only. The `context7`,
  `tavily`, and `github` server entries are untouched.
* JSON validity confirmed directly (`ConvertFrom-Json` round-trip succeeds).
* Interpolation contract confirmed against VS Code's MCP configuration
  reference: `${workspaceFolder}` (exact camelCase) is the only recognized
  predefined variable for `env`/`args`/`cwd` substitution. `${workspace_folder}`
  (snake_case) is not a recognized token and would have passed through
  literally, breaking workspace binding for the `backlogit` and `engram`
  servers.
* A repository-wide search found no other occurrence of the broken
  `${workspace_folder}` token outside the Stage memory narrative describing
  the defect, so no companion documentation correction is required.
* No `CHANGELOG.md` entry added: the repository-root `.mcp.json` is
  agent-harness dev tooling, not a `graphtor-docs` product/release artifact
  tracked by that file.
* Verdict: the change is correct, minimal, and isolated. Carried forward
  as-is into the PR package.

## Preserved, untouched

`.backlogit/checkpoints/checkpoint-20260822-090657.json`,
`.backlogit/checkpoints/checkpoint-20260822-092508.json`, `.backlogit/runtime/`,
and the root helper scripts (`cmd_runner.sh`, `exec_8_commands.ps1`,
`exec_8_commands.sh`, `exec_commands.ps1`, `exec_git_commands.sh`,
`git_commands_output.ps1`, `run_8_commands.sh`, `run_commands.ps1`,
`run_git_commands.sh`, `search_patterns.ps1`, `temp_git_commands.sh`) are not
staged, edited, or deleted by this session.

## Next steps (this session)

1. Commit `.mcp.json` plus this memory file.
2. Run a fresh current-HEAD local review (report-only) over the full
   `origin/main...HEAD` diff.
3. Push to `origin/chore/stage-049-S`.
4. Rewrite the PR #106 body: 28 total `056-F` tasks, the exact eight-task
   `049-S` evidence manifest, staging/planning plus `.mcp.json` scope only, and
   a refreshed `## Local Review Readiness` block for the final HEAD.
5. Triage all unresolved bot review threads, not only the six newest.
6. Optionally request one fresh Copilot shadow review, then run the §1.9
   defense-in-depth readiness gate.
7. Stop at operator-approved-merge readiness: no merge, no `main` checkout, no
   049-S claim, no post-merge closure.
