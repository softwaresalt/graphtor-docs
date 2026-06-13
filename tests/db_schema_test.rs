//! Integration tests: schema idempotency and relation verification.

use graphtor_core::db::{upsert_source, DataStore, SourceRecord};

fn store() -> DataStore {
    let s = DataStore::open_mem().unwrap();
    s.ensure_schema().unwrap();
    s
}

fn sample_source(id: &str) -> SourceRecord {
    SourceRecord {
        source_id: id.to_owned(),
        url: format!("https://example.com/{id}"),
        kind: "local".to_owned(),
        name: id.to_owned(),
        synced_at: None,
    }
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
        "doc_v4_migration_snapshot_lock",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "relation '{expected}' should exist; found: {names:?}"
        );
    }
}

#[test]
fn hnsw_index_exists_after_schema() {
    let s = store();
    let names = s
        .relation_names()
        .expect("relation_names query should succeed");
    assert!(
        names.iter().any(|n| n == "doc_chunks:embedding_idx"),
        "HNSW index 'doc_chunks:embedding_idx' should exist; found: {names:?}"
    );
}

// ── Migration safety regression tests ─────────────────────────────────────────

/// `ensure_schema` must NOT prune existing data when the schema version is
/// below 4.  Data should survive repeated `ensure_schema` calls.
#[test]
fn ensure_schema_on_pre_v4_does_not_prune_data() {
    let store = DataStore::open_mem().unwrap();
    // Start with a fully initialised schema.
    store.ensure_schema().unwrap();
    // Simulate a pre-v4 database by forcing the version back to 3.
    store.set_schema_version_for_test(3).unwrap();
    // Insert a record to represent existing pre-v4 index data.
    upsert_source(&store, &sample_source("legacy-source")).unwrap();

    // Re-run ensure_schema as it would be called on the next open.
    store
        .ensure_schema()
        .expect("ensure_schema on pre-v4 DB should succeed");

    // The source record must still be present — no data prune should occur.
    let sources = graphtor_core::db::list_sources(&store)
        .expect("listing sources should succeed after ensure_schema");
    assert_eq!(
        sources.len(),
        1,
        "pre-v4 data must not be pruned by ensure_schema (expected 1 source, got {})",
        sources.len()
    );
}

/// `needs_v4_migration` returns `true` when the schema version is below 4.
#[test]
fn needs_v4_migration_returns_true_for_pre_v4_db() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    store.set_schema_version_for_test(3).unwrap();
    assert!(
        store
            .needs_v4_migration()
            .expect("needs_v4_migration should succeed"),
        "should detect pre-v4 DB as needing migration"
    );
}

/// `needs_v4_migration` returns `false` for a freshly initialised v4 database.
#[test]
fn needs_v4_migration_returns_false_for_v4_db() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    assert!(
        !store
            .needs_v4_migration()
            .expect("needs_v4_migration should succeed"),
        "v4 DB should not require migration"
    );
}

/// `needs_v4_migration` returns `false` for a completely uninitialised store
/// (i.e. `ensure_schema` has never been called).
#[test]
fn needs_v4_migration_returns_false_for_fresh_uninitialised_db() {
    // Open a raw store without calling ensure_schema.
    let store = DataStore::open_mem().unwrap();
    assert!(
        !store
            .needs_v4_migration()
            .expect("needs_v4_migration should succeed on uninitialised store"),
        "uninitialised store should not report migration needed"
    );
}

