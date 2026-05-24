//! Integration tests: `DataStore` lifecycle (`open_mem`, `open_sqlite`, path enforcement).

use std::path::PathBuf;

use sqlite::{Connection, State};
use tempfile::TempDir;

use graphtor_core::db::{upsert_source, DataStore, SourceRecord};
use graphtor_core::GraphtorError;

fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
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
fn open_mem_succeeds() {
    let store = DataStore::open_mem().expect("open_mem should succeed");
    // Verify the store is usable by running the schema.
    store
        .ensure_schema()
        .expect("ensure_schema on in-memory store should succeed");
}

#[test]
fn open_sqlite_creates_file() {
    let dir = temp_dir();
    let db_root = dir.path().join(".graphtor");
    std::fs::create_dir_all(&db_root).expect("create db root");
    let db_path = db_root.join("test.db");
    let store = DataStore::open_sqlite(&db_path, dir.path())
        .expect("open_sqlite with valid path should succeed");
    store
        .ensure_schema()
        .expect("ensure_schema on SQLite store should succeed");
    assert!(db_path.exists(), "SQLite file should exist after open");
}

#[test]
fn open_sqlite_enables_write_ahead_logging() {
    let dir = temp_dir();
    let db_root = dir.path().join(".graphtor");
    std::fs::create_dir_all(&db_root).expect("create db root");
    let db_path = db_root.join("wal.db");
    let store = DataStore::open_sqlite(&db_path, dir.path())
        .expect("open_sqlite with valid path should succeed");
    store
        .ensure_schema()
        .expect("ensure_schema on SQLite store should succeed");

    let connection = Connection::open_thread_safe(&db_path).expect("open sqlite connection");
    let mut statement = connection
        .prepare("PRAGMA journal_mode;")
        .expect("prepare pragma statement");
    let mode = match statement.next().expect("run pragma statement") {
        State::Row => statement
            .read::<String, _>(0)
            .expect("read pragma result")
            .to_lowercase(),
        State::Done => panic!("expected PRAGMA journal_mode to return a row"),
    };

    assert_eq!(mode, "wal", "SQLite database should use WAL mode");
}

#[test]
fn open_sqlite_rejects_path_outside_root() {
    let dir = temp_dir();
    let root = dir.path().join("subdir");
    std::fs::create_dir_all(&root).unwrap();
    // Construct a path that escapes the root via `..`.
    let escaped = root.join("..").join("outside.db");
    let err = DataStore::open_sqlite(&escaped, &root).expect_err("path outside root should fail");
    assert!(
        matches!(err, GraphtorError::PathViolation { .. }),
        "expected PathViolation, got: {err:?}"
    );
}

#[test]
fn open_sqlite_nonexistent_root_is_rejected() {
    // A root that does not exist cannot contain a valid path.
    let root = PathBuf::from("/this/path/does/not/exist/root");
    let db_path = root.join("test.db");
    let err = DataStore::open_sqlite(&db_path, &root).expect_err("non-existent root should fail");
    // GraphtorError::PathViolation or IoError are both acceptable.
    let _ = err;
}

#[test]
fn open_sqlite_readonly_rejects_mutations_but_allows_reads() {
    let dir = temp_dir();
    let db_root = dir.path().join(".graphtor");
    std::fs::create_dir_all(&db_root).expect("create db root");
    let db_path = db_root.join("readonly.db");

    let store = DataStore::open_sqlite(&db_path, dir.path())
        .expect("open_sqlite with valid path should succeed");
    store
        .ensure_schema()
        .expect("ensure_schema on SQLite store should succeed");
    upsert_source(&store, &sample_source("docs"))
        .expect("seed source through read-write store should succeed");

    let readonly = DataStore::open_sqlite_readonly(&db_path, dir.path())
        .expect("open_sqlite_readonly should succeed");
    let status = readonly
        .get_status()
        .expect("read-only status query should succeed");
    assert_eq!(
        status.source_count, 1,
        "read-only store should read existing data"
    );

    let error = upsert_source(&readonly, &sample_source("should-fail"))
        .expect_err("read-only store should reject mutations");
    assert!(
        matches!(error, GraphtorError::Database { .. }),
        "read-only writes should return a database error, got: {error:?}"
    );
}

#[test]
fn data_store_is_clone() {
    let store = DataStore::open_mem().unwrap();
    let cloned = store.clone();
    // Both handles refer to the same underlying database.
    cloned
        .ensure_schema()
        .expect("cloned store should be usable");
}
