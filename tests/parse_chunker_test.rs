//! Tests for heading-based document chunker — 003.002-T.

use graphtor_core::parse::{ast::parse_ast, chunker::chunk};

const SRC: &str = "docs/guide.md";

/// Document with no headings produces a single intro chunk.
#[test]
fn test_no_headings_produces_single_intro_chunk() {
    let md = "Some intro text.\n\nMore text here.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    assert_eq!(chunks.len(), 1);
    assert!(
        chunks[0].heading_hierarchy.is_empty(),
        "intro chunk has no hierarchy"
    );
    assert_eq!(chunks[0].source_path, SRC);
    assert_eq!(chunks[0].position, 0);
}

/// H2 headings split the document into separate chunks.
#[test]
fn test_h2_headings_split_into_chunks() {
    let md = "# Doc Title\n\nIntro paragraph.\n\n## Section One\n\nContent A.\n\n## Section Two\n\nContent B.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    // Expect: intro (before first H2), Section One, Section Two
    assert_eq!(chunks.len(), 3, "expected 3 chunks, got {}", chunks.len());
    assert!(chunks[1].content.contains("Section One"));
    assert!(chunks[2].content.contains("Section Two"));
}

/// H3 headings within an H2 section start new chunks.
#[test]
fn test_h3_headings_also_split() {
    let md = "## Overview\n\nIntro.\n\n### Details\n\nDetail text.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].content.contains("Overview"));
    assert!(chunks[1].content.contains("Details"));
}

/// H4+ headings are folded into the enclosing H2/H3 chunk, not split.
#[test]
fn test_h4_and_deeper_folded_into_parent_chunk() {
    let md = "## Section\n\nIntro.\n\n#### Deep Heading\n\nDeep content.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    // H4 should NOT split — everything stays in one chunk.
    assert_eq!(
        chunks.len(),
        1,
        "H4 should not split: got {} chunks",
        chunks.len()
    );
    assert!(chunks[0].content.contains("Deep Heading"));
    assert!(chunks[0].content.contains("Deep content"));
}

/// Each chunk carries a stable `chunk_id` (64 hex chars).
#[test]
fn test_chunks_have_stable_ids() {
    let md = "## Alpha\n\nContent.\n\n## Beta\n\nMore.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    for c in &chunks {
        assert_eq!(c.chunk_id.len(), 64, "chunk_id must be 64 hex chars");
        assert!(
            c.chunk_id
                .chars()
                .all(|ch| matches!(ch, '0'..='9' | 'a'..='f')),
            "chunk_id must be lowercase hex: {}",
            c.chunk_id
        );
    }
}

/// Chunk IDs are deterministic: same input always produces same IDs.
#[test]
fn test_chunk_ids_are_deterministic() {
    let md = "## Section\n\nContent.\n";
    let nodes = parse_ast(md);
    let chunks1 = chunk(&nodes, SRC).expect("first call");
    let chunks2 = chunk(&nodes, SRC).expect("second call");
    assert_eq!(
        chunks1[0].chunk_id, chunks2[0].chunk_id,
        "chunk_id must be deterministic"
    );
}

/// Heading hierarchy is recorded correctly on each chunk.
#[test]
fn test_heading_hierarchy_populated() {
    let md = "# Doc\n\n## Section\n\nText.\n\n### Sub\n\nMore.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    // Chunk at H2 "Section": hierarchy = [Doc, Section]
    let section = chunks
        .iter()
        .find(|c| c.content.contains("## Section"))
        .unwrap();
    assert!(section.heading_hierarchy.contains(&"Doc".to_string()));
    assert!(section.heading_hierarchy.contains(&"Section".to_string()));
    // Chunk at H3 "Sub": hierarchy = [Doc, Section, Sub]
    let sub = chunks
        .iter()
        .find(|c| c.content.contains("### Sub"))
        .unwrap();
    assert!(sub.heading_hierarchy.contains(&"Sub".to_string()));
}

/// Position counter increments with each chunk.
#[test]
fn test_chunk_positions_are_ordered() {
    let md = "## A\n\nContent A.\n\n## B\n\nContent B.\n\n## C\n\nContent C.\n";
    let nodes = parse_ast(md);
    let chunks = chunk(&nodes, SRC).expect("chunk should not fail");
    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].position < chunks[1].position);
    assert!(chunks[1].position < chunks[2].position);
}
