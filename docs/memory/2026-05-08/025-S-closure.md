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
release_run: "25577828629"
release_outcome: success
---

## Summary

Shipped two PRs, tagged v0.2.0, and after 5 workflow iterations successfully
published a 4-platform GitHub Release with binaries and SHA256SUMS.

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

### Release Workflow — 5 Iterations to Green

The release workflow required 5 fix cycles after the tag was first pushed:

**Iteration 1** — `dtolnay/rust-toolchain` missing `toolchain` input.
When pinned by SHA (not `@stable` tag alias), the action requires explicit
`toolchain: stable`. Added to both `test` and `build` jobs.

**Iteration 2** — mold linker not installed on runners.
`.cargo/config.toml` sets `-fuse-ld=mold` for `x86_64-unknown-linux-gnu`;
GitHub runners don't pre-install mold. Added `sudo apt-get install -y mold`
to `test` job and conditional install in `build` matrix for Linux.

**Iteration 3** — `openssl-sys` couldn't find x86_64 OpenSSL on ARM64 runner.
The `x86_64-apple-darwin` matrix entry used `macos-latest` (ARM64); changed
to `macos-13` (Intel) to avoid cross-compilation.

**Iteration 4** — `macos-13` runners gone/unavailable (jobs stuck "queued"
indefinitely). Switched back to `macos-latest` and added `OPENSSL_VENDORED=1`
env var. This did NOT work — `openssl-sys` ignores that env var entirely.

**Iteration 5** — Correct fix: `OPENSSL_VENDORED` is a Cargo **feature**, not
an env var. Used a target-specific dep in `Cargo.toml`:
```toml
[target.'cfg(all(target_os = "macos", target_arch = "x86_64"))'.dependencies]
openssl = { version = "0.10", features = ["vendored"] }
```
This activates vendored OpenSSL (compiled from source via Perl, which macOS
ships) only for `x86_64-apple-darwin`, leaving Linux and Windows builds
unaffected. All 4 platform builds succeeded.

## Final Release Artifacts

- `graphtor-docs-v0.2.0-x86_64-unknown-linux-gnu.tar.gz`
- `graphtor-docs-v0.2.0-aarch64-apple-darwin.tar.gz`
- `graphtor-docs-v0.2.0-x86_64-apple-darwin.tar.gz`
- `graphtor-docs-v0.2.0-x86_64-pc-windows-msvc.zip`
- `SHA256SUMS`

Published at: https://github.com/softwaresalt/graphtor-docs/releases/tag/v0.2.0

## Compound Learnings Written

- `docs/compound/best-practices/github-actions-tag-glob-pattern-2026-05-08.md`
- `docs/compound/best-practices/installer-checksum-empty-grep-pattern-2026-05-08.md`
- `docs/compound/best-practices/openssl-sys-vendored-cross-compile-macos-2026-05-08.md`

## Install Commands

```sh
# macOS / Linux
curl -sSf https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/softwaresalt/graphtor-docs/main/install.ps1 | iex

# cargo
cargo install --git https://github.com/softwaresalt/graphtor-docs --bin graphtor-docs --locked
```


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
