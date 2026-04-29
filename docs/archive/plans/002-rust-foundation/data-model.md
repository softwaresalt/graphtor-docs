# Data Model: Rust Foundation & Core Types

**Branch**: `002-rust-foundation` | **Date**: 2026-03-10

## Entities

### SourceConfig

Top-level configuration parsed from `sources.yaml`.

| Field | Type | Description |
|-------|------|-------------|
| sources | Vec&lt;Source&gt; | List of documentation sources (Git or local) |

**Source**: Parsed from `sources.yaml` at application startup.

### Source (enum)

A documentation source, either a Git repository or a local directory. Discriminated in YAML by the `type` field (e.g., `type: git` or `type: local`). Uses serde's internally tagged enum deserialization (`#[serde(tag = "type")]`).

**Variant: GitSource**

| Field | Type | Description |
|-------|------|-------------|
| id | String | Unique identifier for this source (e.g., "ms-azure-core") |
| url | String | Git clone URL |
| branch | String | Branch to clone (default: "main") |
| include | Vec&lt;String&gt; | Glob patterns for files to include (e.g., `["**/*.md"]`) |
| exclude | Vec&lt;String&gt; | Glob patterns for files to exclude (e.g., `["**/drafts/**"]`) |

**Variant: LocalSource**

| Field | Type | Description |
|-------|------|-------------|
| id | String | Unique identifier for this source (e.g., "internal-api-docs") |
| path | String | Filesystem path to the local documentation directory |
| include | Vec&lt;String&gt; | Glob patterns for files to include |
| exclude | Vec&lt;String&gt; | Glob patterns for files to exclude (default: empty) |

**Uniqueness**: Source `id` must be unique across all sources in the config.

**Validation rules**:
- `id` must be non-empty, composed of alphanumeric characters, hyphens, and underscores.
- `url` must be a valid Git URL format (for GitSource).
- `path` must point to an existing directory (for LocalSource).
- `include` and `exclude` patterns must be valid glob syntax.
- No duplicate `id` values across all sources.

### ChunkId

A deterministic identifier for a documentation chunk.

| Field | Type | Description |
|-------|------|-------------|
| value | String | 64-character lowercase hexadecimal SHA-256 hash |

**Generation**: `SHA-256(text_content + "\0" + source_path)` → hex-encoded lowercase.

**Properties**:
- Deterministic: same input always produces the same output.
- Unique: different inputs produce different outputs (within collision resistance bounds).
- Fixed-length: always 64 hex characters.
- Cross-database correlation key linking LanceDB vectors to Kùzu graph nodes.

### GraphtorError

Categorized error type for all system failures.

| Variant | Description | Context Fields |
|---------|-------------|---------------|
| Config | Configuration parsing or validation error | field_name, file_path |
| Database | Database operation failure | operation, db_type |
| Pipeline | Pipeline stage execution failure | stage_name, item_path |
| Parse | Markdown parsing error | file_path, line_number |
| Embed | Embedding generation failure | chunk_id, model_name |
| PathViolation | Path escapes allowed root | attempted_path, allowed_root |
| Sync | Sync state or diff detection failure | source_id, operation |
| Io | Filesystem I/O error | path, operation |

**Error Display**: Each variant produces a human-readable message in the format:
`[{category}] {description}: {context}` (e.g., `[config] missing required field 'id': sources.yaml, source #2`)

### LogLevel

Severity levels for structured log output.

| Level | Purpose | Example |
|-------|---------|---------|
| ERROR | Failures requiring attention | `[ERROR] failed to parse sources.yaml: invalid glob pattern at line 12` |
| WARN | Recoverable issues | `[WARN] skipping file: path contains non-UTF-8 characters` |
| INFO | Pipeline milestones | `[INFO] stage 'parse' complete: 1,523 chunks from 847 files (12.3s)` |
| DEBUG | Per-item processing details | `[DEBUG] parsing file: docs/azure/auth.md (2.1KB)` |

## Relationships

```text
SourceConfig ──1:N──▸ Source (Git or Local)
                         │
                         ├── id (unique across config)
                         ├── include/exclude patterns
                         └── validated at parse time

ChunkId ──── generated from (text_content, source_path)
    │
    └── Used by: FG-005 (vector store), FG-006 (graph store)
         as cross-database correlation key

GraphtorError ──── used by all feature groups
    │
    └── Each variant carries context specific to the failure domain
```

## Validation Rules

- Source `id` must match regex `^[a-zA-Z0-9][a-zA-Z0-9_-]*$` (starts with alphanumeric).
- Source `id` must be unique within a `SourceConfig`.
- GitSource `url` must be non-empty.
- GitSource `branch` defaults to `"main"` if not specified.
- LocalSource `path` must be non-empty.
- All glob patterns must be parseable by `globset`.
- ChunkId input `text_content` must be non-empty.
- ChunkId input `source_path` must be non-empty.
- Path validation requires the allowed root to be an absolute, canonicalized path.
