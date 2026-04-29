# CLI Interface Contract: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09

## Entry Point

```
python -m src.cli <command> [options]
```

## Commands

### `run` — Execute Full Pipeline

```
python -m src.cli run --manifest <path> [--groups <list>] [--output-dir <path>] [--ollama-url <url>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--manifest` | path | Yes | — | Path to repository manifest file (e.g., `paths/ms-docs-grouped.txt`) |
| `--groups` | comma-separated ints | No | all | Which groups to process (e.g., `1,2,5`) |
| `--output-dir` | path | No | `./data` | Base directory for acquired repos, normalized files, and databases |
| `--ollama-url` | URL | No | `http://localhost:11434` | Ollama API endpoint |
| `--embed-model` | string | No | `nomic-embed-text` | Embedding model name |
| `--extract-model` | string | No | `phi-4` | Extraction LLM model name |
| `--verbose` | flag | No | false | Enable detailed progress output |

**Exit codes**:
- `0` — all stages completed successfully
- `1` — one or more stages had partial failures (some files failed, pipeline continued)
- `2` — a stage failed completely (could not proceed)

**Stdout**: Structured progress messages: `[STAGE] message`
**Stderr**: Error details for failed items

---

### `acquire` — Clone Repositories

```
python -m src.cli acquire --manifest <path> [--groups <list>] [--output-dir <path>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--manifest` | path | Yes | — | Path to repository manifest file |
| `--groups` | comma-separated ints | No | all | Which groups to clone |
| `--output-dir` | path | No | `./data/repos` | Where to clone repositories |

---

### `normalize` — Strip Frontmatter & UI Tags

```
python -m src.cli normalize --source <path> [--output-dir <path>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--source` | path | Yes | — | Directory of acquired repositories |
| `--output-dir` | path | No | `./data/normalized` | Where to write normalized files |

---

### `chunk` — Split Documents into Segments

```
python -m src.cli chunk --source <path> [--output-dir <path>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--source` | path | Yes | — | Directory of normalized markdown files |
| `--output-dir` | path | No | `./data/chunks` | Where to write chunk JSON files |

**Output format**: One JSON file per document, containing an array of chunk objects:
```json
[
  {
    "chunk_id": "a1b2c3...",
    "text": "## Authentication\n\nAzure AD supports...",
    "document_title": "Authentication Overview",
    "source_url": "azure-docs/articles/active-directory/auth-overview.md",
    "parent_headers": ["## Authentication"],
    "heading_level": 2
  }
]
```

---

### `extract` — Extract Graph Entities

```
python -m src.cli extract --chunks <path> [--output-dir <path>] [--ollama-url <url>] [--model <name>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--chunks` | path | Yes | — | Directory of chunk JSON files |
| `--output-dir` | path | No | `./data/entities` | Where to write extracted entity JSON files |
| `--ollama-url` | URL | No | `http://localhost:11434` | Ollama API endpoint |
| `--model` | string | No | `phi-4` | LLM model for extraction |

**Output format**: One JSON file per chunk file, containing nodes and edges:
```json
{
  "nodes": [
    {"name": "Azure AD", "node_type": "Service", "description": "...", "chunk_id": "a1b2c3..."}
  ],
  "edges": [
    {"source_name": "Azure AD", "source_type": "Service", "target_name": "MsalClient", "target_type": "SDK_Class", "relationship": "CONTAINS"}
  ]
}
```

---

### `embed` — Compute Vector Embeddings

```
python -m src.cli embed --chunks <path> [--output-dir <path>] [--ollama-url <url>] [--model <name>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--chunks` | path | Yes | — | Directory of chunk JSON files |
| `--output-dir` | path | No | `./data/embeddings` | Where to write embedding files |
| `--ollama-url` | URL | No | `http://localhost:11434` | Ollama API endpoint |
| `--model` | string | No | `nomic-embed-text` | Embedding model name |

---

### `load` — Load into Databases

```
python -m src.cli load --embeddings <path> --entities <path> [--lance-dir <path>] [--kuzu-dir <path>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--embeddings` | path | Yes | — | Directory of embedding files |
| `--entities` | path | Yes | — | Directory of extracted entity files |
| `--lance-dir` | path | No | `./data/lancedb` | LanceDB storage directory |
| `--kuzu-dir` | path | No | `./data/kuzudb` | Kùzu storage directory |

---

### `verify` — Verify Database State

```
python -m src.cli verify [--db <name>] [--correlation] [--lance-dir <path>] [--kuzu-dir <path>]
```

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `--db` | enum(lance,kuzu) | No | both | Which database to check |
| `--correlation` | flag | No | false | Check chunk_id cross-database correlation |
| `--lance-dir` | path | No | `./data/lancedb` | LanceDB directory |
| `--kuzu-dir` | path | No | `./data/kuzudb` | Kùzu directory |
