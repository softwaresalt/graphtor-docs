//! `DataStore` — the unified `CozoDB` database handle.
//!
//! All read and write operations in the `db` module funnel through
//! [`DataStore`], which wraps [`cozo::DbInstance`] behind an [`std::sync::Arc`]
//! so that handles can be cheaply cloned and shared across threads.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cozo::{DataValue, DbInstance, NamedRows, ScriptMutability};
use sqlite::{Connection, OpenFlags, State};
use tracing::info;

use crate::error::GraphtorError;
use crate::path::validate_path;

/// A snapshot of the current state of the embedded database.
///
/// Returned by [`DataStore::get_status`] and intended for display via the
/// `get_status` MCP tool.
#[derive(Debug, Clone, PartialEq)]
pub struct DbStatus {
    /// Number of registered documentation sources in `doc_sources`.
    pub source_count: u64,
    /// Total number of stored document chunks in `doc_chunks`.
    pub chunk_count: u64,
    /// Schema version recorded in `doc_schema_ver`, or `0` if not set.
    pub schema_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    ReadWrite,
    ReadOnly,
}

/// A cloneable, thread-safe handle to the embedded [`DbInstance`].
///
/// Wraps `cozo::DbInstance` in an [`Arc`] so that clones share the same
/// underlying database connection pool without copying state.
#[derive(Clone)]
pub struct DataStore {
    pub(crate) db: Arc<DbInstance>,
    access_mode: AccessMode,
    backing_path: Option<PathBuf>,
    /// Filesystem read-only lock for [`DataStore::open_engine_readonly`].
    ///
    /// `None` for every other constructor. Shared across clones via [`Arc`]
    /// so the underlying file is restored to writable only when the last
    /// clone is dropped, not when any single clone goes out of scope.
    engine_readonly_guard: Option<Arc<EngineReadonlyGuard>>,
}

impl std::fmt::Debug for DataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataStore").finish_non_exhaustive()
    }
}

impl DataStore {
    /// Open an in-memory `DataStore`.
    ///
    /// Data is lost when the process exits. Suitable for tests and
    /// ephemeral pipelines.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] if `CozoDB` fails to initialise
    /// the in-memory backend.
    pub fn open_mem() -> Result<Self, GraphtorError> {
        let db = DbInstance::new("mem", "", Default::default()).map_err(|e| {
            GraphtorError::Database {
                message: e.to_string(),
                operation: "open_mem".to_string(),
            }
        })?;
        info!("opened in-memory DataStore");
        Ok(Self {
            db: Arc::new(db),
            access_mode: AccessMode::ReadWrite,
            backing_path: None,
            engine_readonly_guard: None,
        })
    }

    /// Open a persistent `DataStore` backed by an `SQLite` file at `path`.
    ///
    /// The `root` parameter is the allowed workspace root. `path` must be
    /// within `root`; otherwise [`GraphtorError::PathViolation`] is returned.
    ///
    /// # Errors
    ///
    /// - [`GraphtorError::PathViolation`] — `path` escapes `root`.
    /// - [`GraphtorError::Database`] — `CozoDB` fails to open or create the
    ///   `SQLite` file, or the path contains non-UTF-8 bytes.
    pub fn open_sqlite(path: &Path, root: &Path) -> Result<Self, GraphtorError> {
        let safe_path = validate_path(path, root)?;
        ensure_database_parent_dir(&safe_path)?;
        // Self-heal a filesystem-readonly lock left behind by a crashed
        // `open_engine_readonly` session (see `EngineReadonlyGuard`). Without
        // this, a stale lock would make SQLite silently fall back to a
        // read-only connection here — a silent downgrade of a write-mode
        // open — instead of failing loudly or working as expected.
        clear_stale_readonly_lock(&safe_path)?;
        configure_sqlite_wal(&safe_path)?;
        let db = open_sqlite_instance(&safe_path, "open_sqlite")?;
        let path_str = path_to_utf8(&safe_path, "open_sqlite")?;
        info!(path = path_str, "opened SQLite DataStore");
        Ok(Self {
            db: Arc::new(db),
            access_mode: AccessMode::ReadWrite,
            backing_path: Some(safe_path),
            engine_readonly_guard: None,
        })
    }

