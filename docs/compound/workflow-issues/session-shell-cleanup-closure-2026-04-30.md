---
title: "Session Shell Cleanup and Branch-Fix Steps in Operational Closure"
category: workflow-issues
date: 2026-04-30
tags: [closure, git, windows, shell-cleanup, async-processes]
---

## Problem

At the end of a multi-session task, git operations (especially `git checkout main`) can fail
or produce unexpected results due to:

1. **Stale async shells** — previous Copilot CLI async shell sessions (`mode="async"`) leave
   background processes whose file locks interfere with git checkout and rebase.
2. **MCP server file locks** — long-running MCP servers (e.g., backlogit) hold SQLite databases
   open exclusively on Windows, causing `git checkout` to fail with
   `"unable to unlink .backlogit/backlogit.db"`.
3. **Corrupt local branch pointer** — a branch's local ref can point to a corrupt or stale
   commit (e.g., from a failed force-push recovery), while `origin/<branch>` is already clean.
   `git checkout <branch>` then fails with `"invalid path"` errors because git attempts to
   apply the diff from the corrupt commit into the working tree.

## Pattern: Pre-Closure Shell and Branch Audit

Add these steps at the START of every operational closure cycle, before creating the closure
branch:

### Step 1 — Inventory active processes

```powershell
Get-Process | Where-Object { $_.Name -match 'cargo|rustc|git' } | Select-Object Id, Name, CPU, StartTime | Format-Table -AutoSize
```

- **Stale `cargo`/`rustc`** processes (start time more than a few minutes ago and still
  running): stop them with `Stop-Process -Id <PID>`.
- **Transient `git`** processes: leave them; they complete naturally.
- **MCP server processes** (backlogit, etc.): leave them running — they are intentional
  background services.

### Step 2 — Verify local branch pointers before checkout

```powershell
git branch -v
```

Compare local branch tip SHAs against the refreshed `origin/<branch>` tip: run `git fetch`
first, then inspect it with `git log --oneline -3 origin/<branch>`. If the local branch is
**behind or ahead unexpectedly**, use `git update-ref` to align it:

```powershell
git update-ref refs/heads/main <clean-sha>
```

This moves the branch pointer without touching the working tree — safe even when files are
locked by MCP servers.

### Step 3 — Stop stale Copilot CLI async shells

Without `stop_powershell` (the companion tool that is sometimes absent from the tool manifest),
identify and stop any PowerShell processes spawned as async shells:

```powershell
# List long-running PowerShell child processes
Get-Process pwsh | Where-Object { $_.StartTime -lt (Get-Date).AddHours(-1) } | Select-Object Id, StartTime
```

Stop each stale one individually:

```powershell
Stop-Process -Id <PID>
```

> **Note:** Do NOT use `Stop-Process -Name pwsh` — this kills all PowerShell instances
> including your current session.

### Step 4 — Switch to main

After Steps 1–3, `git checkout main` should succeed cleanly. If it still fails with
`"unable to unlink"` errors, the MCP server lock is still active. Solutions:

- Restart the MCP server (if safe to do so) to release file handles.
- Alternatively, continue the closure work on the feature branch and use
  `git push origin HEAD:main --force-with-lease` to push directly.

## Adding These Steps to the Closure Skill

The `operational-closure` skill (`SKILL.md`) should include a **Step 0: Environment Cleanup**
section before "Step 1: Gather Closure Context":

```text
### Step 0: Environment Cleanup

Before switching branches or creating a closure branch:

1. Run process inventory: `Get-Process | Where-Object { $_.Name -match 'cargo|rustc' }`
   Stop any stale build processes with `Stop-Process -Id <PID>`.

2. Check local branch pointers: `git branch -v`
   If a local branch is behind origin or points to a corrupt commit, advance it:
   `git update-ref refs/heads/<branch> <target-sha>`

3. If `stop_powershell` is available (Copilot CLI async tool):
   Close all shellIds opened during the session before switching branches.
   If `stop_powershell` is not available, use `Get-Process pwsh` to identify
   long-lived async shells and stop them by PID.

4. Verify `git checkout main` succeeds before creating the closure branch.
```

## Root Cause of the `invalid path` Failures

The `git checkout main` → `"error: invalid path '.autoharness\r/config.yaml'"` errors
arise when:

1. A subagent creates a commit with CRLF line endings that become embedded in **file paths**
   (not just file content). This produces paths containing literal `\r` characters.
2. The local `main` branch pointer is not updated to a clean commit after a force-push recovery.
3. Git tries to apply the delta between the current HEAD and `main` (the corrupt commit),
   which includes files with `\r` in their paths — Windows rejects these as invalid.

Fix: `git update-ref refs/heads/main <clean-sha>` to skip the corrupt commit entirely.

## Evidence

- Session: 003-S pipeline foundation closure (2026-04-30)
- Branch: `feature/007-pipeline-orchestrator`
- Corrupt commit: `77ddae8` (force-pushed away; `origin/main` restored to `f892973`)
- Local `main` still at `77ddae8` after force-push (git does not auto-update local tracking)
- Fix applied: `git update-ref refs/heads/main f892973` → `git checkout main` succeeded

## See Also

- `docs/compound/git-pull-blocked-by-sqlite-wal-lock.md` — related Windows SQLite lock issue
- `.gitignore` entry `.backlogit/backlogit.db` — prevents the lock from blocking git operations
