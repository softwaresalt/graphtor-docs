# Library API Contract: graphtor-core

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10

This document defines the public API surface of the `graphtor-core` library crate.

## Module: `config`

### Public Types

```text
SourceConfig
  - sources: Vec<Source>
  - parse(path: &Path) -> Result<SourceConfig, GraphtorError>
  - validate(&self) -> Result<(), GraphtorError>

Source (enum)
  - Git(GitSource)
  - Local(LocalSource)

GitSource
  - id: String
  - url: String
  - branch: String (default: "main")
  - include: Vec<String>
  - exclude: Vec<String>

LocalSource
  - id: String
  - path: String
  - include: Vec<String>
  - exclude: Vec<String> (default: empty)
```

### Key Behaviors

- `SourceConfig::parse()` reads a YAML file, deserializes it, and runs validation.
- Validation checks: duplicate IDs, empty required fields, glob syntax validity.
- Errors include the file path and the specific field/line that failed.

## Module: `error`

### Public Types

```text
GraphtorError (enum)
  - Config { message: String, field: Option<String> }
  - Database { message: String, operation: String }
  - Pipeline { message: String, stage: String }
  - Parse { message: String, path: Option<PathBuf> }
  - Embed { message: String, chunk_id: Option<String> }
  - PathViolation { attempted: PathBuf, allowed_root: PathBuf }
  - Sync { message: String, source_id: String }
  - Io(std::io::Error)
```

### Key Behaviors

- All variants implement `Display` with format: `[{category}] {message}: {context}`
- `From<std::io::Error>` converts to `GraphtorError::Io` automatically.
- `From<serde_yaml::Error>` converts to `GraphtorError::Config` with parse context.

## Module: `chunk`

### Public Functions

```text
generate_chunk_id(content: &str, source_path: &str) -> Result<String, GraphtorError>
```

### Key Behaviors

- Returns 64-character lowercase hex string (SHA-256) on success.
- Input: `content + "\0" + source_path`.
- Returns `GraphtorError::Parse` if `content` or `source_path` is empty.
- Deterministic: same inputs always produce the same output.

## Module: `logging`

### Public Functions

```text
init_logging(verbosity: LogVerbosity) -> Result<(), GraphtorError>

LogVerbosity (enum)
  - Quiet   (ERROR only)
  - Normal  (INFO + WARN + ERROR)
  - Verbose (DEBUG + INFO + WARN + ERROR)
```

### Key Behaviors

- Initializes the global tracing subscriber. Call once at application startup.
- Output goes to stderr with format: `{timestamp} {level} {target}: {message}`.
- Returns error if called more than once (tracing subscriber already set).

## Module: `path`

### Public Functions

```text
validate_path(path: &Path, allowed_root: &Path) -> Result<PathBuf, GraphtorError>
```

### Key Behaviors

- Resolves `path` to absolute, canonicalized form.
- Checks that resolved path starts with `allowed_root` (also canonicalized).
- Returns the resolved `PathBuf` on success.
- Returns `GraphtorError::PathViolation` on failure.
- Handles `..` traversal, symlinks, and relative paths.
