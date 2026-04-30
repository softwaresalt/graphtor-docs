---
title: "gh pr create --body fails with backticks in PowerShell — use --body-file"
description: "Backtick characters in gh pr create --body conflict with PowerShell's escape character, causing parse errors"
problem_type: "shell_escaping"
category: "workflow-issues"
component: "scripts/pr-creation"
root_cause: "PowerShell uses backtick as its escape character; backtick-quoted identifiers in Markdown PR bodies break the parser"
resolution_type: "workaround"
severity: "medium"
message: "ParserError: The Unicode escape sequence is not valid. A valid sequence is `u{ followed by one to six hex digits"
file_path: "logs/pr-body.md"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/7"
tags:
  - "powershell"
  - "gh-cli"
  - "pr-creation"
  - "escaping"
---

## Problem

When using `gh pr create --body "..."` with Markdown content that includes
backtick-quoted identifiers (e.g., `` `SourceRecord` ``, `` `kind="local"` ``),
PowerShell raises a parse error:

```text
ParserError:
Line |
  29 |  … extracts real `kind`/`url` |
     |                         ~~
     | The Unicode escape sequence is not valid. A valid sequence is `u{
     | followed by one to six hex digits and a closing '}'.
```

PowerShell treats the backtick (`` ` ``) as its string escape character.
Inside double-quoted strings, `` `u `` is interpreted as a Unicode escape
sequence prefix, causing the parser to fail on any `` `url ``, `` `use ``,
or similar sequences.

## Root Cause

PowerShell's escape character is `` ` `` (backtick), not `\`. Inside a
double-quoted string `"..."`, backtick begins an escape sequence. The
pattern `` `u `` is interpreted as the start of a Unicode codepoint escape
(`` `u{XXXXXX} ``), so any Markdown containing `` `url` ``, `` `use` ``,
or `` `unique` `` will fail with the Unicode escape error.

## Resolution

Write the PR body to a file in `logs/` and use `--body-file` instead:

```powershell
# Write body to temp file
Set-Content logs\pr-body.md -Value $prBody

# Create PR using file
gh pr create --title "..." --body-file logs\pr-body.md --base main --head feature/...
```

The `--body-file` flag reads the file directly, bypassing PowerShell string
parsing entirely.

Alternatively, use single-quoted strings in PowerShell (no variable
interpolation, no escape processing), but this breaks if the body needs
dynamic content.

## Prevention

- **Always use `--body-file`** when creating GitHub PRs with Markdown content
  from PowerShell. Never inline Markdown in `--body "..."` on Windows.
- Keep a reusable `logs/pr-body.md` template file in the repo; overwrite it
  before each PR creation.
- The `logs/` directory is gitignored — PR body files there are transient and
  will not pollute history.
- When generating PR bodies programmatically, write to `logs/pr-body.md` as
  an intermediate step before calling `gh pr create`.
