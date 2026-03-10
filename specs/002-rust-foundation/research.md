# Research: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10

## Research Tasks

### RT-001: Error Handling Strategy in Rust

**Decision**: Use `thiserror` for the domain error enum (`GraphtorError`) with typed variants for each failure category.

**Rationale**: `thiserror` generates `Display` and `Error` trait implementations from derive macros, keeping error definitions concise. It integrates cleanly with the `?` operator and `From` trait for automatic error conversion from external crate errors (e.g., `std::io::Error`, `serde_yaml::Error`).

**Alternatives considered**:
- `anyhow` — provides ad-hoc error propagation but loses type information. Better suited for application code (binary crate) than library code where callers need to match on error variants. Will use `anyhow` in the binary crate (`main.rs`) for top-level error reporting.
- Manual `Error` trait implementations — more boilerplate, no benefit over `thiserror`.
- `eyre` — similar to `anyhow` with better error reports. Same trade-off: loses typed variants.

### RT-002: Configuration Format and Parsing

**Decision**: Use `sources.yaml` parsed via `serde` + `serde_yaml` with strongly typed structs. Validate at parse time using custom `serde` deserializers and a post-parse validation pass.

**Rationale**: YAML is human-readable and supports comments (unlike JSON). `serde_yaml` provides zero-copy deserialization into Rust structs with excellent error messages for malformed input. Post-parse validation catches semantic errors (duplicate IDs, invalid glob patterns) that YAML syntax validation cannot.

**Alternatives considered**:
- TOML — also human-readable with comments, but less natural for deeply nested structures like include/exclude pattern lists. YAML's list syntax is more ergonomic for this use case.
- JSON — no comment support, less human-friendly for configuration files.
- Custom format — unnecessary complexity with no benefit.

### RT-003: Chunk ID Generation Algorithm

**Decision**: SHA-256 hash of `(normalized_text + "\0" + source_path)` producing a 64-character lowercase hex string. Use the `sha2` crate.

**Rationale**: SHA-256 provides collision resistance sufficient for the expected corpus size (millions of chunks). The null-byte separator prevents ambiguity between content and path. Hex encoding produces a fixed-length, URL-safe, filesystem-safe identifier.

**Alternatives considered**:
- UUID v4 — random, not deterministic. Same content would get different IDs on each run, breaking the reproducibility requirement.
- UUID v5 (name-based) — deterministic but SHA-1 based (weaker collision resistance) and longer string format. SHA-256 hex is more compact for this use case.
- BLAKE3 — faster than SHA-256 but adds a dependency for negligible gain at this scale. `sha2` is more widely used and audited.
- Content-only hash (no path) — same text at different paths would collide. Path is part of identity per spec FR-005/FR-006.

### RT-004: Logging Framework

**Decision**: Use `tracing` + `tracing-subscriber` for structured, async-safe logging with level filtering.

**Rationale**: `tracing` is the Rust ecosystem standard for structured diagnostics. It supports spans (for tracking pipeline stage context), structured fields, and level-based filtering. `tracing-subscriber` provides the output formatting layer. Both are maintained by the Tokio project and integrate seamlessly with the async runtime needed by FG-009 (MCP server).

**Alternatives considered**:
- `log` + `env_logger` — simpler but lacks structured spans and async-safety. Would need migration when FG-009 adds async.
- `slog` — powerful but more complex API. `tracing` has become the de facto standard.
- `println!` / `eprintln!` — no level filtering, no structured output. Violates constitution's structured logging requirement.

### RT-005: Path Security Approach

**Decision**: Use `std::fs::canonicalize()` to resolve symlinks and `..` segments, then check `starts_with()` against the canonicalized allowed root.

**Rationale**: `canonicalize()` resolves all symbolic links and relative components, producing an absolute path. The `starts_with()` check on canonicalized paths is immune to traversal attacks. This is the standard approach in Rust for path sandboxing.

**Alternatives considered**:
- String-based path prefix checking — vulnerable to `..` traversal and symlink attacks.
- `path_clean` crate — normalizes paths without resolving symlinks. Insufficient for security since symlinks can escape the boundary.
- `cap-std` (capability-based filesystem) — more robust sandboxing but heavier dependency. Overkill for this use case where we control all filesystem access.

**Caveat**: `canonicalize()` requires the path to exist on disk. For validating paths before creation, we canonicalize the parent directory and check the constructed child path.

### RT-006: Glob Pattern Matching

**Decision**: Use the `globset` crate for compiling and matching include/exclude glob patterns against file paths.

**Rationale**: `globset` (from the same author as `ripgrep`) is highly optimized for matching a single path against multiple patterns simultaneously. It compiles patterns into an efficient automaton. Supports `**` recursive matching, `!` negation, and brace expansion.

**Alternatives considered**:
- `glob` crate — designed for filesystem traversal (reading directories), not pattern matching against known paths. Less efficient for the filter-a-file-list use case.
- Regex — more powerful but less readable for file patterns. Glob syntax is the user expectation for file filtering.
- Custom glob implementation — unnecessary when `globset` exists and is well-maintained.

## Dependency Justification

| Crate | Purpose | Constitution Check |
|-------|---------|-------------------|
| `thiserror` | Derive macros for typed error enum | ✅ No stdlib equivalent for derive-based error types |
| `serde` | Serialization/deserialization framework | ✅ Required by constitution for sources.yaml and .sync_state.json |
| `serde_yaml` | YAML configuration parsing | ✅ Required by constitution for sources.yaml |
| `serde_json` | JSON state persistence | ✅ Required by constitution for .sync_state.json |
| `sha2` | SHA-256 hash for chunk_id | ✅ No stdlib SHA-256. Required for deterministic chunk correlation |
| `tracing` | Structured logging | ✅ Required by constitution for structured logging via tracing |
| `tracing-subscriber` | Log output formatting | ✅ Required companion to tracing |
| `globset` | Glob pattern matching | ✅ No stdlib glob matching. Required for include/exclude file filtering |
