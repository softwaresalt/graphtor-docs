---
problem_type: workflow-bug
category: ci-cd
component: github-actions
root_cause: misuse-of-glob-syntax
resolution_type: pattern-fix
severity: high
message: "GitHub Actions on.push.tags uses glob patterns, not regex — [0-9]+ does not mean one-or-more digits"
file_path: .github/workflows/release.yml
citations:
  - pr: 44
    sha: 00e3e30
tags: [github-actions, release, glob, tag-trigger]
---

## Problem

`.github/workflows/release.yml` used a tag trigger pattern that appeared valid
but never fired for normal semver tags:

```yaml
on:
  push:
    tags:
      - "v[0-9]+.[0-9]+.[0-9]+*"
```

The `+` in GitHub Actions glob patterns is a **literal character**, not a
regex quantifier meaning "one or more". As a result, only tags containing
literal `+` characters would match — no normal semver tag ever triggered
the release workflow.

## Root Cause

GitHub Actions `on.push.tags` uses **fnmatch-style glob patterns**, not
regular expressions. Glob metacharacters: `*` (any sequence), `?` (single
character), `[abc]` (character class). The `+` quantifier has no special
meaning in globs — it matches only the literal `+` character.

## Resolution

Use valid glob syntax for semver tag matching:

```yaml
on:
  push:
    tags:
      - "v[0-9]*.[0-9]*.[0-9]*"
```

`[0-9]*` in glob means "one digit followed by zero or more of any character"
— effectively "starts with a digit, then anything". This correctly matches
`v0.2.0`, `v1.0.0`, `v1.2.3-rc1`, `v10.0.0-beta`, etc.

## Prevention

- Never use `+` as a quantifier in GitHub Actions glob patterns.
- Test tag trigger patterns mentally against concrete examples before
  committing (`v0.2.0` matches, `v0+.2+.0+` does not).
- Prefer `v[0-9]*.[0-9]*.[0-9]*` as the canonical semver glob for
  GitHub Actions release workflows.
