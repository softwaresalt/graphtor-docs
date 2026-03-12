//! Configuration parsing and validation for `sources.yaml`.
//!
//! This module provides [`SourceConfig`], [`Source`], [`GitSource`], and
//! [`LocalSource`] types for reading and validating the documentation source
//! registry. Configuration is parsed from a YAML file and validated before
//! any pipeline stage begins.

pub mod source;
pub(crate) mod validation;

pub use source::{GitSource, LocalSource, Source, SourceConfig};
