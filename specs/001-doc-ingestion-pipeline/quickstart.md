# Quickstart: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09

## Prerequisites

1. **Python 3.11+** installed and on PATH
2. **Ollama** installed and running locally with models pulled:
   ```bash
   ollama pull nomic-embed-text
   ollama pull phi-4
   ```
3. **Git** installed and on PATH
4. Repository cloned and on the feature branch:
   ```bash
   git clone https://github.com/<org>/graphtor-docs.git
   cd graphtor-docs
   git checkout 001-doc-ingestion-pipeline
   ```

## Install Dependencies

```bash
pip install -r requirements.txt
```

## Run the Full Pipeline

```bash
python -m src.cli run --manifest paths/ms-docs-grouped.txt --groups 1,2
```

This executes:
1. **Acquire** — clones repositories from groups 1 and 2 (shallow, skip-if-exists)
2. **Normalize** — strips frontmatter and UI tags from all `.md` files
3. **Chunk** — splits documents into semantic segments by heading structure
4. **Extract** — sends chunks to local LLM for knowledge graph entity extraction
5. **Embed** — computes vector embeddings via nomic-embed-text
6. **Load** — stores embedded chunks in LanceDB and graph entities in Kùzu

## Run Individual Stages

```bash
# Acquire only
python -m src.cli acquire --manifest paths/ms-docs-grouped.txt --groups 1

# Normalize only (requires acquired docs)
python -m src.cli normalize --source <acquired-docs-dir>

# Chunk only (requires normalized docs)
python -m src.cli chunk --source <normalized-docs-dir>

# Extract only (requires chunks)
python -m src.cli extract --chunks <chunks-dir-or-file>

# Embed only (requires chunks)
python -m src.cli embed --chunks <chunks-dir-or-file>

# Load only (requires embedded chunks and extracted entities)
python -m src.cli load --embeddings <embeddings-dir> --entities <entities-dir>
```

## Verify Results

```bash
# Check LanceDB
python -m src.cli verify --db lance

# Check Kùzu
python -m src.cli verify --db kuzu

# Cross-database correlation check
python -m src.cli verify --correlation
```

## Run Tests

```bash
pytest tests/ -v
```

## Common Issues

| Issue | Solution |
|-------|----------|
| Ollama not responding | Ensure `ollama serve` is running on localhost:11434 |
| Out of disk space during acquire | Use `--groups` to limit which groups to clone |
| Extraction failures | Check `stderr` output; transient LLM errors are retried automatically |
| Duplicate records after re-run | This shouldn't happen — loading uses upsert. Check `chunk_id` generation if observed. |
