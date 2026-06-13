//! Regression test: pre-v4 database must not be pruned when candidate files
//! include an invalid document.
//!
//! # Scenario
//!
//! A database at schema version 3 (pre-v4) contains ingested source records.
//! Before a rebuild the caller must pre-validate all candidate markdown files.
//! If any file fails contract validation, `validate_and_apply_v4_migration`
//! must return an error **without** calling `apply_v4_prune`, preserving the
//! existing data.
//!
//! This prevents partial data loss where a malformed file in the source tree
//! would cause the migration to clear all existing data without a successful
//! replacement ingest completing first.

use std::fs;
use std::path::PathBuf;

use graphtor_core::db::{ensure_schema, upsert_source, DataStore, SourceRecord};
use graphtor_core::sync::{validate_and_apply_v4_migration, validate_and_begin_v4_migration};

fn make_store_at_v3() -> DataStore {
    let store = DataStore::open_mem().expect("open in-memory store");
    store.ensure_schema().expect("ensure schema");
    // Force version back to 3 to simulate a pre-v4 database.
    store
        .set_schema_version_for_test(3)
        .expect("set version to 3");
    // Insert a source record to represent pre-v4 index data.
    upsert_source(
        &store,
        &SourceRecord {
            source_id: "legacy-source".to_string(),
            url: "file:///docs".to_string(),
            kind: "local".to_string(),
            name: "Legacy Source".to_string(),
            synced_at: None,
        },
    )
    .expect("seed legacy source");
    store
}

/// Build a docline-conformant markdown string.
fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

// ── T-V4-PREFLIGHT-01: invalid file aborts migration ─────────────────────────

/// When a pre-v4 DB has one valid and one invalid candidate file,
/// `validate_and_apply_v4_migration` must:
/// - Return an error
/// - NOT call `apply_v4_prune` (source data survives)
/// - Leave the schema version below 4
#[test]
fn invalid_doc_aborts_v4_migration_no_data_loss() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let store = make_store_at_v3();

    // Pre-condition: DB is at v3 and has data.
    assert!(
        store.needs_v4_migration().expect("check migration"),
        "pre-condition: DB must need v4 migration"
    );
    let sources_before = graphtor_core::db::list_sources(&store).expect("list sources before");
    assert_eq!(
        sources_before.len(),
        1,
        "pre-condition: one source must exist before attempted migration"
    );

    // Write one valid and one invalid (no frontmatter) candidate file.
    let valid_md = docline_md("good/doc.md", "Good Doc", "# Good\n\nContent.\n");
    let invalid_md = "# Bad\n\nNo frontmatter here.\n";
    let valid_path = root.join("good.md");
    let invalid_path = root.join("bad.md");
    fs::write(&valid_path, valid_md.as_bytes()).expect("write valid.md");
    fs::write(&invalid_path, invalid_md.as_bytes()).expect("write bad.md");

    let candidate_files: Vec<PathBuf> = vec![valid_path, invalid_path];

    // Act: attempt migration — must fail due to invalid file.
    let result = validate_and_apply_v4_migration(&store, &candidate_files);
    assert!(
        result.is_err(),
        "migration must fail when a candidate file is invalid; got: {result:?}"
    );

    // The schema version must still be below 4.
    assert!(
        store.needs_v4_migration().expect("check migration after"),
        "schema version must remain below 4 after aborted migration"
    );

    // The pre-v4 source data must still exist.
    let sources_after = graphtor_core::db::list_sources(&store).expect("list sources after");
    assert_eq!(
        sources_after.len(),
        1,
        "pre-v4 source data must survive aborted migration; \
         expected 1 source, got {}",
        sources_after.len()
    );
}

// ── T-V4-PREFLIGHT-02: all-valid files proceed to prune ──────────────────────

/// When all candidate files pass contract validation, `validate_and_apply_v4_migration`
/// must call `apply_v4_prune` (data cleared) and stamp the DB as v4.
#[test]
fn all_valid_docs_proceed_to_v4_prune() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let store = make_store_at_v3();

    let valid_md_1 = docline_md("doc/a.md", "Doc A", "# A\n\nContent.\n");
    let valid_md_2 = docline_md("doc/b.md", "Doc B", "# B\n\nContent.\n");
    let path_1 = root.join("a.md");
    let path_2 = root.join("b.md");
    fs::write(&path_1, valid_md_1.as_bytes()).expect("write a.md");
    fs::write(&path_2, valid_md_2.as_bytes()).expect("write b.md");

    let candidate_files: Vec<PathBuf> = vec![path_1, path_2];

    // Act.
    validate_and_apply_v4_migration(&store, &candidate_files)
        .expect("all valid files must allow migration to proceed");

    // DB must now be at v4.
    assert!(
        !store.needs_v4_migration().expect("check migration"),
        "DB must be at v4 after successful preflight + prune"
    );

    // All pre-v4 data must be cleared (prune applied).
    let sources_after = graphtor_core::db::list_sources(&store).expect("list sources after");
    assert_eq!(
        sources_after.len(),
        0,
        "pre-v4 data must be cleared after successful migration"
    );
}

