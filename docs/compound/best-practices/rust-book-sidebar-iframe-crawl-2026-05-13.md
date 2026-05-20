---
title: "Rust Book sidebar TOC requires raw iframe discovery and directory-aware URL joins"
description: "mdBook-style sites can hide chapter navigation in noscript iframes, so crawlers need raw iframe scanning and directory-aware URL joins to capture the full book"
problem_type: "crawler-missed-pages"
category: "best-practices"
component: "url crawler"
root_cause: "The live Rust Book exposes toc.html through a noscript iframe, and normalized directory URLs like /book lose directory semantics during relative-link joins"
resolution_type: "code_fix"
severity: "medium"
message: "Rust Book crawl only captures a small subset of pages because chapter links are hidden behind toc.html and relative joins can escape the /book section"
file_path: "src\\acquire\\url.rs"
citations:
  - "docs\\memory\\2026-05-13\\rust-book-crawler-fix-memory.md:8-64"
  - "src\\acquire\\url.rs:197-213"
  - "src\\acquire\\url.rs:256-376"
  - "tests\\acquire_url_test.rs:150-217"
  - "tests\\acquire_url_test.rs:220-316"
tags:
  - "crawler"
  - "mdbook"
  - "iframe"
  - "url-resolution"
  - "rust-book"
---

## Problem

The Rust Book crawl looked badly incomplete even with a generous page
budget. The data directory contained only a small subset of the book at
first, and live verification showed that chapter pages from the sidebar
TOC were missing from the crawl output.

## Root Cause

Two behaviors combined to hide the real chapter pages. First, the live
Rust Book does not expose the chapter list as ordinary anchors on the
root page; it references `toc.html` through an `iframe` inside
`<noscript>`, which DOM-only extraction missed. Second, URL
normalization stripped the trailing slash from `https://doc.rust-lang.org/book/`,
so relative joins treated `book` like a file name and could resolve
links outside the intended documentation section.

## Resolution

Update `extract_links` to scan raw HTML for iframe `src` values before
anchor traversal so `toc.html` is discovered even when it is embedded in
`<noscript>`. Update `resolve_link` to restore directory semantics for
normalized base URLs whose last segment has no file extension. Keep
iframe-derived links ahead of broad same-domain anchors like
`print.html`, and delete stale crawl output files that are not produced
by the current run. Lock the behavior down with integration tests that
cover directory-root links, iframe sidebar discovery, page-cap
prioritization, and stale-file cleanup.

## Prevention

When a documentation crawl misses large site sections, inspect the live
HTML rather than assuming all navigation is exposed through normal anchor
tags. Treat normalized directory URLs carefully before using them as
bases for relative-link joins, and add regression tests for real-site
navigation patterns whenever a crawler fix depends on HTML structure or
queue ordering.
