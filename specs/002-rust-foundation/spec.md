# Feature Specification: Rust Foundation & Core Types

**Feature Branch**: `002-rust-foundation`  
**Created**: 2026-03-10  
**Status**: Draft  
**Input**: User description: "Rust project foundation: Cargo workspace scaffolding, core error types, configuration structures for sources.yaml, logging infrastructure, path security utilities, and deterministic chunk_id generation. Foundational layer for the Rust-native GraphRAG MCP plugin."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Initialize a New Documentation Index Project (Priority: P1)

A developer wants to start using the GraphRAG documentation plugin in their workspace. They create a `sources.yaml` configuration file that defines which documentation sources (Git repositories and local directories) they want indexed. The system validates the configuration file and reports any errors in a clear, actionable format before any processing begins.

**Why this priority**: Without a valid configuration, no pipeline stages can execute. Configuration parsing and validation is the entry point to the entire system.

**Independent Test**: Can be fully tested by providing various well-formed and malformed `sources.yaml` files and verifying that valid configs are accepted and invalid configs produce clear error messages identifying the exact problem.

**Acceptance Scenarios**:

1. **Given** a `sources.yaml` file with one Git source and one local directory source, **When** the system reads the configuration, **Then** it correctly parses all fields (id, url, branch, path, include patterns, exclude patterns) and makes them available for downstream processing.
2. **Given** a `sources.yaml` file with a missing required field (e.g., missing `id` for a source), **When** the system attempts to parse it, **Then** it reports a clear error message identifying the missing field and its location in the file.
3. **Given** a `sources.yaml` file with an invalid glob pattern in the include/exclude fields, **When** the system validates the configuration, **Then** it reports which pattern is invalid and why, without crashing.
4. **Given** a `sources.yaml` file with no sources defined, **When** the system reads it, **Then** it reports that no sources are configured and provides guidance on how to add sources.

---

### User Story 2 - Receive Clear Error Diagnostics During Pipeline Execution (Priority: P1)

A developer encounters an error during any pipeline stage (acquisition, parsing, embedding, or loading). The system provides a structured error message that identifies the error category, the specific operation that failed, and enough context to diagnose and fix the issue. Errors in one file do not halt processing of other files.

**Why this priority**: Without clear error diagnostics, developers cannot troubleshoot failures. Error handling infrastructure is required by every pipeline stage.

**Independent Test**: Can be fully tested by triggering various error conditions (invalid paths, missing files, permission errors) and verifying that each produces a categorized, human-readable error message with actionable context.

**Acceptance Scenarios**:

1. **Given** a pipeline operation encounters a file that does not exist, **When** the error is reported, **Then** the error message includes the error category (e.g., I/O), the file path that was not found, and the operation that was attempted.
2. **Given** a pipeline operation attempts to access a path outside the allowed workspace, **When** the error is reported, **Then** the error message identifies it as a path security violation and includes both the attempted path and the allowed boundary.
3. **Given** a pipeline stage encounters errors in multiple files, **When** processing completes, **Then** each error is individually logged with its category and context, and a summary reports the total count of successes and failures.
4. **Given** any error occurs, **When** a developer reads the error message, **Then** they can determine the category of error, what operation was being performed, and what specific input caused the failure — without needing to read source code.

---

### User Story 3 - Track Document Chunks Across Storage Systems (Priority: P2)

A developer has documentation that is stored in both a vector database (for semantic search) and a graph database (for structural navigation). When they retrieve a chunk from either system, they need a stable identifier that allows them to cross-reference the same chunk in the other system. The identifier must be deterministic — the same document content at the same path must always produce the same identifier, even across separate pipeline runs.

**Why this priority**: Chunk correlation is the fundamental data integrity mechanism that links the vector and graph databases. Without stable identifiers, the two databases cannot be cross-referenced.

**Independent Test**: Can be fully tested by generating identifiers for known inputs and verifying determinism (same input → same ID), uniqueness (different input → different ID), and format consistency.

