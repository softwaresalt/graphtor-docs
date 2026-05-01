---
title: CozoScript query columns must match SearchResult struct fields
tags: [rust, cozodb, search, struct-alignment]
date: 2026-04-30
---

## Problem

`search_by_text` queried `?[chunk_id, path, headings, content]` but
`SearchResult` had no `source_id` field. The MCP tool filtered by
`r.path` prefix as a workaround, which was semantically wrong (source IDs
are not path prefixes) and produced unreliable results.

## Solution

Add `source_id` to both the struct and the Cozo query:

```rust
// db/search.rs
pub struct SearchResult {
    pub chunk_id: String,
    pub source_id: String,   // ← added
    pub path: String,
    pub heading_hierarchy: Vec<String>,
    pub content: String,
}

// query
?[chunk_id, source_id, path, headings, content]
    := *doc_chunks{ chunk_id, source_id, path, headings, content },
       str_includes(lowercase(content), lowercase($query))
```

Then filter on `r.source_id == sid` in the MCP tool — exact equality,
not path prefix matching.

## Empty source_id normalization

`Some("")` passed as `source_id` should be treated as `None` (no filter):

```rust
let sid_filter = params.source_id.as_deref()
    .and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
```

## Rule

Keep Cozo query column projections in sync with the Rust struct fields that
decode them. Cozo result rows are positional — column order in the query
head must match the `row[idx]` decoding sequence in `row_to_result`.

## Citations

- `src/db/search.rs` — `SearchResult`, `search_by_text`, `row_to_result`
- `src/mcp/server.rs` — `source_id` filter (PR #12, commit 61e9d8b)
