//! Glob pattern filtering — include/exclude file set reduction.
//!
//! Provides [`filter_files`] which applies include and exclude glob patterns
//! to a list of discovered files, producing a [`FilteredFileSet`] (FR-006–FR-010).
//!
//! [`FilteredFileSet`]: crate::acquire::result::FilteredFileSet
