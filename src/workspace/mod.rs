//! Workspace plugin installation and lifecycle management.
//!
//! This module provides commands for installing, diagnosing, upgrading, and
//! uninstalling `graphtor-docs` as a per-project workspace plugin. All
//! runtime artifacts are stored under a `.graphtor/` directory at the
//! project root.
//!
//! # Sub-modules
//!
//! - [`paths`]: Cross-platform workspace path resolution and constants.
//! - [`install`]: Scaffold and binary installation.
//! - [`init`]: `sources.yaml` template generation.
//! - [`gitignore`]: `.gitignore` entry management.
//! - [`mcp_config`]: MCP client configuration generation.
//! - [`doctor`]: Health-check diagnostics.
//! - [`upgrade`]: Binary upgrade workflow.
//! - [`uninstall`]: Workspace removal.
//! - [`lock`]: Advisory workspace lock file.
//! - [`serve_discovery`]: `.graphtor/` root auto-discovery for `serve`/`status`.

pub mod doctor;
pub mod gitignore;
pub mod init;
pub mod install;
pub mod lock;
pub mod mcp_config;
pub mod paths;
pub mod serve_discovery;
pub mod uninstall;
pub mod upgrade;
