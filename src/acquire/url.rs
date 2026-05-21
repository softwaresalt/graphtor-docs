//! URL source acquisition via BFS HTTP crawling.
//!
//! Entry point: [`crawl_url_source`]. Fetches pages starting at the configured
//! URL, converts HTML to Markdown via [`htmd`], and writes each page as a
//! `{hash}.md` file under `target_dir`. Returns the list of written file paths
//! for downstream pipeline stages.
//!
//! # Behaviour
//!
//! - BFS traversal bounded by `max_depth` and `max_pages`.
//! - When `domain_lock` is `true` (default), only links whose registered domain
//!   matches the start URL are followed.
//! - `robots.txt` is fetched once from the start-URL origin and honoured for
//!   every subsequent request.
//! - Per-page HTTP failures are logged as warnings; crawling continues.
//! - A minimum `rate_limit_ms` delay is inserted between consecutive requests.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};
use url::Url;

use crate::config::source::UrlSource;
use crate::error::GraphtorError;

// ── Public entry point ────────────────────────────────────────────────────────

/// Crawl a [`UrlSource`] starting at `source.url` and write converted Markdown
/// files to `target_dir`.
///
/// Returns an ordered list of the Markdown file paths written.
///
/// # Errors
///
/// Returns [`GraphtorError`] only when `target_dir` cannot be created.
/// HTTP client construction is now infallible (see [`build_client`]).
///
/// Per-page HTTP errors are logged as warnings and the page is skipped.
pub fn crawl_url_source(
    source: &UrlSource,
    target_dir: &Path,
) -> Result<Vec<PathBuf>, GraphtorError> {
    std::fs::create_dir_all(target_dir).map_err(GraphtorError::Io)?;

    let client = build_client(source.rate_limit_ms);
    let robots = fetch_robots_txt(&source.url);
    let origin = extract_origin(&source.url);

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, u32)> = VecDeque::new();
    let mut written: Vec<PathBuf> = Vec::new();

    queue.push_back((normalise_url(&source.url), 0));

    while let Some((url, depth)) = queue.pop_front() {
        if visited.contains(&url) {
            continue;
        }
        if written.len() >= source.max_pages {
            debug!(
                limit = source.max_pages,
                "page limit reached; stopping crawl"
            );
            break;
        }
        if depth > source.max_depth {
            continue;
        }

        // Honour robots.txt
        if let Some(ref robot) = robots {
            if !robot.allowed(&url) {
                debug!(%url, "robots.txt disallows; skipping");
                visited.insert(url);
                continue;
            }
        }

        visited.insert(url.clone());

        // Rate-limit delay
        if !written.is_empty() {
            std::thread::sleep(Duration::from_millis(source.rate_limit_ms));
        }

        // Fetch and convert
        let html = match fetch_html(&client, &url) {
            Ok(h) => h,
            Err(e) => {
                warn!(%url, err = %e, "failed to fetch page; skipping");
                continue;
            }
        };

        let markdown = match htmd::convert(&html) {
            Ok(md) => md,
            Err(e) => {
                warn!(%url, err = %e, "failed to convert HTML to Markdown; skipping");
                continue;
            }
        };

        // Write to disk — skip if content is unchanged to avoid spurious mtime bumps.
        let file_path = url_to_file_path(target_dir, &url);
        let existing = std::fs::read(&file_path).unwrap_or_default();
        if existing == markdown.as_bytes() {
            debug!(%url, path = %file_path.display(), "content unchanged; skipping write");
        } else {
            if let Err(e) = std::fs::write(&file_path, markdown.as_bytes()) {
                warn!(%url, err = %e, "failed to write Markdown file; skipping");
                continue;
            }
            info!(%url, path = %file_path.display(), "crawled page");
        }
        written.push(file_path);

        // Enqueue links for next depth
        if depth < source.max_depth {
            for link in extract_links(&html, &url) {
                let normalised = normalise_url(&link);
                if visited.contains(&normalised) {
                    continue;
                }
                if source.domain_lock && !same_domain(&normalised, &origin) {
                    continue;
                }
                queue.push_back((normalised, depth + 1));
            }
        }
    }

    remove_stale_crawled_files(target_dir, &written);

    info!(
        source_id = %source.id,
        pages = written.len(),
        "url crawl complete"
    );
    Ok(written)
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Build a `ureq` HTTP agent with appropriate timeouts and user-agent.
///
/// `ureq` is a pure-sync HTTP client with no internal tokio dependency,
/// which avoids the "nested runtime" panic when called from within a
/// `#[tokio::main]` context.
fn build_client(_rate_limit_ms: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("graphtor-docs/1.0 (documentation crawler)")
        .build()
}

