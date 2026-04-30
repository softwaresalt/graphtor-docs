//! graphtor-core — `GraphRAG` documentation index core library.
//!
//! This crate provides the foundational types and utilities used by all
//! pipeline stages and the MCP server plugin:
//!
//! - [`acquire`]: Source acquisition — clone Git repos, scan local directories, apply glob filters.
//! - [`config`]: Parse and validate `sources.yaml` documentation registries.
//! - [`db`]: Unified embedded database (`CozoDB`) — chunk storage, graph edges, full-text search.
//! - [`embed`]: Dense text embedding via `all-MiniLM-L6-v2` (Candle, in-process).
//! - [`error`]: Categorized error type hierarchy ([`GraphtorError`]).
//! - [`chunk`]: Deterministic SHA-256 chunk identifier generation.
//! - [`logging`]: Structured logging initialization via `tracing`.
//! - [`parse`]: Markdown parsing pipeline — frontmatter, AST, chunking, links, code blocks.
//! - [`path`]: Path security utilities for workspace boundary enforcement.
//! - [`pipeline`]: End-to-end ingestion orchestrator — acquire → parse → embed → load.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod acquire;
pub mod chunk;
pub mod config;
pub mod db;
pub mod embed;
pub mod error;
pub mod logging;
pub mod parse;
pub mod path;
/// End-to-end ingestion pipeline orchestrator (acquire → parse → embed → load).
pub mod pipeline;

pub use acquire::{
    AcquiredSource, AcquisitionPlan, AcquisitionResult, FilteredFileSet, PlannedSource,
    SourceAction, SourceOutcome, SourceType, ValidationError, ValidationReport,
};
pub use chunk::generate_chunk_id;
pub use config::{GitSource, LocalSource, Source, SourceConfig};
pub use db::DataStore;
pub use embed::{embed_batch, embed_text, EmbeddingModel};
pub use error::GraphtorError;
pub use logging::{init_logging, LogVerbosity};
pub use path::validate_path;
pub use pipeline::{FileError, PipelineConfig, PipelineResult};
