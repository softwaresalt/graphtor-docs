//! Tests for hyperlink reference extraction — 003.003-T.

use graphtor_core::parse::{ast::parse_ast, links::extract};

const CHUNK_ID: &str = "abc123chunk";

/// No links in document produces empty reference list.
#[test]
fn test_no_links_returns_empty() {
    let md = "## Section\n\nNo links here.\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert!(refs.is_empty());
}

/// A single inline link is extracted with correct fields.
#[test]
fn test_single_link_extracted() {
    let md = "See [the guide](../guide/intro.md) for details.\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert_eq!(refs.len(), 1);
    let r = &refs[0];
    assert_eq!(r.source_chunk_id, CHUNK_ID);
    assert_eq!(r.target_path, "../guide/intro.md");
    assert_eq!(r.link_text, "the guide");
    assert!(r.anchor.is_none());
}

/// A link with a `#fragment` is split into path and anchor.
#[test]
fn test_link_with_anchor_split_correctly() {
    let md = "Read [section](overview.md#installation) now.\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert_eq!(refs.len(), 1);
    let r = &refs[0];
    assert_eq!(r.target_path, "overview.md");
    assert_eq!(r.anchor.as_deref(), Some("installation"));
}

/// Multiple links in one document are all extracted.
#[test]
fn test_multiple_links_extracted() {
    let md = "See [A](a.md) and [B](b.md#sec) and [C](c.md).\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert_eq!(refs.len(), 3);
    let paths: Vec<&str> = refs.iter().map(|r| r.target_path.as_str()).collect();
    assert!(paths.contains(&"a.md"));
    assert!(paths.contains(&"b.md"));
    assert!(paths.contains(&"c.md"));
}

/// An anchor-only link (`#heading`) results in empty target_path.
#[test]
fn test_anchor_only_link() {
    let md = "Jump to [section](#heading-name).\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert_eq!(refs.len(), 1);
    let r = &refs[0];
    assert_eq!(r.target_path, "");
    assert_eq!(r.anchor.as_deref(), Some("heading-name"));
}

/// External URLs are extracted unchanged.
#[test]
fn test_external_url_extracted() {
    let md = "Visit [the site](https://example.com/page) for info.\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].target_path, "https://example.com/page");
    assert!(refs[0].anchor.is_none());
}

/// `source_chunk_id` is set to the provided chunk identifier on all refs.
#[test]
fn test_source_chunk_id_set_on_all_refs() {
    let md = "See [A](a.md) and [B](b.md).\n";
    let nodes = parse_ast(md);
    let refs = extract(&nodes, CHUNK_ID);
    for r in &refs {
        assert_eq!(r.source_chunk_id, CHUNK_ID);
    }
}
