//! Regression tests for URL source acquisition.
//!
//! Covers the tokio nested-runtime panic that occurred when `reqwest::blocking`
//! was used inside a `#[tokio::main]` context, and basic crawl behaviour.

use graphtor_core::{acquire::url::crawl_url_source, config::source::UrlSource};

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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    // Spawn a minimal HTTP/1.1 server using std::net (no tokio "net" feature needed).
    // Uses Content-Length + Connection: close so ureq knows exactly when to stop reading.
    thread::spawn(move || {
        // 30 bytes: <html><body>test</body></html>
        let html = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "Content-Length: 30\r\n",
            "Connection: close\r\n",
            "\r\n",
            "<html><body>test</body></html>",
        );
        let not_found =
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        // Handle up to 3 connections: robots.txt probe + main page + any retry.
        for _ in 0..3_u8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 512];
            let Ok(n) = stream.read(&mut buf) else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("robots.txt") {
                not_found
            } else {
                html
            };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let source = make_url_source("regression-no-panic", &format!("http://127.0.0.1:{port}/"));
    let target = tempfile::tempdir().expect("tempdir");

    // If this panics the test fails; that is the regression signal.
    let result = crawl_url_source(&source, target.path());

    // The crawl must succeed without panicking — per-page failures are warnings.
    assert!(
        result.is_ok(),
        "crawl_url_source must not return Err inside a tokio runtime: {result:?}"
    );
}
