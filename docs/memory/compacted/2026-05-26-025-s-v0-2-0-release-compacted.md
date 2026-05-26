---
type: compacted-memory
agent: ship
timestamp: 2026-05-26T00:00:00Z
title: "Compacted Memory: 025-S v0.2.0 release shipment"
date: 2026-05-26
shipment: 025-S
compacted_from:
  - docs/memory/2026-05-08/025-S-closure.md
---

## Summary

This compacted memory preserves the shipped `v0.2.0` release record for
shipment `025-S`. The detailed original captured the version bump, installer
delivery, release-workflow repair loop, and final multi-platform release
publication.

## Consolidated outcomes

* Shipped PR `#43` for the `0.1.0 -> 0.2.0` version bump and PR `#44` for
  installer scripts plus release-workflow hardening
* Tagged `v0.2.0` and published the first publicly installable graphtor-docs
  release with Linux, macOS ARM64, macOS x86_64, and Windows artifacts plus
  `SHA256SUMS`
* Added `install.sh`, `install.ps1`, `cliff.toml`, release-workflow fixes, the
  corrected repository URL in `Cargo.toml`, and updated install guidance in
  `README.md`
* Closed all Copilot findings on installer checksum validation, unsupported
  Linux ARM mapping, PowerShell checksum disambiguation, and the tag-trigger
  glob
* Repaired the release workflow in five iterations, ending with the correct
  target-specific vendored OpenSSL dependency for `x86_64-apple-darwin`

## Key files and surfaces

* `install.sh`
* `install.ps1`
* `.github/workflows/release.yml`
* `cliff.toml`
* `Cargo.toml`
* `README.md`

## Decisions and learnings

* Use explicit checksum-line capture and fail closed when a release artifact
  lookup returns no match
* Treat `OPENSSL_VENDORED` as a Cargo feature decision, not an environment
  variable toggle
* Keep the release tag glob on the GitHub Actions trigger in the compatible
  `v[0-9]*.[0-9]*.[0-9]*` form

## Archived originals

The original detailed memory file was moved to `docs/archive/memory/2026-05-08/`.
