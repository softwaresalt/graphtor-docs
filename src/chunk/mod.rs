//! Deterministic chunk identifier generation.
//!
//! This module provides [`generate_chunk_id`] which computes a stable
//! SHA-256-based identifier for a documentation chunk. The identifier is
//! the cross-store correlation key linking vector embeddings to graph
//! nodes in `CozoDB`.

pub mod id;

pub use id::generate_chunk_id;
