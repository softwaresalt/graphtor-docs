# Compound Learning: Normalize Path Separators Before Storing as Chunk Keys

**Category:** Cross-Platform / Data Pipeline Integrity  
**Discovered:** 2026-05-01  
**Context:** PR #13 — PDF document ingestion pipeline (007-S)

## Problem

`Path::strip_prefix(root).to_string_lossy()` on Windows produces backslash-separated paths
(e.g., `docs\\guide\\intro.md`). When this string is stored as `chunk_id_source` or
`Chunk.source_path`, it breaks:

1. **Chunk IDs become platform-dependent** — SHA-256 of `docs\\guide\\intro.md` ≠ SHA-256 of
   `docs/guide/intro.md`. The same file produces different IDs on Windows vs Linux, making the
   database non-portable.
2. **MCP `path_matches_source` fails on Windows** — the helper checks for `"{prefix}/"` with a
   forward slash. A backslash path never matches, breaking all MCP search and retrieval tools
   for locally-ingested content.

## Solution

After stripping the source root prefix from an absolute path, immediately normalize to forward
slashes:

```rust
let rel = file
    .strip_prefix(&source_root)
    .map_err(|_| PipelineError::PathViolation)?;
let path_str = rel.to_string_lossy().replace('\\', "/");
```

This one-liner is safe on all platforms:
- On Linux/macOS: no-op (no backslashes present)
- On Windows: converts `docs\\guide\\intro.md` → `docs/guide/intro.md`

## Why Not `Path::display()`

`Path::display()` also calls `to_string_lossy()` internally and does **not** normalize separators
on Windows. It is equally unsafe for use as a stable key.

## Where to Apply

Apply this normalization at every point where a `Path` or `PathBuf` becomes a stored string key:

| Location | Variable |
|---|---|
| `src/pipeline/mod.rs` — before `parse_document` / `parse_pdf_document` | `path_str` |
| Any future pipeline stage that builds `source_path` from filesystem paths | same pattern |

## Evidence

- PR #13 commit `b6ca29b`: `fix(pipeline): address copilot review — path normalization and chunk ID uniqueness`
- `src/pipeline/mod.rs` line ~310 — `.replace('\\', "/")`
- Copilot review comment (resolved): `PRRC_kwDORiB5E869FpZd`
