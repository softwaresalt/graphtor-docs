---
title: Pipeline, Schema, and Embeddings Reference
description: "Docline-Markdown ingestion pipeline, CozoDB schema, embedding model details, and Datalog query examples"
---

graphtor-docs processes documentation through five sequential pipeline stages:
**Acquire → Validate → Parse → Embed → Load**. Each stage has a defined input
contract, output contract, and idempotency guarantee.

## Stage Overview

```text
sources.yaml
     │
     ▼
┌─────────────────────────────────────────────────┐
│ 1. Acquire                                      │
│ Input:  local source definitions                 │
│ Output: .md file paths from directory scan       │
│ Idempotent: yes — reads files in-place           │
└───────────────────────┬─────────────────────────┘
                        │ .md file paths
                        ▼
┌─────────────────────────────────────────────────┐
│ 2. Validate (docline v1 frontmatter contract)   │
│ Input:  .md file content                         │
│ Output: ValidatedFrontmatter + body text         │
│ Idempotent: yes — deterministic from file bytes  │
└───────────────────────┬─────────────────────────┘
                        │ ValidatedFrontmatter + body
                        ▼
┌─────────────────────────────────────────────────┐
│ 3. Parse                                        │
│ Input:  validated body text                      │
│ Output: ParsedDocument { chunks, edges }         │
│ Idempotent: yes — deterministic from file bytes  │
└───────────────────────┬─────────────────────────┘
                        │ ParsedDocument
                        ▼
┌─────────────────────────────────────────────────┐
│ 4. Embed (skippable with --no-embed)            │
│ Input:  chunk text                              │
│ Output: Vec<f32> (384-dim vector per chunk)      │
│ Idempotent: yes — deterministic from chunk text  │
└───────────────────────┬─────────────────────────┘
                        │ chunks + vectors
                        ▼
┌─────────────────────────────────────────────────┐
│ 5. Load                                         │
│ Input:  ParsedDocument + vectors + source ID     │
│ Output: upserts into CozoDB                     │
│ Idempotent: yes — upserts by stable chunk ID     │
└─────────────────────────────────────────────────┘
```

---

## Stage 1: Acquire

The acquire stage scans each configured local directory and applies the
source's include/exclude glob filters and Markdown format allow-list. Files are
read in-place — no copy is made to a cache directory. The directory path
configured in `sources.yaml` is the acquisition root.

Ingestion is docline-Markdown only. `type: database` entries are served
read-only and never scanned, and non-Markdown formats are excluded before parse.

---

## Stage 2: Validate

The validate stage reads each `.md` file and checks it against the docline v1
frontmatter contract before any chunking or embedding work begins.

### Docline v1 Frontmatter Contract

Every indexed file must begin with a YAML frontmatter block delimited by
`---`. The following fields are **required** and must be non-empty:

| Field | Type | Description |
|---|---|---|
| `title` | string | Human-readable document title |
| `source` | string | Origin URI or path of the source document |
| `ingested_at` | string | ISO-8601 timestamp when docline ingested the source |
| `doc_type` | string | Document-type identifier |
| `source_path` | string | Workspace-relative, forward-slash normalized path of the source artifact |

The following fields are **optional**:

| Field | Type | Default | Description |
|---|---|---|---|
| `description` | string | `""` | Short human-readable description |
| `content_sha256` | string | `""` | SHA-256 hex digest of the Markdown body; verified when present |
| `chunk_strategy` | string | `"h1-h2-h3"` | Chunk-boundary strategy identifier |
| `schema_version` | string | `"1.0"` | Semver contract version; only major version `1` is accepted |
| `docline` | object | `null` | Namespace for docline-only metadata; not promoted to the contract surface |

### Validation rules (fail-closed)

- Malformed YAML frontmatter → rejected
- Missing or empty required field → rejected
- `schema_version` major ≠ `1` → rejected
- `source_path` is empty, absolute, drive-prefixed, or contains `.`/`..` components → rejected
- `content_sha256` present and mismatches SHA-256(LF-normalised body) → rejected

Files that fail validation are reported as errors and excluded from the
pipeline. The sync continues with the remaining files; the run exits with
code `1` when any file is rejected.

### `source_path` as canonical identity

