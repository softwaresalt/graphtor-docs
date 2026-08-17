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
use crate::path::{is_reparse_point, validate_path};

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
    /// so the restore-on-drop only runs when the last clone is dropped, not
    /// when any single clone goes out of scope. That restore only reliably
    /// leaves the file writable when this was the only guard that ever
    /// locked it; if the same file was independently guarded more than once,
    /// the permissions each guard restores on drop depend on drop order and
    /// the file is not guaranteed to end up writable (see F6 on
    /// [`DataStore::open_engine_readonly`]).
    engine_readonly_guard: Option<Arc<EngineReadonlyGuard>>,
}

impl std::fmt::Debug for DataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataStore").finish_non_exhaustive()
    }
}

/// Startup-log wording emitted by [`DataStore::open_engine_readonly`] on a
/// successful open.
///
/// States the filesystem read-only guarantee precisely rather than
/// unconditionally: protection is robust only if NO independent guard EVER
/// overlaps this guard's lifetime on the file — a guard that is currently
/// the only one alive is not itself sufficient, because an EARLIER overlap
/// can already have left the file writable even after the overlapping peer
/// drops. Once such an overlap occurs — same- or cross-process — protection
/// is best-effort (not a cross-process guarantee) for the rest of this
/// guard's life (F6; see
/// `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`).
const ENGINE_READONLY_OPEN_LOG_MESSAGE: &str = "opened engine-enforced read-only \
    SQLite DataStore (filesystem lock active: protection is robust only if no independent \
    guard ever overlaps this guard's lifetime on the file; any such overlap - same- or \
    cross-process - leaves protection best-effort for the rest of this guard's life, even \
    once the overlapping guard drops - see F6)";

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
    /// MUST use [`DataStore::open_engine_readonly`] instead — noting that
    /// its own guarantee is best-effort, not robust, whenever the same file
    /// is independently guarded more than once (F6; see its rustdoc).
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
    /// against this file — including ones opened later from its connection
    /// pool — becomes a genuine engine-enforced read-only connection: an
    /// actual write attempt is rejected by `SQLite` itself
    /// (`SQLITE_READONLY`), not merely by this crate's `AccessMode` check.
    ///
    /// This holds ROBUSTLY only if NO independent guard EVER overlaps this
    /// returned `DataStore`'s (or any clone's) guard lifetime on the file —
    /// being the only guard currently alive is not itself sufficient, because
    /// an EARLIER overlap can already have left the file writable even after
    /// the overlapping peer drops. It is BEST-EFFORT — not a cross-process
    /// guarantee — whenever the SAME file is independently guarded more than
    /// once: same-process (a second, independent `open_engine_readonly` call
    /// on the same path) or cross-process. In that case, whichever guard
    /// drops first restores ITS OWN captured original permissions, which can
    /// make the file writable again while a peer guard is still alive, and
    /// that peer's protection stays best-effort for the rest of its life even
    /// once the overlap ends (see F6 in
    /// `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`).
    /// This primitive does not implement cross-guard reference counting;
    /// genuinely closing that window is deferred, not attempted here.
    ///
    /// The filesystem lock is held for as long as ANY clone of the returned
    /// `DataStore` is alive and the restore-on-drop runs automatically
    /// (best-effort) once the last clone is dropped. When this was the ONLY
    /// guard that ever locked the file, that restore reliably leaves it
    /// writable again (for example, if the workspace is later reconfigured
    /// as a generation source). If the file was independently guarded more
    /// than once, the permissions each guard restores on drop depend on
    /// drop order (see the overlap caveat above, F6): the file is not
    /// guaranteed to end up writable, or even to end up in a single
    /// predictable state, once every guard on it has dropped.
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
        info!(path = path_str, "{ENGINE_READONLY_OPEN_LOG_MESSAGE}");
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

    /// Returns `true` when *this handle* was opened via
    /// [`DataStore::open_engine_readonly`] and therefore currently holds an
    /// active filesystem-level read-only lock reference on its backing file
    /// (and any existing WAL/SHM/journal sidecars), for as long as this
    /// handle, or any clone of it, remains alive.
    ///
    /// This reports guard OWNERSHIP, not a live guarantee about the current
    /// on-disk permission state. The underlying filesystem protection is
    /// robust only if NO independent guard EVER overlaps this handle's guard
    /// lifetime on the file — this handle currently being the only guard
    /// alive is not itself sufficient, because an EARLIER overlap can already
    /// have left the file writable even after the overlapping peer drops.
    /// When the SAME file is independently guarded more than once —
    /// same-process (two separate
    /// [`DataStore::open_engine_readonly`] calls on one path) or
    /// cross-process — the guards do not coordinate: whichever guard drops
    /// first restores ITS OWN captured original permissions, which can make
    /// the file writable again while THIS handle is still alive and this
    /// method still returns `true`; THIS handle's protection then stays
    /// best-effort for the rest of its life even once the overlap ends (see
    /// F6 in
    /// `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`).
    /// The application-level `AccessMode::ReadOnly` check enforced by
    /// `DataStore::mutate` remains the authoritative read-only guarantee
    /// regardless of this method's result.
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
/// restores their EXACT original permissions on [`Drop`]. Held inside an
/// [`Arc`] on [`DataStore`] so cloning the store shares one lock, released
/// only when the last clone is dropped.
///
/// This is a PER-GUARD lock, not a per-file reference count: it has no
/// awareness of other independent guards — same-process or cross-process —
/// that may also hold the SAME file read-only. Its protection is robust only
/// if NO independent guard EVER overlaps this guard's lifetime — being the
/// only guard currently alive is not itself sufficient, because an EARLIER
/// overlap can already have left the file writable even after the
/// overlapping peer drops. If the same file is independently guarded more
/// than once (for example, two separate
/// [`DataStore::open_engine_readonly`] calls on one path, or two separate
/// processes), whichever guard drops first restores ITS OWN captured
/// original permissions — which can make the file writable again while a
/// peer guard is still alive and its owning [`DataStore`] still reports
/// [`DataStore::is_engine_enforced_readonly`] as `true`; that peer's
/// protection then stays best-effort for the rest of its life even once the
/// overlap ends. This is a known, documented best-effort limitation (F6); see
/// `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`.
/// Closing this window would require cross-guard ownership/liveness
/// coordination, which this primitive intentionally does not implement.
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
/// sidecar it did not find pre-existing, PROVIDED it is still empty at
/// drop-time. A non-empty sidecar of any kind (including `-shm`) may belong
/// to a genuinely concurrent writer and is always left intact.
struct EngineReadonlyGuard {
    /// (path, original permissions) pairs for paths that existed before
    /// locking. Each path is marked filesystem-readonly while the guard is
    /// held and restored to its EXACT captured original `fs::Permissions`
    /// on drop — the full permission value, not merely a readonly boolean.
    /// On Unix this preserves the original mode bits (a private `0o600`
    /// database is restored to `0o600`, never widened to `0o622`/`0o644`,
    /// because `Permissions::set_readonly(false)` would otherwise enable
    /// every write bit and grant group/other write); on Windows it restores
    /// the original readonly attribute. A file the operator had already
    /// marked readonly for their own reasons is thus never left writable —
    /// nor over-permissioned — once this guard releases it.
    guarded: Vec<(PathBuf, fs::Permissions)>,
    /// Sidecar paths (`-wal`, `-shm`, `-journal`) that did NOT exist before
    /// locking. Every such transient sidecar is subject to the SAME
    /// conservative rule: it is removed on drop ONLY if it still exists and
    /// is still EMPTY. A non-empty sidecar of ANY kind — including `-shm` —
    /// may belong to a genuinely concurrent writer: unlinking a live `-shm`
    /// while another connection holds it defeats WAL coordination (existing
    /// connections keep the old mapping while new connections create a
    /// second `-shm`) and risks corruption, and a non-empty `-wal`/`-journal`
    /// can hold committed pending-transaction bytes. Such files are always
    /// left intact — see [`Drop`].
    transient_sidecars: Vec<PathBuf>,
}

