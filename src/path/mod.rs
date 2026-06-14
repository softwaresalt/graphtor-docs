//! Path security utilities for boundary enforcement.
//!
//! This module provides [`validate_path`] which canonicalizes a path and
//! confirms it lies within an allowed root directory, preventing directory
//! traversal and symlink escape attacks.

use std::path::{Path, PathBuf};

pub mod security;

/// Reserved subdirectory name used under `data_root` for frozen v4 migration
/// snapshots.
pub const V4_MIGRATION_SNAPSHOTS_DIR: &str = "v4-migration-snapshots";

/// Return the workspace-internal base directory that stores frozen v4
/// migration snapshots for a given data root.
#[must_use]
pub fn v4_migration_snapshot_dir(data_root: &Path) -> PathBuf {
    data_root.join(V4_MIGRATION_SNAPSHOTS_DIR)
}

pub(crate) use security::canonicalize_clean;
pub use security::validate_path;
