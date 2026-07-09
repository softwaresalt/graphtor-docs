---
title: "Stale harness-manifest paths silently hide real template drift from diff-based tuning"
description: "When harness-manifest.yaml references file paths that no longer exist (post-upstream-rename), diff-based drift detection silently skips the comparison instead of flagging a gap"
problem_type: "silent_tooling_gap"
category: "workflow-issues"
component: "autoharness tune-harness / harness-manifest.yaml"
root_cause: "The tuning workflow's drift-detection step compares manifest-recorded paths against current template content via os.path.exists() gating; when upstream renames or relocates a tracked file, the manifest still points at the old path, the existence check fails, and the comparison is skipped entirely rather than raising a mismatch"
resolution_type: "workaround"
severity: "high"
message: "N/A — absence of an error is the symptom"
file_path: ".autoharness/harness-manifest.yaml"
citations:
  - "PR #85 (softwaresalt/graphtor-docs) — .ship.agent.md / .stage.agent.md / _orchestrator.agent.md drift discovery"
  - ".autoharness/tuning-reports/2026-07-08-tuning-report.md"
tags:
  - "autoharness"
  - "harness-manifest"
  - "drift-detection"
  - "tune-harness"
  - "silent-failure"
---

## Problem

A tuning pass initially reported the harness as fully up to date, but
`.ship.agent.md`, `.stage.agent.md`, and `_orchestrator.agent.md` had
actually drifted significantly from current upstream templates (missing an
entire new governance framework: P-014 through P-017). The manifest showed
these three files as tracked and unchanged.

## Root Cause

The manifest's recorded paths for these three agents were stale — they
pointed at a pre-rename file layout. The diff-based drift-detection script
gated its comparison on `os.path.exists(manifest_path)`; since the recorded
path didn't exist, the check silently returned "no diff" instead of
surfacing "path not found, cannot verify." Files present under their
**correct**, renamed path were therefore never actually diffed against the
current template — they were invisible to the tuning pass by construction,
not because they were verified clean.

## Detection Method

Discovered only via a targeted follow-up: grepping all upstream templates
for feature keywords known to be new (`P-016|P-017|DARK_MODE`) and noticing
that three agent *templates* matched but the corresponding *installed*
files had never actually been compared.

## Resolution / Prevention

1. Treat "manifest path does not exist on disk" as a **hard error**
   requiring investigation, never a silent pass, in any custom
   drift-detection tooling built on top of `harness-manifest.yaml`.
2. After any upstream file rename/relocation is discovered, immediately
   audit the manifest for other paths that may have gone stale in the same
   release, rather than fixing only the one path that triggered discovery.
3. Periodically cross-check the full manifest's path list against
   `os.path.exists()` as an explicit pre-flight step (separate from content
   diffing) so missing-file gaps surface before drift analysis even begins.
4. When template content is suspected of drifting, grep the upstream
   template source for keywords tied to recently-announced features as a
   sanity check independent of manifest-driven comparison.
