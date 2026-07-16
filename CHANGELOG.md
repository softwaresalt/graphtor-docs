# Changelog

All notable changes to graphtor-docs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2026-07-16

Security patch release. Extends the PR #90 workspace-containment hardening to the
two remaining reparse-point gaps on the `upgrade` and `install` write paths.

### Security

- `upgrade` now rejects a symlinked or junction `.graphtor` workspace root before
  acquiring the workspace lock, matching the existing `install` and `uninstall`
  guards. Previously a linked root let `--force-unlock` create, remove, or replace
  `graphtor.lock` inside an external target — an out-of-workspace mutation.
- `upgrade` also rejects a symlinked or junction `bin/` directory or binary
  destination before copying the running binary, so a linked destination can no
  longer redirect the copy onto an external target.
- The `install` write path now refuses a symlinked or junction `.gitignore` or
  `.mcp.json`. `add_gitignore_entry` and `generate_mcp_config` fail closed rather
  than read or write through a pre-planted linked file into an external target,
  closing the last follow-a-symlink gaps on the write side.

### Known issues

- `RUSTSEC-2026-0041` (lz4_flex 0.10.0 — uninitialized memory on invalid
  decompression input) remains open and is suppressed through the documented
  `cargo audit` allowlist. It is reached only transitively as
  cozo → swapvec → lz4_flex (semver-locked) and serves cozo's internal disk-swap
  buffers, never user-supplied input. The upstream fix is blocked until cozo
  releases swapvec 0.4+. See `audit.toml` for the full rationale.

## [0.3.0] - 2026-07-10

This release consolidates the work merged since `v0.2.0` (2026-05-08), including
the docline ingestion pivot to schema v4 and the initial CLI query surface.

### Added

- CLI query subcommands that mirror the MCP tools: `search`, `search-semantic`,
  `research`, `traverse`, `list-sources`, `get-chunk`, and `get-document` (#82).
- `prewarm` subcommand that pre-warms every configured source with progress
  reporting and JSONL telemetry (#53).
- Local embedding-model directory support via the `GRAPHTOR_EMBED_MODEL_DIR`
  environment variable, with `.env.local` as the documented convention (#80).
- Multi-database file support: each source can target a per-source `database:`
  file, with hardened multi-database runtime access (#55, #58).
- Cross-source link resolution through the `canonical_url` document index (#74).
- Auto-generated `sources.yaml` stub for databases imported without a registry
  (#65).
- Single workspace-root `.mcp.json` written on install (#72).
- Sync telemetry and operator-facing progress reporting (#51).
- User-friendly console error output: the full error cause chain is surfaced
  with actionable hints and TTY colour (auto-detected; `NO_COLOR` honoured), and
  `--json` errors carry a structured `data` payload (`category`, `cause_chain`,
  `hint`).

### Changed

- Pivoted ingestion to docline-standardized Markdown; the store advances to
  schema v4 (#69, #70).
- Normalized the source registry and added a duplicate-intake preflight (#60).
- Hardened release sync with embedding-availability diagnostics and clearer
  degraded-mode reporting (#67).
- CI skips the Rust pipeline for documentation-only changes and runs the
  change-gate through PowerShell (#75, #77).
- Suppressed known unmaintained-crate advisories through a documented
  `cargo audit` allowlist gate (#71).

### Removed

- Dropped the HNSW vector index in favour of exact brute-force cosine search,
  removing per-insert index maintenance while keeping 100% recall (#83).
- Removed the non-functional `Editor::Copilot` MCP configuration path (#49).

### Performance

- Batched the ingestion load-stage upserts, cutting a representative sync from
  roughly 33 minutes to 4.2 minutes (#79).

### Migration notes

- A database synced by 0.2.x carries the pre-v4 schema. On first use with 0.3.0,
  read commands (`status`, `search`, `search-semantic`) report a pre-v4 error.
  Run `graphtor-docs sync --full` with the embedding model available
  (`GRAPHTOR_EMBED_MODEL_DIR` set to the local model directory) to rebuild the
  index and generate embeddings.

## [0.2.0] - 2026-05-08

- Initial tagged release.
