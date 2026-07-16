---
title: "Reject symlinks and junctions fail-closed at every workspace containment boundary"
date: 2026-07-16
tags: [security, filesystem, containment, symlink, junction, windows, uninstall]
---

## Problem

A workspace-scoped tool that enumerates paths for destructive work (uninstall
removal) or that derives a scan root (read-only serve auto-discovery) can be
tricked into escaping its containment boundary when a directory entry is a
symlink, a Windows junction, or another reparse point. Following the link lets
an operation delete, scan, or write outside the intended workspace root — a
classic path-containment escape. Checking only string prefixes or
`Path::canonicalize` results is not enough, because the link target may resolve
inside the root at check time yet be repointed later, and canonicalize has
different semantics across platforms.

## Solution

Add one shared primitive that classifies a path as a reparse point without
following it, and call it fail-closed at every boundary:

```rust
/// Returns true when `path` is a symlink, junction, or other reparse point.
/// Uses symlink_metadata so the link itself is inspected, not its target.
fn is_reparse_point(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_symlink(),
        // Fail closed: an unreadable entry is treated as suspect.
        Err(_) => true,
    }
}
```

Enforce it at each containment boundary the shipment exposes:

- **serve scan-root**: reject a reparse-point scan root before discovery walks it.
- **uninstall linked-root skip**: skip a linked workspace root in both the
  mutation-enumeration pass and the empty-root removal / root-removal prediction,
  so a junctioned root is never removed.
- **mcp candidate validation**: validate the config candidate path is not a
  reparse point before writing.
- **gitignore marker handling**: reject a reparse-point gitignore path rather
  than editing through the link.

## Key Facts

- Use `symlink_metadata` (not `metadata`) so the **link itself** is inspected;
  `metadata` follows the link and hides the reparse point.
- Fail closed: on any `symlink_metadata` error (permission, race, corruption),
  treat the entry as a reparse point and refuse the operation. An unreadable
  subtree must not silently widen a destructive or discovery scope.
- One shared primitive beats scattered ad-hoc checks: every new boundary that
  enumerates or removes paths must route through it, or the escape reopens.
- Windows junctions and directory symlinks both surface as
  `file_type().is_symlink()` via `symlink_metadata` on the reparse point.

## Evidence

Shipment 045-S, PR #90 review waves 5–10 (2026-07-16). Copilot surfaced four
symlink/junction containment findings (serve scan-root, uninstall linked-root,
mcp candidate, gitignore). Fixed as one bounded fail-closed cluster on a shared
`is_reparse_point` primitive (`2df9af9`); wave 10 hardened the discovery walk to
also fail closed on `WalkDir` errors (`d858bc2`). Two independent adversarial
reviewers (correctness + security) confirmed no residual P0/P1.