    /// Open a query-only `DataStore` backed by an existing `SQLite` file.
    ///
    /// `CozoDB`'s public `SQLite` API does not expose a true read-only connection
    /// flag, so this constructor enforces read-only behaviour at the
    /// `DataStore` boundary: immutable queries continue to work, while any
    /// mutable script routed through [`DataStore::mutate`] is rejected.
    ///
    /// This is an APPLICATION-level guard only — it does not stop a caller
    /// that bypasses [`DataStore::mutate`] (or a bug in this crate) from
    /// reaching the underlying engine. Callers that need an
    /// engine/filesystem-enforced guarantee (for example, serving
    /// auto-discovered databases from an untrusted or unattended workspace)
    /// MUST use [`DataStore::open_engine_readonly`] instead.
    ///
    /// # Errors
    ///
    /// - [`GraphtorError::PathViolation`] — `path` escapes `root`
    /// - [`GraphtorError::Database`] — the database file does not exist, the
    ///   path contains non-UTF-8 bytes, or `CozoDB` fails to open the database
    pub fn open_sqlite_readonly(path: &Path, root: &Path) -> Result<Self, GraphtorError> {
        let safe_path = validate_path(path, root)?;
        if !safe_path.exists() {
            return Err(GraphtorError::Database {
                message: format!("database file '{}' does not exist", safe_path.display()),
                operation: "open_sqlite_readonly".to_string(),
            });
        }

        let db = open_sqlite_instance(&safe_path, "open_sqlite_readonly")?;
        let path_str = path_to_utf8(&safe_path, "open_sqlite_readonly")?;
        info!(path = path_str, "opened read-only SQLite DataStore");
        Ok(Self {
            db: Arc::new(db),
            access_mode: AccessMode::ReadOnly,
            backing_path: Some(safe_path),
            engine_readonly_guard: None,
        })
    }

    /// Open a `DataStore` backed by an existing `SQLite` file with an
    /// ENGINE/FILESYSTEM-enforced read-only guarantee.
    ///
    /// `CozoDB`'s public `SQLite` backend (`DbInstance::new("sqlite", ..)`)
    /// always opens its underlying connections with
    /// `SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE` and ignores the `options`
    /// string entirely — there is no supported way to ask Cozo itself for a
    /// read-only connection, and every new connection it opens from its
    /// internal pool (one per concurrent transaction, opened lazily) uses
    /// the same hard-coded flags. [`DataStore::open_sqlite_readonly`] only
    /// prevents writes at the `DataStore` boundary (the [`AccessMode`]
    /// guard in [`DataStore::mutate`]) — a caller that reaches the
    /// underlying `cozo::DbInstance` directly is not stopped.
    ///
    /// This constructor closes that gap at the filesystem level instead:
    /// before Cozo opens the file, it (and any existing `-wal`/`-shm`/
    /// `-journal` sidecars) are marked filesystem-readonly. `SQLite`'s
    /// documented open-path behaviour is to retry a `SQLITE_OPEN_READWRITE`
    /// request as read-only when the file cannot be opened for writing
    /// (rather than failing outright), so every connection Cozo opens
    /// against this file for the remainder of this `DataStore`'s lifetime —
    /// including ones opened later from its connection pool — becomes a
    /// genuine engine-enforced read-only connection: an actual write
    /// attempt is rejected by `SQLite` itself (`SQLITE_READONLY`), not merely
    /// by this crate's `AccessMode` check.
    ///
    /// The filesystem lock is held for as long as ANY clone of the returned
    /// `DataStore` is alive and is released automatically (best-effort) once
    /// the last clone is dropped, so the file can be opened read-write again
    /// later (for example, if the workspace is later reconfigured as a
    /// generation source).
    ///
    /// # Errors
    ///
    /// - [`GraphtorError::PathViolation`] — `path` escapes `root`
    /// - [`GraphtorError::Database`] — the database file does not exist, the
    ///   path contains non-UTF-8 bytes, or `CozoDB` fails to open the database
    /// - [`GraphtorError::Io`] — the filesystem read-only lock could not be
    ///   applied to the database file or one of its sidecars
    pub fn open_engine_readonly(path: &Path, root: &Path) -> Result<Self, GraphtorError> {
        let safe_path = validate_path(path, root)?;
        if !safe_path.exists() {
            return Err(GraphtorError::Database {
                message: format!("database file '{}' does not exist", safe_path.display()),
                operation: "open_engine_readonly".to_string(),
            });
        }

        let guard = EngineReadonlyGuard::lock(&safe_path)?;
        let db = open_sqlite_instance(&safe_path, "open_engine_readonly")?;
        let path_str = path_to_utf8(&safe_path, "open_engine_readonly")?;
        info!(
            path = path_str,
            "opened engine-enforced read-only SQLite DataStore (filesystem lock active)"
        );
        Ok(Self {
            db: Arc::new(db),
            access_mode: AccessMode::ReadOnly,
            backing_path: Some(safe_path),
            engine_readonly_guard: Some(Arc::new(guard)),
        })
    }

