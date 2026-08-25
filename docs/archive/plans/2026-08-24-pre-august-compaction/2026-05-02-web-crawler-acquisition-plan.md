---
title: "Web Crawler Acquisition Source"
type: impl-plan
date: 2026-05-02
source: docs/decisions/2026-05-02-web-crawler-acquisition-spike.md
linked_feature: "019-F"
requires_hardening: false
---

## Problem Frame

graphtor-docs currently acquires documentation from Git repositories and local
directories. Feature 019-F adds a third acquisition backend — `type: url` — that
crawls live documentation sites, converts HTML to Markdown, and feeds the result
into the existing parse → embed → load pipeline.

The spike (`docs/decisions/2026-05-02-web-crawler-acquisition-spike.md`) confirmed
feasibility with `reqwest` (HTTP), `htmd` (HTML→MD), `scraper` (content extraction),
and `texting_robots` (robots.txt). The architecture follows the existing acquisition
dispatch pattern: a new `Source::Url` variant flows through planning, execution, and
filtering to produce a `FilteredFileSet`.

## Requirements Trace

| Requirement | Implementation Unit |
|---|---|
| New `type: url` source in `sources.yaml` | Unit 1: Config schema |
| HTTP fetching with rate limiting | Unit 2: Crawl engine |
| HTML-to-Markdown conversion | Unit 2: Crawl engine |
| robots.txt compliance | Unit 2: Crawl engine |
| Domain locking and depth/page limits | Unit 2: Crawl engine |
| Integration into acquisition dispatcher | Unit 3: Pipeline wiring |
| Deduplication of visited URLs | Unit 2: Crawl engine |
| Configurable CSS selectors for content extraction | Unit 2: Crawl engine |

## Implementation Units

### Unit 1: Config Schema — `UrlSource` type and validation

**What**: Add `Url(UrlSource)` variant to the `Source` enum in `src/config/source.rs`
with fields: `id`, `url`, `max_depth`, `max_pages`, `domain_lock`, `rate_limit_ms`,
`user_agent`, `content_selector`, `include`, `exclude`. Add validation rules in
`src/config/validation.rs`.

**Files**:
- `src/config/source.rs` — Add `UrlSource` struct and `Url` variant (~30 lines)
- `src/config/validation.rs` — Add URL-specific validation (~20 lines)

**Tests**: Add unit tests in `src/config/source.rs` module tests:
- `url_source_deserializes_all_fields` — round-trip YAML parse
- `url_source_defaults` — verify default values for optional fields
- `url_source_validation_rejects_invalid_url` — empty/malformed URL
- `url_source_validation_rejects_zero_limits` — zero max_depth/max_pages

**Posture**: Test-first.

### Unit 2: Crawl Engine — `src/acquire/url.rs`

**What**: New module implementing BFS web crawling with rate limiting,
robots.txt compliance, HTML content extraction, and Markdown conversion.
Writes converted pages as `.md` files to a temp directory under `data_root`.

**Files**:
- `src/acquire/url.rs` — New module (~250 lines):
  - `crawl_url_source(source: &UrlSource, target_dir: &Path) -> Result<Vec<PathBuf>>`
  - Internal: `fetch_page`, `extract_links`, `convert_to_markdown`,
    `check_robots`, `normalize_url`
- `Cargo.toml` — Add dependencies: `reqwest = { version = "0.12", features = ["blocking"] }`,
  `htmd = "0.5"`, `scraper = "0.22"`, `texting_robots = "0.2"`

**Tests**: Integration test file `tests/acquire_url_test.rs`:
- `crawl_respects_max_pages_limit` — mock server, verify page count
- `crawl_respects_domain_lock` — verify external links not followed
- `crawl_skips_robots_disallowed` — mock robots.txt, verify skipped URLs
- `crawl_produces_markdown_files` — verify .md files in temp dir

**Posture**: Test-first. Note: integration tests need a local HTTP server mock.
Use `actix-web` or `warp` as a dev-dependency for test fixtures, or use
file-based test doubles.

**Compound learnings to apply**:
- **Chunk ID uniqueness** (`pdf-chunk-id-uniqueness-pattern`): crawled pages may
  have repeated nav/footer content. Include URL path in `chunk_id_source`.
- **Path normalization** (`windows-path-normalization`): URL-derived paths should
  use forward slashes when stored as chunk keys.

### Unit 3: Pipeline Wiring — dispatcher and plan integration

**What**: Wire `Source::Url` into the acquisition dispatcher and planner so
URL sources flow through the pipeline like Git and Local sources.

**Files**:
- `src/acquire/result.rs` — Add `CrawlUrl` to `SourceAction` enum (~5 lines)
- `src/acquire/mod.rs` — Add `pub mod url;`, `CrawlUrl` dispatch arm,
  `execute_crawl_url()` handler (~30 lines)
- `src/acquire/plan.rs` — Add URL source planning logic: create temp dir,
  determine action (~15 lines)
- `src/config/source.rs` — Add `Source::id()` match arm for `Url` (~2 lines)

**Tests**: Integration test `tests/pipeline_url_test.rs`:
- `url_source_flows_through_pipeline` — end-to-end with mock server

**Posture**: Test-first. Depends on Unit 1 and Unit 2.