`source_path` combined with the source's `id` (from `sources.yaml`) is the
canonical logical identity for each document. The pipeline uses
`(source_id, source_path)` for duplicate-intake detection before loading any
chunks. `source_path` must be unique within a source.

---

## Stage 3: Parse

The parse stage reads the validated Markdown body and produces a
`ParsedDocument` containing chunks and graph edges.

### Markdown (`.md`, `.markdown`)

Uses `pulldown-cmark`'s AST event stream:

1. **Frontmatter stripping** — YAML frontmatter (between `---` delimiters) is
   detected and removed before chunking (already validated in Stage 2)
2. **Heading-based chunking** — the document is split at H1, H2, and H3 heading
   boundaries (`#`, `##`, `###`); H4–H6 headings are kept inside the enclosing
   chunk and do not start a new chunk
3. **Link extraction** — `[text](url)` links become `doc_edges` with
   `(src_chunk_id, target_path, link_text, anchor)`
4. **Code block extraction** — fenced code blocks become `doc_code` entries
   with language tag

### Chunk ID Derivation

Every chunk is assigned a stable **chunk ID**:

```text
chunk_id = SHA-256(content + "\0" + forward_slash_normalized_path)
```

- `content` is the raw text content of the chunk (after heading extraction)
- `"\0"` is a NUL byte separator that prevents collisions between content and
  path components
- `path` is the source-relative file path with **forward-slash separators**
  on all platforms (including Windows)
- The SHA-256 hex string is the chunk ID stored in `doc_chunks` and used as
  the key in `doc_edges` and `doc_code`

**Why forward-slash normalization matters:** On Windows, `Path::to_string_lossy()`
produces backslash paths. Without normalization, the same file would produce
different chunk IDs on Windows vs Linux/macOS, breaking incremental sync and
MCP search. All path strings stored in the database use forward slashes.

---

## Stage 4: Embed

The embed stage computes a 384-dimensional float32 vector for each chunk using
the `all-MiniLM-L6-v2` model.

### Model Details

| Property | Value |
|---|---|
| Model | `sentence-transformers/all-MiniLM-L6-v2` |
| Dimensions | 384 |
| Max token length | 512 tokens (content is truncated if longer) |
| Pooling | Mean pooling over token embeddings |
| Inference | In-process via Candle (pure Rust ML) |
| Runtime dependency | None — no external model server required |
| Cache location | `~/.cache/huggingface/hub/` (downloaded on first use) |
| Download size | ~80 MB |

### First-Run Download

On first use, the model files are downloaded from HuggingFace Hub and cached.
**Network access is required for first-run only.** Subsequent runs use the
local cache.

### Skipping Embeddings

Pass `--no-embed` to the `sync` subcommand to skip embedding entirely:

```sh
graphtor-docs sync --no-embed
```

When embeddings are skipped:
- No embedding vectors are stored in `doc_chunks.embedding`
- `search_semantic` in the MCP server returns empty results (no vectors stored;
  the model may still be loaded — it just has nothing to search)
- All other tools (`search_local_docs`, `traverse_doc_links`, etc.) work normally
- Sync is significantly faster — useful for text-only indexing

---

## Stage 5: Load

The load stage upserts parsed chunks and vectors into CozoDB. All writes use
upsert semantics (`?[...] <- [...] :put ...`) keyed by chunk ID — safe to
re-run on the same input.

Errors are **non-fatal by default**: if a single chunk fails to load, the
pipeline continues with the next file and reports the error at the end
(exit code `1`). Use `graphtor-docs sync --verbose` to see per-file failures.

---

## CozoDB Schema (v4)

The database uses CozoDB with a SQLite backend. By default, sync writes to
`.graphtor/graph.db`. Sources can override that target with the `database`
field in `sources.yaml`, which routes those sources into additional `.db`
files under `.graphtor/`.
The schema is a set of CozoDB relations. Embeddings are stored inline on
`doc_chunks.embedding`; semantic search performs exact brute-force cosine k-NN
over non-null embeddings at query time. No vector index is maintained, and
`ensure_schema` drops a legacy `doc_chunks:embedding_idx` relation if one is
present.

### `doc_schema_ver`

Tracks the schema version. Used by `ensure_schema` to verify compatibility.

```datalog
:create doc_schema_ver { ver: Int }
```

| Column | Type | Description |
|---|---|---|
| `ver` | Int | Schema version (current: `4`) |