    /// Return the canonical filesystem path for a file-backed database.
    ///
    /// In-memory stores return `None`.
    #[must_use]
    pub fn database_path(&self) -> Option<&Path> {
        self.backing_path.as_deref()
    }

    /// Returns `true` when this store was opened via
    /// [`DataStore::open_engine_readonly`] and therefore holds an active
    /// filesystem-level read-only lock on its backing file (and any
    /// existing WAL/SHM/journal sidecars) for as long as this handle, or any
    /// clone of it, remains alive.
    #[must_use]
    pub fn is_engine_enforced_readonly(&self) -> bool {
        self.engine_readonly_guard.is_some()
    }

    /// Return the names of all stored relations present in the database.
    ///
    /// Queries the `CozoDB` system catalogue (`::relations`). Useful for
    /// diagnostic and integration-test purposes.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] on query failure.
    pub fn relation_names(&self) -> Result<Vec<String>, GraphtorError> {
        let rows = self.query("::relations", BTreeMap::new())?;
        Ok(rows
            .rows
            .iter()
            .filter_map(|row: &Vec<DataValue>| {
                row.first()
                    .and_then(|v: &DataValue| v.get_str())
                    .map(str::to_owned)
            })
            .collect())
    }

    /// Create all required database relations if they do not already exist.
    ///
    /// Delegates to [`crate::db::schema::ensure_schema`]. Safe to call
    /// on every startup (idempotent).
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from schema creation.
    pub fn ensure_schema(&self) -> Result<(), GraphtorError> {
        crate::db::schema::ensure_schema(self)
    }

    /// Return `true` if the named stored relation exists in the database.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] if the relations query fails.
    pub fn relation_exists(&self, name: &str) -> Result<bool, GraphtorError> {
        let rows = self.query("::relations", BTreeMap::new())?;
        Ok(rows
            .rows
            .iter()
            .any(|row| row.first().and_then(cozo::DataValue::get_str) == Some(name)))
    }

