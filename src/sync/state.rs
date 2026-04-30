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
    /// SHA-1 commit hash of the last fully processed git commit.
    ///
    /// `None` if this source has never been synced or is not a git source.
    pub last_commit: Option<String>,

    /// Map of source-root-relative file path → Unix mtime (seconds).
    ///
    /// Used for local directory sources to detect changed files. Keys use
    /// forward-slash separators on all platforms.
    pub file_mtimes: HashMap<String, u64>,

    /// Unix-epoch seconds string (decimal) recording when this source was last synced.
    ///
    /// `None` if this source has never been synced.
    pub last_sync: Option<String>,
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
            src.last_commit = Some("abc123".to_string());
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
        state.source_mut("s1").last_commit = Some("aa".to_string());

        state.save(&path, dir.path()).expect("save 1");
        state.save(&path, dir.path()).expect("save 2");

        let loaded = SyncState::load(&path, dir.path()).expect("load");
        assert_eq!(
            loaded.source("s1").and_then(|s| s.last_commit.as_deref()),
            Some("aa")
        );
    }

    #[test]
    fn source_mut_creates_default_entry() {
        let mut state = SyncState::default();
        let entry = state.source_mut("new-source");
        assert!(entry.last_commit.is_none());
        assert!(entry.file_mtimes.is_empty());
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
