---
title: Ship 016-S — Comprehensive graphtor-docs Documentation
date: 2026-05-05
shipment: 016-S
feature: 026-F
pr: 27
branch: feat/comprehensive-docs-026-f
status: awaiting-merge
---

## Summary

Shipped feature 026-F (comprehensive graphtor-docs documentation) as shipment
016-S via PR #27. Three rounds of Copilot review were completed and all
comments addressed.

## Commits

| SHA | Description |
|---|---|
| `8abe141` | Initial 8-file documentation set |
| `1fced72` | Round-1 fixes: 29 Copilot comments addressed |
| `a42056a` | Round-2 fixes: 25 Copilot comments addressed |
| `85970b8` | Round-3 fixes: 8 Copilot comments addressed |

## Files Authored

- `docs/architecture.md`
- `docs/cli-reference/graphtor-docs.md`
- `docs/developer-guide.md`
- `docs/incremental-sync.md`
- `docs/mcp-tools.md`
- `docs/pipeline.md`
- `docs/quickstart.md`
- `docs/troubleshooting.md`

## Review Rounds

### Round 1 (29 comments on 8abe141)
All addressed in `1fced72`. CI passed.

### Round 2 (25 comments on 1fced72)
8 false-positive `||` table comments declined (tables already single-pipe).
Real fixes in `a42056a`: fenced code block language tags, heading chunking,
search_semantic empty-vec behavior, model cache path, concurrent sync lock
behavior, sync_state.json naming, PDFium env-var OS split, security fix
(token-in-URL removed).

### Round 3 (8 comments on a42056a)
All 8 were real accuracy issues. Fixed in `85970b8`:
- `--verbose`: sets internal tracing filter, not RUST_LOG
- `--no-embed`: search_semantic returns empty results (not unavailable)
- `status`/`list_sources`: synced_at not populated; examples show null/never
- incremental-sync: only .md files tracked; non-Markdown requires --full
- URL sources: BFS crawl runs each sync, mtime-based re-ingest of changed pages
- Re-ingestion: doc_vectors NOT deleted or updated during incremental sync
- `synced_at` always null in current pipeline (reserved for future)

## Key Source-Code Facts

- `src/sync/git_diff.rs:197-198` — only `.md` files in `compute_git_diff`
- `src/sync/mtime_diff.rs:113-116` — only `.md` files in `scan_mtimes`
- `src/acquire/url.rs:105` — URL crawler skips write when content unchanged
- `src/sync/mod.rs:111-116` — URL sources use same mtime_diff as local
- `src/sync/reingest.rs` — `delete_file_data` omits `doc_vectors`; embeddings
  computed but not persisted via `upsert_vector`
- `src/pipeline/mod.rs:224,558,568,578` — `synced_at: None` everywhere
- `src/sync/mod.rs:93` — `synced_at: None` in sync path

## Status

CI ✅ green on 85970b8 | All review threads resolved | Awaiting operator merge

## Next Steps

After operator merges PR #27:
1. `backlogit_ship_shipment 016-S`
2. Verify all 8 tasks (026.001-T through 026.008-T) are done
