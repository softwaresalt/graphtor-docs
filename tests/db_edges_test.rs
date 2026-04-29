//! Integration tests: edges and code-snippet operations.

use graphtor_core::db::{list_edges_from_chunk, upsert_code_snippet, upsert_edge, DataStore};
use graphtor_core::parse::types::{CodeSnippet, Reference};

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn sample_ref(src: &str, target: &str) -> Reference {
    Reference {
        source_chunk_id: src.to_owned(),
        target_path: target.to_owned(),
        link_text: format!("link to {target}"),
        anchor: None,
    }
}

fn sample_snippet(id: &str, chunk_id: &str) -> CodeSnippet {
    CodeSnippet {
        id: id.to_owned(),
        chunk_id: chunk_id.to_owned(),
        language: Some("rust".to_owned()),
        content: "fn main() {}".to_owned(),
    }
}

#[test]
fn upsert_and_list_edges_round_trip() {
    let s = store();
    let r = sample_ref("chunk-001", "docs/other.md");
    upsert_edge(&s, &r).expect("upsert_edge should succeed");

    let edges = list_edges_from_chunk(&s, "chunk-001").expect("list_edges should succeed");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target_path, "docs/other.md");
    assert_eq!(edges[0].link_text, r.link_text);
    assert_eq!(edges[0].anchor, None);
}

#[test]
fn upsert_edge_with_anchor() {
    let s = store();
    let r = Reference {
        source_chunk_id: "chunk-002".to_owned(),
        target_path: "docs/guide.md".to_owned(),
        link_text: "guide section".to_owned(),
        anchor: Some("#section-1".to_owned()),
    };
    upsert_edge(&s, &r).unwrap();
    let edges = list_edges_from_chunk(&s, "chunk-002").unwrap();
    assert_eq!(edges[0].anchor, Some("#section-1".to_owned()));
}

#[test]
fn list_edges_returns_empty_for_unknown_chunk() {
    let s = store();
    let edges = list_edges_from_chunk(&s, "no-such-chunk").expect("list_edges should succeed");
    assert!(edges.is_empty());
}

#[test]
fn multiple_edges_from_same_chunk() {
    let s = store();
    for i in 0..5 {
        upsert_edge(&s, &sample_ref("chunk-multi", &format!("docs/page-{i}.md"))).unwrap();
    }
    let edges = list_edges_from_chunk(&s, "chunk-multi").unwrap();
    assert_eq!(edges.len(), 5);
}

#[test]
fn upsert_code_snippet_round_trip() {
    let s = store();
    let snippet = sample_snippet("snip-001", "chunk-001");
    upsert_code_snippet(&s, &snippet).expect("upsert_code_snippet should succeed");
    // There is no get_code_snippet in the public API; verify by checking no error was returned.
}

#[test]
fn upsert_code_snippet_is_idempotent() {
    let s = store();
    let snippet = sample_snippet("snip-002", "chunk-001");
    upsert_code_snippet(&s, &snippet).unwrap();
    upsert_code_snippet(&s, &snippet).expect("second upsert_code_snippet should succeed");
}
