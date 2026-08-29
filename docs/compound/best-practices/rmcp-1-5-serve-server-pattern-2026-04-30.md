---
title: "rmcp 1.5 uses serve_server() free function, not .serve() from ServiceExt"
description: "rmcp 1.5 replaced the ServiceExt trait pattern with a serve_server() free function; #[tool_router]/#[tool_handler] macros and schemars 1.x are required"
problem_type: "api_mismatch"
category: "best-practices"
component: "src/mcp/server.rs"
root_cause: "rmcp API changed between 0.x and 1.5: serve entry point, tool macros, error constructors, and schemars version all differ"
resolution_type: "code_fix"
severity: "high"
message: "error[E0599]: no method named `serve` found for struct `DocServer`"
file_path: "src/mcp/server.rs"
citations:
  - "https://github.com/softwaresalt/graphtor-docs/pull/10"
tags:
  - "rmcp"
  - "mcp"
  - "server"
  - "tool_router"
  - "serve_server"
  - "schemars"
---

## Problem

Documentation and older tutorials for `rmcp` describe starting the MCP server
with `.serve(transport)` via a `ServiceExt` trait import:

```rust
use rmcp::ServiceExt;
server.serve(rmcp::transport::stdio()).await
```

This API does not exist in rmcp 1.5 and produces a compile error. Additionally,
older examples use `rmcp-macros` as a separate crate, `#[tool]` on free
functions, and `schemars = "0.8"`.

## Root Cause

rmcp underwent a major API revision between 0.x and 1.5:

| Concern | rmcp 0.x / docs | rmcp 1.5 (actual) |
|---|---|---|
| Server startup | `server.serve(transport)` via `ServiceExt` | `rmcp::serve_server(server, transport)` free function |
| Tool registration | `#[tool]` on impl block | `#[tool_router]` on impl block |
| Handler impl | varies | `#[tool_handler]` on `impl ServerHandler` |
| schemars version | `schemars = "0.8"` | `schemars = "1"` (re-exported as `rmcp::schemars`) |
| Error factory | `ErrorData::new(...)` | `ErrorData::invalid_params(msg, data)`, `ErrorData::internal_error(msg, data)` |

## Resolution

> [!IMPORTANT]
> This is an API-shape reference for rmcp 1.5, not a dependency-selection
> recommendation. The published rmcp 1.5.0 manifest uses edition 2024, which
> Cargo 1.75 cannot parse. Workspaces that declare Rust 1.75 must select and pin
> a compatible dependency strategy before applying these patterns, then verify
> the selected strategy with the exact declared toolchain.

### Historical rmcp 1.5 Cargo shape

```toml
# Historical API-shape example only; do not copy this version as an MSRV choice.
rmcp = { version = "1.5", features = ["server", "transport-io"] }
# Do NOT add schemars separately — use rmcp::schemars (re-exported at version 1.x)
```

### Server startup in main.rs

```rust
use rmcp::transport::stdio;

rmcp::serve_server(server, stdio()).await
```

### Tool impl structure

```rust
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};

#[tool_router]
impl MyServer {
    #[tool(description = "...")]
    fn my_tool(
        &self,
        Parameters(params): Parameters<MyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // ...
    }
}

#[tool_handler]
impl ServerHandler for MyServer {}
```

### Parameter types

```rust
#[derive(Debug, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct MyParams {
    pub field: String,
}
```

### Error construction

```rust
// Validation errors
return Err(ErrorData::invalid_params("field cannot be empty", None));

// Internal errors
return Err(ErrorData::internal_error("database query failed", None));
```

## Prevention

- Always check `rmcp`'s crates.io README for the current API — it has changed
  significantly across major versions.
- Use `rmcp::schemars` re-export, never add `schemars` as a direct dependency
  (version mismatch will cause trait impls to not resolve).
- `#[tool_router]` goes on the `impl` block containing the tool methods.
  `#[tool_handler]` goes on `impl ServerHandler for MyServer`.
- Keep `.github/copilot-instructions.md` and `rust-mcp-server.instructions.md`
  version references synchronized with `Cargo.toml`.
