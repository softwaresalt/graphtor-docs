---
title: "Web crawler acquisition source crate selection and architecture"
type: spike
date: 2026-05-02
time_box: "2h"
conclusion: "proceed"
confidence: "high"
linked_parent_work_item: "019-F"
promoted_to: ["plan"]
tags:
  - "web-crawler"
  - "acquisition-backend"
  - "crate-evaluation"
---

## Goal

Which Rust crates should graphtor-docs use for web crawling (HTTP fetch +
HTML-to-Markdown conversion), and how should a `type: url` acquisition source
integrate into the existing pipeline architecture?

## Success Criteria

- Evaluate candidate HTTP client and HTML-to-Markdown crates for API simplicity,
  dependency weight, async compatibility, and Rust version compatibility
- Determine the architecture integration pattern (new acquisition backend
  alongside `git` and `local`)
- Produce a recommendation with enough detail to feed `impl-plan`

## Scope Constraints

- Read-only investigation — no code changes or prototype
- Must be compatible with `rust-version = "1.75"` (current project minimum)
- Must respect the single-binary, local-first architecture principles
- Must NOT require external services or cloud APIs
- Crawler must be rate-limited and robots.txt compliant

## Investigation Approach

1. Audit the existing acquisition pipeline to identify extension points
2. Evaluate HTTP client crates (reqwest is already transitive via hf-hub)
3. Evaluate HTML-to-Markdown conversion crates
4. Design the `type: url` source configuration schema
5. Design the integration pattern into the existing pipeline
6. Assess compliance requirements (robots.txt, rate limiting)

## Findings

### Existing Acquisition Pipeline Extension Points

The current pipeline has a clean three-variant dispatch pattern:

```text
Source enum (tag = "type")
  ├─ Git(GitSource)    → clone_git_source() → scan → filter → FilteredFileSet
  ├─ Local(LocalSource) → scan_local_source() → filter → FilteredFileSet
  └─ Url(UrlSource)     → [NEW] crawl_url_source() → filter → FilteredFileSet
```

Key observations:
- `Source` enum in `src/config/source.rs` uses `#[serde(tag = "type")]` — adding
  a new `Url` variant is a one-line change plus the struct definition
- `SourceAction` enum in `src/acquire/result.rs` needs a `CrawlUrl` variant
- `dispatch_planned_source()` in `src/acquire/mod.rs` needs a new match arm
- The `FilteredFileSet` output type is format-agnostic — works for any source
- The `plan()` function needs URL-specific planning logic (no local dir check)

### HTTP Client Evaluation

| Crate | Version | Async | Rust Version | Weight | Notes |
|-------|---------|-------|-------------|--------|-------|
| `reqwest` | 0.12 | ✅ tokio | 1.63+ ✅ | ~15 MB compiled | Already a transitive dep via `hf-hub`. Mature, well-maintained, TLS built-in |
| `ureq` | 3.0 | ❌ sync | 1.67+ ✅ | ~3 MB | Pure Rust, no async. Simpler but blocks the thread |
| `hyper` | 1.5 | ✅ tokio | 1.63+ ✅ | ~8 MB | Low-level, requires manual connection management |

**Recommendation: `reqwest`**
- Already a transitive dependency (zero incremental cost to add as direct dep)
- Async with tokio (already in our dep tree)
- Mature redirect following, cookie support, configurable timeouts
- `reqwest::Client` supports connection pooling and concurrent requests

### HTML-to-Markdown Conversion Evaluation

| Crate | Version | Rust Version | Weight | Quality | Notes |
|-------|---------|-------------|--------|---------|-------|
| `htmd` | 0.5 | 1.70+ ✅ | 47 KB | ✅ Good | HTML-to-Markdown converter using `markup5ever`. Clean API: `htmd::convert(html)` |
| `html2md` | 0.2 | 1.56+ ✅ | 30 KB | ⚠️ Fair | Older, less maintained. Basic conversion |
| `scraper` + manual | 0.22 | 1.65+ ✅ | 180 KB | ✅ Flexible | CSS selector-based HTML parser. Not a converter — need manual MD generation |

