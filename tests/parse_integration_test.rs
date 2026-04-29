//! End-to-end integration tests for `parse_document()` — 003.005-T.
//!
//! These tests exercise the full pipeline: frontmatter → AST → chunks →
//! references → code snippets → [`graphtor_core::parse::ParsedDocument`] assembly.

use graphtor_core::parse::parse_document;

/// Full document with all features produces a correctly assembled [`graphtor_core::parse::ParsedDocument`].
#[test]
fn test_full_document_parsed() {
    let md = "\
---
title: My Guide
description: A comprehensive guide
---
# My Guide

Intro paragraph with a [link](other.md).

## Section One

Content for section one.

```rust
fn hello() {}
```

## Section Two

More content. See [details](details.md#anchor).

### Subsection

Deep content.
";
    let doc = parse_document(md, "docs/guide.md").expect("parse should succeed");

    assert_eq!(doc.path, "docs/guide.md");
    assert!(
        doc.title.is_some(),
        "title should be extracted from frontmatter"
    );
    assert_eq!(doc.title.as_deref(), Some("My Guide"));

    // Frontmatter extracted
    let fm = doc.frontmatter.as_ref().expect("frontmatter expected");
    assert_eq!(fm.description.as_deref(), Some("A comprehensive guide"));

    // Chunks: intro + Section One + Section Two + Subsection
    assert!(
        doc.chunks.len() >= 3,
        "expected at least 3 chunks, got {}",
        doc.chunks.len()
    );

    // All chunks have provenance
    for chunk in &doc.chunks {
        assert_eq!(chunk.source_path, "docs/guide.md");
        assert_eq!(chunk.chunk_id.len(), 64);
    }

    // References extracted (link in intro + link in Section Two)
    assert!(doc.references.len() >= 2, "expected at least 2 references");
    let targets: Vec<&str> = doc
        .references
        .iter()
        .map(|r| r.target_path.as_str())
        .collect();
    assert!(targets.contains(&"other.md"), "expected other.md reference");
    assert!(
        targets.contains(&"details.md"),
        "expected details.md reference"
    );

    // Code snippet extracted
    assert_eq!(doc.code_snippets.len(), 1);
    assert_eq!(doc.code_snippets[0].language.as_deref(), Some("rust"));
}

/// Document without frontmatter is handled correctly.
#[test]
fn test_document_without_frontmatter() {
    let md = "# Title\n\nSome content.\n";
    let doc = parse_document(md, "docs/simple.md").expect("parse should succeed");
    assert!(doc.frontmatter.is_none());
    assert_eq!(doc.path, "docs/simple.md");
    assert!(!doc.chunks.is_empty());
}

/// Document with no headings produces one intro chunk.
#[test]
fn test_document_with_no_headings() {
    let md = "Just some plain text.\n";
    let doc = parse_document(md, "docs/plain.md").expect("parse should succeed");
    assert_eq!(doc.chunks.len(), 1);
    assert!(doc.references.is_empty());
    assert!(doc.code_snippets.is_empty());
}

/// `parse_document` propagates provenance (`source_path`) to all sub-records.
#[test]
fn test_provenance_propagated_to_all_records() {
    let md = "## Section\n\nSee [ref](ref.md).\n\n```bash\necho hi\n```\n";
    let doc = parse_document(md, "docs/prov.md").expect("parse should succeed");

    for c in &doc.chunks {
        assert_eq!(c.source_path, "docs/prov.md");
    }
    for r in &doc.references {
        // source_chunk_id must be a valid chunk_id (present in doc.chunks)
        let found = doc.chunks.iter().any(|c| c.chunk_id == r.source_chunk_id);
        assert!(
            found,
            "reference source_chunk_id '{}' not in chunks",
            r.source_chunk_id
        );
    }
    for s in &doc.code_snippets {
        let found = doc.chunks.iter().any(|c| c.chunk_id == s.chunk_id);
        assert!(found, "snippet chunk_id '{}' not in chunks", s.chunk_id);
    }
}

/// Title falls back to the first H1 heading when frontmatter has no title.
#[test]
fn test_title_from_h1_when_no_frontmatter() {
    let md = "# Document Title\n\n## Section\n\nContent.\n";
    let doc = parse_document(md, "docs/h1.md").expect("parse should succeed");
    assert_eq!(doc.title.as_deref(), Some("Document Title"));
}

/// Empty document does not panic and returns empty collections.
#[test]
fn test_empty_document_does_not_panic() {
    let doc = parse_document("", "docs/empty.md").expect("parse should succeed");
    assert!(doc.chunks.is_empty());
    assert!(doc.references.is_empty());
    assert!(doc.code_snippets.is_empty());
}
