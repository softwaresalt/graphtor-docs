---
title: MCP Tool Reference
description: "Complete reference for all 8 MCP tools exposed by graphtor-docs — parameters, examples, and usage patterns"
---

graphtor-docs exposes 8 tools via the [Model Context Protocol (MCP)][mcp].
The server runs as a local STDIO process and is available only to MCP clients
on the same machine (localhost only).

[mcp]: https://modelcontextprotocol.io

## Installation

### 1. Run `graphtor-docs install`

```sh
graphtor-docs install
```

This generates MCP client config files for all supported editors (`vscode`,
`cursor`, `copilot`). Use `--editor` to target specific editors only.

### 2. Configure your MCP client

Add graphtor-docs to your MCP client configuration file (e.g., `.mcp.json`):

```json
{
  "mcpServers": {
    "graphtor-docs": {
      "type": "stdio",
      "command": "graphtor-docs",
      "args": ["serve"]
    }
  }
}
```

The `graphtor-docs` binary must be on your `PATH`, or use the full path to the
binary. After `graphtor-docs install`, the binary is at
`.graphtor/bin/graphtor-docs`.

### 3. Ensure the database is synced

Run `graphtor-docs sync` at least once before starting the server. The server
opens the primary database at `.graphtor/graph.db` relative to the current
working directory by default. If any sources set `database`, `graphtor-docs
serve` also opens those routed database files and serves one MCP surface
across all loaded stores.

---

## Quick Selection Guide

| I want to… | Use this tool |
|---|---|
| Find documentation about a topic | `search_local_docs` |
| Find conceptually related content (not just keyword matches) | `search_semantic` |
| Research a topic in depth using both search and graph traversal | `research_topic` |
| Explore linked documentation from a chunk | `traverse_doc_links` |
| See which sources are indexed | `list_sources` |
| Read the full text of a chunk I already have an ID for | `get_chunk_by_id` |
| Read a complete document by path | `get_document` |
| Check if the database is healthy / has data | `get_status` |

**Typical workflow:**
1. `list_sources` — discover what is indexed
2. `search_local_docs` — find relevant chunks; note `chunk_id` values
3. `traverse_doc_links` — follow links from a chunk to related content
4. `get_chunk_by_id` or `get_document` — retrieve full content

For comprehensive topic exploration, use `research_topic` in place of steps 2–3.

---

## Tools

### `search_local_docs`

Full-text keyword search over indexed documentation chunks.

**When to use:** looking for specific terms, APIs, error messages, or any
literal text that should appear in the documentation.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | **yes** | — | Keyword or phrase to search for |
| `source_id` | string | no | (all sources) | Restrict results to a specific source ID |
| `top_k` | integer | no | `10` | Maximum number of results (max: 50) |

**Response:** Markdown-formatted list of matching chunks with source path,
heading hierarchy, and content excerpt.

**Example:**
```text
search_local_docs(
  query = "incremental sync git diff",
  source_id = "azure-docs",
  top_k = 5
)
```

---

### `search_semantic`

Semantic similarity search using embeddings.