**Recommendation: `htmd`**
- Clean single-function API: `htmd::convert(html_str)` → `Result<String>`
- Handles headings, links, lists, tables, code blocks, images
- Active maintenance, reasonable size
- Pairs well with `scraper` for pre-processing (extract main content area
  before conversion to avoid nav/footer noise)

### Complementary Crate: `scraper`

Even with `htmd`, we need `scraper` (CSS selector HTML parser) to:
- Extract the main content area (skip nav, sidebar, footer)
- Follow links for recursive crawling (extract `<a href="...">` elements)
- Extract page title from `<title>` or `<h1>`

`scraper` is already widely used, 180 KB, Rust 1.65+.

### robots.txt Compliance

| Crate | Version | Notes |
|-------|---------|-------|
| `robotstxt` | 0.3 | Google's robots.txt parser ported to Rust. 15 KB |
| `texting_robots` | 0.2 | Pure Rust, well-tested, caches parsed rules |

**Recommendation: `texting_robots`**
- Pure Rust, no C dependencies
- Caches parsed robots.txt for efficient repeated checks
- API: `Robot::new(user_agent, robots_txt_content)` → `robot.allowed(url)`

### Configuration Schema Design

```yaml
sources:
  - type: url
    id: azure-cosmos-db
    url: https://learn.microsoft.com/azure/cosmos-db/
    max_depth: 2
    max_pages: 100
    domain_lock: true        # only follow links within same domain
    rate_limit_ms: 500       # milliseconds between requests
    user_agent: "graphtor-docs/0.1"
    include:
      - "**/*.md"            # applied to converted markdown filenames
    exclude:
      - "**/changelog/**"
```

New fields specific to `UrlSource`:
- `url`: Seed URL (required)
- `max_depth`: Maximum link-follow depth from seed (default: 2)
- `max_pages`: Maximum total pages to crawl (default: 100, safety limit)
- `domain_lock`: Only follow links within the seed URL's domain (default: true)
- `rate_limit_ms`: Delay between requests in milliseconds (default: 500)
- `user_agent`: Custom User-Agent string (default: "graphtor-docs/{version}")

### Architecture Design

#### Crawl Pipeline

```text
UrlSource
  ├─ Fetch robots.txt → parse rules
  ├─ Seed URL → fetch HTML → extract links
  ├─ For each link (BFS, up to max_depth):
  │   ├─ Check robots.txt compliance
  │   ├─ Check domain_lock
  │   ├─ Check max_pages limit
  │   ├─ Rate limit delay
  │   ├─ Fetch HTML
  │   ├─ Extract main content (scraper CSS selectors)
  │   ├─ Convert to Markdown (htmd)
  │   └─ Write to temp dir as .md file
  └─ Return FilteredFileSet pointing to temp dir
```

#### Module Layout

| File | Purpose |
|------|---------|
| `src/acquire/url.rs` | New module: URL crawling, link extraction, robots.txt |
| `src/config/source.rs` | Add `Url(UrlSource)` variant to `Source` enum |
| `src/acquire/result.rs` | Add `CrawlUrl` to `SourceAction` enum |
| `src/acquire/mod.rs` | Add dispatch arm and `execute_crawl_url()` |
| `src/acquire/plan.rs` | Add URL source planning (temp dir creation) |
| `Cargo.toml` | Add `reqwest`, `htmd`, `scraper`, `texting_robots` |

#### Key Design Decisions

1. **Write to temp dir**: Crawled pages are converted to Markdown and written
   to a temp directory under `data_root`. This reuses the existing
   `scan_and_filter` + `FilteredFileSet` pipeline without changes.

2. **Sync acquisition**: Although `reqwest` is async, the acquisition pipeline
   is currently sync. Use `tokio::runtime::Handle::current().block_on()` or
   create a small sync wrapper. Alternatively, make `crawl_url_source()` async
   and add `#[tokio::main]` to the acquisition executor. The latter is cleaner
   but requires propagating async through `execute()`.

3. **Domain locking**: Essential for safety. Without it, a crawler starting at
   `learn.microsoft.com` could follow external links indefinitely.

4. **Deduplication**: Track visited URLs in a `HashSet<String>` during the
   crawl. Normalize URLs (remove fragments, trailing slashes) before comparison.

