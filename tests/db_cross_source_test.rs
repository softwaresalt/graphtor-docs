//! Integration tests: cross-source cross-product link resolution.
//!
//! Verifies the two-tier resolver in [`find_related_chunks`]:
//! - Tier 1 intra-source relative links resolve exactly as before (no regression,
//!   no cross-source pollution).
//! - Tier 2 absolute `canonical_url` targets resolve globally to a document in a
//!   different source, re-scoping subsequent hops to that document's home source.
//! - Unmatched cross-product links remain dangling (graceful).

use graphtor_core::db::traverse::find_related_chunks;
use graphtor_core::db::{upsert_chunk, upsert_edge, upsert_url_index, DataStore};
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

/// Acceptance (1): a `/fabric/admin/foo` link from a powerbi chunk resolves to
/// the fabric chunk (different `source_id`) when a matching `canonical_url` exists.
#[test]
fn cross_source_link_resolves_via_canonical_url() {
    let s = store();
    upsert_chunk(&s, "powerbi", &chunk("pb1", "report.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("fab1", "admin/foo.md")).unwrap();
    upsert_url_index(&s, "/fabric/admin/foo", "fab1").unwrap();
    upsert_edge(&s, &edge("pb1", "/fabric/admin/foo")).unwrap();

    let results = find_related_chunks(&s, "pb1", 3).expect("traversal should succeed");

    assert_eq!(results.len(), 1, "should resolve the cross-source target");
    assert_eq!(results[0].chunk_id, "fab1");
    assert_eq!(
        results[0].source_id, "fabric",
        "result carries target source"
    );
    assert_eq!(results[0].depth, 1);
    assert!(
        results[0].cross_source,
        "hop crossed a source boundary (powerbi -> fabric)"
    );
}

/// Acceptance (2): intra-source relative links resolve as before, and identical
/// relative paths across sources are NOT cross-linked.
#[test]
fn intra_source_relative_link_has_no_cross_source_pollution() {
    let s = store();
    upsert_chunk(&s, "a", &chunk("a1", "start.md")).unwrap();
    upsert_chunk(&s, "a", &chunk("a2", "shared.md")).unwrap();
    // A different source also has "shared.md" — must not be picked up.
    upsert_chunk(&s, "b", &chunk("b2", "shared.md")).unwrap();
    upsert_edge(&s, &edge("a1", "shared.md")).unwrap();

    let results = find_related_chunks(&s, "a1", 3).expect("traversal should succeed");

    assert_eq!(
        results.len(),
        1,
        "relative link resolves only within source a"
    );
    assert_eq!(results[0].chunk_id, "a2");
    assert!(
        !results[0].cross_source,
        "intra-source hop is not cross_source"
    );
}

/// Acceptance (3): a cross-product link with no matching `canonical_url` stays
/// dangling — no crash, no false match.
#[test]
fn cross_source_link_without_index_entry_is_dangling() {
    let s = store();
    upsert_chunk(&s, "powerbi", &chunk("pb1", "report.md")).unwrap();
    upsert_edge(&s, &edge("pb1", "/fabric/missing")).unwrap();

    let results = find_related_chunks(&s, "pb1", 3).expect("traversal should succeed");

    assert!(
        results.is_empty(),
        "unmatched absolute target must not resolve"
    );
}

/// Acceptance (4): traversal hops across sources up to `max_depth`, and the hop
/// AFTER a cross-source jump is re-scoped to the target document's home source.
#[test]
fn cross_source_hop_rescopes_to_target_source() {
    let s = store();
    upsert_chunk(&s, "powerbi", &chunk("pb1", "report.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("fab1", "admin/foo.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("fab2", "bar.md")).unwrap();
    // Decoy: powerbi ALSO has "bar.md". The fabric-scoped second hop must not
    // resolve the relative "bar.md" link to the powerbi chunk.
    upsert_chunk(&s, "powerbi", &chunk("pb_bar", "bar.md")).unwrap();

    upsert_url_index(&s, "/fabric/admin/foo", "fab1").unwrap();
    upsert_edge(&s, &edge("pb1", "/fabric/admin/foo")).unwrap();
    upsert_edge(&s, &edge("fab1", "bar.md")).unwrap();

    let results = find_related_chunks(&s, "pb1", 3).expect("traversal should succeed");
    let ids: Vec<&str> = results.iter().map(|r| r.chunk_id.as_str()).collect();

    assert!(ids.contains(&"fab1"), "depth-1 cross-source target");
    assert!(
        ids.contains(&"fab2"),
        "depth-2 target reached via fabric scope"
    );
    assert!(
        !ids.contains(&"pb_bar"),
        "second hop must be scoped to fabric, not the seed's source"
    );

    let fab2 = results.iter().find(|r| r.chunk_id == "fab2").unwrap();
    assert_eq!(fab2.depth, 2);
    assert_eq!(
        fab2.source_id, "fabric",
        "fab2 belongs to the fabric source"
    );
    assert!(
        fab2.cross_source,
        "fab2 is in a different source than the powerbi seed (seed-relative flag)"
    );
}

/// `max_depth` bounds the cross-source traversal: depth 1 reaches only the
/// direct cross-source neighbour, not the chunk one hop further.
#[test]
fn cross_source_respects_max_depth() {
    let s = store();
    upsert_chunk(&s, "powerbi", &chunk("pb1", "report.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("fab1", "admin/foo.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("fab2", "bar.md")).unwrap();
    upsert_url_index(&s, "/fabric/admin/foo", "fab1").unwrap();
    upsert_edge(&s, &edge("pb1", "/fabric/admin/foo")).unwrap();
    upsert_edge(&s, &edge("fab1", "bar.md")).unwrap();

    let results = find_related_chunks(&s, "pb1", 1).expect("traversal should succeed");

    assert_eq!(
        results.len(),
        1,
        "depth 1 reaches only the direct neighbour"
    );
    assert_eq!(results[0].chunk_id, "fab1");
}

/// An absolute target whose `canonical_url` resolves within the *same* source
/// as the current chunk is not flagged as `cross_source`.
#[test]
fn same_source_canonical_resolution_is_not_cross_source() {
    let s = store();
    upsert_chunk(&s, "fabric", &chunk("f1", "a.md")).unwrap();
    upsert_chunk(&s, "fabric", &chunk("f2", "b.md")).unwrap();
    upsert_url_index(&s, "/fabric/b", "f2").unwrap();
    upsert_edge(&s, &edge("f1", "/fabric/b")).unwrap();

    let results = find_related_chunks(&s, "f1", 2).expect("traversal should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk_id, "f2");
    assert!(
        !results[0].cross_source,
        "same-source URL resolution must not be flagged cross_source"
    );
}

/// A broken *relative* link must not fall back to a global `canonical_url`
/// lookup: Tier 2 fires only for absolute targets, so a relative link that
/// coincidentally matches another source's `canonical_url` stays dangling.
#[test]
fn broken_relative_link_does_not_resolve_cross_source() {
    let s = store();
    upsert_chunk(&s, "a", &chunk("a1", "start.md")).unwrap();
    // Another source publishes a canonical_url equal to the relative target.
    upsert_chunk(&s, "b", &chunk("b1", "setup.md")).unwrap();
    upsert_url_index(&s, "setup.md", "b1").unwrap();
    // a1 links to a relative "setup.md" that does NOT exist in source a.
    upsert_edge(&s, &edge("a1", "setup.md")).unwrap();

    let results = find_related_chunks(&s, "a1", 3).expect("traversal should succeed");

    assert!(
        results.is_empty(),
        "a broken relative link must stay dangling, not cross-link via canonical_url"
    );
}
