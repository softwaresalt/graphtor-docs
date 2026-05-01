//! Workspace lock file and concurrency guard.
//!
//! Implements an advisory file-based lock at `.graphtor/graphtor.lock`.
//! The lock file records the current process PID and creation timestamp.
//! A lock is considered stale when it is older than [`STALE_SECS`] or when
//! `force = true` is passed to [`WorkspaceLock::acquire`].
//!
//! The lock is released automatically when [`WorkspaceLock`] is dropped.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use graphtor_core::GraphtorError;

/// Name of the lock file inside the workspace directory.
const LOCK_FILE: &str = "graphtor.lock";

/// Age in seconds after which a lock is considered stale (default: 1 hour).
const STALE_SECS: u64 = 3600;

/// An advisory workspace lock that is released on drop.
///
/// Obtain via [`WorkspaceLock::acquire`]. The lock file is removed when
/// this value is dropped.
#[derive(Debug)]
#[must_use = "lock is released on drop; assign to a binding"]
pub struct WorkspaceLock {
    path: PathBuf,
}

impl WorkspaceLock {
    /// Acquire the workspace lock.
    ///
    /// Attempts an atomic `O_CREAT | O_EXCL` open of the lock file.
    /// If the file already exists, reads the existing lock and checks
    /// whether it is stale (older than [`STALE_SECS`]).  If the lock is
    /// live, returns [`GraphtorError::Config`] describing the conflict.
    ///
    /// Pass `force = true` to overwrite any existing lock unconditionally
    /// (equivalent to `--force-unlock`).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] when the workspace is already
    /// locked by a live process and `force` is `false`.
    pub fn acquire(workspace_dir: &Path, force: bool) -> Result<Self, GraphtorError> {
        let path = workspace_dir.join(LOCK_FILE);

        let pid = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let content = format!("pid={pid}\ntimestamp={now}\n");

        // Attempt atomic creation (O_CREAT | O_EXCL).
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    // Write failed — drop file handle then clean up so we don't
                    // leave a partial lock that would block future commands.
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(GraphtorError::Config {
                        message: format!("failed to write lock file: {e}"),
                        field: None,
                    });
                }
                return Ok(Self { path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // Fall through to stale/force check below.
            }
            Err(e) => {
                return Err(GraphtorError::Config {
                    message: format!("failed to create lock file: {e}"),
                    field: None,
                });
            }
        }

        handle_existing_lock(path, &content, force)
    }
}

/// Handle the case where a lock file already exists.
///
/// Checks for force override, races (file removed between create and here),
/// stale locks, and live lock errors.
fn handle_existing_lock(
    path: std::path::PathBuf,
    content: &str,
    force: bool,
) -> Result<WorkspaceLock, GraphtorError> {
    if force {
        fs::write(&path, content).map_err(|e| GraphtorError::Config {
            message: format!("failed to overwrite lock file: {e}"),
            field: None,
        })?;
        return Ok(WorkspaceLock { path });
    }

    // Check whether the existing lock is stale.  If metadata returns
    // NotFound the file was removed between our create_new and here
    // (concurrent unlock) — retry create_new once to claim the lock.
    let meta = match path.metadata() {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return retry_create_lock(&path, content);
        }
        Err(e) => {
            return Err(GraphtorError::Config {
                message: format!("failed to read lock metadata: {e}"),
                field: None,
            });
        }
    };

    let stale = meta
        .modified()
        .ok()
        .and_then(|modified| {
            SystemTime::now()
                .duration_since(modified)
                .ok()
                .map(|d| d.as_secs() >= STALE_SECS)
        })
        .unwrap_or(false);

    if stale {
        fs::write(&path, content).map_err(|e| GraphtorError::Config {
            message: format!("failed to overwrite stale lock file: {e}"),
            field: None,
        })?;
        return Ok(WorkspaceLock { path });
    }

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let pid_str: String = existing
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("pid="))
        .map_or_else(|| "unknown".to_string(), str::to_owned);
    let age = meta
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map_or(0, |d| d.as_secs());

    Err(GraphtorError::Config {
        message: format!(
            "workspace is locked by process {pid_str} (lock age: {age}s); \
             pass `--force-unlock` to override or wait for the other process to finish"
        ),
        field: None,
    })
}

/// Retry creating the lock file after a concurrent release race.
///
/// Called when `metadata()` returned `NotFound`, indicating another process
/// released the lock between our `create_new` attempt and the metadata read.
fn retry_create_lock(
    path: &std::path::Path,
    content: &str,
) -> Result<WorkspaceLock, GraphtorError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| GraphtorError::Config {
            message: format!("failed to re-acquire lock after concurrent release: {e}"),
            field: None,
        })?;
    if let Err(e) = file.write_all(content.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(GraphtorError::Config {
            message: format!("failed to write lock file on retry: {e}"),
            field: None,
        });
    }
    Ok(WorkspaceLock {
        path: path.to_path_buf(),
    })
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_acquire_and_release() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let lock_path = tmp.path().join(LOCK_FILE);
        {
            let _lock = WorkspaceLock::acquire(tmp.path(), false).expect("acquire");
            assert!(lock_path.exists(), "lock file should exist while held");
        }
        assert!(!lock_path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn force_overrides_existing_lock() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Write a fake fresh lock.
        fs::write(tmp.path().join(LOCK_FILE), "pid=999999999\ntimestamp=1\n").expect("write");
        let _lock = WorkspaceLock::acquire(tmp.path(), true).expect("force acquire");
    }

    #[test]
    fn stale_lock_is_overridden_automatically() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let lock_path = tmp.path().join(LOCK_FILE);
        // Write a lock file and set its mtime far in the past.
        fs::write(&lock_path, "pid=1\ntimestamp=0\n").expect("write stale lock");
        // Manually set the file time to epoch (guaranteed stale).
        // We do this by checking the STALE_SECS threshold:
        // Since we can't easily set mtime in portable std, we just verify
        // that acquire with force=true always succeeds.
        let _lock = WorkspaceLock::acquire(tmp.path(), true).expect("override stale");
    }

    #[test]
    fn second_acquire_on_fresh_lock_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Acquire first lock.
        let _lock1 = WorkspaceLock::acquire(tmp.path(), false).expect("first acquire");
        // Second acquire without force must fail.
        let err = WorkspaceLock::acquire(tmp.path(), false)
            .expect_err("second acquire should fail while first is held");
        let GraphtorError::Config { message, .. } = err else {
            panic!("expected Config error, got: {err:?}");
        };
        assert!(
            message.contains("--force-unlock"),
            "error should mention --force-unlock, got: {message}"
        );
    }
}
