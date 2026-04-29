//! Integration tests: schema idempotency and relation verification.

use graphtor_core::db::DataStore;

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

#[test]
fn ensure_schema_is_idempotent() {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().expect("first call should succeed");
    s.ensure_schema()
        .expect("second call should succeed (idempotent)");
    s.ensure_schema().expect("third call should succeed");
}

#[test]
fn all_expected_relations_exist_after_schema() {
    let s = store();
    let names = s
        .relation_names()
        .expect("relation_names query should succeed");
    for expected in &[
        "doc_sources",
        "doc_chunks",
        "doc_edges",
        "doc_code",
        "doc_schema_ver",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "relation '{expected}' should exist; found: {names:?}"
        );
    }
}
