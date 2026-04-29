//! Integration tests: `SourceRecord` CRUD (`upsert_source`, `get_source`, `list_sources`).

use graphtor_core::db::{get_source, list_sources, upsert_source, DataStore, SourceRecord};

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn sample_source(id: &str) -> SourceRecord {
    SourceRecord {
        source_id: id.to_owned(),
        url: format!("https://github.com/example/{id}"),
        kind: "git".to_owned(),
        name: format!("Example {id}"),
        synced_at: None,
    }
}

#[test]
fn upsert_and_get_source_round_trip() {
    let s = store();
    let rec = sample_source("src-001");
    upsert_source(&s, &rec).expect("upsert should succeed");

    let retrieved = get_source(&s, "src-001")
        .expect("get should succeed")
        .expect("record should exist");
    assert_eq!(retrieved.source_id, rec.source_id);
    assert_eq!(retrieved.url, rec.url);
    assert_eq!(retrieved.kind, rec.kind);
    assert_eq!(retrieved.name, rec.name);
    assert_eq!(retrieved.synced_at, rec.synced_at);
}

#[test]
fn get_source_returns_none_for_missing() {
    let s = store();
    let result = get_source(&s, "nonexistent").expect("get should succeed");
    assert!(result.is_none(), "expected None for missing source");
}

#[test]
fn upsert_source_overwrites_existing() {
    let s = store();
    let rec = sample_source("src-002");
    upsert_source(&s, &rec).unwrap();

    let updated = SourceRecord {
        synced_at: Some("2024-01-01T00:00:00Z".to_owned()),
        ..rec.clone()
    };
    upsert_source(&s, &updated).expect("second upsert should succeed");

    let retrieved = get_source(&s, "src-002").unwrap().unwrap();
    assert_eq!(retrieved.synced_at, updated.synced_at);
}

#[test]
fn list_sources_returns_all_records() {
    let s = store();
    for i in 0..3 {
        upsert_source(&s, &sample_source(&format!("src-{i:03}"))).unwrap();
    }
    let all = list_sources(&s).expect("list should succeed");
    assert_eq!(all.len(), 3, "expected exactly 3 sources");
}

#[test]
fn list_sources_empty_when_no_sources() {
    let s = store();
    let all = list_sources(&s).expect("list should succeed on empty store");
    assert!(all.is_empty());
}
