# Behavioral Scenarios: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10  
**Source**: spec.md, plan.md, data-model.md

## Scenario Matrix

### Configuration Parsing

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S001 | Config – Happy Path | Parse valid sources.yaml with Git and local sources | A sources.yaml with one Git source (id, url, branch, include, exclude) and one local source (id, path, include) | The config is parsed | All fields are correctly deserialized into typed structures; Git source has all fields; local source has all fields | FR-001 | P1 |
| S002 | Config – Happy Path | Parse config with defaults | A sources.yaml where a Git source omits the `branch` field | The config is parsed | The `branch` field defaults to "main" | FR-001 | P1 |
| S003 | Config – Happy Path | Parse config with multiple sources | A sources.yaml with 5 Git sources and 3 local sources | The config is parsed | All 8 sources are parsed with correct types and field values | FR-001 | P1 |
| S004 | Config – Validation | Duplicate source IDs rejected | A sources.yaml where two sources share the same `id` value | Validation runs | An error is reported identifying the duplicated ID and both source positions | FR-011 | P1 |
| S005 | Config – Validation | Missing required field reported | A sources.yaml where a Git source has no `id` field | The config is parsed | A Config error is reported identifying the missing field and the source position | FR-002 | P1 |
| S006 | Config – Validation | Invalid glob pattern reported | A sources.yaml where include contains `[invalid` (unclosed bracket) | Validation runs | An error is reported identifying the invalid pattern and explaining the syntax issue | FR-002, FR-012 | P1 |
| S007 | Config – Validation | Empty sources list | A sources.yaml with an empty `sources: []` array | The config is parsed | A warning or error is reported that no sources are configured | FR-002 | P2 |
| S008 | Config – Edge | Valid YAML but wrong structure | A sources.yaml containing a YAML string instead of a mapping | The config is parsed | A Config error is reported explaining the expected structure | FR-002 | P2 |
| S009 | Config – Edge | Non-existent config file | The path to sources.yaml does not exist on disk | The config is parsed | An Io error is reported with the file path and "not found" context | FR-002 | P2 |
| S010 | Config – Edge | UTF-8 BOM in config file | A sources.yaml file starting with a UTF-8 BOM marker | The config is parsed | The file is parsed successfully (BOM is handled) | FR-001 | P3 |
| S011 | Config – Edge | Source ID with special characters | A sources.yaml where a source `id` is "my source!" (contains spaces and punctuation) | Validation runs | An error is reported that the ID contains invalid characters, explaining the allowed pattern | FR-002 | P2 |
| S040 | Config – Edge | Include/exclude pattern precedence | A sources.yaml where a file matches both an `include` and an `exclude` pattern | Validation or filtering runs | The `exclude` pattern takes priority and the file is excluded | FR-012 | P2 |

### Error Handling

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S012 | Error – Format | Config error display | A Config error variant with message "missing field 'id'" and field "sources[1].id" | The error is displayed | Output reads: `[config] missing field 'id': sources[1].id` | FR-004 | P1 |
| S013 | Error – Format | PathViolation error display | A PathViolation error with attempted path "/etc/passwd" and allowed root "/home/dev/docs" | The error is displayed | Output includes the category "path violation", the attempted path, and the allowed root | FR-004 | P1 |
| S014 | Error – Format | Io error conversion | A std::io::Error of kind NotFound for path "missing.yaml" | The error is converted to GraphtorError | It becomes a GraphtorError::Io variant preserving the original error context | FR-003 | P1 |
| S015 | Error – Format | Pipeline error with stage context | A Pipeline error for stage "parse" processing file "docs/auth.md" | The error is displayed | Output includes the stage name and the file path, enabling the developer to locate the failure | FR-004 | P1 |
| S016 | Error – Hierarchy | All variant categories exist | The error type is inspected | Each variant is constructed | All 8 categories (Config, Database, Pipeline, Parse, Embed, PathViolation, Sync, Io) are distinct and matchable | FR-003 | P1 |
| S017 | Error – Edge | Error with empty message | An error variant with an empty string message | The error is displayed | Output still includes the category prefix; no panic or empty output | FR-004 | P3 |

