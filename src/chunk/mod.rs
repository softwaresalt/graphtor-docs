//! Deterministic chunk identifier generation.
//!
//! This module provides [`generate_chunk_id`] which computes a stable
//! SHA-256-based identifier for a documentation chunk. The identifier is
//! the cross-database correlation key linking `LanceDB` vectors to Kùzu
//! graph nodes.