    /// Return a [`DbStatus`] snapshot describing the current state of the database.
    ///
    /// Queries `doc_sources`, `doc_chunks`, and `doc_schema_ver` to produce
    /// counts and the active schema version.  Safe to call at any time after
    /// [`DataStore::ensure_schema`].
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] if any of the status queries fail.
    pub fn get_status(&self) -> Result<DbStatus, GraphtorError> {
        let source_rows =
            self.query("?[source_id] := *doc_sources{ source_id }", BTreeMap::new())?;
        let chunk_rows = self.query("?[chunk_id] := *doc_chunks{ chunk_id }", BTreeMap::new())?;
        let schema_version = self.read_schema_version()?;

        Ok(DbStatus {
            // usize is at most 64 bits on all Rust targets, so the cast is safe.
            #[allow(clippy::cast_possible_truncation)]
            source_count: source_rows.rows.len() as u64,
            #[allow(clippy::cast_possible_truncation)]
            chunk_count: chunk_rows.rows.len() as u64,
            schema_version,
        })
    }

    /// Read the schema version stored in `doc_schema_ver`.
    ///
    /// Returns `0` if the relation is empty (schema not yet applied).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] if the query fails.
    fn read_schema_version(&self) -> Result<u32, GraphtorError> {
        let rows = self.query("?[ver] := *doc_schema_ver{ ver }", BTreeMap::new())?;
        if let Some(row) = rows.rows.into_iter().next() {
            if let Some(v) = row.into_iter().next() {
                if let Some(n) = v.get_int() {
                    return u32::try_from(n).map_err(|_| GraphtorError::Database {
                        message: format!("schema version {n} is out of u32 range"),
                        operation: "read_schema_version".to_string(),
                    });
                }
            }
        }
        Ok(0)
    }

    /// Return `true` if the database contains pre-v4 data that has not yet
    /// been rebuilt through the docline ingestion pipeline.
    ///
    /// Delegates to [`crate::db::schema::needs_v4_migration`]. Read-only —
    /// safe to call on any open store, including read-only handles.
    ///
    /// Query surfaces (`serve`, `status`) MUST check this and gate with an
    /// actionable error before exposing index data to callers.
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from the schema-version query.
    pub fn needs_v4_migration(&self) -> Result<bool, GraphtorError> {
        crate::db::schema::needs_v4_migration(self)
    }

    /// Return `true` when staged v4 retries must reuse the persisted frozen
    /// snapshot instead of refreezing from live input.
    ///
    /// This becomes `true` immediately before the destructive staged prune and
    /// clears after [`Self::mark_v4_migration_complete`] succeeds.
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from the lock-state query.
    pub fn v4_migration_snapshot_locked(&self) -> Result<bool, GraphtorError> {
        crate::db::schema::v4_migration_snapshot_locked(self)
    }

    /// Prune all pre-v4 ingested data without clearing the migration gate.
    ///
    /// Delegates to [`crate::db::schema::prune_v4_data_for_rebuild`]. Callers
    /// MUST follow this with [`Self::mark_v4_migration_complete`] only after a
    /// clean rebuild has finished.
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from the prune operation.
    pub fn prune_v4_data_for_rebuild(&self) -> Result<(), GraphtorError> {
        crate::db::schema::prune_v4_data_for_rebuild(self)
    }

    /// Mark a staged v4 rebuild as complete by stamping schema version 4.
    ///
    /// Delegates to [`crate::db::schema::mark_v4_migration_complete`].
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from the version-stamp
    /// operation.
    pub fn mark_v4_migration_complete(&self) -> Result<(), GraphtorError> {
        crate::db::schema::mark_v4_migration_complete(self)
    }

    /// Prune all pre-v4 ingested data and stamp the schema version as 4.
    ///
    /// Delegates to [`crate::db::schema::apply_v4_prune`]. This compatibility
    /// helper immediately marks the migration complete; new write paths SHOULD
    /// prefer the staged prune + completion methods instead.
    ///
    /// # Errors
    ///
    /// Propagates [`GraphtorError::Database`] from the migration operations.
    pub fn apply_v4_prune(&self) -> Result<(), GraphtorError> {
        crate::db::schema::apply_v4_prune(self)
    }

    /// Force-set the schema version stored in `doc_schema_ver`.
    ///
    /// Intended only for test use — use to simulate pre-v4 database state
    /// for migration regression tests.  Not part of the stable public API.
    #[doc(hidden)]
    pub fn set_schema_version_for_test(&self, ver: i64) -> Result<(), GraphtorError> {
        crate::db::schema::set_schema_version_for_test(self, ver)
    }

    /// Execute a read-only `CozoScript` query.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] on query failure.
    pub(crate) fn query(
        &self,
        script: &str,
        params: BTreeMap<String, DataValue>,
    ) -> Result<NamedRows, GraphtorError> {
        self.db
            .run_script(script, params, ScriptMutability::Immutable)
            .map_err(|e| GraphtorError::Database {
                message: e.to_string(),
                operation: "query".to_string(),
            })
    }

    /// Execute a `CozoScript` mutation (upsert, delete, schema DDL).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Database`] on mutation failure.
    pub(crate) fn mutate(
        &self,
        script: &str,
        params: BTreeMap<String, DataValue>,
    ) -> Result<NamedRows, GraphtorError> {
        if self.access_mode == AccessMode::ReadOnly {
            return Err(GraphtorError::Database {
                message: "attempted mutable operation on a read-only datastore".to_string(),
                operation: "mutate".to_string(),
            });
        }

        self.db
            .run_script(script, params, ScriptMutability::Mutable)
            .map_err(|e| GraphtorError::Database {
                message: e.to_string(),
                operation: "mutate".to_string(),
            })
    }
}

/// Filesystem-level read-only lock backing [`DataStore::open_engine_readonly`].
///
/// Marks the database file and any existing `-wal`/`-shm`/`-journal`
/// sidecars filesystem-readonly for as long as this guard is alive, and
/// restores their original writable state on [`Drop`]. Held inside an
/// [`Arc`] on [`DataStore`] so cloning the store shares one lock, released
/// only when the last clone is dropped.
///
/// A WAL-mode database's journal mode is recorded in the main file's own
/// header, so ANY connection that opens it — including a read-only
/// fallback connection — still expects the WAL shared-memory index and
/// transiently (re)creates an empty `-wal` file and a `-shm` bookkeeping
/// file if they are not already present, purely as an artifact of how
/// `SQLite` readers cooperate with WAL writers. Neither file receives any
/// served content — the main database file is proven byte-for-byte
/// unchanged (see the `store::tests` module) — but to leave **no** on-disk
/// trace once the read-only session ends, this guard also removes any such
/// sidecar it did not find pre-existing, restoring the workspace to
/// exactly its prior state.
struct EngineReadonlyGuard {
    /// Paths that existed before locking; marked readonly and restored on drop.
    guarded: Vec<PathBuf>,
    /// Sidecar paths that did NOT exist before locking; removed on drop if
    /// `SQLite`'s WAL-reader machinery transiently created them.
    transient_sidecars: Vec<PathBuf>,
}

