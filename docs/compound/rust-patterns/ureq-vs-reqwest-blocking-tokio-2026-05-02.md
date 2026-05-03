---
title: "Replace reqwest::blocking with ureq to avoid tokio nested-runtime panic"
date: 2026-05-02
tags: [rust, async, tokio, http, ureq, reqwest]
pr: "https://github.com/softwaresalt/graphtor-docs/pull/20"
---

## Problem

`reqwest::blocking` internally creates a `tokio::runtime::Runtime`. When
that internal runtime is **dropped inside an existing tokio context**
(e.g., from inside `#[tokio::main]` or any `async` function running on
the tokio thread pool), Rust panics:

```text
Cannot drop a runtime in a context where blocking is not allowed.
This happens when a runtime is dropped from within an asynchronous context.
```

This manifests as a panic when `crawl_url_source` (a sync function) was
called from the MCP server's async dispatch loop.

## Solution

Replace `reqwest::blocking` with `ureq`, a pure-sync HTTP client that has
**no tokio dependency**. `ureq` does not create an internal runtime, so it
can be dropped freely from any context.

### Key API differences

| Concern | `reqwest::blocking` | `ureq` 2.x |
|---|---|---|
| Client creation | `reqwest::blocking::Client::builder().build()?` (fallible) | `ureq::AgentBuilder::new().build()` (infallible) |
| GET request | `client.get(url).send()?` | `agent.get(url).call()?` |
| Response body | `response.text()?` | `response.into_string()?` |
| Non-2xx error | `response.status().is_success()` check | `Err(ureq::Error::Status(code, response))` |
| Per-request timeout | `client.get(url).timeout(dur).send()` | Must build a **new** `Agent` with different timeout |
| TLS features | `rustls-tls` or `native-tls` feature | Default = rustls only; avoid `features = ["tls"]` (pulls both) |

### ureq Cargo.toml

```toml
# CORRECT — default features use rustls only
ureq = "2"

# WRONG — features = ["tls"] pulls both native-tls AND rustls in ureq 2.x
ureq = { version = "2", features = ["tls"] }
```

### Per-operation timeout pattern

`ureq` 2.x has no per-request timeout API. Build a separate `Agent` for
operations that need a different timeout:

```rust
fn build_client() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("my-app/1.0")
        .build()
}

fn fetch_robots_txt(url: &str) -> Result<String, ureq::Error> {
    // Separate short-timeout agent — never share the main agent
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .user_agent("my-app/1.0")
        .build();
    Ok(agent.get(url).call()?.into_string()?)
}
```

### Regression test pattern

Guard against the nested-runtime panic with a `#[tokio::test]`:

```rust
#[tokio::test]
async fn crawl_does_not_panic_inside_tokio() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    // Use std::net (not tokio::net) — no tokio "net" feature needed
    thread::spawn(move || {
        // HTTP/1.1 with Content-Length — avoids ureq waiting for TCP EOF
        let html = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "Content-Length: 30\r\n",
            "Connection: close\r\n",
            "\r\n",
            "<html><body>test</body></html>",
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        for _ in 0..3_u8 {
            let Ok((mut stream, _)) = listener.accept() else { break; };
            let mut buf = vec![0u8; 512];
            let Ok(n) = stream.read(&mut buf) else { break; };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("robots.txt") { not_found } else { html };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let result = my_crawler(&format!("http://127.0.0.1:{port}/"));
    assert!(result.is_ok(), "must not panic inside tokio: {result:?}");
}
```

**Critical**: use HTTP/1.1 with explicit `Content-Length`. HTTP/1.0 without
Content-Length tells ureq to read until TCP close, which can cause timeouts
on some CI Linux TCP stacks.

## Import path gotcha

`UrlSource` is defined in `src/config/source.rs` and is **not** re-exported
at the `config` module level. Use the full path:

```rust
// CORRECT
use graphtor_core::config::source::UrlSource;

// WRONG — UrlSource is not re-exported at config::
use graphtor_core::config::UrlSource;
```

## tokio dev-dependency

`tokio::net::TcpListener` requires the `"net"` feature flag. If your
`dev-dependencies` only have `rt-multi-thread` + `macros`, use `std::net`
instead for test servers:

```toml
# Only rt-multi-thread + macros in dev-deps → use std::net in tests
[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```
