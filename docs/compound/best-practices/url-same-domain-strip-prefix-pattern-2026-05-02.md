---
title: URL same-domain check — avoid prefix ambiguity with strip_prefix
tags: [rust, url, web-crawler, security]
date: 2026-05-02
shipment: 011-S
---

## Problem

When implementing a BFS web crawler with same-domain enforcement, a naive
`url.starts_with(origin)` check has a prefix collision vulnerability:

```rust
// BUG: "https://example.com.evil.com/path" starts with "https://example.com"
if url.starts_with(origin) { /* allows evil.com! */ }
```

This can cause the crawler to follow links off the intended domain if an
attacker-controlled page embeds links to a similarly-prefixed hostname.

## Fix

Use `strip_prefix(origin)` and verify the remainder is either empty (exact
origin match) or starts with `/` (a path under the origin):

```rust
fn same_domain(url: &str, origin: &str) -> bool {
    url.strip_prefix(origin)
        .map_or(false, |rest| rest.is_empty() || rest.starts_with('/'))
}
```

This correctly handles:
- `https://example.com` → remainder `""` → ✅ same domain
- `https://example.com/docs/page` → remainder `/docs/page` → ✅ same domain
- `https://example.com.evil.com/x` → no strip match → ❌ different domain
- `https://example.com-phishing.io` → no strip match → ❌ different domain

## Context

This is especially important for crawlers that write fetched content to disk,
as the domain scope also controls what external content the application will
index and persist. An overly-permissive domain check could cause the crawler
to ingest unintended third-party sites.
