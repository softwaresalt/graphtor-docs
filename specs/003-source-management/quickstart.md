# Quickstart: Source Registry & Acquisition

**Feature**: 003-source-management

## Prerequisites

- Rust stable toolchain (edition 2021, MSRV 1.75)
- Cargo workspace already set up (from FG-001)
- `graphtor-core` library crate compiling

## Setup

### 1. Add new dependencies to `Cargo.toml`

```toml
[dependencies]
# ... existing deps ...
git2 = "0.19"
walkdir = "2"
```

### 2. Create the acquire module

```text
src/acquire/
├── mod.rs       # Module root with re-exports
├── git.rs       # Git cloning
├── local.rs     # Local directory scanning
├── filter.rs    # Glob pattern filtering
├── plan.rs      # Acquisition planning
└── result.rs    # Result types
```

### 3. Register in `lib.rs`

```rust
pub mod acquire;
```

## Verification

```bash
# Compile the new module
cargo check

# Run all tests (existing + new)
cargo test

# Run only acquire tests
cargo test acquire
```

## Usage Example

```rust
use graphtor_core::config::SourceConfig;
use graphtor_core::acquire;
use std::path::Path;

// Parse the sources.yaml (FG-001 provides this)
let config = SourceConfig::parse(Path::new("sources.yaml"))?;

// Validate sources before acquisition
let report = acquire::validate_sources(&config, Path::new("/workspace"));
if !report.errors.is_empty() {
    for err in &report.errors {
        eprintln!("  [{}] {}: {}", err.source_id, err.field, err.message);
    }
    return Err(...);
}

// Plan the acquisition
let data_root = Path::new(".graphtor-data");
let plan = acquire::plan(&config, data_root, Path::new("/workspace"))?;

// Execute
let result = acquire::execute(&plan);
println!("Sources: {} ok, {} skipped, {} failed, {} files",
    result.succeeded, result.skipped, result.failed, result.total_files);
```

## Test Strategy

- **Unit tests**: Inline in each module (`#[cfg(test)]` blocks) for pure logic
- **Integration tests**: In `tests/acquire_*.rs` files using `tempfile` for filesystem isolation
- Git tests use `tempfile::tempdir()` as the data root
- No network access in tests — use `git2::Repository::init()` to create test repos locally
