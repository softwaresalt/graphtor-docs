# Feature Specification: Documentation Ingestion Pipeline

**Feature Branch**: `001-doc-ingestion-pipeline`  
**Created**: 2026-03-09  
**Status**: Draft  
**Input**: User description: "Local documentation ingestion pipeline for acquiring, normalizing, chunking, extracting graph entities, and loading Microsoft documentation into embedded LanceDB and Kùzu databases"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Acquire Documentation Repositories (Priority: P1)

A developer wants to populate the local knowledge base with Microsoft documentation. They run a single command that downloads targeted documentation repositories to their local machine, organized by thematic group, without downloading unnecessary history or duplicate localized content.

**Why this priority**: Without local documentation content, no other pipeline stage can operate. Acquisition is the foundation of the entire system.

**Independent Test**: Can be fully tested by running the acquisition command against a target repository list and verifying that documentation files arrive locally in the expected group-based directory structure.

**Acceptance Scenarios**:

1. **Given** a manifest of target documentation repositories and a local storage directory, **When** the developer runs the acquisition command for the first time, **Then** all listed repositories are downloaded with only the latest snapshot (no full history) and organized into their assigned thematic groups.
2. **Given** some repositories have already been acquired locally, **When** the developer re-runs the acquisition command, **Then** already-present repositories are skipped and only missing repositories are downloaded.
3. **Given** a repository contains content in multiple languages/locales, **When** the acquisition completes, **Then** only English (`en-us`) content is retained and other locales are excluded.

---

### User Story 2 - Normalize Raw Documentation (Priority: P1)

A developer has acquired raw documentation files that contain platform-specific metadata and formatting extensions not useful for knowledge retrieval. They run the normalization step to produce clean, standardized markdown files that preserve only the meaningful content and essential metadata.

**Why this priority**: Raw documentation contains noise (frontmatter, UI-specific tags, platform extensions) that degrades chunking quality and wastes storage. Normalization is required before any downstream processing.

**Independent Test**: Can be fully tested by running normalization against a directory of raw markdown files and verifying the output files contain only clean markdown with preserved essential metadata (title, date, description).

**Acceptance Scenarios**:

1. **Given** a directory of raw markdown documentation files with platform-specific YAML frontmatter, **When** normalization runs, **Then** all frontmatter is removed except for title, date, and description fields.
2. **Given** markdown files containing platform-specific UI extensions (zone markers, special note blocks), **When** normalization runs, **Then** those extensions are stripped while preserving the surrounding content structure.
3. **Given** the same set of files is normalized twice, **When** comparing the output of both runs, **Then** the results are identical (idempotent processing).

---

### User Story 3 - Chunk Documents into Retrievable Segments (Priority: P2)

A developer has normalized documentation and needs it broken into self-contained, semantically meaningful segments suitable for embedding and retrieval. They run the chunking step, which produces segments based on document structure (headings), each tagged with its provenance.

**Why this priority**: Chunking determines retrieval granularity. Too large and results are unfocused; too small and context is lost. This step directly impacts the quality of every downstream search.

**Independent Test**: Can be fully tested by running the chunker against normalized markdown files and verifying that each output segment corresponds to a logical section, carries a stable unique identifier, and includes source metadata.

**Acceptance Scenarios**:

1. **Given** a normalized markdown file with multiple heading levels, **When** the chunker processes it, **Then** each top-level section (H2) and sub-section (H3) becomes a separate chunk representing a self-contained concept.
2. **Given** any chunk produced by the system, **When** inspecting its metadata, **Then** it contains a stable unique identifier (`chunk_id`), the source document title, the parent heading hierarchy, and the source URL or file path.
3. **Given** a markdown file with no headings, **When** the chunker processes it, **Then** the entire file is treated as a single chunk with appropriate metadata.

---

### User Story 4 - Extract Knowledge Graph Entities (Priority: P2)

A developer has documentation chunks and wants to build a knowledge graph capturing the relationships between services, classes, and concepts mentioned in the documentation. They run the extraction step, which sends each chunk to a local language model and produces structured entities and relationships.

**Why this priority**: The knowledge graph enables architectural exploration queries (e.g., "which SDK classes belong to Azure Blob Storage?") that pure text search cannot answer. It is the differentiating capability over basic RAG.

**Independent Test**: Can be fully tested by running extraction against a set of documentation chunks and verifying that structured entities (services, classes, concepts) and relationships are produced with valid identifiers linking back to source chunks.

**Acceptance Scenarios**:

