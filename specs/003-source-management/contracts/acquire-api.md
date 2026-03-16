# API Contract: Source Acquisition

**Feature**: 003-source-management
**Module**: `graphtor_core::acquire`

## Public Functions

### `acquire::plan`

```rust
/// Resolve an acquisition plan from a parsed source configuration.
///
/// Examines each source in the config, checks whether a local directory
/// already exists for Git sources, and produces a plan with per-source actions.
pub fn plan(
    config: &SourceConfig,
    data_root: &Path,
    allowed_root: &Path,
) -> Result<AcquisitionPlan, GraphtorError>;
```

### `acquire::execute`

```rust
/// Execute an acquisition plan: clone Git repos, scan local dirs, filter files.
///
/// Processes each source in the plan. Failures in one source do not stop
/// processing of others. Returns an aggregate result with per-source outcomes.
///
/// When `dry_run` is `true`, no filesystem or network I/O is performed.
/// All sources are reported as [`SourceOutcome::Skipped`] with zero files.
pub fn execute(plan: &AcquisitionPlan, dry_run: bool) -> AcquisitionResult;
```

### `acquire::validate_sources`

```rust
/// Validate all source definitions without performing acquisition.
///
/// Checks URL format (Git), path existence (local), and glob syntax.
/// Collects ALL errors across all sources in a single pass.
pub fn validate_sources(
    config: &SourceConfig,
    allowed_root: &Path,
) -> ValidationReport;
```

### `acquire::filter_files`

```rust
/// Apply include/exclude glob patterns to a list of file paths.
///
/// Returns only files that match at least one include pattern (or all files
/// if no include patterns) AND do not match any exclude pattern.
pub fn filter_files(
    files: &[PathBuf],
    include: &[String],
    exclude: &[String],
) -> Result<Vec<PathBuf>, GraphtorError>;
```

### `acquire::clone_git_source`

```rust
/// Clone a single Git repository with shallow fetch (depth=1, single branch).
///
/// Skips if target directory already exists. Returns the local directory path.
pub fn clone_git_source(
    source: &GitSource,
    target_dir: &Path,
) -> Result<PathBuf, GraphtorError>;
```

### `acquire::scan_local_source`

```rust
/// Recursively scan a local directory and return all regular file paths.
///
/// Symlinks are not followed. Results are sorted for deterministic ordering.
pub fn scan_local_source(
    source: &LocalSource,
    allowed_root: &Path,
) -> Result<Vec<PathBuf>, GraphtorError>;
```

## Re-exports from `graphtor_core`

```rust
// In lib.rs
pub mod acquire;
pub use acquire::{
    AcquisitionPlan, AcquisitionResult, AcquiredSource,
    FilteredFileSet, SourceOutcome, ValidationReport,
};
```

## Error Mapping

| Source Error | Maps To |
|-------------|---------|
| `git2::Error` | `GraphtorError::Pipeline { stage: "acquire", message: ... }` |
| `walkdir::Error` | `GraphtorError::Pipeline { stage: "acquire", message: ... }` |
| `std::io::Error` | `GraphtorError::Io` |
| Path security violation | `GraphtorError::PathViolation` |
| Invalid glob pattern | `GraphtorError::Config` (via existing validation) |
