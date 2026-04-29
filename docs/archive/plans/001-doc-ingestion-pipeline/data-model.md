# Data Model: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09

## Entities

### RepositoryManifest

Represents the configuration source that defines which documentation repositories to target.

| Field | Type | Description |
|-------|------|-------------|
| file_path | string | Path to the manifest file (e.g., `paths/ms-docs-grouped.txt`) |
| groups | list[RepositoryGroup] | Parsed thematic groups from the manifest |

**Source**: Parsed from `paths/ms-docs-grouped.txt` at pipeline start.

### RepositoryGroup

A thematic grouping of documentation repositories.

| Field | Type | Description |
|-------|------|-------------|
| number | int | Group number (1-based, from manifest headers) |
| title | string | Group title (e.g., "AZURE CORE & CLI") |
| description | string | Group description from manifest comments |
| urls | list[string] | Git clone URLs for repositories in this group |
| folder_name | string | Sanitized folder name derived from title |

**Uniqueness**: Group number is unique within a manifest.

### Document

A single markdown file from an acquired repository, after normalization.

| Field | Type | Description |
|-------|------|-------------|
| source_path | string | Absolute file path to the normalized markdown file |
| source_url | string | Reconstructed URL or relative path within the repository |
| title | string | Extracted from YAML frontmatter `title` field |
| date | string | Extracted from YAML frontmatter `ms.date` field |
| description | string | Extracted from YAML frontmatter `description` field |
| group_name | string | Thematic group this document belongs to |
| content | string | Normalized markdown text (frontmatter and UI tags stripped) |

**Lifecycle**: Raw → Normalized (frontmatter stripped, UI tags removed).

### Chunk

A self-contained segment of a document, suitable for embedding and retrieval.

| Field | Type | Description |
|-------|------|-------------|
| chunk_id | string | Deterministic hash (SHA-256 of normalized_text + source_path), hex-encoded |
| text | string | The markdown text content of this chunk |
| document_title | string | Title of the parent document |
| source_url | string | Source URL or path of the parent document |
| parent_headers | list[string] | Heading hierarchy leading to this chunk (e.g., ["## Authentication", "### OAuth2"]) |
| heading_level | int | The heading level that starts this chunk (2 for H2, 3 for H3) |

**Uniqueness**: `chunk_id` is globally unique (content-hash). Same content at the same path always produces the same ID.

**State transitions**: Created → Embedded (vector computed) → Loaded (stored in LanceDB).

### EmbeddedChunk

A chunk with its computed vector embedding, ready for LanceDB storage.

| Field | Type | Description |
|-------|------|-------------|
| chunk_id | string | Same as Chunk.chunk_id — the correlation key |
| vector | list[float] | 768-dimensional embedding from nomic-embed-text |
| text | string | The markdown text content |
| document_title | string | Title of the parent document |
| source_url | string | Source URL or path |

**Storage**: LanceDB table with `chunk_id` as primary key.

### GraphNode

A structured entity extracted from a chunk by the LLM.

| Field | Type | Description |
|-------|------|-------------|
| name | string | Entity name (e.g., "Azure Blob Storage", "BlobClient") |
| node_type | enum | One of: Service, SDK_Class, Concept, CodeSnippet |
| description | string | Brief description extracted by the LLM |
| language | string | Programming language (for SDK_Class and CodeSnippet; null otherwise) |
| chunk_id | string | The chunk this entity was extracted from — correlation key to LanceDB |

**Uniqueness**: (name, node_type) is unique. If the same entity appears in multiple chunks, the first occurrence defines it; subsequent occurrences add chunk_id associations.

### GraphEdge

A directed relationship between two graph nodes.

| Field | Type | Description |
|-------|------|-------------|
| source_name | string | Name of the source node |
| source_type | enum | Node type of the source |
| target_name | string | Name of the target node |
| target_type | enum | Node type of the target |
| relationship | enum | One of: CONTAINS, REQUIRES_CONFIG, HAS_EXAMPLE |

**Validation**: Both source and target nodes must exist before the edge is created.

## Relationships

```text
RepositoryManifest ──1:N──▸ RepositoryGroup ──1:N──▸ Document ──1:N──▸ Chunk
                                                                         │
                                                            ┌────────────┴────────────┐
                                                            ▼                         ▼
                                                      EmbeddedChunk              GraphNode
                                                      (LanceDB)               (Kùzu - nodes)
                                                            ▲                         │
                                                            │                    GraphEdge
                                                            │                 (Kùzu - relationships)
                                                            │                         │
                                                            └─── chunk_id ────────────┘
                                                        (cross-database correlation key)
```

## Kùzu Property Graph Schema

```cypher
CREATE NODE TABLE Service (
    name STRING,
    description STRING,
    PRIMARY KEY (name)
);

CREATE NODE TABLE SDK_Class (
    name STRING,
    language STRING,
    chunk_id STRING,
    description STRING,
    PRIMARY KEY (name)
);

CREATE NODE TABLE Concept (
    name STRING,
    description STRING,
    chunk_id STRING,
    PRIMARY KEY (name)
);

CREATE NODE TABLE CodeSnippet (
    chunk_id STRING,
    language STRING,
    description STRING,
    PRIMARY KEY (chunk_id)
);

CREATE REL TABLE CONTAINS (FROM Service TO SDK_Class);
CREATE REL TABLE REQUIRES_CONFIG (FROM SDK_Class TO Service);
CREATE REL TABLE HAS_EXAMPLE (FROM SDK_Class TO CodeSnippet);
```

## LanceDB Schema

```python
schema = pa.schema([
    pa.field("chunk_id", pa.string()),              # Primary key, correlates to Kùzu
    pa.field("vector", pa.list_(pa.float32(), 768)),# nomic-embed-text embedding
    pa.field("text", pa.string()),                  # Markdown content
    pa.field("document_title", pa.string()),        # Parent document title
    pa.field("source_url", pa.string()),            # Source URL or path
])
```

## Validation Rules

- `chunk_id` must be a 64-character hex string (SHA-256).
- `vector` must have exactly 768 dimensions.
- `text` must be non-empty.
- `node_type` must be one of the four defined enum values.
- `relationship` must be one of the three defined enum values.
- All graph nodes must have a non-empty `name`.
- `CodeSnippet` nodes must have a non-empty `language`.