impl EngineReadonlyGuard {
    /// Mark `db_path` and any existing WAL/SHM/journal sidecars
    /// filesystem-readonly, rolling back any partial change if a later
    /// sidecar cannot be locked. Also records which sidecar paths do not
    /// yet exist so they can be cleaned up on [`Drop`] if the read-only
    /// engine open transiently creates them.
    fn lock(db_path: &Path) -> Result<Self, GraphtorError> {
        let mut guarded: Vec<PathBuf> = Vec::new();
        let mut transient_sidecars: Vec<PathBuf> = Vec::new();
        let candidates = sidecar_candidates(db_path);
        for (index, candidate) in candidates.into_iter().enumerate() {
            if !candidate.exists() {
                // Index 0 is the main db file, which callers already verified
                // exists; only sidecars (index > 0) reach this branch.
                if index > 0 {
                    transient_sidecars.push(candidate);
                }
                continue;
            }
            if let Err(error) = set_readonly(&candidate, true) {
                for already_locked in guarded.iter().rev() {
                    let _ = set_readonly(already_locked, false);
                }
                return Err(error);
            }
            guarded.push(candidate);
        }
        Ok(Self {
            guarded,
            transient_sidecars,
        })
    }
}

impl Drop for EngineReadonlyGuard {
    fn drop(&mut self) {
        // Best-effort: Drop cannot propagate errors, and a failed restore
        // here only affects a FUTURE write-mode open of this same file —
        // `open_sqlite` self-heals exactly this case via
        // `clear_stale_readonly_lock`, and the file itself is untouched
        // (still fully readable) regardless of whether this restore
        // succeeds.
        for path in &self.guarded {
            let _ = set_readonly(path, false);
        }
        // Remove any WAL-reader bookkeeping sidecar this session created so
        // the workspace shows no persistent trace of having been served.
        for path in &self.transient_sidecars {
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }
    }
}

/// Return the database file path plus its conventional `-wal`, `-shm`, and
/// `-journal` sidecar paths (`SQLite` appends these suffixes directly to the
/// filename; they are never present all at once, but any subset may exist
/// depending on journal mode and checkpoint state).
fn sidecar_candidates(db_path: &Path) -> [PathBuf; 4] {
    [
        db_path.to_path_buf(),
        append_suffix(db_path, "-wal"),
        append_suffix(db_path, "-shm"),
        append_suffix(db_path, "-journal"),
    ]
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os_string = path.as_os_str().to_owned();
    os_string.push(suffix);
    PathBuf::from(os_string)
}

fn set_readonly(path: &Path, readonly: bool) -> Result<(), GraphtorError> {
    let metadata = fs::metadata(path).map_err(GraphtorError::from)?;
    let mut permissions = metadata.permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).map_err(GraphtorError::from)
}

/// Clear a stale filesystem-readonly lock left behind by a crashed
/// [`DataStore::open_engine_readonly`] session, so [`DataStore::open_sqlite`]
/// never silently falls back to a read-only connection because of leftover
/// state from a previous process. Idempotent and a no-op when nothing is
/// locked.
fn clear_stale_readonly_lock(db_path: &Path) -> Result<(), GraphtorError> {
    for candidate in sidecar_candidates(db_path) {
        if !candidate.exists() {
            continue;
        }
        if fs::metadata(&candidate)
            .map_err(GraphtorError::from)?
            .permissions()
            .readonly()
        {
            set_readonly(&candidate, false)?;
        }
    }
    Ok(())
}

fn path_to_utf8(path: &Path, operation: &str) -> Result<String, GraphtorError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Database {
            message: "database path contains non-UTF-8 characters".to_string(),
            operation: operation.to_string(),
        })
}