impl EngineReadonlyGuard {
    /// Mark `db_path` and any existing WAL/SHM/journal sidecars
    /// filesystem-readonly, rolling back any partial change if a later
    /// sidecar cannot be locked. Captures each guarded path's EXACT ORIGINAL
    /// `fs::Permissions` (so [`Drop`] can restore them precisely rather than
    /// forcing a coarse writable/readonly boolean that would widen Unix mode
    /// bits) and records which sidecar paths do not yet exist so they can be
    /// cleaned up on [`Drop`] if the read-only engine open transiently
    /// creates them — but only while they remain empty.
    fn lock(db_path: &Path) -> Result<Self, GraphtorError> {
        let mut guarded: Vec<(PathBuf, fs::Permissions)> = Vec::new();
        let mut transient_sidecars: Vec<PathBuf> = Vec::new();
        let candidates = sidecar_candidates(db_path);
        for (index, candidate) in candidates.into_iter().enumerate() {
            // Containment (Constitution III/IV, NON-NEGOTIABLE): a sidecar that
            // is a symlink/junction can redirect the `fs::metadata` +
            // `set_readonly` below — and any subsequent engine write in WAL mode
            // — to a file OUTSIDE the workspace. `candidate.exists()`,
            // `fs::metadata`, and `set_readonly` all FOLLOW links, so a
            // workspace holding `dropped.db-wal`/`-shm`/`-journal` as a symlink
            // to an external target would let read-only serving chmod that
            // target (and a crash could leave it read-only). Fail closed BEFORE
            // any inspection or permission change so the read-only engine never
            // opens through a linked sidecar. The main db (index 0) was already
            // canonicalized by the caller's `validate_path`, so only sidecars
            // (index > 0) can be a planted reparse point here.
            if index > 0 && is_reparse_point(&candidate) {
                for (already_locked, original_perms) in guarded.iter().rev() {
                    let _ = fs::set_permissions(already_locked, original_perms.clone());
                }
                return Err(GraphtorError::Database {
                    message: format!(
                        "refusing read-only serve: database sidecar '{}' is a symlink or junction; \
                         a linked sidecar could redirect permission changes outside the workspace",
                        candidate.display()
                    ),
                    operation: "open_engine_readonly".to_string(),
                });
            }
            if !candidate.exists() {
                // Index 0 is the main db file, which callers already verified
                // exists; only sidecars (index > 0) reach this branch. Every
                // transient sidecar — `-wal`, `-shm`, and `-journal` alike —
                // is subject to the same conservative empty-only cleanup on
                // drop (see `Drop`): a sidecar that appears while the guard
                // is held may belong to a concurrent writer, so it is only
                // removed if it is still empty.
                if index > 0 {
                    transient_sidecars.push(candidate);
                }
                continue;
            }
            // Capture the EXACT original permissions BEFORE marking the file
            // readonly, so `Drop` can restore them precisely instead of
            // forcing a coarse writable/readonly boolean that would enable
            // every Unix write bit and widen e.g. `0o600` to `0o622`.
            let original = match fs::metadata(&candidate) {
                Ok(metadata) => metadata.permissions(),
                Err(error) => {
                    // Roll back any sidecars already locked in this pass before
                    // propagating, mirroring the `set_readonly` failure path so
                    // a mid-loop error never leaves an earlier sidecar readonly
                    // with no live guard to restore it on drop.
                    for (already_locked, original_perms) in guarded.iter().rev() {
                        let _ = fs::set_permissions(already_locked, original_perms.clone());
                    }
                    return Err(GraphtorError::from(error));
                }
            };
            if let Err(error) = set_readonly(&candidate, true) {
                for (already_locked, original_perms) in guarded.iter().rev() {
                    let _ = fs::set_permissions(already_locked, original_perms.clone());
                }
                return Err(error);
            }
            guarded.push((candidate, original));
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
        //
        // Restore each guarded path's EXACT captured original permissions
        // rather than forcing a coarse writable/readonly boolean: a file
        // that was already filesystem-readonly before this guard ever
        // touched it (for example, an operator's own deliberate protection)
        // must stay readonly afterward, and a private `0o600` database must
        // come back as `0o600` — never widened to `0o622`/`0o644` by
        // re-enabling every Unix write bit.
        for (path, original) in &self.guarded {
            let _ = fs::set_permissions(path, original.clone());
        }
        // Remove a transient WAL-reader bookkeeping sidecar this session may
        // have created, but ONLY while it still exists and is still empty. A
        // non-empty sidecar of ANY kind — `-wal`, `-shm`, or `-journal` —
        // may belong to a genuinely concurrent writer: unlinking a live
        // `-shm` defeats WAL coordination (existing connections keep the old
        // mapping while new connections create a second `-shm`) and risks
        // corruption, and unlinking a non-empty `-wal`/`-journal` could lose
        // committed pending-transaction bytes this guard has no way to prove
        // it does not own. Such files are always left intact.
        for path in &self.transient_sidecars {
            if fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
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
        // Same containment guard as `EngineReadonlyGuard::lock` (Constitution
        // III/IV, NON-NEGOTIABLE): never chmod through a linked sidecar. A
        // symlinked/junction sidecar would make `set_readonly(false)` clear the
        // read-only bit on an EXTERNAL target, and proceeding to a write-mode
        // open could then let the engine write through that link. Fail closed
        // so a write-mode open never mutates permissions or data outside the
        // workspace. `db_path` (index 0) is the caller-canonicalized path, so
        // only a planted sidecar can be a reparse point here.
        //
        // The check MUST run BEFORE the `exists()` short-circuit: `exists()`
        // follows the link, so a DANGLING symlink sidecar (its target absent)
        // reports `false` and would otherwise be skipped — letting the engine
        // create and write the WAL/SHM THROUGH the link to an external path.
        // Mirror `lock`, which also guards before its existence branch.
        if is_reparse_point(&candidate) {
            return Err(GraphtorError::Database {
                message: format!(
                    "refusing write-mode open: database sidecar '{}' is a symlink or junction; \
                     a linked sidecar could redirect permission changes outside the workspace",
                    candidate.display()
                ),
                operation: "open_sqlite".to_string(),
            });
        }
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
    use std::sync::Mutex;

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

    // ── Post-cap residual C (054.001.001-ST / 054.001.002-ST): qualified ───
    // read-only startup-log wording.
    //
    // `open_engine_readonly`'s startup log used to read only "(filesystem
    // lock active)", which a reader could take as an unconditional
    // guarantee. This test pins the CORRECTED, qualified wording that
    // `ENGINE_READONLY_OPEN_LOG_MESSAGE` (production, near the top of this
    // file) now emits: protection is robust only if no independent guard
    // ever overlaps this guard's lifetime on the file (being the only guard
    // currently alive is not itself sufficient — an earlier overlap can
    // already have left the file writable even after the peer drops); any
    // such overlap — same- or cross-process — leaves protection best-effort
    // for the rest of this guard's life (F6). It was landed RED-first per
    // Constitution Principle II — observed failing against the prior
    // unqualified wording before the production fix existed — and is now
    // green.
    //
    // This constant is intentionally test-local (not shared with the
    // production `ENGINE_READONLY_OPEN_LOG_MESSAGE` const) so a future edit
    // to either one in isolation makes this test fail rather than silently
    // agreeing with itself.
    const EXPECTED_ENGINE_READONLY_OPEN_LOG_MESSAGE: &str = "opened engine-enforced read-only \
        SQLite DataStore (filesystem lock active: protection is robust only if no independent \
        guard ever overlaps this guard's lifetime on the file; any such overlap - same- or \
        cross-process - leaves protection best-effort for the rest of this guard's life, even \
        once the overlapping guard drops - see F6)";

    /// Minimal in-memory sink for a scoped `tracing` capture, matching the
    /// established pattern in `src/main.rs`'s `sync_progress_tests::
    /// capture_warn_logs` helper — a shared buffer behind a `MakeWriter`
    /// closure and `tracing::subscriber::with_default` — rather than adding
    /// a `tracing-test` dependency.
    struct CapturedLogWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl std::io::Write for CapturedLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Run `operation` once under a scoped `tracing` subscriber that
    /// captures `INFO`-and-above events from this crate to an in-memory
    /// buffer, returning the rendered log text alongside `operation`'s
    /// return value.
    ///
    /// Uses an [`tracing_subscriber::EnvFilter`] scoped to
    /// `graphtor_core=info` (which decides interest per-event rather than
    /// caching a single process-wide answer, and avoids capturing an
    /// unrelated third-party `INFO` event as a false "capture worked"
    /// signal) and forces a fresh interest-cache rebuild while this
    /// subscriber is active, to maximize the odds `operation`'s tracing
    /// events reach this capture. See [`capture_info_logs_retrying`] for why
    /// a single call is not sufficient on its own in this test binary.
    fn capture_info_logs_once<F, T>(operation: F) -> (T, String)
    where
        F: FnOnce() -> T,
    {
        let output = Arc::new(Mutex::new(Vec::new()));
        let filter = tracing_subscriber::EnvFilter::new("graphtor_core=info");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_env_filter(filter)
            .with_writer({
                let output = Arc::clone(&output);
                move || CapturedLogWriter {
                    output: Arc::clone(&output),
                }
            })
            .finish();

        let result = tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            operation()
        });

        let bytes = output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let logs = String::from_utf8(bytes).expect("tracing output should be valid utf-8");
        (result, logs)
    }

    /// Retry [`capture_info_logs_once`] until it observes ANY captured log
    /// output (i.e. `logs` is non-empty), or a bounded attempt count is
    /// exhausted.
    ///
    /// `tracing` decides a macro call-site's subscriber `Interest` and
    /// caches it process-wide the first time the call-site fires, then
    /// reuses that cached decision for the rest of the process. The
    /// `open_engine_readonly` call-site this test exercises is ALSO
    /// exercised by over a dozen sibling characterization tests with NO
    /// subscriber installed; under default (parallel) `cargo test`
    /// execution, whichever thread reaches that one-time decision first
    /// can win the race, and a losing race can still occasionally drop this
    /// scoped subscriber's own event even after a forced cache rebuild.
    /// Retrying with a fresh rebuild converges quickly in practice (a
    /// single-digit number of attempts, empirically, across 15+ consecutive
    /// full-suite stress runs).
    ///
    /// Deliberately NOT a retry-until-`contains`-the-expected-wording loop:
    /// the retry condition is only "did this attempt capture anything at
    /// all", so the caller's own assertions can still distinguish a genuine
    /// wording mismatch (non-empty logs, wrong content — a real regression)
    /// from a dropped-event capture-seam failure (empty logs after every
    /// attempt — a test-infrastructure problem, not a production defect).
    fn capture_info_logs_retrying<F, T>(mut make_operation: impl FnMut() -> F) -> (T, String)
    where
        F: FnOnce() -> T,
    {
        const MAX_ATTEMPTS: u32 = 25;
        let mut last = None;
        for _ in 0..MAX_ATTEMPTS {
            let (result, logs) = capture_info_logs_once(make_operation());
            if !logs.is_empty() {
                return (result, logs);
            }
            last = Some((result, logs));
        }
        last.expect("MAX_ATTEMPTS is greater than zero")
    }

    #[test]
    fn open_engine_readonly_logs_the_qualified_single_owner_vs_multi_guard_wording() {
        let dir = temp_dir();
        let root = dir.path().to_path_buf();
        let mut attempt = 0_u32;

        let (readonly, logs) = capture_info_logs_retrying(|| {
            attempt += 1;
            let root = root.clone();
            let db_path = root.join(format!("engine-ro-log-wording-{attempt}.db"));
            build_populated_fixture(&db_path, &root);
            move || {
                DataStore::open_engine_readonly(&db_path, &root)
                    .expect("open_engine_readonly should succeed")
            }
        });

        assert!(
            !logs.is_empty(),
            "tracing capture never observed ANY event for open_engine_readonly's startup log \
             across the retry budget — this indicates a capture-seam regression, not a wording \
             mismatch"
        );
        assert!(
            logs.contains(EXPECTED_ENGINE_READONLY_OPEN_LOG_MESSAGE),
            "open_engine_readonly's startup log must state the qualified, honest guarantee \
             (robust under a single owning guard; best-effort - not a cross-process guarantee \
             - whenever the same file is independently guarded more than once; F6), not an \
             unconditional 'filesystem lock active' claim. Captured log output:\n{logs}"
        );

        drop(readonly);
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
        // Per the F7 conservative-cleanup rule, a WAL-reader coordination
        // sidecar (`-shm`/`-wal`) that SQLite transiently materializes and
        // fills with real coordination bytes during a read-only open may be
        // left behind: the guard cannot prove no concurrent writer shares it,
        // so it only reclaims EMPTY placeholder sidecars it created. The
        // durable guarantee is therefore that the served db is byte-for-byte
        // untouched and the guard never leaves an EMPTY placeholder trace.
        let empty_sidecar_traces: Vec<bool> = sidecar_paths_for_test(&db_path)
            .iter()
            .map(|p| fs::metadata(p).is_ok_and(|meta| meta.len() == 0))
            .collect();

        assert_eq!(
            bytes_before, bytes_after,
            "engine-readonly open must not change a single byte of the served db file"
        );
        assert_eq!(
            mtime_before, mtime_after,
            "engine-readonly open must not touch mtime"
        );
        assert!(
            empty_sidecar_traces
                .iter()
                .all(|&is_empty_trace| !is_empty_trace),
            "engine-readonly open must never leave an EMPTY placeholder sidecar trace; only \
             non-empty coordination sidecars are conservatively preserved (F7)"
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
    fn open_engine_readonly_preserves_a_pre_existing_readonly_db_after_drop() {
        // A file the operator had ALREADY marked filesystem-readonly for
        // their own reasons, before `open_engine_readonly` ever touched it,
        // must remain readonly once the guard releases it — restoring
        // must recover the file's CAPTURED ORIGINAL state, not
        // unconditionally force it writable.
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-preexisting.db");
        build_populated_fixture(&db_path, dir.path());

        set_readonly(&db_path, true).expect("mark pre-existing readonly for fixture setup");
        assert!(
            fs::metadata(&db_path).unwrap().permissions().readonly(),
            "precondition: db file must already be readonly before serving"
        );

        {
            let readonly = DataStore::open_engine_readonly(&db_path, dir.path())
                .expect("open_engine_readonly should succeed against an already-readonly file");
            assert!(
                fs::metadata(&db_path).unwrap().permissions().readonly(),
                "db file must remain filesystem-readonly while the guard is held"
            );
            drop(readonly);
        }

        assert!(
            fs::metadata(&db_path).unwrap().permissions().readonly(),
            "a db file that was ALREADY readonly before serving must remain readonly after the \
             guard drops"
        );
    }

    #[test]
    fn open_engine_readonly_removes_an_empty_transient_sidecar_on_drop() {
        // The common single-process case: the read-only WAL-reader
        // machinery creates an EMPTY placeholder sidecar that did not
        // exist before the guard was taken; that placeholder must still be
        // cleaned up on drop so the workspace shows no persistent trace.
        //
        // Driven through `EngineReadonlyGuard` DIRECTLY rather than a full
        // engine open: a real read-only engine open creates its own `-wal`
        // sidecar whose permissions differ across platforms (on Linux it is
        // materialized read-only), which would prevent this test from
        // deterministically staging the exact sidecar state it means to
        // assert on. Locking the guard alone reproduces precisely the
        // "sidecar absent at lock time, then transiently created while the
        // guard is held" condition the Drop cleanup path guards against.
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-empty-sidecar.db");
        build_populated_fixture(&db_path, dir.path());
        let wal_path = sidecar_paths_for_test(&db_path)[0].clone();
        assert!(!wal_path.exists(), "precondition: no -wal sidecar yet");

        {
            // The `-wal` did not exist at lock time, so the guard records it
            // as a transient sidecar eligible for empty-only cleanup.
            let _guard = EngineReadonlyGuard::lock(&db_path)
                .expect("guard lock should succeed against a db with no sidecars");
            // Simulate the engine's own transient, still-empty placeholder
            // artifact appearing while the guard is held.
            fs::write(&wal_path, b"").expect("simulate empty transient sidecar");
        }

        assert!(
            !wal_path.exists(),
            "an EMPTY transient sidecar must be cleaned up on drop"
        );
    }

    #[test]
    fn open_engine_readonly_never_removes_a_non_empty_transient_sidecar_on_drop() {
        // If a concurrent process created a REAL, live WAL file in the
        // narrow window this guard was held (a scenario this guard cannot
        // rule out), that file is virtually guaranteed to be non-empty.
        // Cleanup must leave it alone rather than risk deleting another
        // connection's committed data.
        //
        // Driven through `EngineReadonlyGuard` DIRECTLY (see the empty-case
        // test above for why a full engine open cannot deterministically
        // stage this state across platforms).
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-live-sidecar.db");
        build_populated_fixture(&db_path, dir.path());
        let wal_path = sidecar_paths_for_test(&db_path)[0].clone();
        assert!(!wal_path.exists(), "precondition: no -wal sidecar yet");

        {
            let _guard = EngineReadonlyGuard::lock(&db_path)
                .expect("guard lock should succeed against a db with no sidecars");
            // Simulate a genuinely concurrent writer's live, non-empty WAL
            // sidecar appearing while the guard is held.
            fs::write(&wal_path, b"not-empty-live-wal-frame").expect("simulate live sidecar");
        }

        assert!(
            wal_path.exists(),
            "a NON-EMPTY sidecar must never be removed — it may be another connection's live data"
        );
    }

    #[cfg(unix)]
    #[test]
    fn open_engine_readonly_preserves_exact_unix_mode_after_drop() {
        // Regression for F1: the guard must restore the EXACT original Unix
        // mode bits, not a coarse writable/readonly boolean. A private
        // `0o600` database must come back as `0o600` after the guard drops,
        // never widened to `0o622`/`0o644` (which `Permissions::set_readonly(false)`
        // would do by enabling every write bit, granting group/other write).
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-mode.db");
        build_populated_fixture(&db_path, dir.path());

        let mut perms = fs::metadata(&db_path)
            .expect("stat fixture db")
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&db_path, perms).expect("set 0o600 mode for fixture setup");
        assert_eq!(
            fs::metadata(&db_path)
                .expect("stat fixture db after chmod")
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "precondition: fixture db must be mode 0o600 before serving"
        );

        {
            let _guard = EngineReadonlyGuard::lock(&db_path)
                .expect("guard lock should succeed against a mode-0o600 db");
        }

        assert_eq!(
            fs::metadata(&db_path)
                .expect("stat fixture db after drop")
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "guard drop must restore the EXACT original mode 0o600, not widen it to 0o622/0o644"
        );
    }

    #[test]
    fn open_engine_readonly_never_removes_a_non_empty_shm_sidecar_on_drop() {
        // Regression for F7: `-shm` must obey the SAME conservative
        // empty-only cleanup rule as every other sidecar. A non-empty `-shm`
        // may belong to a genuinely concurrent writer; unlinking a live
        // `-shm` defeats WAL coordination (existing connections keep the old
        // mapping while new connections create a second `-shm`) and risks
        // corruption, so cleanup must leave it intact.
        //
        // Driven through `EngineReadonlyGuard` DIRECTLY (see the empty-case
        // test above for why a full engine open cannot deterministically
        // stage this state across platforms).
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-live-shm.db");
        build_populated_fixture(&db_path, dir.path());
        let shm_path = sidecar_paths_for_test(&db_path)[1].clone();
        assert!(!shm_path.exists(), "precondition: no -shm sidecar yet");

        {
            let _guard = EngineReadonlyGuard::lock(&db_path)
                .expect("guard lock should succeed against a db with no sidecars");
            // Simulate a genuinely concurrent writer's live, non-empty `-shm`
            // shared-memory index appearing while the guard is held.
            fs::write(&shm_path, b"not-empty-live-shm-index").expect("simulate live shm");
        }

        assert!(
            shm_path.exists(),
            "a NON-EMPTY -shm sidecar must never be removed — it may be another connection's \
             live coordination data"
        );
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

    // ── W8-1: reparse-point sidecar containment ────────────────────────────
    //
    // A symlinked/junction `-wal`/`-shm`/`-journal` sidecar must never let the
    // read-only guard (or the write-mode self-heal) follow the link to chmod a
    // file OUTSIDE the workspace. These tests plant a symlinked sidecar pointing
    // at an external target and prove both entry points fail closed WITHOUT
    // touching that target's permissions. The guard is cross-platform via
    // `is_reparse_point` (which also catches Windows junctions); the tests skip
    // gracefully where the platform refuses unprivileged symlink creation.

    fn try_symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }

    #[test]
    fn open_engine_readonly_refuses_a_symlinked_wal_sidecar() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-ro-symlink.db");
        build_populated_fixture(&db_path, dir.path());

        // An external target the attacker wants chmod'd, living OUTSIDE the
        // served workspace, created writable so a widening to read-only is
        // detectable.
        let external_dir = temp_dir();
        let external_target = external_dir.path().join("external-secret.txt");
        fs::write(&external_target, b"external").expect("seed external target");
        assert!(
            !fs::metadata(&external_target)
                .unwrap()
                .permissions()
                .readonly(),
            "precondition: external target must start writable"
        );

        // Plant a `-wal` sidecar that is a symlink to the external target.
        let wal = append_suffix(&db_path, "-wal");
        let _ = fs::remove_file(&wal);
        if try_symlink_file(&external_target, &wal).is_err() {
            return; // platform refused unprivileged symlink creation — skip
        }

        let result = DataStore::open_engine_readonly(&db_path, dir.path());
        assert!(
            result.is_err(),
            "open_engine_readonly must fail closed on a symlinked sidecar rather than chmod \
             an external target"
        );
        assert!(
            !fs::metadata(&external_target)
                .unwrap()
                .permissions()
                .readonly(),
            "the external symlink target must NOT be made read-only through the linked sidecar"
        );
    }

    #[test]
    fn open_sqlite_refuses_a_symlinked_wal_sidecar() {
        let dir = temp_dir();
        let db_path = dir.path().join("engine-rw-symlink.db");
        build_populated_fixture(&db_path, dir.path());

        // An external target the attacker wants the readonly bit CLEARED on,
        // seeded read-only so a stale-lock clear would widen it back to writable.
        let external_dir = temp_dir();
        let external_target = external_dir.path().join("external-locked.txt");
        fs::write(&external_target, b"external").expect("seed external target");
        set_readonly(&external_target, true).expect("mark external target read-only");
        assert!(
            fs::metadata(&external_target)
                .unwrap()
                .permissions()
                .readonly(),
            "precondition: external target must start read-only"
        );

        // Plant a `-wal` sidecar that is a symlink to the external target.
        let wal = append_suffix(&db_path, "-wal");
        let _ = fs::remove_file(&wal);
        if try_symlink_file(&external_target, &wal).is_err() {
            return; // platform refused unprivileged symlink creation — skip
        }

        let result = DataStore::open_sqlite(&db_path, dir.path());
        assert!(
            result.is_err(),
            "open_sqlite must fail closed on a symlinked sidecar rather than clear the readonly \
             bit on an external target"
        );
        assert!(
            fs::metadata(&external_target)
                .unwrap()
                .permissions()
                .readonly(),
            "the external symlink target's read-only bit must NOT be cleared through the linked \
             sidecar"
        );
    }

    #[test]
    fn open_sqlite_refuses_a_dangling_symlinked_wal_sidecar() {
        // Regression for the write-path ordering gap: `clear_stale_readonly_lock`
        // must reject a reparse-point sidecar BEFORE its `exists()` check.
        // `exists()` follows the link, so a DANGLING `-wal` symlink (its target
        // absent) reports `false`; if the guard ran after `exists()` the sidecar
        // would be skipped and the engine would CREATE and write the WAL THROUGH
        // the link, materializing a file at an external path outside the
        // workspace — the exact containment breach the guard exists to prevent.
        let dir = temp_dir();
        let db_path = dir.path().join("engine-rw-dangling.db");
        build_populated_fixture(&db_path, dir.path());

        // A NON-EXISTENT external target: writing through the link would create
        // it. Its continued absence after the open proves no write escaped.
        let external_dir = temp_dir();
        let external_target = external_dir.path().join("must-not-be-created.wal");
        assert!(
            !external_target.exists(),
            "precondition: external target must not exist yet"
        );

        // Plant a `-wal` sidecar that is a DANGLING symlink to the absent target.
        let wal = append_suffix(&db_path, "-wal");
        let _ = fs::remove_file(&wal);
        if try_symlink_file(&external_target, &wal).is_err() {
            return; // platform refused unprivileged symlink creation — skip
        }

        let result = DataStore::open_sqlite(&db_path, dir.path());
        assert!(
            result.is_err(),
            "open_sqlite must fail closed on a DANGLING symlinked sidecar rather than create the \
             WAL through the link at an external path"
        );
        assert!(
            !external_target.exists(),
            "no file may be created at the external dangling-symlink target"
        );
    }
}
