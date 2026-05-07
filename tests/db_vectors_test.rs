//! Integration tests for HNSW vector search via `doc_chunks`.
//!
//! These tests exercise [`graphtor_core::db::vectors`] functions directly
//! using synthetic 384-dimensional unit vectors — no real ML model is loaded.
//! All vectors must be 384-dim to match the HNSW index dimensionality.

use graphtor_core::db::{
    upsert_chunk,
    vectors::{delete_vectors_by_chunk_ids, get_vector, search_by_vector, upsert_vector},
    DataStore,
};
use graphtor_core::parse::types::Chunk;

/// Open an in-memory store with schema applied.
fn store() -> DataStore {
    let s = DataStore::open_mem().expect("in-memory store");
    s.ensure_schema().expect("schema");
    s
}

/// Build a 384-dimensional unit vector with `1.0` at `pos` and `0.0` elsewhere.
fn unit_vec(pos: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 384];
    v[pos] = 1.0;
    v
}

/// Insert a minimal chunk so the HNSW join-put can find the row.
///
/// `upsert_vector` requires the chunk to exist in `doc_chunks` first.
fn insert_chunk(store: &DataStore, chunk_id: &str, path: &str) {
    let chunk = Chunk {
        chunk_id: chunk_id.to_owned(),
        content: format!("content for {chunk_id}"),
        heading_hierarchy: vec!["Heading".to_owned()],
        position: 0,
        char_offset: 0,
        source_path: path.to_owned(),
    };
    upsert_chunk(store, "test-src", &chunk).expect("chunk upsert");
}

// ── T016.001: upsert and round-trip ──────────────────────────────────────────

#[test]
fn upsert_and_get_vector_round_trip() {
    let s = store();
    insert_chunk(&s, "c001", "docs/c001.md");
    let embedding = unit_vec(0);
    upsert_vector(&s, "c001", &embedding).expect("upsert should succeed");

    let retrieved = get_vector(&s, "c001")
        .expect("get should succeed")
        .expect("vector should exist");

    assert_eq!(retrieved.len(), 384);
    for (got, want) in retrieved.iter().zip(embedding.iter()) {
        assert!(
            (got - want).abs() < 1e-6_f32,
            "float mismatch: got {got}, want {want}"
        );
    }
}

// ── T016.002: upsert is idempotent ────────────────────────────────────────────

#[test]
fn upsert_vector_is_idempotent() {
    let s = store();
    insert_chunk(&s, "c-idem", "docs/idem.md");
    let v1 = unit_vec(0);
    let v2 = unit_vec(1);

    upsert_vector(&s, "c-idem", &v1).expect("first upsert");
    upsert_vector(&s, "c-idem", &v2).expect("second upsert should overwrite");

    let retrieved = get_vector(&s, "c-idem")
        .expect("get")
        .expect("should exist");

    assert_eq!(retrieved.len(), 384);
    // Second write must win: dim-0 = 0.0, dim-1 = 1.0.
    assert!(
        (retrieved[0] - 0.0_f32).abs() < 1e-6,
        "expected 0.0 in dim 0"
    );
    assert!(
        (retrieved[1] - 1.0_f32).abs() < 1e-6,
        "expected 1.0 in dim 1"
    );
}

// ── T016.003: get_vector returns None for unknown chunk ───────────────────────

#[test]
fn get_vector_returns_none_for_missing_chunk() {
    let s = store();
    let result = get_vector(&s, "no-such-chunk").expect("get should not error");
    assert!(result.is_none());
}

// ── T016.004: search_by_vector returns top-k by HNSW similarity ──────────────

#[test]
fn search_by_vector_returns_nearest_first() {
    let s = store();

    // Three orthogonal unit vectors in 384-D.
    insert_chunk(&s, "chunk-a", "docs/a.md");
    insert_chunk(&s, "chunk-b", "docs/b.md");
    insert_chunk(&s, "chunk-c", "docs/c.md");

    upsert_vector(&s, "chunk-a", &unit_vec(0)).expect("upsert a");
    upsert_vector(&s, "chunk-b", &unit_vec(1)).expect("upsert b");
    upsert_vector(&s, "chunk-c", &unit_vec(2)).expect("upsert c");

    // Query exactly matches chunk-a.
    let results = search_by_vector(&s, &unit_vec(0), 3).expect("search should succeed");

    assert!(!results.is_empty(), "expected at least one result");
    assert_eq!(
        results[0].chunk_id, "chunk-a",
        "chunk-a should be the top result"
    );
}

// ── T016.005: search_by_vector respects limit ─────────────────────────────────

#[test]
fn search_by_vector_respects_limit() {
    let s = store();
    for i in 0..5_usize {
        let id = format!("chunk-{i}");
        let path = format!("docs/{i}.md");
        insert_chunk(&s, &id, &path);
        upsert_vector(&s, &id, &unit_vec(i)).expect("upsert");
    }

    let results = search_by_vector(&s, &unit_vec(0), 2).expect("search");
    assert_eq!(results.len(), 2, "limit should be respected");
}

// ── T016.006: search_by_vector on empty store ─────────────────────────────────

#[test]
fn search_by_vector_returns_empty_on_empty_store() {
    let s = store();
    let results = search_by_vector(&s, &unit_vec(0), 10).expect("search");
    assert!(results.is_empty());
}

// ── T016.007: delete_vectors_by_chunk_ids ─────────────────────────────────────

#[test]
fn delete_vectors_removes_stored_embedding() {
    let s = store();
    insert_chunk(&s, "del-c1", "docs/del1.md");
    insert_chunk(&s, "del-c2", "docs/del2.md");
    upsert_vector(&s, "del-c1", &unit_vec(0)).expect("upsert c1");
    upsert_vector(&s, "del-c2", &unit_vec(1)).expect("upsert c2");

    delete_vectors_by_chunk_ids(&s, &["del-c1".to_owned()]).expect("delete should succeed");

    assert!(
        get_vector(&s, "del-c1").expect("get").is_none(),
        "del-c1 embedding should be null after delete"
    );
    assert!(
        get_vector(&s, "del-c2").expect("get").is_some(),
        "del-c2 should still have its embedding"
    );
}

// ── T016.008: delete empty list is a no-op ────────────────────────────────────

#[test]
fn delete_vectors_empty_list_is_noop() {
    let s = store();
    let count = delete_vectors_by_chunk_ids(&s, &[]).expect("should succeed");
    assert_eq!(count, 0);
}
