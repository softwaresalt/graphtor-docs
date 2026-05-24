//! `DataStore` — the unified `CozoDB` database handle.
//!
//! All read and write operations in the `db` module funnel through
//! [`DataStore`], which wraps [`cozo::DbInstance`] behind an [`std::sync::Arc`]
//! so that handles can be cheaply cloned and shared across threads.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
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
        configure_sqlite_wal(&safe_path)?;
        let db = open_sqlite_instance(&safe_path, "open_sqlite")?;
        let path_str = path_to_utf8(&safe_path, "open_sqlite")?;
        info!(path = path_str, "opened SQLite DataStore");
        Ok(Self {
            db: Arc::new(db),
            access_mode: AccessMode::ReadWrite,
        })
    }

    /// Open a query-only `DataStore` backed by an existing `SQLite` file.
    ///
    /// `CozoDB`'s public `SQLite` API does not expose a true read-only connection
    /// flag, so this constructor enforces read-only behaviour at the
    /// `DataStore` boundary: immutable queries continue to work, while any
    /// mutable script routed through [`DataStore::mutate`] is rejected.
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
        })
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
