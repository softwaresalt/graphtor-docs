//! Integration tests: graph traversal (`find_related_chunks`).

use graphtor_core::db::traverse::find_related_chunks;
use graphtor_core::db::{upsert_chunk, upsert_edge, DataStore};
use graphtor_core::parse::types::{Chunk, Reference};

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn chunk(id: &str, path: &str) -> Chunk {
    Chunk {
        chunk_id: id.to_owned(),
        content: format!("Content of {id}"),
        heading_hierarchy: vec![],
        position: 0,
        char_offset: 0,
        source_path: path.to_owned(),
    }
}

fn edge(src: &str, target_path: &str) -> Reference {
    Reference {
        source_chunk_id: src.to_owned(),
        target_path: target_path.to_owned(),
        link_text: "link".to_owned(),
        anchor: None,
    }
}

/// Build a small graph: A → B → C (linear chain).
fn build_chain(s: &DataStore) {
    upsert_chunk(s, "src", &chunk("chunk-a", "a.md")).unwrap();
    upsert_chunk(s, "src", &chunk("chunk-b", "b.md")).unwrap();
    upsert_chunk(s, "src", &chunk("chunk-c", "c.md")).unwrap();
    upsert_edge(s, &edge("chunk-a", "b.md")).unwrap();
    upsert_edge(s, &edge("chunk-b", "c.md")).unwrap();
}

#[test]
fn single_hop_traversal() {
    let s = store();
    build_chain(&s);
    let results = find_related_chunks(&s, "chunk-a", 1).expect("traversal should succeed");
    assert_eq!(results.len(), 1, "should find only direct neighbour");
    assert_eq!(results[0].chunk_id, "chunk-b");
    assert_eq!(results[0].depth, 1);
}

#[test]
fn two_hop_traversal() {
    let s = store();
    build_chain(&s);
    let results = find_related_chunks(&s, "chunk-a", 2).expect("traversal should succeed");
    let ids: Vec<&str> = results.iter().map(|r| r.chunk_id.as_str()).collect();
    assert!(ids.contains(&"chunk-b"), "should include depth-1 node");
    assert!(ids.contains(&"chunk-c"), "should include depth-2 node");
    assert_eq!(results.len(), 2);
    let b = results.iter().find(|r| r.chunk_id == "chunk-b").unwrap();
    let c = results.iter().find(|r| r.chunk_id == "chunk-c").unwrap();
    assert_eq!(b.depth, 1);
    assert_eq!(c.depth, 2);
}

#[test]
fn traversal_returns_empty_for_isolated_chunk() {
    let s = store();
    upsert_chunk(&s, "src", &chunk("isolated", "iso.md")).unwrap();
    let results = find_related_chunks(&s, "isolated", 3).expect("traversal should succeed");
    assert!(
        results.is_empty(),
        "isolated chunk should have no related chunks"
    );
}

#[test]
fn traversal_seed_not_included_in_results() {
    let s = store();
    build_chain(&s);
    let results = find_related_chunks(&s, "chunk-a", 5).unwrap();
    assert!(
        !results.iter().any(|r| r.chunk_id == "chunk-a"),
        "seed chunk should not appear in results"
    );
}

#[test]
fn traversal_respects_max_depth_zero() {
    let s = store();
    build_chain(&s);
    let results = find_related_chunks(&s, "chunk-a", 0).expect("traversal should succeed");
    assert!(results.is_empty(), "depth=0 should return no results");
}

#[test]
fn traversal_cycle_does_not_loop_forever() {
    let s = store();
    // Build a cycle: A → B → A
    upsert_chunk(&s, "src", &chunk("cyc-a", "ca.md")).unwrap();
    upsert_chunk(&s, "src", &chunk("cyc-b", "cb.md")).unwrap();
    upsert_edge(&s, &edge("cyc-a", "cb.md")).unwrap();
    upsert_edge(&s, &edge("cyc-b", "ca.md")).unwrap();
    let results = find_related_chunks(&s, "cyc-a", 10).expect("should not loop indefinitely");
    // Should contain cyc-b once, but NOT cyc-a (seed is excluded/visited).
    let ids: Vec<&str> = results.iter().map(|r| r.chunk_id.as_str()).collect();
    assert!(ids.contains(&"cyc-b"));
    assert!(!ids.contains(&"cyc-a"), "cycle should not re-visit seed");
}