/// `apply_v4_prune` clears all pre-v4 index data and stamps the schema as v4.
#[test]
fn apply_v4_prune_clears_data_and_stamps_v4() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    store.set_schema_version_for_test(3).unwrap();
    upsert_source(&store, &sample_source("pre-v4-source")).unwrap();

    // Pre-condition: migration is required and data is present.
    assert!(
        store.needs_v4_migration().unwrap(),
        "pre-condition: DB should need v4 migration"
    );
    let sources_before = graphtor_core::db::list_sources(&store).unwrap();
    assert_eq!(
        sources_before.len(),
        1,
        "one source should exist before prune"
    );

    // Act.
    store
        .apply_v4_prune()
        .expect("apply_v4_prune should succeed");

    // Schema version must now be 4.
    let status = store.get_status().expect("get_status should succeed");
    assert_eq!(
        status.schema_version, 4,
        "schema version must be 4 after apply_v4_prune"
    );

    // Migration flag must be cleared.
    assert!(
        !store.needs_v4_migration().unwrap(),
        "needs_v4_migration must return false after apply_v4_prune"
    );

    // All pre-v4 index data must be cleared.
    let sources_after = graphtor_core::db::list_sources(&store).unwrap();
    assert_eq!(
        sources_after.len(),
        0,
        "source data must be pruned by apply_v4_prune"
    );
}

/// Calling `prune_v4_data_for_rebuild` on a v4 DB is a no-op.
#[test]
fn prune_v4_data_for_rebuild_on_v4_db_is_no_op() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    upsert_source(&store, &sample_source("v4-source")).unwrap();

    store
        .prune_v4_data_for_rebuild()
        .expect("prune_v4_data_for_rebuild on a v4 DB should not error");

    let status = store.get_status().expect("get_status should succeed");
    assert_eq!(status.schema_version, 4, "version should remain 4");
    assert!(
        !store
            .v4_migration_snapshot_locked()
            .expect("snapshot lock check should succeed"),
        "no-op prune must not arm snapshot reuse on a v4 DB"
    );
    let sources = graphtor_core::db::list_sources(&store).unwrap();
    assert_eq!(sources.len(), 1, "v4 data must be preserved by no-op prune");
    assert_eq!(sources[0].source_id, "v4-source");
}

/// Calling `apply_v4_prune` on a v4 DB is a no-op: data is preserved and the
/// version remains 4.
#[test]
fn apply_v4_prune_on_v4_db_is_safe() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    upsert_source(&store, &sample_source("v4-source")).unwrap();

    store
        .apply_v4_prune()
        .expect("apply_v4_prune on a v4 DB should not error");

    let status = store.get_status().expect("get_status should succeed");
    assert_eq!(status.schema_version, 4, "version should remain 4");
    assert!(
        !store
            .v4_migration_snapshot_locked()
            .expect("snapshot lock check should succeed"),
        "apply_v4_prune must remain a no-op on a v4 DB"
    );
    let sources = graphtor_core::db::list_sources(&store).unwrap();
    assert_eq!(
        sources.len(),
        1,
        "v4 data must be preserved by apply_v4_prune"
    );
    assert_eq!(sources[0].source_id, "v4-source");
}

/// Staged v4 prunes must require the persisted frozen snapshot until the
/// rebuild is explicitly marked complete.
#[test]
fn staged_v4_prune_locks_snapshot_reuse_until_completion() {
    let store = DataStore::open_mem().unwrap();
    store.ensure_schema().unwrap();
    store.set_schema_version_for_test(3).unwrap();

    assert!(
        !store
            .v4_migration_snapshot_locked()
            .expect("snapshot lock check should succeed before prune"),
        "pre-condition: pre-prune stores must not require a persisted snapshot yet"
    );

    store
        .prune_v4_data_for_rebuild()
        .expect("staged prune should succeed");

    assert!(
        store
            .needs_v4_migration()
            .expect("needs_v4_migration should succeed after staged prune"),
        "schema gate must stay active until the rebuild is marked complete"
    );
    assert!(
        store
            .v4_migration_snapshot_locked()
            .expect("snapshot lock check should succeed after prune"),
        "staged prune must require the persisted frozen snapshot on retries"
    );

    store
        .mark_v4_migration_complete()
        .expect("completion should succeed");

    assert!(
        !store
            .v4_migration_snapshot_locked()
            .expect("snapshot lock check should succeed after completion"),
        "completion must clear the persisted snapshot requirement"
    );
}
