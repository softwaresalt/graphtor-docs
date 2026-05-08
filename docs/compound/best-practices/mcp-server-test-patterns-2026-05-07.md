# MCP Server Test Patterns for Private Tool Methods

**Date**: 2026-05-07
**Context**: PR #41 — adding positive-path and manifest contract tests for all 8 `DocServer` MCP tools

## Problem

`DocServer` MCP tool methods are private (`impl DocServer` under `#[tool_router]`
has no `pub`). Integration tests in `tests/` cannot call these methods directly.
Positive-path tests need a real `DataStore` with test data, but `DataStore::open`
requires a filesystem path.

## Solution

### 1. Co-locate tests in `#[cfg(test)]` inside `src/mcp/server.rs`

Since the methods are private, all positive-path tests must live in the `mod tests`
block at the bottom of `src/mcp/server.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{upsert_chunk, upsert_edge, upsert_source};
    use crate::db::schema::{Chunk, Reference, SourceRecord};

    fn populated_store() -> DataStore {
        let s = DataStore::open_mem().expect("mem store");
        // upsert test data...
        s
    }
    
    fn server_with_store(store: DataStore) -> DocServer {
        DocServer { store: Arc::new(store) }
    }
    
    #[tokio::test]
    async fn get_chunk_by_id_returns_content() {
        let server = server_with_store(populated_store());
        // call server.get_chunk_by_id(req) via CallToolRequest...
    }
}
```

`DataStore::open_mem()` creates a transient in-memory SQLite-backed store — no
filesystem path, no cleanup needed.

### 2. Contract tests for `list_mcp_tools()` go in `tests/`

`list_mcp_tools()` is a public function on the library crate, so its contract
tests can live in `tests/mcp_manifest_test.rs`:

```rust
const EXPECTED_TOOLS: &[&str] = &[
    "get_chunk_by_id", "get_document", "get_status",
    "list_sources", "research_topic", "search_local_docs",
    "search_semantic", "traverse_doc_links",
];

#[test]
fn tool_manifest_contains_all_expected_tools() { ... }
```

Update `EXPECTED_TOOLS` whenever a tool is added or removed.

## Gotcha: `Arc<str>` vs `&str`

`rmcp::model::Tool.name` is `Arc<str>`. Using `.as_str()` hits unstable feature
`#130366`. Use `.as_ref()` instead:

```rust
// BAD — unstable:
tool.name.as_str()

// GOOD — stable:
tool.name.as_ref()  // or: &*tool.name
```

## Assertion Patterns

### Ordering assertions — use `find()` not `contains()`

When testing reading order, avoid `assert!(text.contains(chunk_a))` — it only
validates presence, not position. Use index comparison:

```rust
let pos0 = text.find("chunk-0 content").expect("chunk-0 not found");
let pos1 = text.find("chunk-1 content").expect("chunk-1 not found");
assert!(pos0 < pos1, "chunk-0 should appear before chunk-1");
```

### Count assertions — use specific formatted strings

When asserting database counts from `format_db_status()`, match the exact output
format rather than `contains('2')` which could match schema version numbers:

```rust
// BAD — too weak, matches schema version:
assert!(text.contains('2'));

// GOOD — matches exact format_db_status() output:
assert!(text.contains("**Chunks:** 2"), "expected 2 chunks");
assert!(text.contains("**Sources:** 1"), "expected 1 source");
```

The `format_db_status()` format is:
```
## Database Status\n\n- **Sources:** N\n- **Chunks:** N\n- **Schema version:** N\n
```
