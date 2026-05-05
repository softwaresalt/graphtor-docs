---
title: "Python subprocess on Windows: encoding for gh api output with Unicode box-drawing"
date: 2026-05-05
tags: [python, windows, subprocess, encoding]
---

## Problem

When calling `gh api graphql` via Python `subprocess.run` on Windows, the
output may contain Unicode box-drawing characters (e.g., in `diff_hunk`
fields). The default `cp1252` encoding raises `UnicodeDecodeError`.

## Solution

Always specify `encoding='utf-8', errors='replace'` in subprocess calls that
invoke `gh api`:

```python
result = subprocess.run(
    ["gh", "api", "graphql", "-f", f"query={MUTATION}", "-f", f"threadId={tid}"],
    capture_output=True,
    encoding="utf-8",
    errors="replace",
)
```

`errors="replace"` substitutes replacement characters for any bytes that
cannot be decoded as UTF-8 — sufficient for JSON payloads where the
`diff_hunk` field may contain Unicode.

## Key Facts

- Windows default encoding is `cp1252`; GitHub API responses use UTF-8.
- `diff_hunk` fields frequently contain `→`, `—`, and other non-ASCII chars.
- `errors="replace"` is safe for JSON parsing because replacement chars only
  appear in string values, not in JSON structural characters.
