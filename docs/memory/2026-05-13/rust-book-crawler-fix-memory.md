---
title: "Session Memory - Rust Book crawler fix"
date: 2026-05-13
branch: fix/rust-book-crawler
status: in-progress
---

## Outcome

Fixed the URL crawler so the Rust Book crawl reaches the sidebar-driven
chapter pages. Verified the actual workspace `rust-book` data directory
contains all 111 TOC targets from
<https://doc.rust-lang.org/book/toc.html>.

## Files Changed

* `.graphtor\config\sources.yaml` - added the `rust-book` URL source and
  restored `max_pages: 250`
* `Cargo.toml` and `Cargo.lock` - added the `url` crate for correct
  relative URL joining
* `src\acquire\url.rs` - fixed directory URL joining, extracted iframe
  sources from raw HTML, prioritized iframe TOC links, and cleaned stale
  crawl output
* `tests\acquire_url_test.rs` - added regressions for directory-root
  links, noscript iframe discovery, print.html queue starvation, and
  stale-file cleanup

## Verification

* `cargo fmt --all -- --check`
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`
* `cargo test --all-targets`
* `.\.graphtor\bin\graphtor-docs.exe doctor`
* Live TOC coverage check: `111/111` Rust Book TOC pages present in
  `.graphtor\data\rust-book`
* Workspace data directory currently contains `250` files because URL
  crawling is domain-locked, not path-locked
* `cargo audit` still reports existing transitive dependency advisories
  (`git2`, `cozo` tree) unrelated to this crawler fix

## Key Decisions

* Treat normalized directory URLs like
  `https://doc.rust-lang.org/book/` as directories when resolving
  relative links
* Scan raw HTML for `iframe src` values so sidebar TOCs inside
  `<noscript>` are discoverable
* Queue iframe-derived links ahead of broad anchor discovery so
  `toc.html` beats `print.html`
* Remove stale files after each URL crawl so reruns converge

## Failed Approaches

* Relying on DOM-only iframe extraction missed the live Rust Book
  sidebar
* Fixing relative URL joining alone still allowed off-book pages to
  consume the crawl budget before chapter discovery

## Open Questions

* The installed `sync --no-embed` workspace refresh remained active at
  last observation while still writing to `.graphtor\graph.db`
* URL sources still support only domain-wide locking; there is no config
  field to confine crawling to the `/book/` subtree
