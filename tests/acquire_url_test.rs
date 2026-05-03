//! Regression tests for URL source acquisition.
//!
//! Covers the tokio nested-runtime panic that occurred when `reqwest::blocking`
//! was used inside a `#[tokio::main]` context, and basic crawl behaviour.

use graphtor_core::{acquire::url::crawl_url_source, config::UrlSource};

/// Build a minimal [`UrlSource`] pointing at `url`.
fn make_url_source(id: &str, url: &str) -> UrlSource {
    UrlSource {
        id: id.to_owned(),
        url: url.to_owned(),
        max_depth: 0,
        max_pages: 1,
        domain_lock: false,
        rate_limit_ms: 0,
        formats: vec![],
        include: vec![],
        exclude: vec![],
    }
}

// ── Regression: no panic inside tokio runtime ─────────────────────────────────

/// `crawl_url_source` MUST NOT panic when called from within a `#[tokio::test]`
/// (i.e. from inside an active tokio runtime).
///
/// Previously `reqwest::blocking` created an internal `tokio::runtime::Runtime`
/// which panicked on drop:
///   "Cannot drop a runtime in a context where blocking is not allowed"
///
/// `ureq` is a pure-sync client with no tokio dependency, so this panic can no
/// longer occur.
#[tokio::test]
async fn crawl_url_source_does_not_panic_inside_tokio_runtime() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    // Spawn a minimal HTTP server that handles robots.txt (404) and one HTML page.
    tokio::spawn(async move {
        let html = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/html\r\n",
            "Content-Length: 27\r\n",
            "\r\n",
            "<html><body>test</body></html>",
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";

        // Handle up to 3 connections: robots.txt probe + main page + any retry.
        for _ in 0..3_u8 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = vec![0u8; 512];
            let Ok(n) = stream.read(&mut buf).await else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("robots.txt") {
                not_found.as_bytes()
            } else {
                html.as_bytes()
            };
            let _ = stream.write_all(resp).await;
        }
    });

    let source = make_url_source("regression-no-panic", &format!("http://127.0.0.1:{port}/"));
    let target = tempfile::tempdir().expect("tempdir");

    // If this panics the test fails; that is the regression signal.
    let result = crawl_url_source(&source, target.path());

    // A successful crawl writes at least one markdown file.
    assert!(
        result.is_ok(),
        "crawl_url_source must not return Err inside a tokio runtime: {result:?}"
    );
}
