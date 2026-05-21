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
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

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

/// `crawl_url_source` MUST preserve directory-style base URLs like `/book/`
/// when following relative sidebar links.
#[test]
fn crawl_url_source_follows_relative_links_from_directory_root() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    thread::spawn(move || {
        let root_body = r#"<html><body><nav><a href="ch01-01-installation.html">Install</a></nav></body></html>"#;
        let chapter_body = "<html><body>chapter</body></html>";
        let root = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{root_body}",
            root_body.len()
        );
        let chapter = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{chapter_body}",
            chapter_body.len()
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        // robots.txt probe + root page + linked chapter + optional retry.
        for _ in 0..4_u8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 1024];
            let Ok(n) = stream.read(&mut buf) else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("GET /robots.txt ") {
                not_found
            } else if req.contains("GET /book ") || req.contains("GET /book/ ") {
                root.as_str()
            } else if req.contains("GET /book/ch01-01-installation.html ") {
                chapter.as_str()
            } else {
                not_found
            };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let mut source = make_url_source("directory-root", &format!("http://127.0.0.1:{port}/book/"));
    source.max_depth = 1;
    source.max_pages = 2;

    let target = tempfile::tempdir().expect("tempdir");
    let result = crawl_url_source(&source, target.path()).expect("crawl succeeds");

    assert_eq!(
        result.len(),
        2,
        "expected root page plus linked chapter to be crawled"
    );
}

/// `crawl_url_source` MUST discover chapter links exposed via a sidebar iframe
/// such as mdBook's `toc.html`.
#[test]
fn crawl_url_source_discovers_links_from_iframe_sidebar() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    thread::spawn(move || {
        let root_body =
            r#"<html><body><noscript><iframe src="toc.html"></iframe></noscript></body></html>"#;
        let toc_body =
            r#"<html><body><a href="ch01-01-installation.html">Install</a></body></html>"#;
        let chapter_body = "<html><body>chapter</body></html>";
        let root = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{root_body}",
            root_body.len()
        );
        let toc = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{toc_body}",
            toc_body.len()
        );
        let chapter = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{chapter_body}",
            chapter_body.len()
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        // robots.txt probe + root page + iframe toc + linked chapter + optional retry.
        for _ in 0..5_u8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 1024];
            let Ok(n) = stream.read(&mut buf) else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("GET /robots.txt ") {
                not_found
            } else if req.contains("GET /book ") || req.contains("GET /book/ ") {
                root.as_str()
            } else if req.contains("GET /book/toc.html ") {
                toc.as_str()
            } else if req.contains("GET /book/ch01-01-installation.html ") {
                chapter.as_str()
            } else {
                not_found
            };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let mut source = make_url_source("iframe-sidebar", &format!("http://127.0.0.1:{port}/book/"));
    source.max_depth = 2;
    source.max_pages = 3;

    let target = tempfile::tempdir().expect("tempdir");
    let result = crawl_url_source(&source, target.path()).expect("crawl succeeds");

    assert_eq!(
        result.len(),
        3,
        "expected root page, iframe toc, and linked chapter to be crawled"
    );
}

/// Sidebar iframe discovery must be prioritised ahead of broad same-domain
/// anchor crawls such as `print.html`, or a page cap can starve real chapters.
#[test]
fn crawl_url_source_prioritises_iframe_sidebar_before_print_links() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    thread::spawn(move || {
        let root_body = r#"<html><body><a href="print.html">Print</a><noscript><iframe src="toc.html"></iframe></noscript></body></html>"#;
        let toc_body =
            r#"<html><body><a href="ch01-01-installation.html">Install</a></body></html>"#;
        let print_body = concat!(
            "<html><body>",
            r#"<a href="offbook-1.html">One</a>"#,
            r#"<a href="offbook-2.html">Two</a>"#,
            r#"<a href="offbook-3.html">Three</a>"#,
            r#"<a href="offbook-4.html">Four</a>"#,
            "</body></html>",
        );
        let chapter_body = "<html><body>chapter</body></html>";
        let offbook_body = "<html><body>offbook</body></html>";
        let root = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{root_body}",
            root_body.len()
        );
        let toc = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{toc_body}",
            toc_body.len()
        );
        let print = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{print_body}",
            print_body.len()
        );
        let chapter = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{chapter_body}",
            chapter_body.len()
        );
        let offbook = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{offbook_body}",
            offbook_body.len()
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        for _ in 0..8_u8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 1024];
            let Ok(n) = stream.read(&mut buf) else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("GET /robots.txt ") {
                not_found
            } else if req.contains("GET /book ") || req.contains("GET /book/ ") {
                root.as_str()
            } else if req.contains("GET /book/toc.html ") {
                toc.as_str()
            } else if req.contains("GET /book/print.html ") {
                print.as_str()
            } else if req.contains("GET /book/ch01-01-installation.html ") {
                chapter.as_str()
            } else if req.contains("GET /book/offbook-") {
                offbook.as_str()
            } else {
                not_found
            };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let mut source = make_url_source("iframe-priority", &format!("http://127.0.0.1:{port}/book/"));
    source.max_depth = 2;
    source.max_pages = 4;

    let target = tempfile::tempdir().expect("tempdir");
    let result = crawl_url_source(&source, target.path()).expect("crawl succeeds");
    let contains_chapter = result
        .iter()
        .any(|path| std::fs::read_to_string(path).is_ok_and(|content| content.contains("chapter")));

    assert!(
        contains_chapter,
        "expected chapter page to survive page-cap prioritisation"
    );
}

/// A fresh URL crawl must remove files that were produced by an older crawl but
/// were not seen this time.
#[test]
fn crawl_url_source_removes_stale_files_from_previous_run() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("get local addr").port();

    thread::spawn(move || {
        let body = "<html><body>fresh</body></html>";
        let page = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

        for _ in 0..3_u8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = vec![0u8; 1024];
            let Ok(n) = stream.read(&mut buf) else {
                break;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let resp = if req.contains("GET /robots.txt ") {
                not_found
            } else if req.contains("GET /book ") || req.contains("GET /book/ ") {
                page.as_str()
            } else {
                not_found
            };
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let source = make_url_source("cleanup", &format!("http://127.0.0.1:{port}/book/"));
    let target = tempfile::tempdir().expect("tempdir");
    let stale_path = target.path().join("stale.md");
    std::fs::write(&stale_path, "stale").expect("write stale file");

    let result = crawl_url_source(&source, target.path()).expect("crawl succeeds");

    assert_eq!(result.len(), 1, "expected a single fresh page");
    assert!(
        !stale_path.exists(),
        "expected stale file from prior crawl to be removed"
    );
}
