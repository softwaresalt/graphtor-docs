---
type: session-memory
timestamp: 2026-05-02T08:30:00Z
agent: Ship
shipment: 011-S
branch: feat/019-020-acquisition-backends
pr: 18
outcome: merged
---

## Shipment 011-S — Session Summary

**Shipment**: 011-S — New Acquisition Backends (Web Crawler & DOCX)  
**Features**: 019-F (Web Crawler Acquisition), 020-F (DOCX Ingestion)  
**Branch**: `feat/019-020-acquisition-backends`  
**PR**: #18 (merged to main as `7747ccc`)

### What Was Built

| Component | Location | Description |
|---|---|---|
| BFS web crawler | `src/acquire/url.rs` | Crawls HTML, converts via htmd, respects robots.txt, content-change idempotency |
| DOCX parser | `src/parse/docx.rs` | docx-rs 0.4 based; paragraphs + table flattening; position-discriminated chunk IDs |
| URL source config | `src/config/source.rs` | `UrlSource` struct + `Source::Url` variant with serde |
| Config validation | `src/config/validation.rs` | URL scheme check, max_pages ≥ 1 |
| Acquisition planning | `src/acquire/plan.rs` | URL → CrawlUrl action with target_dir |
| Acquisition dispatch | `src/acquire/mod.rs` | `execute_crawl_url()` wired in |
| Pipeline dispatch | `src/pipeline/mod.rs` | `.docx` extension arm, `Source::Url` arm |
| Sync integration | `src/sync/mod.rs` | URL sources tracked via mtime |
| CLI integration | `src/main.rs` | `Source::Url` in incremental sync path |

### Decisions Made

- **htmd** over pulldown-cmark for HTML-to-Markdown (better semantic fidelity)
- **docx-rs** over docx-oxide / docx (maintained, well-structured element tree)
- **mold linker** in CI to avoid OOM-on-link for large debug binaries
- **Free disk space** step in CI to recover ~10 GB on ubuntu-latest

### CI Issues Encountered

1. **Linker OOM (Bus error / signal 7)**: Linking large debug binary OOMs ubuntu-latest
   - Fix: `mold` linker via `apt-get` + `.cargo/config.toml` rustflags (`-fuse-ld=mold`)
   - Commit: `46c19f0`

2. **Disk exhaustion (`No space left on device`)**: ~14 GB disk filled by Rust artifacts
   - Fix: Remove pre-installed tools (dotnet, Android SDK, GHC, CodeQL) in CI before build
   - Commit: `3716f3f`
   - Detection: Use `gh api check-runs/{id}/annotations` to get runner-level errors

### Copilot Review — 14 Comments

All addressed and resolved:
- 9 code fixes (chunk_id collision, same_domain prefix bug, mtime write skip,
  max_pages validation, char_offset O(n), etc.)
- 5 deferred to backlog (integration tests, sitemap support, etc.)
- All 14 threads replied to and resolved via `gh api graphql resolveReviewThread`

### Compound Learnings Written

- `docs/compound/build-errors/ci-disk-exhaustion-large-rust-deps-2026-05-02.md`
- `docs/compound/best-practices/docx-rs-table-api-irrefutable-patterns-2026-05-02.md`
- `docs/compound/best-practices/url-same-domain-strip-prefix-pattern-2026-05-02.md`

### Backlog Status

Tasks `019.002-T`, `019.003-T`, `019.004-T`, `020.002-T`, `020.003-T` → `done`  
Shipment `011-S` → `done`

### Next Steps

- Consider integration tests for URL crawler and DOCX parser (deferred from review)
- Consider sitemap.xml support for URL crawler (deferred from review)
- Cargo audit: may need to suppress new advisory IDs for reqwest/scraper transitive deps
  if they appear in future audits