5. **Path mapping**: Convert URL paths to filesystem paths for the temp dir:
   `https://learn.microsoft.com/azure/cosmos-db/introduction` →
   `{temp_dir}/azure/cosmos-db/introduction.md`

### Pipeline Changes Required

| File | Change | Scope |
|------|--------|-------|
| `src/config/source.rs` | Add `Url(UrlSource)` variant and `UrlSource` struct | ~30 lines |
| `src/config/validation.rs` | Add URL-specific validation (valid URL, positive limits) | ~20 lines |
| `src/acquire/url.rs` | New module: crawl engine, link extraction, robots.txt | ~250 lines |
| `src/acquire/result.rs` | Add `CrawlUrl` to `SourceAction` | 3 lines |
| `src/acquire/mod.rs` | Add `execute_crawl_url()` dispatch and handler | ~30 lines |
| `src/acquire/plan.rs` | Add URL source planning (temp dir, action) | ~15 lines |
| `Cargo.toml` | Add reqwest, htmd, scraper, texting_robots | 4 lines |
| `tests/acquire_url_test.rs` | Integration tests with mock HTTP server | New file |

### Dependency Impact Assessment

| Crate | Size | New? | Justification |
|-------|------|------|---------------|
| `reqwest` | 15 MB | Transitive→direct | Already in dep tree via hf-hub. HTTP client for web crawling |
| `htmd` | 47 KB | Yes | HTML-to-Markdown conversion. No alternative in pure Rust |
| `scraper` | 180 KB | Yes | CSS selector HTML parsing for content extraction |
| `texting_robots` | ~15 KB | Yes | robots.txt compliance. Legal/ethical requirement |

Total new dependency weight: ~242 KB (reqwest already present). Acceptable
under the Lightweight Footprint principle given the feature scope.

### What Was Tried and Failed

N/A — this was a research spike, not a prototype.

### Remaining Unknowns

1. **Content extraction quality on learn.microsoft.com**: The site uses React
   SSR with complex layouts. Need to verify that `scraper` + `htmd` produce
   clean Markdown from the rendered HTML. May need site-specific CSS selectors
   (e.g., `article.content`, `main`).

2. **JavaScript-rendered content**: Some doc sites require JS execution for
   full content. `reqwest` fetches raw HTML only. If critical content is
   JS-rendered, a headless browser would be needed (out of scope for v1).

3. **Async propagation**: The current acquisition pipeline is sync. Adding
   async crawling either requires a sync wrapper (simpler) or async propagation
   through `execute()` (cleaner but larger change). Decision deferred to
   impl-plan.

4. **Incremental sync for URL sources**: Git sources use commit hashes for
   change detection. URL sources would need HTTP `ETag`/`Last-Modified`
   headers or content hashing. Design deferred to v2.

## Recommendation

**Conclusion**: Proceed
**Confidence**: High

The web crawler acquisition source is feasible with well-established Rust
crates. The architecture follows the existing acquisition pattern closely:

- `reqwest` (already transitive) + `htmd` + `scraper` + `texting_robots`
- Write crawled pages as Markdown to a temp dir
- Reuse `FilteredFileSet` pipeline for downstream processing
- New `type: url` variant in `sources.yaml` with safety limits

The implementation is well-bounded (one new acquisition module + config
extension) and does not affect existing pipeline stages.

**Risk**: Content extraction quality varies by site. A v1 implementation
should include configurable CSS selectors for main content extraction, with
sensible defaults for common documentation site layouts.

## Next Steps

1. Promote to `impl-plan` for detailed implementation planning
2. Decompose into tasks (config schema, crawl engine, content extraction,
   robots.txt, tests)
3. Consider pairing with 020-F (DOCX ingestion) in shipment 011-S since
   both extend the pipeline's input surface

## References

- `src/config/source.rs` — Source enum (extension point)
- `src/acquire/mod.rs` — Acquisition dispatcher
- `src/acquire/result.rs` — SourceAction and FilteredFileSet types
- `docs/decisions/2026-04-30-pdf-ingestion-crate-spike.md` — Prior spike
  with analogous integration pattern
- `Cargo.toml` — Current dependency manifest