// ── T-V4-PREFLIGHT-02A: staged migration keeps gate until completion ──────────

/// The staged v4 migration helper must prune pre-v4 data without stamping the
/// schema to v4 until the caller explicitly marks the rebuild complete.
#[test]
fn staged_v4_migration_keeps_gate_until_completion() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let store = make_store_at_v3();

    let valid_md = docline_md("doc/a.md", "Doc A", "# A\n\nContent.\n");
    let path = root.join("a.md");
    fs::write(&path, valid_md.as_bytes()).expect("write a.md");

    let started = validate_and_begin_v4_migration(&store, &[path])
        .expect("valid files must allow staged migration to begin");
    assert!(started, "pre-v4 database must begin staged migration");

    assert!(
        store
            .needs_v4_migration()
            .expect("check migration after prune"),
        "migration gate must remain active until rebuild completion is recorded"
    );

    let sources_after_prune = graphtor_core::db::list_sources(&store).expect("list sources");
    assert_eq!(
        sources_after_prune.len(),
        0,
        "staged migration must prune pre-v4 source data before rebuild"
    );

    store
        .mark_v4_migration_complete()
        .expect("complete staged v4 migration");
    assert!(
        !store
            .needs_v4_migration()
            .expect("check migration after completion"),
        "migration gate must clear after rebuild completion is recorded"
    );
}

// ── T-V4-PREFLIGHT-02B: duplicate source_path aborts migration ─────────────────

/// When two candidate files for the same source declare the same `source_path`,
/// v4 migration must fail before pruning to avoid later sync-cycle rejection
/// after the legacy index has already been cleared.
#[test]
fn duplicate_source_path_aborts_v4_migration_no_data_loss() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let store = make_store_at_v3();

    let duplicate_path = "shared/doc.md";
    let valid_md_1 = docline_md(duplicate_path, "Doc A", "# A\n\nContent A.\n");
    let valid_md_2 = docline_md(duplicate_path, "Doc B", "# B\n\nContent B.\n");
    let path_1 = root.join("a.md");
    let path_2 = root.join("b.md");
    fs::write(&path_1, valid_md_1.as_bytes()).expect("write a.md");
    fs::write(&path_2, valid_md_2.as_bytes()).expect("write b.md");

    let candidate_files: Vec<PathBuf> = vec![path_1, path_2];

    let result = validate_and_apply_v4_migration(&store, &candidate_files);
    assert!(
        result.is_err(),
        "migration must fail when candidate files collide on source_path; got: {result:?}"
    );

    assert!(
        store.needs_v4_migration().expect("check migration after"),
        "schema version must remain below 4 after aborted migration"
    );

    let sources_after = graphtor_core::db::list_sources(&store).expect("list sources after");
    assert_eq!(
        sources_after.len(),
        1,
        "pre-v4 source data must survive aborted duplicate-path migration; \
         expected 1 source, got {}",
        sources_after.len()
    );
}

// ── T-V4-PREFLIGHT-03: no migration needed → no-op ───────────────────────────

/// When the DB is already at v4, `validate_and_apply_v4_migration` must be a
/// no-op: it must not error, must not alter data, and must not change the
/// schema version.
#[test]
fn no_op_when_migration_not_needed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Fresh v4 DB with data.
    let store = DataStore::open_mem().expect("open in-memory store");
    ensure_schema(&store).expect("ensure schema");
    upsert_source(
        &store,
        &SourceRecord {
            source_id: "v4-source".to_string(),
            url: "file:///docs".to_string(),
            kind: "local".to_string(),
            name: "v4 Source".to_string(),
            synced_at: None,
        },
    )
    .expect("seed v4 source");

    assert!(
        !store.needs_v4_migration().expect("check migration"),
        "pre-condition: DB must NOT need migration"
    );

    // Pass an invalid file — but since no migration is needed, it must be ignored.
    let invalid_md = "# Bad\n\nNo frontmatter.\n";
    let invalid_path = root.join("bad.md");
    fs::write(&invalid_path, invalid_md.as_bytes()).expect("write bad.md");

    let result = validate_and_apply_v4_migration(&store, &[invalid_path]);
    assert!(
        result.is_ok(),
        "must be a no-op (Ok) when no v4 migration is needed; got: {result:?}"
    );

    // Data must be unchanged.
    let sources = graphtor_core::db::list_sources(&store).expect("list sources");
    assert_eq!(
        sources.len(),
        1,
        "data must be unchanged when migration is not needed"
    );
}

// ── T-V4-PREFLIGHT-04: empty candidate list → prune proceeds ─────────────────

/// An empty candidate list is treated as "all valid" — migration proceeds.
#[test]
fn empty_candidate_list_proceeds_to_prune() {
    let store = make_store_at_v3();
    validate_and_apply_v4_migration(&store, &[])
        .expect("empty candidate list must allow migration to proceed");
    assert!(
        !store.needs_v4_migration().expect("check migration"),
        "DB must be at v4 after empty-candidate migration"
    );
}
