//! Integration tests: `DataStore` lifecycle (`open_mem`, `open_sqlite`, path enforcement).

use std::path::PathBuf;

use tempfile::TempDir;

use graphtor_core::db::DataStore;
use graphtor_core::GraphtorError;

fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
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
    let db_path = dir.path().join("test.db");
    let store = DataStore::open_sqlite(&db_path, dir.path())
        .expect("open_sqlite with valid path should succeed");
    store
        .ensure_schema()
        .expect("ensure_schema on SQLite store should succeed");
    assert!(db_path.exists(), "SQLite file should exist after open");
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
fn data_store_is_clone() {
    let store = DataStore::open_mem().unwrap();
    let cloned = store.clone();
    // Both handles refer to the same underlying database.
    cloned
        .ensure_schema()
        .expect("cloned store should be usable");
}
