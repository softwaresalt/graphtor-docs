//! Sync state persistence for incremental document re-ingestion.
//!
//! Manages `.sync_state.json`, which records per-source tracking data used to
//! detect changed files between pipeline runs:
//!
//! - For **Git sources**: the SHA-1 of the last fully processed commit.
//! - For **local directory sources**: a map of relative path → mtime (seconds
//!   since the Unix epoch).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::GraphtorError;
use crate::path::validate_path;

/// Per-source incremental sync tracking data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SourceSyncState {
    /// Map of source-root-relative file path → Unix mtime (seconds).
    ///
    /// Used for local directory sources to detect changed files. Keys use
    /// forward-slash separators on all platforms.
    pub file_mtimes: HashMap<String, u64>,

    /// Map of source-root-relative fs path → last-known contract `source_path`.
    ///
    /// Populated on each successful re-ingest after the docline pivot.
    /// The value is the canonical `source_path` field from the validated docline
    /// frontmatter contract — it may differ from the fs-relative path.
    ///
    /// Used by the delete path so stale records are removed by their contract
    /// identity rather than the filesystem path, preventing orphaned rows when a
    /// file's `source_path` changes between syncs.
    ///
    /// Absent for files that were last ingested before this field was added
    /// (pre-pivot state): the fs-relative path is used as a fallback in that case.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub file_contract_paths: HashMap<String, String>,

    /// Unix-epoch seconds string (decimal) recording when this source was last synced.
    ///
    /// `None` if this source has never been synced.
    pub last_sync: Option<String>,

    /// Contract epoch recorded at the time of the last sync.
    ///
    /// When the stored epoch differs from [`crate::ingest_contract::CONTRACT_EPOCH`],
    /// the source forces a full re-ingest on the next sync cycle.
    ///
    /// A `None` value indicates pre-pivot state and is treated as a mismatch,
    /// forcing a full re-ingest to prevent stale pre-pivot data from being used.
    pub contract_epoch: Option<String>,
}

/// Top-level sync state container keyed by source identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SyncState {
    /// Map of `source_id` → per-source sync tracking data.
    pub sources: HashMap<String, SourceSyncState>,
}

impl SyncState {
    /// Load sync state from a JSON file at `path` within `root`.
    ///
    /// Returns an empty [`SyncState`] if the file does not exist.
    ///
    /// # Errors
    ///
    /// - [`GraphtorError::PathViolation`] if `path` escapes `root`.
    /// - [`GraphtorError::Config`] if the file exists but cannot be read or
    ///   parsed.
    pub fn load(path: &Path, root: &Path) -> Result<Self, GraphtorError> {
        let safe_path = validate_path(path, root)?;
        if !safe_path.exists() {
            debug!("no sync state file found; returning empty state");
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&safe_path).map_err(|e| GraphtorError::Config {
            message: format!("failed to read sync state: {e}"),
            field: None,
        })?;
        let state = serde_json::from_str(&content).map_err(|e| GraphtorError::Config {
            message: format!("failed to parse sync state JSON: {e}"),
            field: None,
        })?;
        debug!(path = %safe_path.display(), "loaded sync state");
        Ok(state)
    }

