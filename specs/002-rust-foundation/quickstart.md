# Quickstart: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10

## Prerequisites

- Rust toolchain (stable, 1.75+) installed via `rustup`
- Cargo available in PATH

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## Usage Examples

### Parse a sources.yaml configuration

```rust
use graphtor_core::config::SourceConfig;
use std::path::Path;

let config = SourceConfig::parse(Path::new("sources.yaml"))?;
for source in &config.sources {
    println!("Source: {}", source.id());
}
```

### Generate a chunk ID

```rust
use graphtor_core::chunk::generate_chunk_id;

let id = generate_chunk_id(
    "## Authentication\nUse OAuth2 to authenticate...",
    "docs/azure/auth.md"
);
// id is a 64-char hex string like "a1b2c3d4..."
assert_eq!(id.len(), 64);
```

### Validate a file path

```rust
use graphtor_core::path::validate_path;
use std::path::Path;

let allowed_root = Path::new("/home/dev/docs");
let safe_path = validate_path(
    Path::new("azure/auth.md"),
    allowed_root
)?;
// safe_path is the resolved absolute path

// This would fail with PathViolation:
// validate_path(Path::new("../../etc/passwd"), allowed_root)?;
```

### Initialize logging

```rust
use graphtor_core::logging::{init_logging, LogVerbosity};

init_logging(LogVerbosity::Normal)?;
// Now INFO, WARN, and ERROR messages go to stderr
```

### Handle errors

```rust
use graphtor_core::error::GraphtorError;

match some_operation() {
    Ok(result) => println!("Success: {result}"),
    Err(GraphtorError::Config { message, field }) => {
        eprintln!("Configuration error: {message}");
    }
    Err(GraphtorError::PathViolation { attempted, allowed_root }) => {
        eprintln!("Security: {attempted:?} escapes {allowed_root:?}");
    }
    Err(e) => eprintln!("Error: {e}"),
}
```

## Project Structure

```text
Cargo.toml          # Workspace root
src/
├── lib.rs          # Library crate — re-exports modules
├── config/         # sources.yaml parsing and validation
├── error/          # GraphtorError enum
├── chunk/          # Chunk ID generation
├── logging/        # tracing initialization
├── path/           # Path security validation
└── main.rs         # Binary (placeholder for FG-010)
tests/              # Integration tests
```