**Acceptance Scenarios**:

1. **Given** a text chunk and its source file path, **When** the system generates a chunk identifier, **Then** the identifier is a 64-character hexadecimal string derived from the content and path.
2. **Given** the same text chunk and source path are processed in two separate pipeline runs, **When** identifiers are compared, **Then** they are identical.
3. **Given** two different text chunks from the same file, **When** identifiers are generated for each, **Then** the identifiers are different.
4. **Given** the same text content at two different file paths, **When** identifiers are generated for each, **Then** the identifiers are different (path is part of the identity).

---

### User Story 4 - Monitor Pipeline Progress Through Structured Logs (Priority: P2)

A developer runs the ingestion pipeline and wants to understand what is happening at each stage. The system emits structured log messages that report milestones (stage starts/completions, item counts), per-item details (at verbose level), warnings for recoverable issues, and errors for failures. Logs are filterable by verbosity level.

**Why this priority**: Without structured logging, developers cannot monitor pipeline health or diagnose issues in large documentation sets.

**Independent Test**: Can be fully tested by running pipeline operations and verifying that log output contains expected structure (timestamps, levels, stage context) and that verbosity filtering works correctly.

**Acceptance Scenarios**:

1. **Given** a pipeline stage begins processing, **When** the stage starts, **Then** a log message is emitted at INFO level identifying the stage name and the number of items to process.
2. **Given** a pipeline stage completes, **When** the stage finishes, **Then** a log message is emitted at INFO level reporting the stage name, items processed, items failed, and elapsed time.
3. **Given** the system is run with default verbosity, **When** per-file processing occurs, **Then** per-file details are NOT shown (they are at DEBUG level only).
4. **Given** a recoverable issue occurs (e.g., a file is skipped), **When** the warning is logged, **Then** the log message includes the WARN level, the file path, and the reason for skipping.

---

### User Story 5 - Ensure File Operations Stay Within Allowed Boundaries (Priority: P2)

A developer's documentation sources may reference paths that attempt to escape the designated workspace or data directory (through symlinks, `..` traversal, or absolute paths). The system validates all file paths against an allowed root directory and rejects any path that resolves outside the boundary, preventing unintended access to files outside the designated scope.

**Why this priority**: Path security prevents the system from reading or writing files outside the intended workspace, which is critical when processing untrusted documentation sources.

**Independent Test**: Can be fully tested by constructing paths with various traversal techniques (relative `..`, absolute paths, symlinks) and verifying that only paths resolving within the allowed root are accepted.

**Acceptance Scenarios**:

1. **Given** a file path that is within the allowed root directory, **When** the path is validated, **Then** it is accepted and the resolved absolute path is returned.
2. **Given** a file path containing `..` segments that would escape the allowed root, **When** the path is validated, **Then** it is rejected with a path security violation error.
3. **Given** an absolute path that is outside the allowed root, **When** the path is validated, **Then** it is rejected with a path security violation error.
4. **Given** a relative path that resolves within the allowed root after normalization, **When** the path is validated, **Then** it is accepted.

---

### Edge Cases