1. **Given** a documentation chunk describing a cloud service and its SDK classes, **When** the extraction runs, **Then** it produces structured entity records for the service and each SDK class, plus relationship records connecting them.
2. **Given** the extraction model returns malformed or unstructured output for a chunk, **When** the extraction pipeline encounters this, **Then** the chunk is retried, and if it fails again, it is logged as a failure without halting the overall pipeline.
3. **Given** an extracted entity, **When** inspecting its data, **Then** it includes the `chunk_id` of the source chunk so it can be correlated back to the original text.

---

### User Story 5 - Load Data into Embedded Databases (Priority: P3)

A developer has processed chunks (with embeddings) and extracted graph entities. They run the loading step, which populates both the vector store and the graph store, linked by their shared chunk identifiers. After loading, the databases are ready for querying.

**Why this priority**: Loading is the final pipeline stage that makes data queryable. It depends on all prior stages but is essential for the system to deliver value.

**Independent Test**: Can be fully tested by running the loader against a set of processed chunks and extracted entities, then querying both databases to verify data presence, correct schema, and cross-database correlation via chunk identifiers.

**Acceptance Scenarios**:

1. **Given** a set of document chunks with computed vector embeddings, **When** the loader runs, **Then** each chunk is stored in the vector database with its text, embedding, metadata, and `chunk_id`.
2. **Given** a set of extracted graph entities and relationships, **When** the loader runs, **Then** each entity is inserted as a node and each relationship as an edge in the graph database, with `chunk_id` properties linking nodes back to the vector store.
3. **Given** the loader is run twice with the same input data, **When** comparing the database state after each run, **Then** no duplicate records exist (upsert behavior).
4. **Given** both databases are loaded, **When** a `chunk_id` is looked up in the graph database, **Then** the same `chunk_id` can retrieve the corresponding text from the vector database.

---

### User Story 6 - Run the Full Pipeline End-to-End (Priority: P3)

A developer wants to go from zero to a fully populated knowledge base with a single command or ordered script sequence. They invoke the full pipeline, which executes acquisition, normalization, chunking, extraction, and loading in the correct order, reporting progress throughout.

**Why this priority**: While each stage is independently valuable and testable, the end-to-end pipeline provides the production workflow. It depends on all individual stages being functional.

**Independent Test**: Can be fully tested by running the pipeline command against the target repository manifest on a clean system and verifying that both databases are populated and queryable after completion.

**Acceptance Scenarios**:

1. **Given** a clean local environment with no prior data, **When** the developer runs the full pipeline, **Then** all stages execute in order (acquire → normalize → chunk → extract → load) and both databases are populated.
2. **Given** the pipeline is running, **When** each stage completes, **Then** progress information is emitted to standard output including stage name, items processed, and any warnings.
3. **Given** a stage encounters a non-fatal error (e.g., one file fails normalization), **When** the pipeline continues, **Then** the error is logged to standard error and processing continues with remaining items.
4. **Given** the pipeline has already been run successfully, **When** the developer re-runs it, **Then** already-acquired repositories are skipped, and the pipeline completes without duplicating data.

---

### Edge Cases

- What happens when a target repository is unavailable or has been archived?
- How does the system handle a markdown file that is excessively large (>1MB of content)?
- What happens when the local language model is not available or not responding during extraction?
- How does the system handle corrupted or binary files in the documentation repository?
- What happens when disk space runs out during acquisition or database loading?
- How does the system handle markdown files with non-standard or deeply nested heading structures (H4+)?
- What happens when a documentation repository reorganizes its directory structure between pipeline runs?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST acquire documentation from a configurable manifest of target repositories using shallow downloads (latest snapshot only).
- **FR-002**: System MUST organize acquired documentation into thematic groups as defined by the source manifest.
- **FR-003**: System MUST skip acquisition of repositories that already exist locally.
- **FR-004**: System MUST filter out non-English localized content during or immediately after acquisition.
- **FR-005**: System MUST strip platform-specific YAML frontmatter from documentation files, retaining only title, date, and description metadata.
- **FR-006**: System MUST remove platform-specific UI markup extensions (zone markers, special note syntax) from documentation files.
- **FR-007**: System MUST produce identical output when normalization is run multiple times on the same input (idempotent).
- **FR-008**: System MUST chunk normalized documents based on markdown heading structure (H2, H3 boundaries).
- **FR-009**: System MUST assign a stable unique identifier (`chunk_id`) to each chunk that persists across pipeline re-runs for the same content.
- **FR-010**: System MUST attach provenance metadata to each chunk: source document title, parent heading hierarchy, and source file path or URL.
- **FR-011**: System MUST extract structured knowledge graph entities (services, SDK classes, concepts, code snippets) from each chunk using a local language model.
- **FR-012**: System MUST extract structured relationships between entities (containment, configuration dependencies, example associations) from each chunk.
- **FR-013**: System MUST enforce structured output from the extraction model and reject malformed responses with retry logic.
- **FR-014**: System MUST compute vector embeddings for each chunk using a local embedding model.
- **FR-015**: System MUST store chunks with their text, embeddings, metadata, and `chunk_id` in the vector database.
- **FR-016**: System MUST store extracted entities as nodes and relationships as edges in the graph database, with `chunk_id` properties for cross-database correlation.
- **FR-017**: System MUST support upsert behavior during loading to prevent duplicate records on re-runs.
- **FR-018**: System MUST provide a way to execute the full pipeline (acquire → normalize → chunk → extract → embed → load) as a single command or ordered sequence.
- **FR-019**: System MUST emit structured progress information to standard output and errors to standard error during pipeline execution.
- **FR-020**: System MUST continue processing remaining items when individual items fail, logging failures without halting the pipeline.

