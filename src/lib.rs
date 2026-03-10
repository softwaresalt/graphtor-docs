//! graphtor-core — `GraphRAG` documentation index core library.
//!
//! This crate provides the foundational types and utilities used by all
//! pipeline stages and the MCP server plugin:
//!
//! - [`config`]: Parse and validate `sources.yaml` documentation registries.
//! - [`error`]: Categorized error type hierarchy ([`GraphtorError`]).
//! - [`chunk`]: Deterministic SHA-256 chunk identifier generation.
//! - [`logging`]: Structured logging initialization via `tracing`.
//! - [`path`]: Path security utilities for workspace boundary enforcement.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chunk;
pub mod config;
pub mod error;
pub mod logging;
pub mod path;

pub use error::GraphtorError;
