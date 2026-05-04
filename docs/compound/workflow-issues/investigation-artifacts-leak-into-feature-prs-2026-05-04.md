---
title: "Investigation artifacts leaking into feature PRs"
description: "Performance investigation changes (profile settings, diagnostic binaries) can accidentally leak into unrelated feature PRs"
problem_type: "scope_creep"
category: "workflow-issues"
component: "Cargo.toml"
root_cause: "Performance investigation (release profile tuning) done on the same branch as the feature implementation gets committed together"
resolution_type: "workaround"
severity: "low"
message: "[profile.release] settings were added in this PR (including panic = abort). This is a behavior and build-output change not described in the PR summary"
file_path: "Cargo.toml"
citations:
  - "PR #25 review comment PRRT_kwDORiB5E85_QxZN"
tags:
  - "scope-creep"
  - "pr-hygiene"
  - "cargo-profile"
  - "investigation"
---

## Problem

During performance investigation of large PDF ingestion, a `[profile.release]` section was added to `Cargo.toml` to build optimized release binaries for timing. This section (`opt-level = 3`, `lto = "thin"`, `panic = "abort"`, `strip = "symbols"`) was still present when the feature branch was committed, and Copilot review flagged it as unrelated scope that affects the entire workspace.

## Root Cause

Performance investigation and feature implementation were done on the same branch. Investigation artifacts (build profile tuning, diagnostic binaries) naturally accumulate alongside implementation code. Without explicit separation, they get committed together.

## Resolution

Removed `[profile.release]` from the feature PR during review fix cycle. Diagnostic binary (`pdf_diag.rs`) was kept as it has standalone value for future PDF performance analysis.

## Prevention

When doing performance investigation that leads to a feature:

1. **Keep investigation on a separate throwaway branch** or use `git stash` to isolate investigation-only changes
2. **Before committing the feature**, review `git diff` against main to catch workspace-wide changes (profiles, settings, CI config)
3. **`[profile.release]` changes deserve their own PR** — they affect all binaries in the workspace and should be reviewed for their build-time and binary-size implications
4. **Diagnostic binaries** (`src/bin/*.rs`) are fine to include if they have ongoing value, but should be called out in the PR description
