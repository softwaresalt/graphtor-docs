# Research: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09

## Research Tasks & Findings

### R-01: Chunk ID Stability Strategy

**Decision**: Use content-based deterministic hashing (SHA-256 of normalized text + source path) to generate `chunk_id` values.

**Rationale**: Content-hash chunk IDs remain stable across pipeline re-runs as long as the source content hasn't changed. This enables idempotent upsert loading — unchanged chunks produce the same ID and are safely overwritten, while changed chunks produce new IDs. Random UUIDs would require a separate mapping table and break cross-run correlation.

**Alternatives considered**:
- Random UUID with persistent mapping file — rejected: adds state management complexity, breaks if mapping file is lost
- Sequential integer IDs — rejected: not stable across re-runs, ordering-dependent
- File path hash only — rejected: content changes wouldn't generate new IDs

### R-02: Markdown Chunking Approach

**Decision**: Use heading-based chunking (H2/H3 boundaries) with metadata propagation from parent headings.

**Rationale**: Microsoft documentation follows consistent heading hierarchy. H2 sections represent top-level concepts; H3 sections represent sub-topics. This natural structure produces semantically coherent chunks without requiring token counting or overlap windows. The langchain `MarkdownHeaderTextSplitter` or a custom equivalent handles this cleanly.

**Alternatives considered**:
- Token-based chunking with overlap — rejected: breaks mid-concept, requires tuning overlap size
- Recursive character splitting — rejected: no semantic awareness, produces incoherent chunks
- AST-based parsing — rejected: over-engineered for markdown; heading splitting is sufficient

### R-03: LanceDB Embedding & Storage Patterns

**Decision**: Use PyArrow schema with `chunk_id` as primary key. Embeddings via `nomic-embed-text` (768 dimensions) through Ollama's local API.

**Rationale**: LanceDB is columnar and uses PyArrow natively. The 768-dimension embedding from nomic-embed-text fits well within LanceDB's vector search capabilities. The `chunk_id` primary key enables upsert-on-conflict behavior for idempotent loading.

**Alternatives considered**:
- Storing embeddings in a separate FAISS index — rejected: adds a third storage system, no benefit over LanceDB native vectors
- Using OpenAI embeddings — rejected: violates local-first principle
- Using sentence-transformers directly — rejected: Ollama already manages model lifecycle; adding another runtime increases footprint

### R-04: Kùzu Graph Schema Design

**Decision**: Use the schema from the research blueprint with 4 node types (Service, SDK_Class, Concept, CodeSnippet) and 3+ relationship types (CONTAINS, REQUIRES_CONFIG, HAS_EXAMPLE), all nodes carrying `chunk_id` for cross-DB correlation.

**Rationale**: The schema captures the three main relationship patterns in Microsoft docs: service-to-SDK containment, SDK-to-service configuration dependencies, and SDK-to-code example associations. The `chunk_id` on nodes enables the "graph-guided retrieval" pattern where graph traversal yields chunk IDs that retrieve full text from LanceDB.

**Alternatives considered**:
- Richer schema with more relationship types (IMPLEMENTS, REQUIRES_PERMISSION, PART_OF from research) — deferred: start minimal, extend as extraction quality proves out
- RDF/triple store instead of property graph — rejected: Cypher queries are more intuitive; Kùzu is already in the constitution

### R-05: Graph Extraction LLM Strategy

**Decision**: Use `phi-4` (primary) or `llama-3.2` (fallback) via Ollama with a strict JSON schema prompt. Enforce structured output via JSON mode and validate against a Pydantic schema before loading.

**Rationale**: Both models are small-footprint and sufficient for structured extraction tasks. JSON mode in Ollama constrains output format, reducing parsing failures. Pydantic validation catches any remaining schema violations before they reach the database.

**Alternatives considered**:
- Using a larger model (llama-3.1-70B) — rejected: violates lightweight footprint principle, excessive for structured extraction
- Regex-based extraction without LLM — rejected: too brittle for varied documentation formats
- Few-shot prompting without JSON mode — rejected: higher failure rate on structured output

### R-06: Normalization Rules for Microsoft Docs

**Decision**: Three normalization passes: (1) strip YAML frontmatter keeping only `title`, `ms.date`, `description`; (2) remove Microsoft-specific UI extensions (`::: zone`, `[!NOTE]`, `[!TIP]`, `[!WARNING]`, `[!IMPORTANT]`, `[!CAUTION]`, tab groups, moniker ranges); (3) filter to `en-us` locale during acquisition.

**Rationale**: Microsoft docs use a custom Markdown superset called DocFX. The extensions are rendering directives that add noise to embeddings and graph extraction. Keeping title/date/description preserves essential metadata. Locale filtering at acquisition time (via manifest structure) prevents duplicate content.

**Alternatives considered**:
- Keep all frontmatter — rejected: most fields (ms.author, ms.topic, ms.service) are platform metadata, not content
- Convert extensions to plain markdown equivalents — rejected: complexity not justified; the content inside the extensions is preserved, only the wrapper syntax is stripped
- Normalize after chunking — rejected: chunks with UI tags produce inconsistent embeddings

### R-07: Pipeline Orchestration Pattern

**Decision**: Simple sequential function calls orchestrated by a CLI entry point (`cli.py`). Each stage is a standalone function that reads from the filesystem and writes results. No workflow engine.

**Rationale**: The pipeline has a strict linear dependency chain (acquire → normalize → chunk → extract+embed → load). A simple `main()` function calling each stage in sequence, with progress logging between stages, is sufficient. Workflow engines add complexity without benefit for a single-machine batch pipeline.

**Alternatives considered**:
- Apache Airflow/Prefect — rejected: server-based, violates local-first; massive footprint
- Make/Taskfile — rejected: adds build tool dependency for a Python project
- Python asyncio pipeline — rejected: I/O is disk-bound (DB writes) and LLM-bound (serial Ollama calls); async adds complexity without parallelism benefit

### R-08: Existing Codebase Integration

**Decision**: Leverage existing acquisition infrastructure (`.scripts/clone_ms_docs_repos.py`, `.scripts/generate_clone_scripts.py`, `paths/ms-docs-grouped.txt`, `.scripts/clone-groups/*.bat`) as the acquisition layer. New pipeline code extends from normalization onward.

**Rationale**: The project already has a working system for discovering, grouping, and cloning Microsoft documentation repositories. The clone scripts use `--depth 1` and skip-if-exists patterns that match the spec's requirements. Building on this avoids reimplementing acquisition.

**Alternatives considered**:
- Rewrite acquisition in Python — deferred: existing batch scripts work, can be unified later
- Replace batch scripts with Python subprocess calls — considered for the CLI orchestrator; the CLI can invoke existing scripts or wrap git clone directly
