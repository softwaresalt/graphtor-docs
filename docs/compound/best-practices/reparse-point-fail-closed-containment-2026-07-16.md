---
title: "Reject a linked workspace root before write/mutate: the is_reparse_point guard"
description: "A managed workspace root that is itself a symlink or junction is its own trust anchor, so canonicalisation-based containment accepts every descendant; reject the linked root up front instead"
problem_type: "security_containment"
category: "best-practices"
component: "src/path/security.rs"
root_cause: "validate_path canonicalises against the workspace root, so a root that IS a symlink/junction passes every descendant check and lets writes escape containment"
resolution_type: "guard-primitive"
severity: "high"
message: "N/A (design-level containment guard, not a runtime error string)"
file_path: "src/path/security.rs"
date: 2026-07-16
citations:
  - "src/path/security.rs:158-187"
  - "src/workspace/serve_discovery.rs:109"
  - "src/main.rs:3215"
  - "src/main.rs:3556"
  - "https://github.com/softwaresalt/graphtor-docs/pull/90"
tags: [security, filesystem, containment, symlink, junction, windows, uninstall]
---

## Problem

`validate_path` enforces containment by canonicalising a candidate and checking
it stays under the workspace root. But if the **root itself** is a symlink or a
Windows junction, it is its own trust anchor: `validate_path(child, linked_root)`
accepts every descendant, and `create_dir_all` / lock acquisition / scaffold
copy then write *through* the link into an external target — bypassing
workspace containment (Constitution Principles III/IV).

## Solution

Add one companion primitive that inspects the link entry directly and reject a
linked root **before** any read, mutation, or deletion:

```rust
// src/path/security.rs
#[must_use]
pub fn is_reparse_point(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}
```

Callers bail (or skip) when it returns `true`:

```rust
// src/main.rs — install and uninstall workspace-root guards
if graphtor_core::path::is_reparse_point(&ws_dir) {
    anyhow::bail!(".graphtor is a symlink or junction; refusing to \
                   install through a linked workspace root");
}
```

Verified call sites in shipment 045-S:

- **serve scan-root** — `src/workspace/serve_discovery.rs:109` rejects a
  reparse-point scan root before discovery walks it.
- **install workspace root** — `src/main.rs:3215` rejects a linked `.graphtor`
  before any scaffold/binary write.
- **uninstall workspace root** — `src/main.rs:3556` rejects a linked `.graphtor`
  before lock acquisition and the prune plan.

## Key Facts

- Use `symlink_metadata` (not `metadata`) so the **link itself** is inspected;
  `metadata` follows the link and hides the reparse point.
- The primitive's own missing/unreadable case returns `false` **by design**
  (`is_ok_and` → `Err` maps to `false`): a path that does not exist is "nothing
  to guard" (a fresh install then creates a real directory), and a real non-link
  entry is safe. Containment does not depend on this probe failing closed —
  it depends on rejecting the link **when the probe returns `true`**.
- Do not conflate this with fail-closed-on-error scanning. A **separate**
  primitive, `source_has_ingestible_content`
  (`src/workspace/serve_discovery.rs:333-342`), propagates `WalkDir` errors and
  returns `false` so an unreadable subtree keeps the discovered target
  `ReadOnly` — that boundary *does* fail closed on I/O error, because there the
  safe default is "do not treat as ingestible."
- MCP config generation (`generate_mcp_config`) does **not** call
  `is_reparse_point`; it relies on `validate_path` canonicalisation, and the
  upstream workspace-root guard already rejects a linked `.graphtor` before any
  config write. Guard at the root, not at every leaf.
- Windows junctions and directory symlinks both surface as
  `file_type().is_symlink()` via `symlink_metadata` on the reparse point.

## Evidence

Shipment 045-S, PR #90 review waves 5–10 (2026-07-16). Copilot surfaced
symlink/junction containment gaps around the managed `.graphtor` root. Fixed by
adding `is_reparse_point` and rejecting a linked root at the serve, install, and
uninstall boundaries; wave 10 additionally hardened `source_has_ingestible_content`
to fail closed on `WalkDir` errors. Two independent adversarial reviewers
(correctness + security) confirmed no residual P0/P1.
