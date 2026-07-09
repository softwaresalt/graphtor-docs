---
title: "install-harness's documented backlog-registry.yaml copy step can be silently skipped"
description: "The install-harness skill explicitly documents copying a pre-built backlog-registry.yaml from autoharness_home, but a prior install completed without that file ever being created, and its absence was initially misdiagnosed as intentional"
problem_type: "silent_tooling_gap"
category: "workflow-issues"
component: "install-harness / .autoharness/backlog-registry.yaml"
root_cause: "An earlier install-harness run completed without executing its own documented step to copy templates/backlog/registries/{tool}.registry.yaml into .autoharness/backlog-registry.yaml, and no downstream verification step (harness-doctor, verify-workspace) treats the file's absence as a hard failure"
resolution_type: "workaround"
severity: "medium"
message: "N/A — absence of a file is the symptom, not an error message"
file_path: ".autoharness/backlog-registry.yaml"
citations:
  - "PR #85 (softwaresalt/graphtor-docs)"
  - ".copilot/installed-plugins/autoharness/autoharness/.github/skills/install-harness/SKILL.md"
tags:
  - "autoharness"
  - "install-harness"
  - "backlogit"
  - "backlog-registry"
  - "silent-failure"
---

## Problem

Session memory notes from a prior install described the missing
`.autoharness/backlog-registry.yaml` as "pre-existing intentional behavior"
associated with a manual/file-backed backlog mode. When the operator's own
intuition contradicted that conclusion twice, re-investigation found that
`install-harness/SKILL.md` has an explicit, unambiguous step to copy this
exact file from `{autoharness_home}/templates/backlog/registries/{tool}.registry.yaml`
during a normal install — it was a genuine install defect, not intentional
design.

## Root Cause

The install workflow's copy step for the backlog registry ran (or was
believed to have run) without producing the expected output file, and no
later verification gate (`harness-doctor`, `autoharness verify-workspace`)
flags the file's absence as a blocker, so the gap persisted undetected
across multiple later tuning sessions.

## Resolution

1. Copy the file verbatim from
   `{autoharness_home}/templates/backlog/registries/{tool_name}.registry.yaml`
   to `.autoharness/backlog-registry.yaml` — it is a static, pre-built
   artifact requiring no template-variable substitution.
2. Cross-check every `operations[].mcp_tool` entry in the copied registry
   against the live backlog tool's own discovery/metadata surface (e.g.
   `backlogit_get_metadata_catalog`, `backlogit_export_command_map`) rather
   than trusting the template's tool names by inspection alone.

## Prevention

* Do not treat a missing generated/installed artifact as "intentional" purely
  because old session notes describe it that way — re-verify against the
  authoritative install skill documentation before accepting that
  explanation.
* When an operator's intuition disagrees with an agent's conclusion twice in
  the same session, re-investigate with fresh evidence rather than
  defending the original conclusion.
* Consider adding an explicit `harness-doctor` check for
  `.autoharness/backlog-registry.yaml` presence when a backlog tool
  capability pack is enabled, so this class of gap surfaces automatically on
  the next health check instead of requiring manual discovery.
