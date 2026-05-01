//! Integration tests: `list_chunks_by_path`.

use graphtor_core::db::{list_chunks_by_path, upsert_chunk, DataStore};
use graphtor_core::parse::types::Chunk;

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn sample_chunk(id: &str, path: &str, pos: usize) -> Chunk {
    Chunk {
        chunk_id: id.to_owned(),
        content: format!("Content of chunk {id}"),
        heading_hierarchy: vec!["H1 Title".to_owned()],
        position: pos,
        char_offset: pos * 100,
        source_path: path.to_owned(),
    }
}

#[test]
fn list_chunks_by_path_returns_matching_chunks() {
    let s = store();
    upsert_chunk(&s, "src-001", &sample_chunk("a-0", "docs/guide.md", 0)).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("a-1", "docs/guide.md", 1)).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("b-0", "docs/other.md", 0)).unwrap();

    let results = list_chunks_by_path(&s, "docs/guide.md").expect("list should succeed");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|c| c.path == "docs/guide.md"));
}

#[test]
fn list_chunks_by_path_returns_empty_for_unknown_path() {
    let s = store();
    let results = list_chunks_by_path(&s, "no/such/path.md").expect("list should succeed");
    assert!(results.is_empty());
}

#[test]
fn list_chunks_by_path_returns_chunks_ordered_by_position() {
    let s = store();
    // Insert out of order to confirm sort is applied.
    upsert_chunk(&s, "src-001", &sample_chunk("c-2", "docs/sorted.md", 2)).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-0", "docs/sorted.md", 0)).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-1", "docs/sorted.md", 1)).unwrap();

    let results = list_chunks_by_path(&s, "docs/sorted.md").expect("list should succeed");
    assert_eq!(results.len(), 3);
    let positions: Vec<usize> = results.iter().map(|c| c.position).collect();
    assert_eq!(
        positions,
        vec![0, 1, 2],
        "chunks must be ordered by position"
    );
}

#[test]
fn list_chunks_by_path_returns_chunks_from_multiple_sources() {
    // The same document path can exist in different sources.
    let s = store();
    upsert_chunk(&s, "src-a", &sample_chunk("x-0", "shared/readme.md", 0)).unwrap();
    upsert_chunk(&s, "src-b", &sample_chunk("x-1", "shared/readme.md", 0)).unwrap();

    let results = list_chunks_by_path(&s, "shared/readme.md").expect("list should succeed");
    assert_eq!(results.len(), 2);
}