/// Fetch the HTML body of `url`, returning `Err` on any HTTP or I/O failure.
fn fetch_html(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("request failed: {e}"))?;

    response
        .into_string()
        .map_err(|e| format!("failed to read response body: {e}"))
}

/// Fetch and parse `robots.txt` from `start_url`'s origin.
///
/// Uses a short 5 s timeout so that a slow or hung origin does not block the
/// entire crawl (the main crawl agent uses a 30 s timeout, which is too long
/// for a preflight fetch).
///
/// Returns `None` if the fetch or parse fails (allow-all semantics).
fn fetch_robots_txt(start_url: &str) -> Option<texting_robots::Robot> {
    use std::io::Read as _;
    // Build a dedicated short-timeout agent for this single preflight request.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .user_agent("graphtor-docs/1.0 (documentation crawler)")
        .build();
    let origin = extract_origin(start_url);
    let robots_url = format!("{origin}/robots.txt");

    let response = agent.get(&robots_url).call().ok()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes).ok()?;

    texting_robots::Robot::new("graphtor-docs", &bytes).ok()
}

/// Remove files left over from previous crawls that were not produced by the
/// current traversal.
fn remove_stale_crawled_files(target_dir: &Path, written: &[PathBuf]) {
    if written.is_empty() {
        return;
    }
    let keep: HashSet<&Path> = written.iter().map(PathBuf::as_path).collect();

    let Ok(entries) = std::fs::read_dir(target_dir) else {
        warn!(path = %target_dir.display(), "failed to enumerate crawl output directory");
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !keep.contains(path.as_path()) {
            if let Err(error) = std::fs::remove_file(&path) {
                warn!(path = %path.display(), %error, "failed to remove stale crawled file");
            }
        }
    }
}

/// Extract the `scheme://host[:port]` portion of a URL.
///
/// Falls back to the full URL if no `://` separator is found.
fn extract_origin(url: &str) -> String {
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let scheme = if url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        let host = rest.split('/').next().unwrap_or(rest);
        format!("{scheme}://{host}")
    } else {
        url.split('/').take(3).collect::<Vec<_>>().join("/")
    }
}

/// Return `true` if `url` belongs to the same registered domain as `origin`.
///
/// Compares origin prefixes at a path-component boundary so that
/// `https://example.com.evil.com/` does not match `https://example.com`.
fn same_domain(url: &str, origin: &str) -> bool {
    if !url.starts_with(origin) {
        return false;
    }
    // The character immediately after the origin prefix must be absent (exact
    // match), '/' (path continues), or '?' (query follows) to be the same domain.
    let remainder = &url[origin.len()..];
    remainder.is_empty() || remainder.starts_with('/') || remainder.starts_with('?')
}

/// Remove a `#fragment` from a URL string.
fn strip_fragment(url: &str) -> &str {
    url.split_once('#').map_or(url, |(before, _)| before)
}

/// Extract all navigable document links from `html`, resolving them against
/// `base_url`.
fn extract_links(html: &str, base_url: &str) -> Vec<String> {
    let document = scraper::Html::parse_document(html);
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    // Raw iframe scanning handles both ordinary iframe elements and iframe tags
    // embedded inside `<noscript>`, which `scraper` treats as text on the live
    // Rust Book site.
    for raw_link in extract_iframe_srcs(html) {
        push_resolved_link(raw_link.as_str(), base_url, &mut links, &mut seen);
    }

    let Ok(selector) = scraper::Selector::parse("a[href]") else {
        warn!("failed to compile link CSS selector");
        return links;
    };

    for element in document.select(&selector) {
        let Some(raw_link) = element.value().attr("href") else {
            continue;
        };
        push_resolved_link(raw_link, base_url, &mut links, &mut seen);
    }

    links
}

