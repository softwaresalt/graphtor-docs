//! Path security utilities for boundary enforcement.
//!
//! This module provides [`validate_path`] which canonicalizes a path and
//! confirms it lies within an allowed root directory, preventing directory
//! traversal and symlink escape attacks.
