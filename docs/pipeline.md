# Pipeline, Schema, and Embeddings Reference

graphtor-docs processes documentation through four sequential pipeline stages:
**Acquire → Parse → Embed → Load**. Each stage has a defined input contract,
output contract, and idempotency guarantee.

## Stage Overview

```
sources.yaml
     │
     ▼
┌─────────────────────────────────────────────────┐
│ 1. Acquire                                      │
│ Input:  source definitions (git/local/url)       │
│ Output: files on disk in .graphtor/data/{id}/    │
│ Idempotent: yes — skips existing clones          │
└───────────────────────┬─────────────────────────┘
                        │ files on disk
                        ▼
┌─────────────────────────────────────────────────┐
│ 2. Parse                                        │
│ Input:  file path + content                     │
│ Output: ParsedDocument { chunks, edges }         │
│ Idempotent: yes — deterministic from file bytes  │
└───────────────────────┬─────────────────────────┘
                        │ ParsedDocument
                        ▼
┌─────────────────────────────────────────────────┐
│ 3. Embed (skippable with --no-embed)            │
│ Input:  chunk text                              │
│ Output: Vec<f32> (384-dim vector per chunk)      │
│ Idempotent: yes — deterministic from chunk text  │
└───────────────────────┬─────────────────────────┘
                        │ chunks + vectors
                        ▼
┌─────────────────────────────────────────────────┐
│ 4. Load                                         │
│ Input:  ParsedDocument + vectors + source ID     │
│ Output: upserts into CozoDB                     │
│ Idempotent: yes — upserts by stable chunk ID     │
└─────────────────────────────────────────────────┘
```

---

## Stage 1: Acquire

The acquire stage fetches documentation from each configured source and places
files under `.graphtor/data/{source_id}/`.

### Git sources

Uses the `git2` crate to perform a shallow clone (`--depth 1`):

- If the repository directory already exists, acquisition is **skipped**
  (cloning is not re-run; incremental sync handles updates via git diff)
- Clone path: `.graphtor/data/{source_id}/`
- Only the configured `branch` is fetched

### Local sources

Scans the local directory path. Files are read in-place (no copy to
`.graphtor/data/`). The source directory is treated as the acquisition root.

### URL sources

Performs a BFS crawl using `ureq` (synchronous HTTP):

- Starts at the configured `url`
- Follows links up to `max_depth` hops
- Stays within the registered domain when `domain_lock: true`
- Waits `rate_limit_ms` milliseconds between requests
- Converts each HTML page to Markdown via `htmd`
- Stops when `max_pages` pages have been fetched
- Crawled pages are cached under `.graphtor/data/{source_id}/`

---

## Stage 2: Parse

The parse stage reads each file and produces a `ParsedDocument` containing
chunks and graph edges. The parser is dispatched by file extension.

### Markdown (`.md`, `.markdown`)

Uses `pulldown-cmark`'s AST event stream:

1. **Frontmatter stripping** — YAML frontmatter (between `---` delimiters) is
   detected and removed before chunking
2. **Heading-based chunking** — the document is split at each heading boundary
   (`#`, `##`, `###`, etc.); each heading + its content forms one chunk
3. **Link extraction** — `[text](url)` links become `doc_edges` with
   `(src_chunk_id, target_path, link_text, anchor)`
4. **Code block extraction** — fenced code blocks become `doc_code` entries
   with language tag

### PDF (`.pdf`)

Two-pass extraction using `pdf-extract`:

1. **Sample scan** — samples the first few pages using `HeadingAwareOutput`
   to build a font-size histogram
2. **Heading detection** — if the histogram shows distinct font sizes,
   larger font text is treated as section headings; otherwise, page-based
   chunking is used
3. **Large PDFs** (≥ 20 MiB) — routed to `PdfiumBackend` first if the
   PDFium DLL is available; falls back to `PdfExtractBackend` if not

### DOCX (`.docx`)

Planned — not yet implemented. DOCX files are silently skipped.

### Chunk ID Derivation

Every chunk is assigned a stable **chunk ID**:

