//! Advisory lock files for workspace-wide and per-database coordination.
//!
//! The CLI uses these lock files to prevent overlapping write operations
//! without blocking read-only commands such as `status` and MCP queries.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Arc, Barrier, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::GraphtorError;

const WORKSPACE_LOCK_FILE: &str = "graphtor.lock";
const STALE_SECS: u64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockDetails {
    pid: Option<u32>,
    timestamp: Option<u64>,
}

#[derive(Debug)]
struct AdvisoryLock {
    path: PathBuf,
}

#[derive(Debug)]
struct ReplacementGuard {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
enum LockKind<'a> {
    Workspace,
    Database { db_name: &'a str },
}

impl AdvisoryLock {
    fn acquire(path: PathBuf, kind: LockKind<'_>, force: bool) -> Result<Self, GraphtorError> {
        let timestamp = current_timestamp_secs();
        let pid = std::process::id();
        let content = format!("pid={pid}\ntimestamp={timestamp}\n");

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(content.as_bytes()) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(GraphtorError::Config {
                        message: format!("failed to write lock file: {error}"),
                        field: None,
                    });
                }
                Ok(Self { path })
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                handle_existing_lock(&path, &content, force, kind)
            }
            Err(error) => Err(GraphtorError::Config {
                message: format!("failed to create lock file: {error}"),
                field: None,
            }),
        }
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for ReplacementGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Workspace-wide advisory lock used by install, upgrade, and uninstall commands.
#[derive(Debug)]
#[must_use = "lock is released on drop; assign to a binding"]
pub struct WorkspaceLock {
    _inner: AdvisoryLock,
}

impl WorkspaceLock {
    /// Acquire the workspace lock at `.graphtor/graphtor.lock`.
    ///
    /// When `force` is true, lock acquisition attempts to replace any existing
    /// lock file immediately. It can still fail if another process is already
    /// performing that replacement through a live `.replacing` marker.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] when another live process already holds
    /// the workspace lock and `force` is false, or when a concurrent
    /// replacement attempt is already in progress.
    pub fn acquire(workspace_dir: &Path, force: bool) -> Result<Self, GraphtorError> {
        let path = workspace_dir.join(WORKSPACE_LOCK_FILE);
        AdvisoryLock::acquire(path, LockKind::Workspace, force).map(|inner| Self { _inner: inner })
    }
}

/// Per-database advisory lock used to exclude overlapping write access.
#[derive(Debug)]
#[must_use = "lock is released on drop; assign to a binding"]
pub struct DatabaseLock {
    _inner: AdvisoryLock,
}

impl DatabaseLock {
    /// Acquire a database-scoped lock file named `{db_name}.lock`.
    ///
    /// `lock_dir` is typically the `.graphtor/` workspace directory and
    /// `db_path` identifies the target database file whose filename becomes the
    /// lock name.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::DatabaseLocked`] when another live process
    /// already holds the database lock and `force` is false.
    pub fn acquire(lock_dir: &Path, db_path: &Path, force: bool) -> Result<Self, GraphtorError> {
        let db_name = db_name(db_path)?;
        let path = lock_dir.join(format!("{db_name}.lock"));
        AdvisoryLock::acquire(path, LockKind::Database { db_name: &db_name }, force)
            .map(|inner| Self { _inner: inner })
    }
}

fn db_name(db_path: &Path) -> Result<String, GraphtorError> {
    db_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Config {
            message: format!(
                "database path '{}' must end with a UTF-8 file name",
                db_path.display()
            ),
            field: None,
        })
}

fn handle_existing_lock(
    path: &Path,
    content: &str,
    force: bool,
    kind: LockKind<'_>,
) -> Result<AdvisoryLock, GraphtorError> {
    if force {
        return replace_lock_file(path, content, force, kind);
    }

    let details = match read_lock_details(path) {
        Ok(details) => details,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return retry_create_lock(path, content);
        }
        Err(error) => {
            return Err(GraphtorError::Config {
                message: format!("failed to read lock file: {error}"),
                field: None,
            });
        }
    };

    if is_stale(path, &details) {
        #[cfg(test)]
        before_stale_lock_replacement(path);
        return replace_lock_file(path, content, force, kind);
    }

    Err(conflict_error(
        kind,
        &details,
        lock_age_secs(path, &details),
    ))
}

fn replace_lock_file(
    path: &Path,
    content: &str,
    force: bool,
    kind: LockKind<'_>,
) -> Result<AdvisoryLock, GraphtorError> {
    let Some(_guard) = try_acquire_replacement_guard(path)? else {
        return recover_from_replacement_race(path, content, kind);
    };

    match read_lock_details(path) {
        Ok(details) if !force && !is_stale(path, &details) => {
            return Err(conflict_error(
                kind,
                &details,
                lock_age_secs(path, &details),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(GraphtorError::Config {
                message: format!("failed to read lock file: {error}"),
                field: None,
            });
        }
    }

    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(GraphtorError::Config {
                message: format!("failed to remove existing lock file: {error}"),
                field: None,
            });
        }
    }

    retry_create_lock(path, content)
}

