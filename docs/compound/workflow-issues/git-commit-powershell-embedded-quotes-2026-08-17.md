---
title: "git commit -m with embedded double-quoted text fails in PowerShell — use -F with a file"
description: "A commit message body containing embedded double-quoted phrases (e.g. quoting a code identifier or config value) breaks PowerShell's argument parsing for git commit -m, misinterpreting the quoted phrase as ending the argument"
problem_type: "shell_escaping"
category: "workflow-issues"
component: "git commit workflow (PowerShell)"
root_cause: "PowerShell's double-quoted string parsing does not treat an embedded, unescaped double-quote inside the message text as literal content; git commit -m \"...text with \"quoted phrase\"...\" is parsed by PowerShell as multiple separate tokens, and git then reports the trailing token as an unrecognized pathspec"
resolution_type: "workaround"
severity: "low"
message: "error: pathspec '...' did not match any file(s) known to git"
file_path: "N/A — PowerShell invocation pattern, not a repository file"
citations:
  - "PR #101 (shipment 048-S) — encountered when the commit message quoted a Rust `target: \"...\"` string literal"
  - "docs/compound/workflow-issues/gh-pr-body-powershell-backtick-conflict-2026-04-29.md (same class of PowerShell quoting conflict, different special character)"
tags:
  - "powershell"
  - "git"
  - "commit-message"
  - "escaping"
---

## Problem

A commit message body quoted a Rust string literal for clarity, e.g.:

```text
add explicit target: "graphtor_core::acquire::filter" override to
stream_ingestible's aggregate warning
```

Passed via `git commit -m "..."` from PowerShell, this failed:

```text
error: pathspec 'graphtor_core::acquire::filter\ override to
  stream_ingestible''s aggregate warning ...' did not match any file(s) known to git
```

## Root Cause

PowerShell's double-quoted string literals do not automatically escape an embedded, unescaped
`"` character as literal text — when a `-m "..."` argument's body contains its own `"..."`
sequence, PowerShell can end the outer string early at that inner quote, and the remaining text
is re-split into new, separate command-line tokens. `git commit` then receives extra positional
arguments after `-m`'s (now truncated) value and interprets them as pathspecs, which fail to
match any tracked file. This is the same underlying class of problem as
`docs/compound/workflow-issues/gh-pr-body-powershell-backtick-conflict-2026-04-29.md` (PowerShell
special-character handling conflicting with a CLI tool's own multi-line/quoted string argument),
but triggered by a literal double-quote rather than a backtick, and affecting `git commit -m`
rather than `gh pr create --body`.

## Resolution

Write the commit message to a file and use `git commit -F <file>` instead of `-m "..."`:

```powershell
# Write message to a temp file (create tool or Set-Content)
Set-Content -Path docs\scratch-commit-msg.txt -Value $commitMessage

# Commit using the file — bypasses PowerShell string parsing entirely
git commit -F docs\scratch-commit-msg.txt

# Clean up the scratch file afterward
Remove-Item -Force docs\scratch-commit-msg.txt
```

`-F` reads the file's raw bytes directly; PowerShell never re-tokenizes the message content.

## Prevention

* **Any commit message that quotes an identifier, string literal, path, or config value in double
  quotes should use `git commit -F <file>`**, not `-m "..."`, when invoked from PowerShell.
* This generalizes the existing `gh pr create --body-file` guidance
  (`gh-pr-body-powershell-backtick-conflict-2026-04-29.md`) to `git commit` — the same
  file-based-argument workaround applies to any CLI tool argument carrying multi-line or
  quote-containing text from PowerShell.
* When in doubt, prefer writing ANY non-trivial, multi-line CLI argument (commit messages, PR
  bodies, review replies) to a scratch file first, rather than inlining it in the shell command.