### Chunk ID Generation

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S018 | ChunkID – Happy Path | Generate ID from content and path | Text "## Auth\nUse OAuth2..." and path "docs/azure/auth.md" | chunk_id is generated | A 64-character lowercase hex string is returned | FR-005 | P1 |
| S019 | ChunkID – Determinism | Same input produces same ID | The exact same text and path are provided in two separate calls | Both chunk_ids are compared | They are identical | FR-006 | P1 |
| S020 | ChunkID – Determinism | Different content produces different ID | Two different text strings with the same path | Both chunk_ids are compared | They are different | FR-005 | P1 |
| S021 | ChunkID – Determinism | Different path produces different ID | The same text content with two different paths | Both chunk_ids are compared | They are different (path is part of identity) | FR-005 | P1 |
| S022 | ChunkID – Format | ID is always 64 hex characters | Any valid content/path input | chunk_id is generated | The result matches regex `^[0-9a-f]{64}$` | FR-005 | P1 |
| S023 | ChunkID – Edge | Empty content | An empty string for content with a valid path | chunk_id generation is attempted | Either an error is returned or a valid (but possibly undesirable) ID is generated; the system does not panic | FR-005 | P2 |
| S024 | ChunkID – Edge | Unicode content | Content containing CJK characters, emoji, and combining marks | chunk_id is generated | A valid 64-char hex ID is returned (UTF-8 bytes are hashed) | FR-005 | P2 |
| S025 | ChunkID – Edge | Very large content | A 10MB text chunk | chunk_id is generated | A valid ID is returned without excessive memory usage or timeout | FR-005 | P3 |

### Logging

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S026 | Logging – Happy Path | INFO message emitted | Logging initialized at Normal verbosity | An INFO-level message is logged | The message appears in stderr output with timestamp and level | FR-009 | P1 |
| S027 | Logging – Filter | DEBUG filtered at Normal verbosity | Logging initialized at Normal verbosity | A DEBUG-level message is logged | The message does NOT appear in output | FR-010 | P1 |
| S028 | Logging – Filter | DEBUG shown at Verbose | Logging initialized at Verbose verbosity | A DEBUG-level message is logged | The message appears in output with timestamp and level | FR-010 | P1 |
| S029 | Logging – Filter | Only ERROR at Quiet | Logging initialized at Quiet verbosity | INFO, WARN, and ERROR messages are logged | Only the ERROR message appears in output | FR-010 | P1 |
| S030 | Logging – Format | Structured log fields | Logging initialized | A message with structured fields (file_count=42, stage="parse") is logged | The structured fields appear in the output | FR-009 | P2 |
| S031 | Logging – Edge | Double initialization | Logging is initialized twice | The second initialization is attempted | An error is returned (not a panic); the first subscriber remains active | FR-009 | P2 |

### Path Security

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S032 | Path – Happy Path | Valid relative path within root | Allowed root is "/home/dev/docs", path is "azure/auth.md" | Path is validated | The resolved absolute path is returned (e.g., "/home/dev/docs/azure/auth.md") | FR-007 | P1 |
| S033 | Path – Happy Path | Valid absolute path within root | Allowed root is "/home/dev/docs", path is "/home/dev/docs/azure/auth.md" | Path is validated | The path is accepted and returned | FR-007 | P1 |
| S034 | Path – Security | Traversal with .. rejected | Allowed root is "/home/dev/docs", path is "../../etc/passwd" | Path is validated | A PathViolation error is returned identifying the attempted path and the allowed root | FR-007, FR-008 | P1 |
| S035 | Path – Security | Absolute path outside root rejected | Allowed root is "/home/dev/docs", path is "/tmp/evil.md" | Path is validated | A PathViolation error is returned | FR-007 | P1 |
| S036 | Path – Security | Deeply nested traversal | Path is "a/b/c/../../../../etc/passwd" | Path is validated against a root 3 levels deep | A PathViolation error is returned (net traversal escapes root) | FR-008 | P1 |
| S037 | Path – Edge | Path with redundant separators | Path is "docs///azure//auth.md" | Path is validated | The path is normalized and accepted if within root | FR-008 | P2 |
| S038 | Path – Edge | Non-existent path within root | A path that doesn't exist yet but is within the allowed root | Path is validated | Validation either accepts it (parent exists) or returns an appropriate error | FR-007 | P2 |
| S039 | Path – Edge | Windows-style path separators | Path uses backslashes "docs\\azure\\auth.md" on Windows | Path is validated | The path is normalized using platform conventions and validated correctly | FR-008 | P2 |
| S041 | Path – Security | Symlink escaping allowed root | A symlink inside the allowed root points to a directory outside the root (e.g., /home/dev/docs/link → /etc/) | A file accessed through the symlink is validated | A PathViolation error is returned because the resolved path is outside the allowed root | FR-007, FR-008 | P1 |

## Summary

| Category | Count | P1 | P2 | P3 |
|----------|-------|----|----|----|
| Configuration Parsing | 12 | 6 | 5 | 1 |
| Error Handling | 6 | 4 | 0 | 1 |
| Chunk ID Generation | 8 | 5 | 2 | 1 |
| Logging | 6 | 4 | 2 | 0 |
| Path Security | 9 | 6 | 3 | 0 |
| **Total** | **41** | **25** | **12** | **3** |
