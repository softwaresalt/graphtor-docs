//! Structured logging initialization via `tracing`.
//!
//! This module provides [`init_logging`] and [`LogVerbosity`] for
//! configuring the global tracing subscriber at application startup.
//! All pipeline stages emit structured diagnostics through this layer.

pub mod init;

pub use init::{init_logging, LogVerbosity};
