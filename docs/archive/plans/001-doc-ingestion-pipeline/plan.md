# Implementation Plan: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-doc-ingestion-pipeline/spec.md`

## Summary

Build a local, end-to-end documentation ingestion pipeline that acquires Microsoft documentation repositories, normalizes raw markdown, chunks content by heading structure, extracts knowledge graph entities via a local LLM, computes vector embeddings, and loads all data into embedded LanceDB (vectors) and Kùzu (property graph) databases linked by shared `chunk_id` keys. The pipeline is fully scriptable, idempotent, and re-runnable as a single command sequence.

## Technical Context

**Language/Version**: Python 3.11+
**Primary Dependencies**: LanceDB, Kùzu, Ollama (Python client), PyArrow, langchain-text-splitters (or custom markdown splitter)
**Storage**: LanceDB (embedded columnar vector DB), Kùzu (embedded property graph DB)
**Testing**: pytest with fixtures for sample markdown files and mock LLM responses
**Target Platform**: Windows 10/11 developer workstation (also compatible with Linux/macOS)
**Project Type**: CLI pipeline tool
**Performance Goals**: Process a 5,000-file documentation repository in under 60 minutes end-to-end (excluding clone time)
**Constraints**: Fully local — no network calls except during repository acquisition; <16GB RAM; offline-capable after acquisition
**Scale/Scope**: 2–20 documentation repositories, ~700 repos in the master manifest with ~38 thematic groups

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Local-First Architecture | ✅ PASS | All databases embedded (LanceDB, Kùzu). LLM inference via local Ollama. No cloud dependencies. |
| II. Lightweight Footprint | ✅ PASS | Embedded DBs, shallow clones (`--depth 1`), efficient models (nomic-embed-text, phi-4/llama-3.2). |
| III. Data Pipeline Integrity | ✅ PASS | Stable `chunk_id` (content-hash based), idempotent normalization, strict JSON schema extraction, upsert loading. |
| IV. MCP-Native Interface | ⬜ N/A | MCP plugin is out of scope for this feature. Will apply to the next feature. |
| V. Automation & Reproducibility | ✅ PASS | Full pipeline executable as ordered script sequence. Idempotent skip logic for re-runs. Deterministic group organization from manifest. |

**Gate result**: PASS — no violations. Proceeding to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/001-doc-ingestion-pipeline/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (CLI interface contracts)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── __init__.py
├── cli.py                  # CLI entry point — orchestrates full pipeline
├── acquire/
│   ├── __init__.py
│   ├── manifest.py         # Parses ms-docs-grouped.txt manifest
│   └── cloner.py           # Shallow-clone logic with skip-if-exists
├── normalize/
│   ├── __init__.py
│   ├── frontmatter.py      # YAML frontmatter stripping (keep title, date, description)
│   ├── ui_tags.py          # Platform-specific UI markup removal
│   └── processor.py        # Orchestrates normalization across files
├── chunk/
│   ├── __init__.py
│   └── markdown_chunker.py # Heading-based semantic chunking with chunk_id generation
├── extract/
│   ├── __init__.py
│   ├── schema.py           # JSON schema definitions for entity/relationship extraction
│   ├── prompt.py           # LLM prompt templates for graph extraction
│   └── extractor.py        # Orchestrates LLM calls with retry logic
├── embed/
│   ├── __init__.py
│   └── embedder.py         # Vector embedding via local Ollama (nomic-embed-text)
├── load/
│   ├── __init__.py
│   ├── lance_loader.py     # LanceDB upsert loading
│   └── kuzu_loader.py      # Kùzu node/edge loading with chunk_id correlation
└── common/
    ├── __init__.py
    ├── chunk_id.py          # Deterministic chunk_id generation (content hash)
    ├── logging.py           # Structured progress/error logging
    └── config.py            # Pipeline configuration and defaults

tests/
├── conftest.py             # Shared fixtures (sample markdown, mock Ollama)
├── unit/
│   ├── test_manifest.py
│   ├── test_frontmatter.py
│   ├── test_ui_tags.py
│   ├── test_chunker.py
│   ├── test_schema.py
│   ├── test_chunk_id.py
│   └── test_extractor.py
├── integration/
│   ├── test_normalize_pipeline.py
│   ├── test_chunk_pipeline.py
│   ├── test_extract_pipeline.py
│   └── test_load_pipeline.py
└── contract/
    └── test_cli_contract.py
```

**Structure Decision**: Single project layout (Option 1) — this is a CLI pipeline tool with no frontend/backend split. All pipeline stages are organized as subpackages under `src/` with corresponding test directories.

## Complexity Tracking

> No constitution violations — this section is intentionally empty.