## Dependency Graph

```text
Unit 1 (Config Schema)
  └─ Unit 2 (Crawl Engine) — needs UrlSource type
       └─ Unit 3 (Pipeline Wiring) — needs crawl functions + config
```

Linear dependency chain. No parallelism possible.

## Decisions and Rationale

| Decision | Rationale |
|---|---|
| Use `reqwest` blocking client | The acquisition pipeline is sync. A blocking client avoids async propagation through `execute()`. `reqwest` is already a transitive dep. |
| Write crawled pages to temp dir as `.md` | Reuses existing `scan_and_filter` + `FilteredFileSet` pipeline without changes. Zero impact on downstream stages. |
| Default `domain_lock: true` | Safety: prevents unbounded crawling across the internet. Users must explicitly opt out. |
| Default `max_pages: 100` | Safety limit. Documentation sites can have thousands of pages; users should set this intentionally. |
| Use `scraper` for content extraction | Documentation sites have nav/sidebar/footer noise. CSS selector-based extraction (`article`, `main`, configurable) produces cleaner Markdown. |
| Use `texting_robots` for robots.txt | Legal/ethical compliance. Pure Rust, cached parsing, minimal weight. |

## Risks and Caveats

| Risk | Mitigation |
|---|---|
| JS-rendered content invisible to `reqwest` | Document limitation. Most doc sites (learn.microsoft.com) use SSR. Headless browser is v2. |
| Content extraction quality varies by site | Make CSS selector configurable via `content_selector` field. Provide sensible default (`main, article, .content`). |
| Rate limiting insufficient for aggressive sites | Default 500ms delay. Respect `Crawl-delay` from robots.txt when present. |
| Large crawls consume disk space | Temp dir under `data_root`, cleaned up configurable. `max_pages` provides hard limit. |
| `reqwest` blocking feature adds weight | Minimal — `reqwest` is already compiled via `hf-hub` transitive chain. |

## Plan Hardening Signals

- Public API, schema, or contract change: **Yes** — new `type: url` in `sources.yaml` schema
- Security, auth, permission, or compliance-sensitive behavior: **Yes** — robots.txt compliance, outbound HTTP
- Migration, backfill, destructive data/config action: **No**
- External integration, operator checkpoint, or external dependency: **Yes** — outbound HTTP to arbitrary URLs
- High runtime, rollout, or rollback risk: **No** — additive feature, no existing behavior changed

**Requires plan hardening: no** — despite schema and HTTP concerns, this is a purely
additive feature with no migration or rollback risk. The schema addition is backwards-compatible
(existing `sources.yaml` files without `type: url` continue to work). robots.txt compliance
is built into the design. The feature can be removed by reverting the Cargo.toml + module additions.

## Runtime Verification and Closure

| Unit | Runtime Surface | Verification |
|---|---|---|
| Unit 1 | CLI (`sync` subcommand) | `sources.yaml` with `type: url` parses without error |
| Unit 2 | Network (outbound HTTP) | Crawl a small test URL and verify `.md` files produced |
| Unit 3 | CLI (`sync` subcommand) | End-to-end: `type: url` source produces chunks in CozoDB |

**Closure**: No monitoring or rollback needed — additive feature with no production deployment.
Verification is via integration tests and manual `cargo run -- sync` with a test `sources.yaml`.

## Plan Review

**Gate Decision: PASS**
**Date**: 2026-05-02
**Reviewers**: Constitution Reviewer, Rust Reviewer, Scope Boundary Auditor, Learnings Researcher

### Hardening Assessment

Three hardening signals are present (schema change, outbound HTTP, external integration).
The plan correctly concludes `requires_hardening: false` because:
- The schema change is purely additive (backwards-compatible `type: url` variant)
- robots.txt compliance is designed in, not bolted on
- The feature is fully revertible by removing the module and Cargo.toml entry
- No migrations, no data transformation, no rollback risk

**Hardening requirement satisfied**: Yes (no hardening needed).

### Findings

| # | Severity | Persona | Finding | Recommendation |
|---|---|---|---|---|
| 1 | P3 | Constitution | New crate dependencies (`htmd`, `scraper`, `texting_robots`) should be checked for `unsafe` usage before final dependency commit | Run `cargo geiger` or audit crate source during implementation |
| 2 | P3 | Rust | Unit 2 test mock server approach is underspecified — "use `actix-web` or `warp` as dev-dependency, or use file-based test doubles" leaves the choice to implementation time | Recommend `wiremock` crate (purpose-built for HTTP mocking, lighter than full web frameworks) or file-based doubles if network tests are flaky |
| 3 | P3 | Scope | Unit 2 at ~250 lines is at the upper bound of the 2-hour rule but acceptable as a single-concern module | Monitor during implementation; if complexity exceeds estimate, split `robots.rs` out as a sub-module |
| 4 | P3 | Rust | `reqwest` blocking feature may pull in additional TLS/connection-pool code beyond the async-only transitive dep | Verify compiled binary size delta after adding the `blocking` feature flag |

### Summary

All findings are P3 (advisory). No P0, P1, or P2 issues detected. The plan
follows established acquisition patterns, correctly references compound learnings,
and has clean unit boundaries. Proceeds to harvest.