fn try_acquire_replacement_guard(path: &Path) -> Result<Option<ReplacementGuard>, GraphtorError> {
    let guard_path = replacement_guard_path(path);
    let timestamp = current_timestamp_secs();
    let pid = std::process::id();
    let content = format!("pid={pid}\ntimestamp={timestamp}\n");

    let Some(guard) = try_create_replacement_guard(&guard_path, &content)? else {
        return reclaim_or_yield_replacement_guard(&guard_path, &content);
    };

    Ok(Some(guard))
}

fn try_create_replacement_guard(
    guard_path: &Path,
    content: &str,
) -> Result<Option<ReplacementGuard>, GraphtorError> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(guard_path)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(content.as_bytes()) {
                drop(file);
                let _ = fs::remove_file(guard_path);
                return Err(GraphtorError::Config {
                    message: format!("failed to write replacement marker: {error}"),
                    field: None,
                });
            }
            Ok(Some(ReplacementGuard {
                path: guard_path.to_path_buf(),
            }))
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(GraphtorError::Config {
            message: format!("failed to create replacement marker: {error}"),
            field: None,
        }),
    }
}

fn reclaim_or_yield_replacement_guard(
    guard_path: &Path,
    content: &str,
) -> Result<Option<ReplacementGuard>, GraphtorError> {
    match read_lock_details(guard_path) {
        Ok(details) if is_stale(guard_path, &details) => {
            remove_stale_replacement_guard(guard_path)?;
            try_create_replacement_guard(guard_path, content)
        }
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            try_create_replacement_guard(guard_path, content)
        }
        Err(error) => Err(GraphtorError::Config {
            message: format!("failed to read replacement marker: {error}"),
            field: None,
        }),
    }
}

fn remove_stale_replacement_guard(guard_path: &Path) -> Result<(), GraphtorError> {
    match fs::remove_file(guard_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(GraphtorError::Config {
            message: format!("failed to remove stale replacement marker: {error}"),
            field: None,
        }),
    }
}

fn replacement_guard_path(path: &Path) -> PathBuf {
    let mut file_name = path.file_name().map_or_else(
        || std::ffi::OsString::from("lock"),
        std::ffi::OsStr::to_os_string,
    );
    file_name.push(".replacing");
    path.with_file_name(file_name)
}

fn recover_from_replacement_race(
    path: &Path,
    content: &str,
    kind: LockKind<'_>,
) -> Result<AdvisoryLock, GraphtorError> {
    match read_lock_details(path) {
        Ok(details) => Err(conflict_error(
            kind,
            &details,
            lock_age_secs(path, &details),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => retry_create_lock(path, content),
        Err(error) => Err(GraphtorError::Config {
            message: format!("failed to read lock file: {error}"),
            field: None,
        }),
    }
}

fn retry_create_lock(path: &Path, content: &str) -> Result<AdvisoryLock, GraphtorError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| GraphtorError::Config {
            message: format!("failed to re-acquire lock after concurrent release: {error}"),
            field: None,
        })?;

    if let Err(error) = file.write_all(content.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(GraphtorError::Config {
            message: format!("failed to write lock file on retry: {error}"),
            field: None,
        });
    }

    Ok(AdvisoryLock {
        path: path.to_path_buf(),
    })
}

fn read_lock_details(path: &Path) -> Result<LockDetails, std::io::Error> {
    fs::read_to_string(path).map(|content| parse_lock_details(&content))
}

fn parse_lock_details(content: &str) -> LockDetails {
    let mut pid = None;
    let mut timestamp = None;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("pid=") {
            pid = value.parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("timestamp=") {
            timestamp = value.parse::<u64>().ok();
        }
    }

    LockDetails { pid, timestamp }
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
#[derive(Clone)]
struct StaleReplacementHook {
    path: PathBuf,
    barrier: Arc<Barrier>,
}

#[cfg(test)]
fn stale_replacement_hook() -> &'static Mutex<Option<StaleReplacementHook>> {
    static HOOK: OnceLock<Mutex<Option<StaleReplacementHook>>> = OnceLock::new();
    HOOK.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn before_stale_lock_replacement(path: &Path) {
    let hook = stale_replacement_hook()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(hook) = hook.filter(|hook| hook.path == path) {
        hook.barrier.wait();
    }
}

#[cfg(test)]
struct StaleReplacementHookGuard;

#[cfg(test)]
impl StaleReplacementHookGuard {
    fn install(path: PathBuf, barrier: Arc<Barrier>) -> Self {
        let mut hook = stale_replacement_hook()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *hook = Some(StaleReplacementHook { path, barrier });
        Self
    }
}

#[cfg(test)]
impl Drop for StaleReplacementHookGuard {
    fn drop(&mut self) {
        let mut hook = stale_replacement_hook()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *hook = None;
    }
}

fn lock_age_secs(path: &Path, details: &LockDetails) -> Option<u64> {
    if let Some(timestamp) = details.timestamp {
        return Some(current_timestamp_secs().saturating_sub(timestamp));
    }

    path.metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|duration| duration.as_secs())
}

