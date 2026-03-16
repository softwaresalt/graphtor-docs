//! graphtor-core — `GraphRAG` documentation index core library.
//!
//! This crate provides the foundational types and utilities used by all
//! pipeline stages and the MCP server plugin:
//!
//! - [`acquire`]: Source acquisition — clone Git repos, scan local directories, apply glob filters.
//! - [`config`]: Parse and validate `sources.yaml` documentation registries.
//! - [`error`]: Categorized error type hierarchy ([`GraphtorError`]).
//! - [`chunk`]: Deterministic SHA-256 chunk identifier generation.
//! - [`logging`]: Structured logging initialization via `tracing`.
//! - [`path`]: Path security utilities for workspace boundary enforcement.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod acquire;
pub mod chunk;
pub mod config;
pub mod error;
pub mod logging;
pub mod path;

pub use acquire::{
    AcquiredSource, AcquisitionPlan, AcquisitionResult, FilteredFileSet, PlannedSource,
    SourceAction, SourceOutcome, SourceType, ValidationError, ValidationReport,
};
pub use chunk::generate_chunk_id;
pub use config::{GitSource, LocalSource, Source, SourceConfig};
pub use error::GraphtorError;
pub use logging::{init_logging, LogVerbosity};
pub use path::validate_path;