**When to use:** looking for conceptually related content where keyword
matching may miss relevant results (e.g., searching for "how to handle
failures" would also match "error recovery strategies").

Requires the embedding model (`all-MiniLM-L6-v2`) to be loaded at server
startup. If the model was not loaded (e.g., `serve` could not download it),
this tool returns a descriptive error. Use `search_local_docs` as a fallback.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | **yes** | — | Natural-language query to embed and compare |
| `top_k` | integer | no | `10` | Maximum number of results (max: 50) |

**Response:** Markdown-formatted list of the most similar chunks, ranked by
cosine similarity. Format matches `search_local_docs` output.

**Example:**
```text
search_semantic(
  query = "how does the pipeline handle large files",
  top_k = 10
)
```

> **Note on `--no-embed` syncs:** If `graphtor-docs sync --no-embed` was used,
> no vectors are stored and `search_semantic` will return empty results even
> if the model is loaded. Re-run sync without `--no-embed` to populate vectors.

---

### `research_topic`

In-depth topic research combining keyword search and graph traversal.

**When to use:** for comprehensive topic research when you want both direct
matches and related documentation discovered through graph links. Combines
the breadth of `search_semantic` with the depth of `traverse_doc_links` in a
single call.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | **yes** | — | Keyword or natural-language topic to research |
| `top_k` | integer | no | `5` | Initial search breadth — maximum number of seed results (max: 20) |
| `max_depth` | integer | no | `1` | Graph traversal depth from each seed result (max: 3) |

**Behavior:**
1. Runs keyword (or semantic, if model is loaded) search for `query`, taking the top `top_k` results as seeds.
2. Performs BFS traversal from each of the top `min(top_k, 3)` seeds at `max_depth` (so if `top_k` is 1 or 2, fewer than 3 seeds are used).
3. Returns initial search hits (with full chunk content) plus BFS-discovered related chunks (depth, path, and chunk ID only — use `get_chunk_by_id` or `get_document` to retrieve their full content), all deduplicated globally.

**Response:** Markdown with two sections:
- `### Search Results` — initial search hits with full chunk content
- `### Related Context` — BFS-discovered related chunks as a bullet list in the format:
  `- **Depth N** — \`path\` (chunk ID: \`...\`)` (no content inline)

**Example:**
```text
research_topic(
  query = "incremental sync change detection",
  top_k = 5,
  max_depth = 2
)
```

---

### `traverse_doc_links`

BFS graph traversal following document link relationships.

**When to use:** after finding a relevant chunk with `search_local_docs`, use
this tool to discover related documentation reachable via hyperlinks.

**Parameters:**

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `chunk_id` | string | **yes** | — | SHA-256 chunk ID to start traversal from (from search results) |
| `max_depth` | integer | no | `2` | Maximum BFS traversal depth (max: 5) |

**Response:** Markdown-formatted list of reachable chunks with their paths and
traversal depth from the starting chunk.

**Example:**
```text
traverse_doc_links(
  chunk_id = "a3f7b2c9d4e1f0a8...",
  max_depth = 3
)
```

---

### `list_sources`

List all registered documentation sources.

**When to use:** discover which sources are indexed before searching; verify
that sync has run; check last-sync timestamps.

> **Note:** `synced_at` is not currently populated by the pipeline, so the
> timestamp column will show `never` for all sources. Use
> the matching `*.sync_state.json` file → `last_sync` (Unix epoch) to inspect
> per-source sync history in the meantime.

**Parameters:** none

**Response:** Markdown table with source ID, kind (`git`/`local`/`url`),
display name, and last-sync timestamp (`never` until a future release).

**Example:**
```text
list_sources()
```

---

### `get_chunk_by_id`

Retrieve a single documentation chunk by its stable SHA-256 chunk ID.

**When to use:** you already have a `chunk_id` from search results and want
the full chunk content without repeating a search query.

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `chunk_id` | string | **yes** | Exact SHA-256 hex chunk ID (from `search_local_docs` or `search_semantic` output) |

**Response:** Full chunk content including source path, heading hierarchy,
position, and text. Returns a "not found" message if the chunk ID is unknown.

**Example:**
```text
get_chunk_by_id(
  chunk_id = "a3f7b2c9d4e1f0a8b5c2d6e3f7a0b1c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0"
)
```

---

### `get_document`

Retrieve all chunks for a document path, assembled in reading order.

**When to use:** you know the document path (e.g., `articles/intro.md`) and
want to read the full document rather than individual search result snippets.

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `source_id` | string | **yes** | Source identifier to scope the lookup; pass an empty string to search across all sources |
| `path` | string | **yes** | Relative document path within the source (e.g., `"articles/intro.md"`) |

**Response:** All chunks for the document sorted by reading position,
formatted as Markdown.

**Example:**
```text
get_document(
  source_id = "azure-docs",
  path = "articles/app-service/overview.md"
)
```

---

### `get_status`

Return current database status.

**When to use:** verify the ingestion pipeline has run; check how many sources
and chunks are indexed; perform a quick health check.

**Parameters:** none

**Response:** Markdown table with registered source count, total chunk count,
and active schema version.

**Example:**
```text
get_status()
```

---

## Tool Chain Patterns

### Pattern 1: Targeted lookup

```text
list_sources()
  → identify source_id

search_local_docs(query="...", source_id="my-source")
  → pick a chunk_id from results

get_chunk_by_id(chunk_id="...")
  → full chunk content
```

### Pattern 2: Topic exploration with traversal

```text
search_local_docs(query="authentication flow")
  → find a high-relevance chunk_id

traverse_doc_links(chunk_id="...", max_depth=2)
  → related documentation network

get_document(source_id="...", path="articles/auth/overview.md")
  → read the full document
```

### Pattern 3: Conceptual search when keywords miss

```text
search_semantic(query="error recovery when network is unavailable")
  → results ranked by semantic similarity

traverse_doc_links(chunk_id="...")
  → follow links to concrete implementation docs
```