/// Extract iframe `src` values from raw HTML text.
fn extract_iframe_srcs(html: &str) -> Vec<String> {
    let mut srcs = Vec::new();
    let mut remaining = html;

    while let Some(start) = remaining.find("<iframe") {
        remaining = &remaining[start + "<iframe".len()..];
        let Some(end) = remaining.find('>') else {
            break;
        };
        let tag = &remaining[..end];

        if let Some(src) = extract_attribute_value(tag, "src") {
            srcs.push(src.to_string());
        }

        remaining = &remaining[end + 1..];
    }

    srcs
}

/// Extract a quoted HTML attribute value from a tag fragment.
fn extract_attribute_value<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let needle = format!("{attr_name}={quote}");
        if let Some(start) = tag.find(&needle) {
            let value = &tag[start + needle.len()..];
            let end = value.find(quote)?;
            return Some(&value[..end]);
        }
    }

    None
}

/// Resolve and append a raw link target if it is navigable and not already
/// present in `links`.
fn push_resolved_link(
    raw_link: &str,
    base_url: &str,
    links: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if raw_link.starts_with('#')
        || raw_link.starts_with("javascript:")
        || raw_link.starts_with("mailto:")
    {
        return;
    }

    if let Some(resolved) = resolve_link(raw_link, base_url) {
        let clean = strip_fragment(&resolved).to_string();
        if !clean.is_empty() && seen.insert(clean.clone()) {
            links.push(clean);
        }
    }
}

/// Resolve `href` against `base_url` into an absolute URL.
///
/// Returns `None` if resolution is not possible.
fn resolve_link(href: &str, base_url: &str) -> Option<String> {
    let mut base = Url::parse(base_url).ok()?;

    // `normalise_url()` strips trailing slashes for deduplication, which makes
    // directory indexes like `.../book/` look like file URLs. Restore the
    // slash before joining relative links so sidebar navigation stays under the
    // current documentation section instead of jumping to the origin root.
    if should_treat_base_as_directory(&base) {
        let directory_path = format!("{}/", base.path());
        base.set_path(&directory_path);
    }

    base.join(href).ok().map(|resolved| resolved.to_string())
}

/// Determine whether a normalised base URL should be treated as a directory for
/// relative-link resolution.
fn should_treat_base_as_directory(base: &Url) -> bool {
    if base.cannot_be_a_base() || base.path().ends_with('/') {
        return false;
    }

    let Some(mut segments) = base.path_segments() else {
        return false;
    };

    let Some(last_segment) = segments.next_back() else {
        return false;
    };

    !last_segment.is_empty() && !last_segment.contains('.')
}

/// Normalise a URL for deduplication: strip trailing slash (except bare origin).
fn normalise_url(url: &str) -> String {
    let stripped = url.trim_end_matches('/');
    if stripped.is_empty() {
        url.to_string()
    } else {
        stripped.to_string()
    }
}

