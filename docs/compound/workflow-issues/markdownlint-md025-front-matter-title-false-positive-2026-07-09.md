---
title: "markdownlint MD025 front_matter_title matches ANY `title:` key in YAML frontmatter"
description: "MD025's front_matter_title default option pattern-matches any line starting with title: anywhere in frontmatter, not just an actual document-title field, causing false 'multiple top-level headings' errors"
problem_type: "false_positive_lint_error"
category: "workflow-issues"
component: "markdownlint / .markdownlint.json"
root_cause: "MD025's default front_matter_title regex matches any YAML key literally named `title` (or `title=`) anywhere in the frontmatter block, including nested schema-style fields such as input.properties.title, and treats it as an implicit H1 competing with real Markdown headings"
resolution_type: "config_change"
severity: "medium"
message: "Multiple top-level headings in the same document"
file_path: ".markdownlint.json"
citations:
  - "PR #85 (softwaresalt/graphtor-docs) — .github/skills/pr-lifecycle/SKILL.md false positive"
  - ".autoharness/tuning-reports/2026-07-08-tuning-report.md"
tags:
  - "markdownlint"
  - "MD025"
  - "frontmatter"
  - "false-positive"
  - "harness-tuning"
---

## Problem

`.github/skills/pr-lifecycle/SKILL.md` produced a persistent MD025 "multiple
top-level headings" error that resisted several hypotheses: frontmatter
parsing quirks, CRLF line endings, a list value as the last frontmatter line,
and YAML nesting depth. None of these explained it.

## Root Cause

MD025's `front_matter_title` option (enabled by default when MD025 is `true`)
scans the raw frontmatter text for any line matching a `title:` or `title=`
pattern — **regardless of nesting** — and treats the matched value as an
implicit document title. In this file, a JSON-schema-style field named
`input.properties.title` (describing a tool input parameter, not the
document) accidentally matched the pattern and was treated as a second H1,
conflicting with the real `# Title` heading in the body.

This can recur with **any** file that has a schema property, config key, or
nested YAML field literally named `title`, not just the one file discovered
here.

## Resolution

Disable the implicit frontmatter-title matching at the config root cause
instead of working around it per-file:

```json
{
  "MD025": { "front_matter_title": "" }
}
```

Setting `front_matter_title` to an empty string disables the pattern
entirely, so MD025 only flags actual duplicate `#`/`##`-style top-level
Markdown headings in the document body — which is the behavior actually
wanted.

## Detection Method

Reproduced via isolated `repro-test*.md` files in a temp directory, varying
one condition at a time (frontmatter list-vs-flow YAML style, nesting depth,
presence/absence of a `title:` key) until the trigger was isolated to: *any*
`title:` key existing anywhere in frontmatter, independent of nesting or
YAML style.

## Prevention

When adding new fields to instruction/skill/agent frontmatter or embedded
JSON-schema blocks, avoid naming a field literally `title` if MD025 is
enabled with default options — or confirm `front_matter_title` is already
disabled in `.markdownlint.json` before assuming the lint config is safe.
