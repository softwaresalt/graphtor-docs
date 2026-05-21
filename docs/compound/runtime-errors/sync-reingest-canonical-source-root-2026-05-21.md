---
title: "Canonicalize source_root before strip_prefix in sync reingest on Windows"
description: "Incremental sync can misclassify valid files as outside the source root when file and root paths are normalized differently"
problem_type: "runtime-failure"
category: "runtime-errors"
component: "src/sync/reingest.rs"
root_cause: "reingest_file canonicalized file_path but compared it against a non-canonical source_root, so Windows path normalization differences broke strip_prefix"
resolution_type: "code_fix"
severity: "medium"
message: "file '<path>' is not within source root '<root>'"
file_path: "src/sync/reingest.rs"
citations:
  - ".backlogit/queue/036.001-T.md"
  - "docs/exec-plans/2026-05-21-backlogit-operator-experience-plan.md"
tags:
  - "sync"
  - "windows"
  - "path-normalization"
  - "reingest"
---

## Problem

The incremental sync pipeline could detect a changed local file but then fail to
reingest it on Windows. The symptom surfaced as sync metrics reporting
`files_total = 1` and `files_synced = 0` for a one-file local source, with the
reingest path treating the file as if it were outside the source root.

## Root Cause

`reingest_file()` validated and canonicalized `file_path` through
`validate_path()`, but it used the original `source_root` when calling
`strip_prefix()`. On Windows, the validated file path and the raw source root
can differ in normalization details even when they refer to the same location.
That mismatch made `strip_prefix()` fail and raised the pipeline error
`file '<path>' is not within source root '<root>'`.

## Resolution

Canonicalize `source_root` with the same `validate_path()` call used for
`file_path`, then run `strip_prefix()` against the canonical root. This keeps
both operands in the same normalized form and preserves the intended workspace
boundary checks.

The fix landed in `src/sync/reingest.rs`, and the sync metrics unit test in
`src/sync/mod.rs` now covers the local-source case that exposed the failure.

## Prevention

* When deriving relative paths from validated filesystem paths, normalize both
  sides before comparing or stripping prefixes
* Do not mix canonicalized paths with raw user-supplied roots in security or
  path-containment checks
* Keep a targeted local-source sync test that asserts both file counts and
  successful reingest metrics on Windows-style path handling