```
chunk_id = SHA-256(content + forward_slash_normalized_path)
```

- `content` is the raw text content of the chunk (after heading extraction)
- `path` is the source-relative file path with **forward-slash separators**
  on all platforms (including Windows)
- The SHA-256 hex string is the chunk ID stored in `doc_chunks` and used as
  the key in `doc_edges` and `doc_vectors`

**Why forward-slash normalization matters:** On Windows, `Path::to_string_lossy()`
produces backslash paths. Without normalization, the same file would produce
different chunk IDs on Windows vs Linux/macOS, breaking incremental sync and
MCP search. All path strings stored in the database use forward slashes.

---

## Stage 3: Embed

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
- No vectors are stored in `doc_vectors`
- `search_semantic` in the MCP server will return an error (model not loaded)
- All other tools (`search_local_docs`, `traverse_doc_links`, etc.) work normally
- Sync is significantly faster — useful for text-only indexing

---

## Stage 4: Load

The load stage upserts parsed chunks and vectors into CozoDB. All writes use
upsert semantics (`?[...] <- [...] :put ...`) keyed by chunk ID — safe to
re-run on the same input.

Errors are **non-fatal by default**: if a single chunk fails to load, the
pipeline continues with the next file and reports the error at the end
(exit code `1`). Use `graphtor-docs sync --verbose` to see per-file failures.

---

## CozoDB Schema (v2)

The database uses CozoDB with a SQLite backend at `.graphtor/graph.db`.
Six stored relations form the schema:

### `doc_schema_ver`

Tracks the schema version. Used by `ensure_schema` to verify compatibility.

```datalog
:create doc_schema_ver { ver: Int }
```

| Column | Type | Description |
|---|---|---|
| `ver` | Int | Schema version (current: `2`) |

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
| `url` | String | Clone URL, filesystem path, or start URL |
| `kind` | String | `"git"`, `"local"`, or `"url"` |
| `name` | String | Display name (same as `source_id` by default) |
| `synced_at` | String? | ISO-8601 timestamp of last successful sync; `null` if never synced |

---

### `doc_chunks`

Stores every indexed text chunk.

```datalog
:create doc_chunks {
    chunk_id: String
    =>
    source_id: String, path: String, title: String?,
    position: Int, char_offset: Int, headings: String, content: String
}
```

| Column | Type | Description |
|---|---|---|
| `chunk_id` | String (PK) | SHA-256 of `content + path` |
| `source_id` | String | Foreign reference to `doc_sources.source_id` |
| `path` | String | Source-relative file path (forward-slash, e.g., `articles/intro.md`) |
| `title` | String? | Chunk heading text; `null` for document-level pre-heading content |
| `position` | Int | Ordinal position of this chunk within the document (0-based) |
| `char_offset` | Int | Character offset of this chunk's start within the raw file |
| `headings` | String | JSON array of the heading hierarchy from H1 down to this chunk's heading |
| `content` | String | Full text content of the chunk |

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
| `anchor` | String? | Fragment identifier (e.g., `#section-name`); `null` if absent |

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

### `doc_vectors`

Embedding vectors for semantic search.

```datalog
:create doc_vectors {
    chunk_id: String
    =>
    embedding: String
}
```

| Column | Type | Description |
|---|---|---|
| `chunk_id` | String (PK) | Chunk this vector corresponds to |
| `embedding` | String | JSON-serialized `Vec<f32>` (384 floats) |

> **Note:** Vectors are stored as JSON strings because CozoDB's SQLite backend
> does not yet natively support float32 arrays. Cosine similarity is computed
> in Rust after loading the vector. HNSW-accelerated vector search is planned
> for a future release when CozoDB native vector support stabilises.

---

## Sample Datalog Queries

These queries can be run via the CozoDB CLI or any client with access to
`.graphtor/graph.db`.

### Find all chunks for a source

```datalog
?[chunk_id, path, title, position]
    := *doc_chunks{ chunk_id, source_id, path, title, position },
       source_id = "azure-docs"
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
