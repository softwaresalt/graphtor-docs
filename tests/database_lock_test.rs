//! Integration tests for per-database advisory locking.

use graphtor_core::lock::DatabaseLock;
use graphtor_core::GraphtorError;

fn lock_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
}

#[test]
fn database_lock_conflicts_are_scoped_per_database() {
    let tempdir = lock_root();
    let workspace_dir = tempdir.path().join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

    let primary_db = workspace_dir.join("primary.db");
    let secondary_db = workspace_dir.join("secondary.db");

    let _primary_lock = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
        .expect("first primary lock should succeed");

    let conflict = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
        .expect_err("second primary lock should fail");
    assert!(
        matches!(conflict, GraphtorError::DatabaseLocked { .. }),
        "expected DatabaseLocked error, got: {conflict:?}"
    );

    let _secondary_lock = DatabaseLock::acquire(&workspace_dir, &secondary_db, false)
        .expect("secondary database should use an independent lock");
}

#[test]
fn database_lock_releases_on_drop() {
    let tempdir = lock_root();
    let workspace_dir = tempdir.path().join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

    let primary_db = workspace_dir.join("primary.db");
    let lock_path = workspace_dir.join("primary.db.lock");

    {
        let _lock = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
            .expect("lock acquisition should succeed");
        assert!(lock_path.exists(), "lock file should exist while held");
    }

    assert!(
        !lock_path.exists(),
        "lock file should be removed after drop"
    );
}

#[test]
fn stale_database_lock_is_replaced_using_embedded_timestamp() {
    let tempdir = lock_root();
    let workspace_dir = tempdir.path().join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

    let primary_db = workspace_dir.join("primary.db");
    let lock_path = workspace_dir.join("primary.db.lock");
    std::fs::write(&lock_path, "pid=42\ntimestamp=0\n").expect("write stale lock");

    let _lock = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
        .expect("stale database lock should be replaced");
}

#[test]
fn fresh_database_lock_with_dead_pid_is_replaced() {
    let tempdir = lock_root();
    let workspace_dir = tempdir.path().join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

    let primary_db = workspace_dir.join("primary.db");
    let lock_path = workspace_dir.join("primary.db.lock");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_secs();
    std::fs::write(&lock_path, format!("pid={}\ntimestamp={now}\n", u32::MAX))
        .expect("write fresh dead-pid lock");

    let _lock = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
        .expect("fresh lock with dead pid should be replaced");
}

#[test]
fn stale_replacement_marker_does_not_block_stale_database_lock_replacement() {
    let tempdir = lock_root();
    let workspace_dir = tempdir.path().join(".graphtor");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

    let primary_db = workspace_dir.join("primary.db");
    let lock_path = workspace_dir.join("primary.db.lock");
    let replacement_path = workspace_dir.join("primary.db.lock.replacing");
    std::fs::write(&lock_path, "pid=42\ntimestamp=0\n").expect("write stale lock");
    std::fs::write(&replacement_path, "pid=7\ntimestamp=0\n")
        .expect("write stale replacement marker");

    let _lock = DatabaseLock::acquire(&workspace_dir, &primary_db, false)
        .expect("stale replacement marker should not block lock replacement");

    assert!(
        !replacement_path.exists(),
        "replacement marker should be removed after successful replacement"
    );
}
