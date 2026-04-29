//! Tests for fenced code block extraction — 003.004-T.

use graphtor_core::parse::{ast::parse_ast, code::extract};

const CHUNK_ID: &str = "parent-chunk-001";

/// Document with no code blocks produces an empty list.
#[test]
fn test_no_code_blocks_returns_empty() {
    let md = "## Section\n\nJust prose, no code.\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    assert!(snippets.is_empty());
}

/// A fenced code block with a language tag is extracted correctly.
#[test]
fn test_fenced_block_with_language_extracted() {
    let md = "## Example\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    assert_eq!(snippets.len(), 1);
    let s = &snippets[0];
    assert_eq!(s.language.as_deref(), Some("rust"));
    assert!(s.content.contains("fn main()"));
    assert_eq!(s.chunk_id, CHUNK_ID);
}

/// A fenced code block with no language tag has `language == None`.
#[test]
fn test_fenced_block_without_language() {
    let md = "```\nplain content\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    assert_eq!(snippets.len(), 1);
    assert!(snippets[0].language.is_none());
    assert!(snippets[0].content.contains("plain content"));
}

/// Multiple code blocks in the same chunk are all extracted.
#[test]
fn test_multiple_code_blocks_all_extracted() {
    let md = "## Doc\n\n```rust\nlet x = 1;\n```\n\n```json\n{\"key\": \"value\"}\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    assert_eq!(snippets.len(), 2);
    let langs: Vec<Option<&str>> = snippets.iter().map(|s| s.language.as_deref()).collect();
    assert!(langs.contains(&Some("rust")));
    assert!(langs.contains(&Some("json")));
}

/// Each snippet has a stable 64-char hex `id` field.
#[test]
fn test_snippet_ids_are_valid() {
    let md = "```python\nprint('hi')\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    assert_eq!(snippets.len(), 1);
    let id = &snippets[0].id;
    assert_eq!(id.len(), 64);
    assert!(id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
}

/// Snippet IDs are deterministic (same input → same ID).
#[test]
fn test_snippet_ids_are_deterministic() {
    let md = "```bash\necho hello\n```\n";
    let nodes = parse_ast(md);
    let s1 = extract(&nodes, CHUNK_ID).expect("first");
    let s2 = extract(&nodes, CHUNK_ID).expect("second");
    assert_eq!(s1[0].id, s2[0].id);
}

/// The `chunk_id` field on every snippet equals the input `chunk_id`.
#[test]
fn test_snippet_chunk_id_matches_input() {
    let md = "```go\nfmt.Println(\"hi\")\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("should not fail");
    for s in &snippets {
        assert_eq!(s.chunk_id, CHUNK_ID);
    }
}

/// An empty fenced code block must not cause a failure — the composite ID
/// scheme handles empty content gracefully.
#[test]
fn test_empty_fenced_code_block_does_not_fail() {
    let md = "```rust\n```\n";
    let nodes = parse_ast(md);
    let snippets = extract(&nodes, CHUNK_ID).expect("empty code block should not fail");
    assert_eq!(snippets.len(), 1);
    let s = &snippets[0];
    assert_eq!(s.content, "");
    assert_eq!(s.language.as_deref(), Some("rust"));
    assert_eq!(s.id.len(), 64, "id must still be a valid 64-char hex string");
}

/// Two empty code blocks with different languages produce different IDs.
#[test]
fn test_empty_blocks_with_different_langs_have_different_ids() {
    let md_rs = "```rust\n```\n";
    let md_py = "```python\n```\n";
    let id_rs = &extract(&parse_ast(md_rs), CHUNK_ID).unwrap()[0].id;
    let id_py = &extract(&parse_ast(md_py), CHUNK_ID).unwrap()[0].id;
    assert_ne!(
        id_rs,
        id_py,
        "different language tags must produce different IDs for empty blocks"
    );
}