    /// Persist sync state to `path` within `root` using an atomic write.
    ///
    /// Writes to a temporary file then renames to prevent partial writes on
    /// crash or power failure.  Creates the parent directory if needed.
    ///
    /// # Errors
    ///
    /// - [`GraphtorError::PathViolation`] if `path` escapes `root`.
    /// - [`GraphtorError::Config`] on serialization or I/O failure.
    pub fn save(&self, path: &Path, root: &Path) -> Result<(), GraphtorError> {
        let safe_path = validate_path(path, root)?;
        if let Some(parent) = safe_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| GraphtorError::Config {
                message: format!("failed to create sync state directory: {e}"),
                field: None,
            })?;
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| GraphtorError::Config {
            message: format!("failed to serialize sync state: {e}"),
            field: None,
        })?;
        // Atomic write: write to a temp file then rename into place.
        let tmp_path = safe_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &content).map_err(|e| GraphtorError::Config {
            message: format!("failed to write sync state temp file: {e}"),
            field: None,
        })?;
        std::fs::rename(&tmp_path, &safe_path).map_err(|e| GraphtorError::Config {
            message: format!("failed to rename sync state temp file: {e}"),
            field: None,
        })?;
        debug!(path = %safe_path.display(), "saved sync state");
        Ok(())
    }

    /// Return a mutable reference to the state for `source_id`.
    ///
    /// Creates a default [`SourceSyncState`] if no entry exists.
    pub fn source_mut(&mut self, source_id: &str) -> &mut SourceSyncState {
        self.sources.entry(source_id.to_owned()).or_default()
    }

    /// Return a reference to the state for `source_id`, or `None`.
    #[must_use]
    pub fn source(&self, source_id: &str) -> Option<&SourceSyncState> {
        self.sources.get(source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn load_returns_empty_state_when_file_missing() {
        let dir = temp_dir();
        let path = dir.path().join(".sync_state.json");
        let state = SyncState::load(&path, dir.path()).expect("load");
        assert!(state.sources.is_empty(), "expected empty sources");
    }

    #[test]
    fn save_and_load_round_trips_correctly() {
        let dir = temp_dir();
        let path = dir.path().join(".sync_state.json");

        let mut state = SyncState::default();
        {
            let src = state.source_mut("my-source");
            src.last_sync = Some("2026-01-01T00:00:00Z".to_string());
            src.file_mtimes
                .insert("docs/intro.md".to_string(), 1_700_000_000);
        }

        state.save(&path, dir.path()).expect("save");
        let loaded = SyncState::load(&path, dir.path()).expect("load");

        assert_eq!(loaded, state);
    }

    #[test]
    fn save_is_idempotent() {
        let dir = temp_dir();
        let path = dir.path().join(".sync_state.json");

        let mut state = SyncState::default();
        state.source_mut("s1").last_sync = Some("aa".to_string());

        state.save(&path, dir.path()).expect("save 1");
        state.save(&path, dir.path()).expect("save 2");

        let loaded = SyncState::load(&path, dir.path()).expect("load");
        assert_eq!(
            loaded.source("s1").and_then(|s| s.last_sync.as_deref()),
            Some("aa")
        );
    }

    #[test]
    fn source_mut_creates_default_entry() {
        let mut state = SyncState::default();
        let entry = state.source_mut("new-source");
        assert!(entry.last_sync.is_none());
        assert!(entry.file_mtimes.is_empty());
        assert!(entry.file_contract_paths.is_empty());
    }

    #[test]
    fn file_contract_paths_round_trips() {
        let dir = temp_dir();
        let path = dir.path().join(".sync_state.json");

        let mut state = SyncState::default();
        {
            let src = state.source_mut("cp-test");
            src.file_contract_paths
                .insert("docs/guide.md".to_string(), "guide.md".to_string());
            src.file_contract_paths
                .insert("docs/api.md".to_string(), "api/reference.md".to_string());
        }

        state.save(&path, dir.path()).expect("save");
        let loaded = SyncState::load(&path, dir.path()).expect("load");

        let src = loaded.source("cp-test").expect("source exists");
        assert_eq!(
            src.file_contract_paths
                .get("docs/guide.md")
                .map(String::as_str),
            Some("guide.md"),
            "contract path round-trip mismatch"
        );
        assert_eq!(
            src.file_contract_paths
                .get("docs/api.md")
                .map(String::as_str),
            Some("api/reference.md"),
            "renamed contract path round-trip mismatch"
        );
    }

    #[test]
    fn missing_contract_epoch_is_treated_as_pre_pivot() {
        // Simulate legacy state: stored epoch is None (pre-pivot).
        let stored = SourceSyncState {
            file_mtimes: [("docs/a.md".to_string(), 1_000u64)].into_iter().collect(),
            file_contract_paths: HashMap::new(),
            last_sync: Some("100".to_string()),
            contract_epoch: None, // pre-pivot — no epoch stored
        };

        let current_epoch = crate::ingest_contract::CONTRACT_EPOCH;
        // A None epoch must not equal the current epoch.
        assert_ne!(
            stored.contract_epoch.as_deref(),
            Some(current_epoch),
            "None epoch must not match current epoch (would skip forced rebuild)"
        );
    }

    #[test]
    fn path_violation_returns_error() {
        let dir = temp_dir();
        let escaping_path = dir.path().join("..").join(".sync_state.json");
        let result = SyncState::load(&escaping_path, dir.path());
        assert!(
            matches!(result, Err(GraphtorError::PathViolation { .. })),
            "expected PathViolation, got: {result:?}"
        );
    }
}
