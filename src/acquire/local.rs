//! Local directory scanning — recursive file discovery via `walkdir`.
//!
//! Provides [`scan_local_source`] which recursively walks a local directory,
//! collecting all regular files in deterministic sort order (FR-005).
