//! Tests for the pulldown-cmark AST parser and parse types — 003.001-T.

use graphtor_core::parse::{parse_ast, AstNode};

/// The AST walker must detect headings at all levels.
#[test]
fn test_ast_detects_headings() {
    let md = "# Title\n\n## Section One\n\nSome text.\n\n### Subsection\n\nMore text.\n";
    let nodes = parse_ast(md);
    let headings: Vec<_> = nodes
        .iter()
        .filter_map(|n| {
            if let AstNode::Heading { level, text } = n {
                Some((*level, text.as_str()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(headings.len(), 3);
    assert_eq!(headings[0], (1, "Title"));
    assert_eq!(headings[1], (2, "Section One"));
    assert_eq!(headings[2], (3, "Subsection"));
}

/// The AST walker must detect inline links.
#[test]
fn test_ast_detects_links() {
    let md = "See [the docs](../other/page.md) for details.\n";
    let nodes = parse_ast(md);
    let links: Vec<_> = nodes
        .iter()
        .filter_map(|n| {
            if let AstNode::Link { url, text } = n {
                Some((url.as_str(), text.as_str()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].0, "../other/page.md");
    assert_eq!(links[0].1, "the docs");
}

/// The AST walker must detect fenced code blocks with language tags.
#[test]
fn test_ast_detects_code_blocks() {
    let md = "Some text.\n\n```rust\nfn main() {}\n```\n\n```\nno lang\n```\n";
    let nodes = parse_ast(md);
    let code_blocks: Vec<_> = nodes
        .iter()
        .filter_map(|n| {
            if let AstNode::CodeBlock { language, content } = n {
                Some((language.as_deref(), content.as_str()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(code_blocks.len(), 2);
    assert_eq!(code_blocks[0].0, Some("rust"));
    assert!(code_blocks[0].1.contains("fn main()"));
    assert_eq!(code_blocks[1].0, None);
}

/// Text content is captured as Paragraph nodes between headings.
#[test]
fn test_ast_captures_paragraph_text() {
    let md = "## Intro\n\nHello world.\n";
    let nodes = parse_ast(md);
    let has_paragraph = nodes.iter().any(|n| matches!(n, AstNode::Paragraph { .. }));
    assert!(has_paragraph, "expected at least one Paragraph node");
}