### Key Entities

- **Documentation Repository**: A source of markdown documentation files, identified by URL, assigned to a thematic group, and tracked for acquisition status.
- **Document**: A single markdown file from a repository, carrying provenance metadata (title, date, description, source path).
- **Chunk**: A self-contained segment of a document, bounded by heading structure, carrying a stable `chunk_id`, text content, provenance metadata, and a computed vector embedding.
- **Graph Node**: A structured entity extracted from a chunk — representing a Service, SDK Class, Concept, or Code Snippet — linked to its source chunk via `chunk_id`.
- **Graph Edge**: A directed relationship between two graph nodes, typed by relationship kind (containment, configuration dependency, example association).
- **Repository Manifest**: The configuration that defines which documentation repositories to target, their URLs, and their thematic group assignments.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The full pipeline completes successfully on a clean system with at least 2 target documentation repositories, populating both databases with queryable data.
- **SC-002**: Re-running the pipeline on an already-processed dataset completes without creating duplicate records in either database.
- **SC-003**: 95% of documentation files in targeted repositories are successfully processed through all pipeline stages (acquire, normalize, chunk, extract, load).
- **SC-004**: Every chunk stored in the vector database can be correlated to at least one node in the graph database via `chunk_id`, and vice versa.
- **SC-005**: The pipeline provides clear progress output that enables a developer to identify which stage is executing and how many items have been processed.
- **SC-006**: Individual file failures do not halt the pipeline; the developer can identify failed items from error output after the run completes.
- **SC-007**: Normalization produces identical output when run twice on the same input, verifiable by file comparison.
- **SC-008**: The extraction step rejects and retries malformed model output, with at least one successful retry on transient model errors.
- **SC-009**: The system supports processing up to 20 documentation repositories on a single developer workstation without exceeding reasonable resource limits.

## Clarifications

### Session 2026-03-09

- Q: What is explicitly out of scope for this feature? → A: The MCP plugin server (query interface), web scraping, non-Microsoft documentation sources, and real-time/incremental sync are all out of scope.
- Q: How many documentation repositories should the system support? → A: The system should support 2–20 repositories on a single developer workstation.
- Q: Should the pipeline support mid-run resume from the last successful stage? → A: No — the pipeline is idempotent and re-runnable from the start. Mid-pipeline checkpointing is deferred to a future enhancement.

## Out of Scope

- **MCP Plugin Server**: The query interface that exposes databases to AI agents (Phase 5 of the research blueprint). This will be a separate feature.
- **Web Scraping**: Documentation is acquired only via repository cloning, not web scraping.
- **Non-Microsoft Sources**: Only Microsoft documentation repositories are supported. Third-party or community documentation is excluded.
- **Real-Time / Incremental Sync**: The pipeline is a batch process. Live watching for documentation changes or incremental delta processing is not included.
- **Mid-Pipeline Resume**: The pipeline does not checkpoint between stages. Re-running restarts from the beginning, relying on idempotent skip logic to avoid redundant work.

## Assumptions

- The developer has a working local model runtime (Ollama or equivalent) installed with appropriate embedding and extraction models pulled before running the pipeline.
- Target documentation repositories are publicly accessible and can be downloaded without authentication.
- The developer's workstation has sufficient disk space for the targeted documentation repositories and the resulting databases (estimated 5-10 GB per major repository).
- The source manifest (`paths/ms-docs-grouped.txt`) exists and defines the target repositories and their group assignments.
- English (`en-us`) is the only locale required; all other locales can be safely discarded.
