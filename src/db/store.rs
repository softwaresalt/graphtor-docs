//! `DataStore` — the unified `CozoDB` database handle.
//!
//! All read and write operations in the `db` module funnel through
//! [`DataStore`], which wraps [`cozo::DbInstance`] behind an [`std::sync::Arc`]
//! so that handles can be cheaply cloned and shared across threads.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use cozo::{DataValue, DbInstance, NamedRows, ScriptMutability};
use tracing::info;

use crate::error::GraphtorError;
use crate::path::validate_path;

/// A cloneable, thread-safe handle to the embedded [`DbInstance`].
///
/// Wraps `cozo::DbInstance` in an [`Arc`] so that clones share the same
/// underlying database connection pool without copying state.
#[derive(Clone)]
pub struct DataStore {
    pub(crate) db: Arc<DbInstance>,
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
        Ok(Self { db: Arc::new(db) })
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
        let path_str = safe_path.to_str().ok_or_else(|| GraphtorError::Database {
            message: "database path contains non-UTF-8 characters".to_string(),
            operation: "open_sqlite".to_string(),
        })?;
        let db = DbInstance::new("sqlite", path_str, Default::default()).map_err(|e| {
            GraphtorError::Database {
                message: e.to_string(),
                operation: "open_sqlite".to_string(),
            }
        })?;
        info!(path = path_str, "opened SQLite DataStore");
        Ok(Self { db: Arc::new(db) })
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
        self.db
            .run_script(script, params, ScriptMutability::Mutable)
            .map_err(|e| GraphtorError::Database {
                message: e.to_string(),
                operation: "mutate".to_string(),
            })
    }
}
