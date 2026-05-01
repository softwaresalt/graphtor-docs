---
title: "Stale Cargo Processes Block Artifact Lock Indefinitely"
description: "Concurrent cargo test invocations leave zombie processes holding the target artifact lock, blocking subsequent runs forever."
problem_type: "deadlock"
category: "workflow-issues"
component: "cargo / target artifact cache"
root_cause: "Two cargo test processes were launched concurrently in separate shell sessions. One process completed but its cargo parent remained alive (low CPU, no progress). It retained the exclusive file lock on target/.cargo-lock, blocking any subsequent cargo invocation."
resolution_type: "workaround"
severity: "high"
message: "Blocking waiting for file lock on artifact directory"
file_path: "target/.cargo-lock"
citations:
  - "Session 85c761f8 — 008-S vector search implementation"
  - "PR #14 feat/vector-search"
tags:
  - "cargo"
  - "file-lock"
  - "concurrent-processes"
  - "artifact-cache"
  - "windows"
---

## Problem

Running `cargo test` printed `Blocking waiting for file lock on artifact directory` and
never progressed. A fresh `cargo test` invocation hung indefinitely after killing the
terminal that had launched a prior test run — the prior cargo process remained alive as
a zombie in the background.

Symptoms:
- `cargo test` blocks permanently with no compilation activity
- `Get-Process | Where-Object { $_.Name -like "cargo*" }` shows one or more cargo PIDs
  with near-zero CPU (`< 5 seconds` total) that have not terminated
- `logs\test-final.txt` contains only the single line:
  `Blocking waiting for file lock on artifact directory`

## Root Cause

`cargo` serialises all builds and tests to a workspace-level artifact directory by
acquiring an exclusive lock on `target/.cargo-lock`. When multiple `cargo test`
commands are run concurrently (e.g., by an agent that starts a second run before the
first finishes), the second process waits for the lock. If the first cargo process
becomes a zombie (its shell session closed but the process was not signalled), it
retains the lock without making forward progress, blocking all subsequent cargo
invocations indefinitely.

On Windows, process handles are inherited and the lock file is not automatically
released when the terminal is closed.

## Resolution

1. Identify stale cargo PIDs:
   ```powershell
   Get-Process | Where-Object { $_.Name -like "cargo*" } | Select-Object Id, Name, CPU, WS
   ```

2. Inspect CPU: if a cargo process has accumulated only a few seconds of CPU and has
   not changed in 30+ seconds, it is stale.

3. Kill each stale process by PID (never by name):
   ```powershell
   Stop-Process -Id <PID> -Force
   ```

4. Confirm no cargo processes remain, then re-run:
   ```powershell
   cargo test 2>&1 | Out-File logs\test-results.txt
   ```

## Prevention

- Never start two concurrent `cargo` invocations against the same workspace.
- When launching long-running `cargo test` in async mode, always wait for the prior
  shell to complete before starting another.
- In agent workflows, use a single synchronous `cargo test` per quality-gate phase
  rather than background shells.
- After a shell session is force-closed mid-cargo-run, always check for lingering
  cargo processes before starting a new build.
