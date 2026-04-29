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

/// Documents that don't start with `---` are not treated as having frontmatter.
#[test]
fn test_content_starting_with_heading_has_no_frontmatter() {
    let md = "# Heading\n\n---\nThis is a separator, not frontmatter.\n";
    let (fm, body) = strip(md);
    assert!(fm.is_none());
    assert_eq!(body, md);
}
