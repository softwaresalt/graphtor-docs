//! Integration tests for the `doc_vectors` vector storage and cosine-similarity search.
//!
//! These tests exercise [`graphtor_core::db::vectors`] functions directly using
//! fake 4-dimensional embeddings — no real ML model is loaded.

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

/// A minimal chunk record so the vector join in `search_by_vector` can resolve metadata.
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
    let embedding: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4];
    upsert_vector(&s, "c001", &embedding).expect("upsert should succeed");

    let retrieved = get_vector(&s, "c001")
        .expect("get should succeed")
        .expect("vector should exist");

    assert_eq!(retrieved.len(), embedding.len());
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
    let v1 = vec![1.0_f32, 0.0, 0.0, 0.0];
    let v2 = vec![0.0_f32, 1.0, 0.0, 0.0];

    upsert_vector(&s, "c-idem", &v1).expect("first upsert");
    upsert_vector(&s, "c-idem", &v2).expect("second upsert should overwrite");

    let retrieved = get_vector(&s, "c-idem")
        .expect("get")
        .expect("should exist");

    // Second write must win.
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

// ── T016.004: search_by_vector returns top-k by cosine similarity ─────────────

#[test]
fn search_by_vector_returns_nearest_first() {
    let s = store();

    // Orthonormal basis vectors in 4-D.
    // chunk-a: [1, 0, 0, 0]
    // chunk-b: [0, 1, 0, 0]  — cosine sim to query [0.9, 0.1, 0, 0] is lower
    // chunk-c: [0.9, 0.436, 0, 0] (roughly 26° from [1,0,0,0])
    insert_chunk(&s, "chunk-a", "docs/a.md");
    insert_chunk(&s, "chunk-b", "docs/b.md");
    insert_chunk(&s, "chunk-c", "docs/c.md");

    upsert_vector(&s, "chunk-a", &[1.0, 0.0, 0.0, 0.0]).expect("upsert a");
    upsert_vector(&s, "chunk-b", &[0.0, 1.0, 0.0, 0.0]).expect("upsert b");
    upsert_vector(&s, "chunk-c", &[0.9, 0.436, 0.0, 0.0]).expect("upsert c");

    // Query close to chunk-a.
    let query = vec![1.0_f32, 0.0, 0.0, 0.0];
    let results = search_by_vector(&s, &query, 3).expect("search should succeed");

    assert!(!results.is_empty(), "expected at least one result");
    // chunk-a must be the top result (cosine sim = 1.0).
    assert_eq!(
        results[0].chunk_id, "chunk-a",
        "chunk-a should be the top result"
    );
    // chunk-b (orthogonal) should have lower similarity than chunk-c.
    let b_rank = results.iter().position(|r| r.chunk_id == "chunk-b");
    let c_rank = results.iter().position(|r| r.chunk_id == "chunk-c");
    if let (Some(br), Some(cr)) = (b_rank, c_rank) {
        assert!(cr < br, "chunk-c should rank above chunk-b");
    }
}

// ── T016.005: search_by_vector respects limit ─────────────────────────────────

#[test]
fn search_by_vector_respects_limit() {
    let s = store();
    for i in 0..5_u32 {
        let id = format!("chunk-{i}");
        let path = format!("docs/{i}.md");
        insert_chunk(&s, &id, &path);
        #[allow(clippy::cast_precision_loss)]
        let v = vec![i as f32, 0.0, 0.0, 0.0];
        upsert_vector(&s, &id, &v).expect("upsert");
    }

    let query = vec![1.0_f32, 0.0, 0.0, 0.0];
    let results = search_by_vector(&s, &query, 2).expect("search");
    assert_eq!(results.len(), 2, "limit should be respected");
}

// ── T016.006: search_by_vector on empty store ─────────────────────────────────

#[test]
fn search_by_vector_returns_empty_on_empty_store() {
    let s = store();
    let query = vec![1.0_f32, 0.0, 0.0, 0.0];
    let results = search_by_vector(&s, &query, 10).expect("search");
    assert!(results.is_empty());
}

// ── T016.007: delete_vectors_by_chunk_ids ─────────────────────────────────────

#[test]
fn delete_vectors_removes_stored_embedding() {
    let s = store();
    upsert_vector(&s, "del-c1", &[1.0, 0.0, 0.0, 0.0]).expect("upsert");
    upsert_vector(&s, "del-c2", &[0.0, 1.0, 0.0, 0.0]).expect("upsert");

    delete_vectors_by_chunk_ids(&s, &["del-c1".to_owned()]).expect("delete should succeed");

    assert!(
        get_vector(&s, "del-c1").expect("get").is_none(),
        "del-c1 should be gone"
    );
    assert!(
        get_vector(&s, "del-c2").expect("get").is_some(),
        "del-c2 should survive"
    );
}

// ── T016.008: delete empty list is a no-op ────────────────────────────────────

#[test]
fn delete_vectors_empty_list_is_noop() {
    let s = store();
    let count = delete_vectors_by_chunk_ids(&s, &[]).expect("should succeed");
    assert_eq!(count, 0);
}
