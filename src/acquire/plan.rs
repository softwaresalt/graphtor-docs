//! Acquisition planning and source validation.
//!
//! Provides:
//! - [`plan`]: resolve a [`SourceConfig`] into an [`AcquisitionPlan`] with per-source actions.
//! - [`validate_sources`]: check all sources for configuration errors in a single pass (FR-011–FR-014).
//!
//! [`SourceConfig`]: crate::config::SourceConfig
//! [`AcquisitionPlan`]: crate::acquire::result::AcquisitionPlan