/// Compute the output file path for a crawled URL.
///
/// Uses the first 16 hex characters of the SHA-256 of the URL as the file name
/// to avoid filesystem path issues with arbitrary URL structures.
fn url_to_file_path(target_dir: &Path, url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let hash = hasher.finalize();
    let hex: String = hash.iter().take(8).fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    });
    target_dir.join(format!("{hex}.md"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_origin_https() {
        assert_eq!(
            extract_origin("https://learn.microsoft.com/en-us/dotnet/"),
            "https://learn.microsoft.com"
        );
    }

    #[test]
    fn extract_origin_http_with_path() {
        assert_eq!(
            extract_origin("http://example.com/foo/bar"),
            "http://example.com"
        );
    }

    #[test]
    fn strip_fragment_removes_hash() {
        assert_eq!(
            strip_fragment("https://example.com/page#section"),
            "https://example.com/page"
        );
    }

    #[test]
    fn strip_fragment_no_fragment_unchanged() {
        assert_eq!(
            strip_fragment("https://example.com/page"),
            "https://example.com/page"
        );
    }

    #[test]
    fn normalise_url_strips_trailing_slash() {
        assert_eq!(
            normalise_url("https://example.com/foo/"),
            "https://example.com/foo"
        );
    }

    #[test]
    fn normalise_url_no_trailing_slash_unchanged() {
        assert_eq!(
            normalise_url("https://example.com/foo"),
            "https://example.com/foo"
        );
    }

    #[test]
    fn same_domain_true_for_same_origin() {
        assert!(same_domain(
            "https://learn.microsoft.com/en-us/dotnet",
            "https://learn.microsoft.com"
        ));
    }

    #[test]
    fn same_domain_false_for_different_origin() {
        assert!(!same_domain(
            "https://other.example.com/page",
            "https://learn.microsoft.com"
        ));
    }

    #[test]
    fn url_to_file_path_produces_md_file() {
        let path = url_to_file_path(Path::new("/data"), "https://example.com/page");
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with(".md"), "expected .md extension, got {name}");
        assert_eq!(
            name.len(),
            19,
            "expected 16 hex chars + .md (19 chars), got {name}"
        );
    }

    #[test]
    fn url_to_file_path_deterministic() {
        let a = url_to_file_path(Path::new("/data"), "https://example.com/page");
        let b = url_to_file_path(Path::new("/data"), "https://example.com/page");
        assert_eq!(a, b);
    }

    #[test]
    fn url_to_file_path_distinct_for_different_urls() {
        let a = url_to_file_path(Path::new("/data"), "https://example.com/page1");
        let b = url_to_file_path(Path::new("/data"), "https://example.com/page2");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_link_absolute_passthrough() {
        let result = resolve_link("https://other.com/page", "https://base.com/dir/");
        assert_eq!(result, Some("https://other.com/page".to_string()));
    }

    #[test]
    fn resolve_link_absolute_path() {
        let result = resolve_link("/docs/overview", "https://example.com/some/path");
        assert_eq!(
            result,
            Some("https://example.com/docs/overview".to_string())
        );
    }

    #[test]
    fn resolve_link_relative_path() {
        let result = resolve_link("next.html", "https://example.com/docs/index.html");
        assert_eq!(
            result,
            Some("https://example.com/docs/next.html".to_string())
        );
    }

    #[test]
    fn resolve_link_preserves_directory_base_without_trailing_slash() {
        let result = resolve_link(
            "ch01-01-installation.html",
            "https://doc.rust-lang.org/book",
        );
        assert_eq!(
            result,
            Some("https://doc.rust-lang.org/book/ch01-01-installation.html".to_string())
        );
    }

    #[test]
    fn extract_links_uses_directory_base_after_url_normalisation() {
        let html = r#"<a href="ch01-01-installation.html">Install</a>"#;
        let base_url = normalise_url("https://doc.rust-lang.org/book/");
        let links = extract_links(html, &base_url);
        assert_eq!(
            links,
            vec!["https://doc.rust-lang.org/book/ch01-01-installation.html".to_string()]
        );
    }

    #[test]
    fn extract_links_skips_fragment_only() {
        let html = r##"<a href="#section">Section</a>"##;
        let links = extract_links(html, "https://example.com/page");
        assert!(links.is_empty(), "fragment-only links should be skipped");
    }

    #[test]
    fn extract_links_skips_mailto() {
        let html = r#"<a href="mailto:info@example.com">Contact</a>"#;
        let links = extract_links(html, "https://example.com/page");
        assert!(links.is_empty(), "mailto links should be skipped");
    }

    #[test]
    fn extract_links_returns_absolute_href() {
        let html = r#"<a href="https://example.com/about">About</a>"#;
        let links = extract_links(html, "https://example.com/home");
        assert_eq!(links, vec!["https://example.com/about".to_string()]);
    }

    #[test]
    fn extract_links_includes_iframe_src() {
        let html = r#"<iframe src="toc.html"></iframe>"#;
        let base_url = normalise_url("https://doc.rust-lang.org/book/");
        let links = extract_links(html, &base_url);
        assert_eq!(
            links,
            vec!["https://doc.rust-lang.org/book/toc.html".to_string()]
        );
    }

    #[test]
    fn extract_links_includes_iframe_src_inside_noscript() {
        let html = r#"<noscript><iframe src="toc.html"></iframe></noscript>"#;
        let base_url = normalise_url("https://doc.rust-lang.org/book/");
        let links = extract_links(html, &base_url);
        assert_eq!(
            links,
            vec!["https://doc.rust-lang.org/book/toc.html".to_string()]
        );
    }
}