- What happens when `sources.yaml` contains duplicate source IDs?
- How does the system handle a `sources.yaml` file that is valid YAML but not a valid source configuration (e.g., a YAML array instead of the expected structure)?
- What happens when the chunk_id hash input contains non-UTF-8 bytes?
- How does path validation handle Windows UNC paths or drive letter differences?
- What happens when the logging infrastructure is initialized multiple times?
- How does the system handle extremely long file paths that approach OS limits?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse a `sources.yaml` configuration file that defines documentation sources, supporting both Git repository sources (with id, url, branch, include/exclude patterns) and local directory sources (with id, path, include patterns).
- **FR-002**: System MUST validate all configuration fields at parse time and report specific, actionable error messages for any malformed entries.
- **FR-003**: System MUST define a categorized error type hierarchy covering: configuration errors, database errors, pipeline errors, parsing errors, embedding errors, path security violations, sync errors, and I/O errors.
- **FR-004**: Every error reported by the system MUST include the error category, a human-readable description, and sufficient context (file path, operation name, etc.) to diagnose the issue.
- **FR-005**: System MUST generate deterministic chunk identifiers by computing a SHA-256 hash of the chunk's text content concatenated with its source file path, producing a 64-character hexadecimal string.
- **FR-006**: System MUST produce identical chunk identifiers for identical inputs across separate pipeline runs (deterministic reproducibility).
- **FR-007**: System MUST validate all file paths against a configurable allowed root directory, rejecting any path that resolves outside the boundary.
- **FR-008**: Path validation MUST handle relative paths, `..` traversal, and resolve symlinks before performing the boundary check.
- **FR-009**: System MUST emit structured log messages with severity levels: DEBUG (per-item details), INFO (pipeline milestones), WARN (recoverable issues), ERROR (failures).
- **FR-010**: System MUST support configurable log verbosity that filters output by severity level.
- **FR-011**: System MUST reject `sources.yaml` files with duplicate source IDs and report which IDs are duplicated.
- **FR-012**: System MUST provide a source configuration structure that supports `include` and `exclude` glob patterns for filtering which files within a source are processed.

### Key Entities

- **SourceConfig**: The top-level configuration representing a parsed `sources.yaml` file, containing a list of documentation sources.
- **GitSource**: A documentation source defined by a Git repository URL, branch, and file filtering patterns (include/exclude globs). Identified by a unique string ID.
- **LocalSource**: A documentation source defined by a local filesystem path and file filtering patterns. Identified by a unique string ID.
- **ChunkId**: A 64-character hexadecimal string derived from SHA-256 hash of content and source path. Used as the cross-database correlation key.
- **GraphtorError**: A categorized error with variant, human-readable message, and diagnostic context. Variants cover all pipeline failure modes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All valid `sources.yaml` configurations (Git + local sources with all supported fields) are parsed successfully with zero data loss.
- **SC-002**: All malformed `sources.yaml` files produce specific error messages that identify the exact field and issue — a developer can fix the problem on first reading of the error.
- **SC-003**: Chunk identifiers are 100% deterministic — the same content and path always produce the same 64-character hex ID across any number of runs.
- **SC-004**: 100% of paths containing `..` traversal or absolute paths outside the allowed root are rejected by path validation.
- **SC-005**: Log messages at each severity level (DEBUG, INFO, WARN, ERROR) contain structured context (timestamp, level, component) and are filterable by verbosity setting.
- **SC-006**: All error messages include category, description, and diagnostic context — no error requires reading source code to understand.
- **SC-007**: The foundation layer compiles and passes all tests as a standalone library with no dependency on downstream pipeline components.

## Assumptions

- The `sources.yaml` file is UTF-8 encoded.
- Source IDs are user-defined strings consisting of alphanumeric characters, hyphens, and underscores.
- The allowed root directory for path validation is set at application startup and does not change during a pipeline run.
- Log output goes to stderr by default, with structured formatting.
- This foundation is designed as a library that other feature groups (source management, parsing, embedding, storage, etc.) will depend on.
- Glob patterns follow standard glob syntax (e.g., `**/*.md`, `!**/drafts/**`).

## Out of Scope

- **Source acquisition**: Actually cloning Git repositories or scanning local directories (covered by FG-002: Source Management).
- **Markdown parsing**: Parsing markdown content or extracting structure (covered by FG-003: Markdown Parser).
- **Embedding generation**: Running ML models for vectorization (covered by FG-004: Embedding Engine).
- **Database operations**: Creating, reading, or writing to LanceDB or Kùzu (covered by FG-005/FG-006).
- **MCP server**: Protocol handling or tool registration (covered by FG-009: MCP Server).
- **CLI interface**: Command-line argument parsing or user interaction (covered by FG-010: CLI).