fn is_stale(path: &Path, details: &LockDetails) -> bool {
    lock_age_secs(path, details).is_some_and(|age| age >= STALE_SECS)
}

fn conflict_error(
    kind: LockKind<'_>,
    details: &LockDetails,
    age_secs: Option<u64>,
) -> GraphtorError {
    match kind {
        LockKind::Workspace => {
            let holder = details
                .pid
                .map_or_else(|| "unknown".to_string(), |pid| pid.to_string());
            let age = age_secs.unwrap_or(0);
            GraphtorError::Config {
                message: format!(
                    "workspace is locked by process {holder} (lock age: {age}s); \
                     pass `--force-unlock` to override or wait for the other process to finish"
                ),
                field: None,
            }
        }
        LockKind::Database { db_name } => GraphtorError::DatabaseLocked {
            db_name: db_name.to_string(),
            holder_pid: details.pid,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn workspace_lock_acquire_and_release() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let lock_path = tmp.path().join(WORKSPACE_LOCK_FILE);
        {
            let _lock = WorkspaceLock::acquire(tmp.path(), false).expect("acquire");
            assert!(lock_path.exists(), "lock file should exist while held");
        }
        assert!(!lock_path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn force_overrides_existing_workspace_lock() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(
            tmp.path().join(WORKSPACE_LOCK_FILE),
            "pid=999\ntimestamp=1\n",
        )
        .expect("write existing lock");
        let _lock = WorkspaceLock::acquire(tmp.path(), true).expect("force acquire");
    }

    #[test]
    fn second_workspace_acquire_on_fresh_lock_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let _lock = WorkspaceLock::acquire(tmp.path(), false).expect("first acquire");
        let error = WorkspaceLock::acquire(tmp.path(), false)
            .expect_err("second acquire should fail while first is held");
        let GraphtorError::Config { message, .. } = error else {
            panic!("expected Config error, got: {error:?}");
        };
        assert!(
            message.contains("--force-unlock"),
            "error should mention --force-unlock, got: {message}"
        );
    }

    #[test]
    fn database_lock_is_scoped_by_database_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let primary = tmp.path().join("primary.db");
        let secondary = tmp.path().join("secondary.db");

        let _primary_lock =
            DatabaseLock::acquire(tmp.path(), &primary, false).expect("primary lock");

        let conflict = DatabaseLock::acquire(tmp.path(), &primary, false)
            .expect_err("second primary lock should fail");
        assert!(
            matches!(conflict, GraphtorError::DatabaseLocked { .. }),
            "expected DatabaseLocked, got: {conflict:?}"
        );

        let _secondary_lock =
            DatabaseLock::acquire(tmp.path(), &secondary, false).expect("secondary lock");
    }

    #[test]
    fn stale_database_lock_is_replaced_using_timestamp_contents() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let primary = tmp.path().join("primary.db");
        fs::write(tmp.path().join("primary.db.lock"), "pid=42\ntimestamp=0\n")
            .expect("write stale lock");

        let _lock = DatabaseLock::acquire(tmp.path(), &primary, false)
            .expect("stale database lock should be replaced");
    }

    #[test]
    fn concurrent_stale_database_lock_replacement_only_allows_one_winner() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let primary = tmp.path().join("primary.db");
        let lock_path = tmp.path().join("primary.db.lock");
        fs::write(&lock_path, "pid=42\ntimestamp=0\n").expect("write stale lock");

        let barrier = Arc::new(Barrier::new(2));
        let _hook_guard = StaleReplacementHookGuard::install(lock_path, Arc::clone(&barrier));

        let worker = {
            let lock_dir = tmp.path().to_path_buf();
            let db_path = primary.clone();
            move || DatabaseLock::acquire(&lock_dir, &db_path, false)
        };

        let first = thread::spawn(worker.clone());
        let second = thread::spawn(worker);

        let results = [
            first.join().expect("first join"),
            second.join().expect("second join"),
        ];

        let success_count = results.iter().filter(|result| result.is_ok()).count();
        assert_eq!(success_count, 1, "exactly one replacement should win");
    }
}