---

### `doc_sources`

Registry of indexed documentation sources.

```datalog
:create doc_sources {
    source_id: String
    =>
    url: String, kind: String, name: String, synced_at: String?
}
```

| Column | Type | Description |
|---|---|---|
| `source_id` | String (PK) | Unique source identifier (from `sources.yaml`) |
| `url` | String | Filesystem path of the local source directory |
| `kind` | String | Always `"local"` |
| `name` | String | Display name (same as `source_id` by default) |
| `synced_at` | String? | ISO-8601 timestamp of last successful sync; reserved for a future release |

---

### `doc_chunks`

Stores every indexed text chunk, including its embedding vector.

```datalog
:create doc_chunks {
    chunk_id: String
    =>
    source_id: String, path: String, title: String?,
    position: Int, char_offset: Int, headings: String, content: String,
    embedding: <F32; 384>?
}
```

| Column | Type | Description |
|---|---|---|
| `chunk_id` | String (PK) | SHA-256 of `content + "\0" + path` |
| `source_id` | String | Foreign reference to `doc_sources.source_id` |
| `path` | String | Source-relative file path (forward-slash, e.g., `articles/intro.md`) |
| `title` | String? | Chunk heading text; `null` for document-level pre-heading content |
| `position` | Int | Ordinal position of this chunk within the document (0-based) |
| `char_offset` | Int | Character offset of this chunk's start within the raw file |
| `headings` | String | JSON array of the heading hierarchy from H1 down to this chunk's heading |
| `content` | String | Full text content of the chunk |
| `embedding` | \<F32; 384\>? | 384-dim float32 embedding vector; `null` when `--no-embed` was used |

Semantic search scans the `embedding` column directly. It computes cosine
distance between the query vector and each non-null stored embedding, orders by
distance, and returns the nearest chunks with exact recall.

---

### `doc_edges`

Document link graph — edges between source chunks and target paths.

```datalog
:create doc_edges {
    src_chunk_id: String, target_path: String
    =>
    link_text: String, anchor: String?
}
```

| Column | Type | Description |
|---|---|---|
| `src_chunk_id` | String (PK) | Source chunk containing the link |
| `target_path` | String (PK) | Target document path (relative or absolute URL) |
| `link_text` | String | Display text of the link |
| `anchor` | String? | Fragment identifier without the leading `#` (e.g., `section-name`); `null` if absent |

---

### `doc_code`

Extracted code snippets from fenced code blocks.

```datalog
:create doc_code {
    snippet_id: String
    =>
    chunk_id: String, language: String?, content: String
}
```

| Column | Type | Description |
|---|---|---|
| `snippet_id` | String (PK) | Stable identifier for the snippet |
| `chunk_id` | String | Parent chunk containing this code block |
| `language` | String? | Language tag from the code fence (e.g., `"rust"`, `"sh"`); `null` if unspecified |
| `content` | String | Raw code content |

---

### `doc_url_index`

Cross-source lookup table for docline-provided canonical document URLs.

```datalog
:create doc_url_index {
    canonical_url: String
    =>
    chunk_id: String
}
```

| Column | Type | Description |
|---|---|---|
| `canonical_url` | String (PK) | Absolute canonical document URL from docline frontmatter |
| `chunk_id` | String | Entry chunk for the document |

---

## Sample Datalog Queries

These queries can be run via the CozoDB CLI or any client with access to a
graphtor-docs database file such as `.graphtor/graph.db`.

### Find all chunks for a source

```datalog
?[chunk_id, path, title, position]
    := *doc_chunks{ chunk_id, source_id, path, title, position },
       source_id = "product-docs"
:order position
```

### Text search (case-insensitive substring)

```datalog
?[chunk_id, path, content]
    := *doc_chunks{ chunk_id, path, content },
       str_includes(lowercase(content), "incremental sync")
```

### Find outgoing links from a chunk

```datalog
?[target_path, link_text]
    := *doc_edges{ src_chunk_id, target_path, link_text },
       src_chunk_id = "abc123..."
```

### Count chunks per source

```datalog
?[source_id, count(chunk_id)]
    := *doc_chunks{ chunk_id, source_id }
```

### Check schema version

```datalog
?[ver] := *doc_schema_ver{ ver }
```
