//! Integration tests: `DataStore::get_status` and `DbStatus`.

use graphtor_core::db::{upsert_chunk, upsert_source, DataStore, SourceRecord};
use graphtor_core::parse::types::Chunk;

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn sample_source(id: &str) -> SourceRecord {
    SourceRecord {
        source_id: id.to_owned(),
        url: format!("https://github.com/test/{id}"),
        kind: "git".to_owned(),
        name: id.to_owned(),
        synced_at: None,
    }
}

fn sample_chunk(id: &str, path: &str) -> Chunk {
    Chunk {
        chunk_id: id.to_owned(),
        content: format!("Content of {id}"),
        heading_hierarchy: vec![],
        position: 0,
        char_offset: 0,
        source_path: path.to_owned(),
    }
}

#[test]
fn get_status_on_empty_store_returns_zero_counts() {
    let s = store();
    let status = s.get_status().expect("get_status should succeed");
    assert_eq!(status.source_count, 0, "expected 0 sources on empty store");
    assert_eq!(status.chunk_count, 0, "expected 0 chunks on empty store");
    // Schema version must reflect the current SCHEMA_VERSION constant (3).
    assert_eq!(
        status.schema_version, 3,
        "schema version should be 3 after ensure_schema"
    );
}

#[test]
fn get_status_reflects_inserted_sources() {
    let s = store();
    upsert_source(&s, &sample_source("src-001")).unwrap();
    upsert_source(&s, &sample_source("src-002")).unwrap();

    let status = s.get_status().expect("get_status should succeed");
    assert_eq!(status.source_count, 2, "expected 2 sources");
}

#[test]
fn get_status_reflects_inserted_chunks() {
    let s = store();
    upsert_chunk(&s, "src-001", &sample_chunk("c-1", "docs/a.md")).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-2", "docs/b.md")).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-3", "docs/c.md")).unwrap();

    let status = s.get_status().expect("get_status should succeed");
    assert_eq!(status.chunk_count, 3, "expected 3 chunks");
}

#[test]
fn get_status_reflects_combined_sources_and_chunks() {
    let s = store();
    upsert_source(&s, &sample_source("src-001")).unwrap();
    upsert_source(&s, &sample_source("src-002")).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-1", "docs/a.md")).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-2", "docs/b.md")).unwrap();
    upsert_chunk(&s, "src-001", &sample_chunk("c-3", "docs/c.md")).unwrap();

    let status = s.get_status().expect("get_status should succeed");
    assert_eq!(status.source_count, 2);
    assert_eq!(status.chunk_count, 3);
    assert_eq!(status.schema_version, 3);
}
