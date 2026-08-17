---
title: "git commit -m with embedded double-quoted text fails in PowerShell — use -F with a file"
description: "A commit message body containing embedded double-quoted phrases (e.g. quoting a code identifier or config value) breaks PowerShell's argument parsing for git commit -m, misinterpreting the quoted phrase as ending the argument"
problem_type: "shell_escaping"
category: "workflow-issues"
component: "git commit workflow (PowerShell)"
root_cause: "PowerShell's double-quoted string parsing ends the current string segment at an embedded, unescaped `\"` even when immediately adjacent to prior text with no whitespace; the following text becomes a SEPARATE argument rather than merging back into the original one (verified via isolated reproduction, not merely inferred) — git commit -m \"...text with \"quoted phrase\"...\" is split into multiple tokens, and git then reports the trailing token as an unrecognized pathspec"
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

## Verified Reproduction

Isolated, minimal repro (no `git` involved, just PowerShell's own argument tokenization) —
run this to see the exact split independently of any git behavior:

```powershell
function Show-Args { $args | ForEach-Object { "ARG: [$_]" } }
Show-Args "line1
line2 target: "bareword" more text
line3"
```

Output:

```text
ARG: [line1
line2 target: ]
ARG: [bareword more text
line3]
```

One intended argument becomes **two** separate arguments, split exactly at the embedded,
unescaped `"` before `bareword`. When `git commit -m` receives this shape, argument 1 becomes the
`-m` value and argument 2 is passed as an extra positional argument, which `git` interprets as a
pathspec.

## Root Cause

Confirmed by the reproduction above: when a PowerShell double-quoted string argument contains an
embedded, unescaped `"`, the parser ends that string segment at the embedded quote. The text after
it — up through the next quote/token boundary — becomes a **separate** argument rather than being
merged back into the first one, even though the two segments are directly adjacent with no
intervening whitespace. (This differs from PowerShell's better-known "adjacent quoted/bareword
segments concatenate into one token" behavior for short inline expressions like `"a"b"c"` on a
single line — the exact conditions under which the two segments split vs. concatenate were not
fully characterized here; the practical takeaway is that a multi-line commit message shaped like
the reproduction above **reliably splits**, and that is what matters for prevention.) `git commit`
then receives extra positional arguments after `-m`'s (now truncated) value and interprets them as
pathspecs, which fail to match any tracked file. This is the same underlying class of problem as
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
