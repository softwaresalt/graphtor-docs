---
type: session-memory
date: 2026-05-08
shipment: 025-S
title: "Version bump 0.2.0 + installer support shipped"
status: shipped
prs_merged:
  - number: 43
    title: "chore(build): bump version 0.1.0 -> 0.2.0"
    sha: 382e0fd
  - number: 44
    title: "feat(build): add installer scripts and fix release workflow"
    sha: 6952e4d
tag: v0.2.0
---

## Summary

Shipped two PRs and tagged v0.2.0 — the first publicly-installable release of
graphtor-docs.

## Work Done

### PR #43 — Version bump 0.1.0 → 0.2.0

MINOR version bump (16 feature shipments since initial stamp, no breaking
changes). One Copilot review finding: truncated PR description. Fixed by
updating the description with full version rationale, replied to comment,
resolved thread.

### PR #44 — Installer scripts + release workflow

Added:
- `install.sh` — macOS/Linux one-liner with OS/arch detection, SHA-256
  verification, installs to `~/.local/bin/`
- `install.ps1` — Windows PowerShell with SHA-256 verification, installs to
  `%LOCALAPPDATA%\graphtor-docs\bin\`, idempotent PATH mutation
- `cliff.toml` — git-cliff conventional commits changelog config
- Rewrote `.github/workflows/release.yml`: correct binary names
  (`agent-intercom` → `graphtor-docs`), SHA-pinned actions,
  `contents:write` scoped to release job, SHA256SUMS generation,
  fixed tag glob (`v[0-9]*.[0-9]*.[0-9]*`)
- Fixed `Cargo.toml` repository URL (`graphtor` → `softwaresalt`)
- Updated `README.md` Install section

5 Copilot review findings, all fixed:
1. `need sha256sum || need shasum` — OR never fires since `need` exits on
   failure. Fixed: explicit `command -v` check setting `SHASUM_CMD`.
2. Linux aarch64 mapping with no published artifact. Fixed: removed with
   helpful error pointing to `cargo install`.
3. Empty grep match could defeat checksum verification. Fixed: capture
   `EXPECTED_LINE`, error if empty.
4. PowerShell multiple-match ambiguity in SHA256SUMS lookup. Fixed: `@()`
   array semantics + Count checks.
5. `on.push.tags` glob `v[0-9]+...` invalid (literal `+`). Fixed:
   `v[0-9]*.[0-9]*.[0-9]*`.

## Release

Tag `v0.2.0` pushed — release workflow running (4-platform cross build:
linux/x86_64, windows/x86_64, macos/arm64, macos/x86_64) + SHA256SUMS +
git-cliff changelog.

## Install commands (after release completes)

```sh
# macOS / Linux
curl -sSf https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.ps1 | iex

# cargo
cargo install --git https://github.com/softwaresalt/graphtor-docs --bin graphtor-docs --locked
```
