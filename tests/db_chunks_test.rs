//! Integration tests: chunk CRUD (`upsert_chunk`, `get_chunk`, `list_chunks_for_source`).

use graphtor_core::db::{
    get_chunk, list_chunks_for_source, upsert_chunk,
    vectors::{get_vector, upsert_vector},
    DataStore,
};
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
        heading_hierarchy: vec!["H1 Title".to_owned(), "H2 Section".to_owned()],
        position: pos,
        char_offset: pos * 100,
        source_path: path.to_owned(),
    }
}

#[test]
fn upsert_and_get_chunk_round_trip() {
    let s = store();
    let chunk = sample_chunk("chunk-001", "docs/index.md", 0);
    upsert_chunk(&s, "src-001", &chunk).expect("upsert should succeed");

    let retrieved = get_chunk(&s, "chunk-001")
        .expect("get should succeed")
        .expect("chunk should exist");
    assert_eq!(retrieved.chunk_id, chunk.chunk_id);
    assert_eq!(retrieved.source_id, "src-001");
    assert_eq!(retrieved.path, chunk.source_path);
    assert_eq!(retrieved.position, chunk.position);
    assert_eq!(retrieved.char_offset, chunk.char_offset);
    assert_eq!(retrieved.heading_hierarchy, chunk.heading_hierarchy);
    assert_eq!(retrieved.content, chunk.content);
}

#[test]
fn get_chunk_returns_none_for_missing() {
    let s = store();
    let result = get_chunk(&s, "no-such-chunk").expect("get should succeed");
    assert!(result.is_none());
}

#[test]
fn upsert_chunk_is_idempotent() {
    let s = store();
    let chunk = sample_chunk("chunk-002", "docs/guide.md", 1);
    upsert_chunk(&s, "src-001", &chunk).unwrap();
    upsert_chunk(&s, "src-001", &chunk).expect("second upsert should succeed without error");
    let all = list_chunks_for_source(&s, "src-001").unwrap();
    assert_eq!(
        all.len(),
        1,
        "idempotent upsert should not create duplicates"
    );
}

#[test]
fn list_chunks_for_source_returns_only_matching_source() {
    let s = store();
    for i in 0..3 {
        upsert_chunk(&s, "src-a", &sample_chunk(&format!("a-{i}"), "a.md", i)).unwrap();
    }
    upsert_chunk(&s, "src-b", &sample_chunk("b-0", "b.md", 0)).unwrap();

    let a_chunks = list_chunks_for_source(&s, "src-a").expect("list should succeed");
    assert_eq!(a_chunks.len(), 3);

    let b_chunks = list_chunks_for_source(&s, "src-b").expect("list should succeed");
    assert_eq!(b_chunks.len(), 1);
}

#[test]
fn upsert_chunk_preserves_existing_embedding() {
    let s = store();
    let chunk = sample_chunk("embed-preserve", "docs/preserve.md", 0);

    // Step 1: insert chunk metadata
    upsert_chunk(&s, "src-001", &chunk).expect("initial upsert");

    // Step 2: store a synthetic 384-dim embedding
    let mut embedding = vec![0.0_f32; 384];
    embedding[0] = 0.1;
    upsert_vector(&s, "embed-preserve", &embedding).expect("upsert_vector");

    // Step 3: re-upsert the same chunk metadata (simulates model=None re-sync)
    upsert_chunk(&s, "src-001", &chunk).expect("re-upsert should succeed");

    // Step 4: embedding must still be present
    let retrieved = get_vector(&s, "embed-preserve")
        .expect("get_vector should not error")
        .expect("embedding should survive chunk re-upsert");
    assert!(
        (retrieved[0] - 0.1_f32).abs() < 1e-6,
        "embedding value should be preserved after re-upsert of chunk metadata"
    );
}

#[test]
fn list_chunks_for_source_empty_when_no_chunks() {
    let s = store();
    let result = list_chunks_for_source(&s, "src-unknown").expect("list should succeed");
    assert!(result.is_empty());
}