fn open_sqlite_instance(path: &Path, operation: &str) -> Result<DbInstance, GraphtorError> {
    let path_str = path_to_utf8(path, operation)?;
    DbInstance::new("sqlite", &path_str, Default::default()).map_err(|error| {
        GraphtorError::Database {
            message: error.to_string(),
            operation: operation.to_string(),
        }
    })
}

fn ensure_database_parent_dir(path: &Path) -> Result<(), GraphtorError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(GraphtorError::from)?;
    }
    Ok(())
}

fn configure_sqlite_wal(path: &Path) -> Result<(), GraphtorError> {
    // Cozo's SQLite backend ignores the `options` string passed to `DbInstance::new`
    // for this engine, so WAL must be enabled directly against the database file
    // before Cozo opens its own connection pool.
    let connection = Connection::open_thread_safe_with_flags(
        path,
        OpenFlags::new().with_create().with_read_write(),
    )
    .map_err(|error| GraphtorError::Database {
        message: error.to_string(),
        operation: "open_sqlite".to_string(),
    })?;

    connection
        .execute("PRAGMA journal_mode=WAL;")
        .map_err(|error| GraphtorError::Database {
            message: error.to_string(),
            operation: "open_sqlite".to_string(),
        })?;

    let mut statement = connection
        .prepare("PRAGMA journal_mode;")
        .map_err(|error| GraphtorError::Database {
            message: error.to_string(),
            operation: "open_sqlite".to_string(),
        })?;
    let mode = match statement.next().map_err(|error| GraphtorError::Database {
        message: error.to_string(),
        operation: "open_sqlite".to_string(),
    })? {
        State::Row => statement
            .read::<String, _>(0)
            .map_err(|error| GraphtorError::Database {
                message: error.to_string(),
                operation: "open_sqlite".to_string(),
            })?,
        State::Done => {
            return Err(GraphtorError::Database {
                message: "PRAGMA journal_mode returned no rows".to_string(),
                operation: "open_sqlite".to_string(),
            })
        }
    };

    if !mode.eq_ignore_ascii_case("wal") {
        return Err(GraphtorError::Database {
            message: format!("failed to enable WAL mode (got '{mode}')"),
            operation: "open_sqlite".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use cozo::{DataValue, Num, ScriptMutability};

    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    /// Build a populated fixture database (schema + one chunk row with an
    /// embedding) at `db_path`, fully checkpoint the WAL into the main file,
    /// and return once every handle has been dropped so the file is not held
    /// open by this process.
    fn build_populated_fixture(db_path: &Path, root: &Path) {
        {
            let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
            store.ensure_schema().expect("ensure_schema for fixture");
            let mut params = BTreeMap::new();
            params.insert(
                "source_id".to_string(),
                DataValue::Str("fixture-src".into()),
            );
            params.insert(
                "url".to_string(),
                DataValue::Str("https://example.com".into()),
            );
            params.insert("kind".to_string(), DataValue::Str("local".into()));
            params.insert("name".to_string(), DataValue::Str("fixture-src".into()));
            store
                .mutate(
                    "?[source_id, url, kind, name, synced_at] <- [[$source_id, $url, $kind, $name, null]] \
                     :put doc_sources { source_id => url, kind, name, synced_at }",
                    params,
                )
                .expect("seed doc_sources for fixture");
        }
        // Fully checkpoint + truncate the WAL into the main db file so the
        // before/after fingerprint captures a stable, fully-flushed baseline.
        let connection = sqlite::Connection::open_thread_safe(db_path)
            .expect("open raw connection for checkpoint");
        connection
            .execute("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("checkpoint WAL before fingerprinting");
        drop(connection);
    }

    fn sidecar_paths_for_test(db_path: &Path) -> Vec<PathBuf> {
        sidecar_candidates(db_path)[1..].to_vec()
    }

    #[test]
    fn open_engine_readonly_rejects_direct_engine_mutation_not_just_the_guard() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro.db");
        build_populated_fixture(&db_path, dir.path());

        let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
            .expect("open_engine_readonly should succeed against an existing v4 fixture");

        // Bypass `DataStore::mutate`'s own `AccessMode` guard entirely and call
        // the underlying `cozo::DbInstance::run_script` DIRECTLY with
        // `ScriptMutability::Mutable`. If the guard were the only thing
        // enforcing read-only behaviour, this call would succeed (it never
        // passes through `mutate()`). Proving this call itself fails
        // demonstrates the rejection happens at the SQLite/engine boundary.
        let mut params = BTreeMap::new();
        params.insert(
            "source_id".to_string(),
            DataValue::Str("should-never-be-written".into()),
        );
        params.insert(
            "url".to_string(),
            DataValue::Str("https://example.com".into()),
        );
        params.insert("kind".to_string(), DataValue::Str("local".into()));
        params.insert(
            "name".to_string(),
            DataValue::Str("should-never-be-written".into()),
        );
        let result = readonly.db.run_script(
            "?[source_id, url, kind, name, synced_at] <- [[$source_id, $url, $kind, $name, null]] \
             :put doc_sources { source_id => url, kind, name, synced_at }",
            params,
            ScriptMutability::Mutable,
        );

        assert!(
            result.is_err(),
            "a direct engine-level write against an engine-readonly store must be rejected \
             by SQLite itself, not merely by the DataStore::mutate guard"
        );

        drop(readonly);
    }

    #[test]
    fn open_engine_readonly_permits_a_full_query_search_semantic_read_cycle() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-read.db");
        build_populated_fixture(&db_path, dir.path());

        let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
            .expect("open_engine_readonly should succeed");

        // query
        let status = readonly
            .get_status()
            .expect("status query must succeed read-only");
        assert_eq!(status.source_count, 1);

        // search (text)
        let names = readonly
            .query("?[source_id] := *doc_sources{ source_id }", BTreeMap::new())
            .expect("relation query must succeed read-only");
        assert_eq!(names.rows.len(), 1);

        // semantic (vector) — exercise the same query surface used by
        // `search_by_vector`, without requiring a loaded embedding model.
        let floats: Vec<DataValue> = vec![0.0_f32; 384]
            .iter()
            .map(|&x| DataValue::Num(Num::Float(f64::from(x))))
            .collect();
        let mut params = BTreeMap::new();
        params.insert("query".to_string(), DataValue::List(floats));
        let semantic = readonly.query(
            "?[chunk_id, dist] := q = vec($query), *doc_chunks{ chunk_id, embedding }, \
             !is_null(embedding), dist = cos_dist(q, embedding) :order dist :limit 5",
            params,
        );
        assert!(
            semantic.is_ok(),
            "semantic-shaped query must succeed read-only: {semantic:?}"
        );

        drop(readonly);
    }

    #[test]
    fn open_engine_readonly_leaves_db_and_sidecars_byte_identical_after_read_cycle() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-fingerprint.db");
        build_populated_fixture(&db_path, dir.path());

        let bytes_before = fs::read(&db_path).expect("read fixture db bytes before open");
        let mtime_before = fs::metadata(&db_path)
            .expect("stat fixture db before open")
            .modified()
            .expect("mtime before open");
        let sidecars_existed_before: Vec<bool> = sidecar_paths_for_test(&db_path)
            .iter()
            .map(|p| p.exists())
            .collect();

        {
            let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
                .expect("open_engine_readonly should succeed");
            let _ = readonly.get_status().expect("status read");
            let _ = readonly
                .query("?[source_id] := *doc_sources{ source_id }", BTreeMap::new())
                .expect("relation read");
        }

        let bytes_after = fs::read(&db_path).expect("read fixture db bytes after open");
        let mtime_after = fs::metadata(&db_path)
            .expect("stat fixture db after open")
            .modified()
            .expect("mtime after open");
        let sidecars_exist_after: Vec<bool> = sidecar_paths_for_test(&db_path)
            .iter()
            .map(|p| p.exists())
            .collect();

        assert_eq!(
            bytes_before, bytes_after,
            "engine-readonly open must not change a single byte of the served db file"
        );
        assert_eq!(
            mtime_before, mtime_after,
            "engine-readonly open must not touch mtime"
        );
        assert_eq!(
            sidecars_existed_before, sidecars_exist_after,
            "engine-readonly open must not create -wal/-shm/-journal sidecars that did not \
             already exist"
        );
    }

    #[test]
    fn open_engine_readonly_restores_writability_on_drop() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-restore.db");
        build_populated_fixture(&db_path, dir.path());

        {
            let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
                .expect("open_engine_readonly should succeed");
            assert!(
                fs::metadata(&db_path).unwrap().permissions().readonly(),
                "db file must be filesystem-readonly while the guard is held"
            );
            drop(readonly);
        }

        assert!(
            !fs::metadata(&db_path).unwrap().permissions().readonly(),
            "db file must be restored to writable once the engine-readonly store is dropped"
        );

        // A later legitimate read-write open (e.g. for `sync`) must still work.
        let rw = DataStore::open_sqlite(&db_path, dir.path())
            .expect("a subsequent read-write open must succeed after the guard is released");
        rw.ensure_schema().expect("ensure_schema after restore");
    }

    #[test]
    fn open_sqlite_clears_a_stale_readonly_lock_left_by_a_crashed_session() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-stale.db");
        build_populated_fixture(&db_path, dir.path());

        // Simulate a crashed engine-readonly session: the guard is
        // constructed and then leaked (never dropped), exactly as would
        // happen if the process were killed instead of exiting cleanly.
        let guard = EngineReadonlyGuard::lock(&db_path).expect("lock db read-only");
        std::mem::forget(guard);
        assert!(
            fs::metadata(&db_path).unwrap().permissions().readonly(),
            "precondition: db file must be readonly after the simulated crash"
        );

        let rw = DataStore::open_sqlite(&db_path, dir.path()).expect(
            "open_sqlite must self-heal a stale readonly lock rather than silently \
                     downgrading to read-only or failing",
        );
        rw.ensure_schema().expect("ensure_schema after self-heal");
        assert!(
            !fs::metadata(&db_path).unwrap().permissions().readonly(),
            "open_sqlite must clear the stale readonly attribute"
        );
    }

    // ── P1-T4: ATTACH / loadable-extension hardening ───────────────────────
    //
    // Cozo's query language (CozoScript) has no raw-SQL escape hatch: its
    // parser (a Pest grammar) does not define `ATTACH DATABASE` as valid
    // syntax, and its built-in function library has no `load_extension`
    // equivalent. The underlying SQLite connection is used purely as an
    // internal single-table key-value store (`cozo(k BLOB, v BLOB)`) that
    // only Cozo's own storage layer ever touches directly — a served
    // read-only store (auto-discovered OR an explicit workspace-contained
    // entry; the primitive is identical either way) can therefore never be
    // asked, via any query this crate exposes, to ATTACH another file or
    // load a native-code extension. These tests PROVE that architectural
    // invariant empirically rather than merely asserting it.

    #[test]
    fn engine_readonly_store_rejects_attach_database_as_invalid_cozoscript() {
        let dir = temp_dir();
        let db_path = dir.path().join("attach-hardening.db");
        build_populated_fixture(&db_path, dir.path());

        let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
            .expect("open_engine_readonly should succeed");

        let result = readonly.query("ATTACH DATABASE 'other.db' AS evil", BTreeMap::new());

        assert!(
            result.is_err(),
            "ATTACH DATABASE is not valid CozoScript syntax and must be rejected, proving no \
             raw-SQL escape hatch exists for a served read-only store"
        );
    }

    #[test]
    fn engine_readonly_store_has_no_load_extension_function() {
        let dir = temp_dir();
        let db_path = dir.path().join("extension-hardening.db");
        build_populated_fixture(&db_path, dir.path());

        let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
            .expect("open_engine_readonly should succeed");

        let result = readonly.query(
            "?[loaded] := loaded = load_extension('evil.so')",
            BTreeMap::new(),
        );

        assert!(
            result.is_err(),
            "load_extension is not a CozoScript built-in function and must be rejected, \
             proving no loadable-extension surface exists for a served read-only store"
        );
    }

    #[test]
    fn engine_readonly_store_explicit_entry_scenario_is_identically_hardened() {
        // The hardening primitive does not distinguish HOW a served path was
        // discovered (auto-discovery vs. an explicit workspace-contained
        // `type: database` entry, P1-T6) — both flow through
        // `open_engine_readonly` identically, so the SAME ATTACH/extension
        // immunity applies uniformly. This test opens a second, differently
        // named fixture standing in for an explicit entry to make that
        // uniformity explicit in the test suite rather than merely implicit
        // in the shared code path.
        let dir = temp_dir();
        let db_path = dir.path().join("explicit-entry-served.db");
        build_populated_fixture(&db_path, dir.path());

        let readonly = DataStore::open_engine_readonly(&db_path, dir.path()).expect(
            "open_engine_readonly should succeed for an explicit workspace-contained entry",
        );

        let attach_result = readonly.query("ATTACH DATABASE 'other.db' AS evil", BTreeMap::new());
        let extension_result = readonly.query(
            "?[loaded] := loaded = load_extension('evil.so')",
            BTreeMap::new(),
        );

        assert!(attach_result.is_err());
        assert!(extension_result.is_err());
    }
}
