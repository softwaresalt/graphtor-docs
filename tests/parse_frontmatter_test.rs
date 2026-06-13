//! Tests for YAML frontmatter detection and stripping — 003.006-T.

use graphtor_core::parse::frontmatter::strip;

/// Documents without frontmatter are returned unchanged.
#[test]
fn test_no_frontmatter_returns_content_unchanged() {
    let md = "# Title\n\nSome content here.\n";
    let (fm, body) = strip(md);
    assert!(fm.is_none(), "expected no frontmatter");
    assert_eq!(body, md);
}

/// Frontmatter is detected by the leading `---\n` delimiter.
#[test]
fn test_frontmatter_stripped_from_body() {
    let md = "---\ntitle: My Doc\ndescription: A test\n---\n# Title\n\nBody text.\n";
    let (fm, body) = strip(md);
    let fm = fm.expect("expected frontmatter");
    assert_eq!(fm.title.as_deref(), Some("My Doc"));
    assert_eq!(fm.description.as_deref(), Some("A test"));
    assert_eq!(body, "# Title\n\nBody text.\n");
}

/// The `raw_yaml` field contains the exact YAML between the delimiters.
#[test]
fn test_raw_yaml_preserved() {
    let md = "---\ntitle: Raw\ndate: 2026-01-01\n---\nbody\n";
    let (fm, _) = strip(md);
    let fm = fm.expect("expected frontmatter");
    assert!(fm.raw_yaml.contains("title: Raw"));
    assert!(fm.raw_yaml.contains("date: 2026-01-01"));
}

/// Unknown frontmatter fields are silently ignored.
#[test]
fn test_unknown_frontmatter_fields_tolerated() {
    let md = "---\ntitle: Known\nunknown_field: value\n---\nbody\n";
    let (fm, _) = strip(md);
    let fm = fm.expect("expected frontmatter");
    assert_eq!(fm.title.as_deref(), Some("Known"));
}

/// Documents that start with `---` but have no closing delimiter are treated
/// as having no frontmatter (malformed).
#[test]
fn test_malformed_frontmatter_no_closing_returns_none() {
    let md = "---\ntitle: Broken\nno closing delimiter\n";
    let (fm, body) = strip(md);
    assert!(fm.is_none(), "malformed frontmatter should return None");
    assert_eq!(body, md);
}

/// Frontmatter with only a `title` field (no `description`) is handled.
#[test]
fn test_frontmatter_title_only() {
    let md = "---\ntitle: Just Title\n---\ncontent\n";
    let (fm, body) = strip(md);
    let fm = fm.expect("expected frontmatter");
    assert_eq!(fm.title.as_deref(), Some("Just Title"));
    assert!(fm.description.is_none());
    assert_eq!(body, "content\n");
}

/// A bare `---` with no trailing newline is treated as no frontmatter — not a
/// panic. Previously the `trimmed == "---"` guard admitted this and then
/// attempted an out-of-bounds slice.
#[test]
fn test_bare_triple_dash_without_newline_does_not_panic() {
    let md = "---";
    let (fm, body) = strip(md);
    assert!(fm.is_none(), "bare --- should return no frontmatter");
    assert_eq!(body, md);
}

/// Documents with leading blank lines before `---` are treated as having no
/// frontmatter; frontmatter must start at byte 0.
#[test]
fn test_leading_newlines_before_frontmatter_returns_none() {
    let md = "\n---\ntitle: Should Not Match\n---\nbody\n";
    let (fm, _) = strip(md);
    assert!(fm.is_none(), "frontmatter must start at byte 0");
}

/// Documents that don't start with `---` are not treated as having frontmatter.
#[test]
fn test_content_starting_with_heading_has_no_frontmatter() {
    let md = "# Heading\n\n---\nThis is a separator, not frontmatter.\n";
    let (fm, body) = strip(md);
    assert!(fm.is_none());
    assert_eq!(body, md);
}

// ── CRLF regression tests ─────────────────────────────────────────────────────

/// CRLF opening delimiter is detected correctly.
#[test]
fn test_crlf_frontmatter_detected() {
    let md =
        "---\r\ntitle: CRLF Doc\r\ndescription: Windows checkout\r\n---\r\n# Title\r\n\r\nBody.\r\n";
    let (fm, body) = strip(md);
    let fm = fm.expect("CRLF frontmatter must be detected");
    assert_eq!(fm.title.as_deref(), Some("CRLF Doc"));
    assert_eq!(fm.description.as_deref(), Some("Windows checkout"));
    // Body starts immediately after the closing delimiter.
    assert!(
        body.starts_with("# Title"),
        "body should start after closing delimiter, got: {body:?}"
    );
}

/// CRLF `raw_yaml` is LF-normalised before storage.
#[test]
fn test_crlf_raw_yaml_is_lf_normalised() {
    let md = "---\r\ntitle: Norm\r\n---\r\nbody\r\n";
    let (fm, _) = strip(md);
    let fm = fm.expect("should detect frontmatter");
    assert!(
        !fm.raw_yaml.contains('\r'),
        "raw_yaml must not contain CR after normalisation; got: {:?}",
        fm.raw_yaml
    );
    assert!(fm.raw_yaml.contains("title: Norm"));
}

/// CRLF document with YAML end-of-document marker (`...`) is handled.
#[test]
fn test_crlf_yaml_eod_marker() {
    let md = "---\r\ntitle: EOD\r\n...\r\n# Body\r\n";
    let (fm, body) = strip(md);
    let fm = fm.expect("CRLF ... delimiter must be accepted");
    assert_eq!(fm.title.as_deref(), Some("EOD"));
    assert!(body.starts_with("# Body"));
}

/// CRLF document with no closing delimiter is treated as malformed.
#[test]
fn test_crlf_malformed_no_closing_delimiter() {
    let md = "---\r\ntitle: Broken\r\nno closing delimiter\r\n";
    let (fm, body) = strip(md);
    assert!(
        fm.is_none(),
        "malformed CRLF frontmatter should return None"
    );
    assert_eq!(body, md);
}

/// LF content is still parsed correctly after CRLF detection changes.
#[test]
fn test_lf_frontmatter_still_works_after_crlf_support() {
    let md = "---\ntitle: LF Doc\n---\nBody text.\n";
    let (fm, body) = strip(md);
    let fm = fm.expect("LF frontmatter must still be detected");
    assert_eq!(fm.title.as_deref(), Some("LF Doc"));
    assert_eq!(body, "Body text.\n");
}
