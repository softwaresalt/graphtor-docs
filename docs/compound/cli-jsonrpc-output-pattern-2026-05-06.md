# Compound Learning: CLI JSON-RPC 2.0 Output Pattern

**Date**: 2026-05-06  
**Shipment**: 021-S / Feature 030-F  
**PR**: #36

## Problem

Adding a `--json` global flag to all CLI commands so that agent workflows can
consume CLI output in the same JSON-RPC 2.0 format used by the MCP server.
The `manifest` subcommand must mirror the MCP `tools/list` response so CLI
and MCP surfaces are interchangeable from an agent's perspective.

## Solution Pattern

### 1. OutputFormat enum + global flag

Add `OutputFormat { Human, Json }` to `src/cli/mod.rs` alongside the global
`--json: bool` flag on the `Cli` struct.  In `run()`, compute `fmt` once and
thread it through every `cmd_*` function signature.

```rust
let fmt = if cli.json { OutputFormat::Json } else { OutputFormat::Human };
```

### 2. JSON-RPC 2.0 envelope module

`src/cli/jsonrpc.rs` provides `wrap_success(impl Serialize)` and
`wrap_error(i32, message, Option<Value>)`.  Critical: `wrap_success` must
match on `to_value` — on `Err`, call `wrap_error(-32603, ...)` directly.
Do NOT use `unwrap_or(Value::Null)` — that silently converts serialization
failures into success envelopes with `result: null`.

### 3. Exposing MCP tool definitions for the manifest command

The `#[tool_router]` macro generates a **private** `fn tool_router()`.  To
expose tool definitions publicly, add a separate `impl DocServer` block in
`src/mcp/server.rs` (same module as the macro, so it can call the private
function):

```rust
impl DocServer {
    pub(crate) fn list_tools() -> Vec<rmcp::model::Tool> {
        let mut tools: Vec<rmcp::model::Tool> = Self::tool_router()
            .map
            .into_values()
            .map(|route| route.attr)
            .collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }
}
```

The `ToolRouter<S>.map` field is a `HashMap`, so iteration order is
non-deterministic.  Always sort by name for reproducible output.  The doc
comment should say the manifest "mirrors the *set* of tools" — not
"exactly mirrors" — because the live `tools/list` response is unordered.

### 4. cmd_manifest fail-fast pattern

In JSON mode, iterate tools with `match to_value(t)` — on `Err`, print the
error envelope and return exit code 1 immediately.  Never collect nulls:

```rust
for t in &tools {
    match serde_json::to_value(t) {
        Ok(v) => tool_values.push(v),
        Err(e) => {
            println!("{}", wrap_error(SERVER_ERROR, format!("...{e}"), None));
            return 1;
        }
    }
}
```

### 5. clippy::unnecessary_wraps

Functions that return `anyhow::Result<i32>` but never actually return `Err()`
trigger `clippy::unnecessary_wraps` under pedantic.  Fix: change return type
to `i32` and wrap the call site with `Ok()`:

```rust
// Before
fn cmd_manifest(fmt: OutputFormat) -> anyhow::Result<i32> { ... Ok(0) }
// run(): cmd_manifest(fmt)?

// After
fn cmd_manifest(fmt: OutputFormat) -> i32 { ... 0 }
// run(): Ok(cmd_manifest(fmt))
```

### 6. clippy::bool_to_int_with_if

```rust
// Bad
if error_count == 0 { 0 } else { 1 }
// Good
i32::from(error_count != 0)
```

### 7. PowerShell PR body with backticks

When creating a PR via `gh pr create --body "..."`, backticks in the body are
interpreted as PowerShell subexpressions.  Write the body to a file and use
`--body-file logs/pr-body.md` instead.

### 8. PowerShell + GraphQL variable name collision

GraphQL queries use `$variable` syntax.  In PowerShell double-quoted strings,
`$variable` is expanded.  Assign the query to a PowerShell variable first:

```powershell
$query = 'query GetThreads($owner: String!) { ... }'
gh api graphql -f query="$query" -f owner="..."
```

## Key Files

| File | Role |
|------|------|
| `src/cli/jsonrpc.rs` | JSON-RPC 2.0 envelope helpers |
| `src/cli/mod.rs` | `OutputFormat` enum, `--json` flag, `Manifest` command |
| `src/mcp/server.rs` | `DocServer::list_tools()` impl block |
| `src/mcp/mod.rs` | `list_mcp_tools()` public API |
| `src/main.rs` | `OutputFormat` threading, `cmd_manifest()` |
