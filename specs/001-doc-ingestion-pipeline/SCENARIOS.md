# Behavioral Scenarios: Documentation Ingestion Pipeline

**Branch**: `001-doc-ingestion-pipeline` | **Date**: 2026-03-09  
**Source**: spec.md, plan.md, data-model.md

## Scenario Matrix

### Acquisition Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S001 | Acquire – Happy Path | First-time clone of a repository group | A manifest with valid repository URLs and no local copies exist | The developer runs `acquire --groups 1` | All repositories in group 1 are shallow-cloned into the group folder | FR-001, FR-002 | P1 |
| S002 | Acquire – Happy Path | Multiple groups at once | A manifest with groups 1, 2, and 3 | The developer runs `acquire --groups 1,2,3` | All three groups are cloned into separate group folders | FR-001, FR-002 | P1 |
| S003 | Acquire – Idempotent | Re-acquire with existing repos | Group 1 repos already cloned locally | The developer runs `acquire --groups 1` again | Existing repos are skipped; no re-clone occurs; output indicates skipped repos | FR-003 | P1 |
| S004 | Acquire – Partial | Some repos in group already exist | 2 of 5 repos in group 1 exist locally | The developer runs `acquire --groups 1` | Only the 3 missing repos are cloned; existing 2 are skipped | FR-003 | P1 |
| S005 | Acquire – Error | Repository URL is unavailable | A manifest URL points to a deleted/archived repo | The developer runs `acquire` | The failed clone is logged to stderr; remaining repos continue cloning | FR-020 | P2 |
| S006 | Acquire – Error | Network disconnected mid-clone | Network drops during a git clone operation | The clone fails | The partial clone directory is cleaned up; error logged; pipeline continues to next repo | FR-020 | P2 |
| S007 | Acquire – Edge | Manifest file is empty | The manifest file exists but contains no URLs | The developer runs `acquire` | Pipeline exits with a warning: "No repositories found in manifest" | FR-001 | P3 |
| S008 | Acquire – Edge | Manifest file is missing | The specified manifest path does not exist | The developer runs `acquire --manifest nonexistent.txt` | Pipeline exits with error code 2: "Manifest file not found" | FR-001 | P2 |
| S009 | Acquire – Locale Filter | Repos with multiple locales | The manifest includes repos that have locale-specific directories | Acquisition completes | Only English (en-us) content is retained; other locale dirs are excluded or ignored | FR-004 | P2 |

### Normalization Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S010 | Normalize – Happy Path | Standard Microsoft doc file | A markdown file with YAML frontmatter containing title, ms.date, description, ms.author, ms.topic | Normalization runs on the file | Only title, ms.date, and description are retained; all other frontmatter fields are stripped | FR-005 | P1 |
| S011 | Normalize – Happy Path | File with UI extensions | A markdown file containing `::: zone pivot="..."`, `[!NOTE]`, `[!TIP]`, `[!WARNING]`, `[!IMPORTANT]` blocks | Normalization runs | All UI extension markers are removed; content inside the extensions is preserved | FR-006 | P1 |
| S012 | Normalize – Happy Path | File with tab groups | A markdown file with DocFX tab group syntax (`# [Tab1](#tab/tab1)`) | Normalization runs | Tab group syntax is stripped; tab content is preserved as plain sections | FR-006 | P2 |
| S013 | Normalize – Idempotent | Double normalization | A file that has already been normalized | Normalization runs again on the same file | Output is byte-for-byte identical to the first normalization | FR-007 | P1 |
| S014 | Normalize – Edge | No frontmatter | A markdown file with no YAML frontmatter block | Normalization runs | File passes through with no frontmatter section; content is unchanged | FR-005 | P2 |
| S015 | Normalize – Edge | Empty file | A 0-byte markdown file | Normalization runs | File is skipped or produces empty output; no error thrown | FR-020 | P3 |
| S016 | Normalize – Edge | Binary file in docs dir | A .png or .pdf file exists alongside .md files | Normalization runs on the directory | Non-markdown files are skipped; only .md files are processed | FR-020 | P2 |
| S017 | Normalize – Edge | Very large file | A markdown file exceeding 1MB | Normalization runs | File is processed successfully (or skipped with warning if too large); no crash | FR-020 | P3 |
| S018 | Normalize – Error | Malformed YAML frontmatter | A file with invalid YAML (unclosed quotes, bad indentation) in frontmatter | Normalization runs | The file is processed best-effort (strip the entire frontmatter block); warning logged | FR-005, FR-020 | P2 |
| S019 | Normalize – Edge | Moniker range syntax | A file with `:::moniker range="..."` blocks | Normalization runs | Moniker markers are stripped; content inside is preserved | FR-006 | P2 |

### Chunking Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S020 | Chunk – Happy Path | Standard document with H2/H3 | A normalized file with 3 H2 sections, each with 2 H3 subsections | Chunking runs | 6 chunks are produced, each with correct parent_headers and heading_level | FR-008 | P1 |
| S021 | Chunk – Happy Path | Chunk ID stability | A normalized file chunked once, then chunked again without changes | Both runs produce chunks | chunk_id values are identical between runs for the same content | FR-009 | P1 |
| S022 | Chunk – Happy Path | Metadata propagation | A normalized file with title "Auth Guide" from source path "azure-docs/auth.md" | Chunking runs | Each chunk carries document_title="Auth Guide" and source_url="azure-docs/auth.md" | FR-010 | P1 |
| S023 | Chunk – Edge | No headings in file | A normalized file with only body text and no headings | Chunking runs | The entire file becomes a single chunk with heading_level=0 and empty parent_headers | FR-008 | P2 |
| S024 | Chunk – Edge | Only H1 heading | A normalized file with only an H1 heading and body text | Chunking runs | The entire file becomes a single chunk (H1 is the document title, not a section boundary) | FR-008 | P2 |
| S025 | Chunk – Edge | Deeply nested headings (H4+) | A file with H2 > H3 > H4 > H5 structure | Chunking runs | H4+ content is included in the parent H3 chunk; no separate chunks for H4+ | FR-008 | P2 |
| S026 | Chunk – Edge | Empty section | A file with `## Section Title` followed immediately by `## Next Section` (no content) | Chunking runs | Empty section is either skipped or produces a minimal chunk; no crash | FR-008 | P3 |
| S027 | Chunk – Edge | Content changes between runs | A file was chunked, then content under H2 "Auth" was modified | Chunking runs again | The chunk for "Auth" gets a new chunk_id (content hash changed); other chunks keep their IDs | FR-009 | P1 |
| S028 | Chunk – Edge | Special characters in headings | A heading like `## C# SDK — Getting Started (v2.0)` | Chunking runs | Chunk is created with correct parent_headers including the special characters | FR-008, FR-010 | P3 |

### Extraction Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S029 | Extract – Happy Path | Service + SDK extraction | A chunk describing "Azure Blob Storage" and its "BlobClient" class | Extraction runs | Produces a Service node ("Azure Blob Storage"), an SDK_Class node ("BlobClient"), and a CONTAINS edge between them; both linked via chunk_id | FR-011, FR-012 | P1 |
| S030 | Extract – Happy Path | Code snippet extraction | A chunk containing a Python code example for BlobClient | Extraction runs | Produces a CodeSnippet node with language="python" and a HAS_EXAMPLE edge from SDK_Class to CodeSnippet | FR-011, FR-012 | P1 |
| S031 | Extract – Happy Path | Concept extraction | A chunk discussing "Role-Based Access Control (RBAC)" | Extraction runs | Produces a Concept node with name="RBAC" and appropriate description | FR-011 | P2 |
| S032 | Extract – Error | Malformed LLM output | The LLM returns plain text instead of JSON | Extraction processes the response | The response is rejected and the chunk is retried (up to configured retry limit) | FR-013 | P1 |
| S033 | Extract – Error | LLM returns valid JSON but wrong schema | The LLM returns JSON with unexpected field names | Extraction validates against schema | The response fails Pydantic validation, chunk is retried | FR-013 | P1 |
| S034 | Extract – Error | LLM unavailable | Ollama is not running or model not pulled | Extraction attempts to call LLM | Connection error is caught; error logged; extraction stage reports failure for affected chunks | FR-020 | P1 |
| S035 | Extract – Error | Persistent extraction failure | A chunk fails extraction after all retries | Pipeline processes remaining chunks | Failed chunk is logged to stderr with chunk_id and reason; pipeline continues | FR-013, FR-020 | P2 |
| S036 | Extract – Edge | No extractable entities | A chunk containing only generic prose with no services, classes, or concepts | Extraction runs | Produces an empty nodes/edges result for that chunk; no error | FR-011 | P2 |
| S037 | Extract – Edge | Duplicate entity across chunks | Two chunks both mention "Azure Blob Storage" | Both chunks are extracted | The Service node appears once (deduplicated by name); both chunk_ids are associated | FR-011 | P2 |

### Embedding Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S038 | Embed – Happy Path | Standard embedding | A set of 100 text chunks | Embedding runs | Each chunk gets a 768-dimensional vector; output contains chunk_id + vector + text + metadata | FR-014 | P1 |
| S039 | Embed – Error | Ollama not available | Ollama is not running | Embedding is attempted | Connection error caught; stage fails with descriptive error message | FR-020 | P1 |
| S040 | Embed – Error | Model not pulled | `nomic-embed-text` model is not available in Ollama | Embedding is attempted | Model-not-found error caught; descriptive error message with fix instructions | FR-020 | P2 |
| S041 | Embed – Edge | Empty text chunk | A chunk with empty string text | Embedding runs | Empty chunk is skipped with warning; no embedding produced | FR-014 | P3 |

### Loading Stage

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S042 | Load – Happy Path | First-time LanceDB load | Embedded chunks ready; LanceDB directory does not yet exist | Loading runs | LanceDB table is created with correct PyArrow schema; all chunks inserted | FR-015 | P1 |
| S043 | Load – Happy Path | First-time Kùzu load | Extracted entities ready; Kùzu directory does not yet exist | Loading runs | Kùzu schema is created (node/rel tables); all nodes and edges inserted with chunk_id properties | FR-016 | P1 |
| S044 | Load – Idempotent | Re-load same data | Both databases already contain data from a previous run; same data loaded again | Loading runs | No duplicate records; upsert overwrites existing entries with same chunk_id/name | FR-017 | P1 |
| S045 | Load – Happy Path | Cross-DB correlation | Data loaded into both databases | A chunk_id from Kùzu node is looked up in LanceDB | The corresponding text chunk is found with matching content | FR-015, FR-016 | P1 |
| S046 | Load – Edge | Orphaned graph node | An entity extracted from a chunk whose embedding failed | Loading runs | Graph node is still loaded with chunk_id; LanceDB may not have corresponding entry; warning logged | FR-016, FR-020 | P3 |
| S047 | Load – Error | Database directory not writable | The output directory has no write permissions | Loading is attempted | Permission error caught; descriptive error logged; exit code 2 | FR-020 | P2 |
| S048 | Load – Edge | Very large batch | 50,000 chunks being loaded in a single run | Loading runs | Loading completes without memory exhaustion; batched writes if needed | FR-015 | P3 |

### Full Pipeline (End-to-End)

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S049 | E2E – Happy Path | Clean-slate full run | No prior data; Ollama running with models; manifest valid | Developer runs `python -m src.cli run --manifest paths/ms-docs-grouped.txt --groups 1` | All stages complete in order; both databases populated; exit code 0 | SC-001 | P1 |
| S050 | E2E – Idempotent | Full re-run | Pipeline has already completed successfully | Developer re-runs the same command | Repos skipped (already cloned); normalization idempotent; no duplicate DB records; exit code 0 | SC-002 | P1 |
| S051 | E2E – Progress | Progress output during run | Pipeline is executing | Developer watches stdout | Each stage reports: `[ACQUIRE] Cloning repo...`, `[NORMALIZE] Processing 450 files...`, etc. | SC-005, FR-019 | P1 |
| S052 | E2E – Partial Failure | Some files fail during pipeline | One file has malformed frontmatter; one chunk fails extraction | Pipeline runs to completion | Failures logged to stderr; pipeline exit code 1 (partial failure); all other files processed | SC-006, FR-020 | P1 |
| S053 | E2E – Scale | Multiple repository groups | Manifest with groups 1-3 (approximately 15 repos) | Full pipeline runs | Pipeline completes; both databases populated with data from all 3 groups | SC-009 | P2 |
| S054 | E2E – Correlation | Verify cross-DB links | Full pipeline has completed | Developer runs `verify --correlation` | Every chunk_id in Kùzu has a matching entry in LanceDB; report shows correlation percentage | SC-004 | P2 |
| S055 | E2E – Error | Ollama completely unavailable | Ollama is not running; developer runs full pipeline | Pipeline reaches extraction stage | Acquire, normalize, chunk stages succeed; extraction fails; pipeline exits with code 2 and clear error message about Ollama | FR-020 | P2 |
| S056 | E2E – Error | Invalid --groups flag | Developer passes `--groups abc` (non-numeric) | CLI argument parsing | Immediate exit with usage error; no stages execute | CLI contract | P3 |
| S057 | E2E – Edge | Repo restructured between runs | A repo reorganized its directory structure since last pipeline run | Full pipeline re-runs | New file paths produce new chunk_ids; old chunks remain in DB (stale); new chunks are added; no crash | FR-009, FR-017 | P3 |

## Summary

| Stage | Total Scenarios | P1 | P2 | P3 |
|-------|----------------|----|----|-----|
| Acquisition | 9 (S001–S009) | 4 | 4 | 1 |
| Normalization | 10 (S010–S019) | 3 | 5 | 2 |
| Chunking | 9 (S020–S028) | 4 | 3 | 2 |
| Extraction | 9 (S029–S037) | 4 | 3 | 2 |
| Embedding | 4 (S038–S041) | 2 | 1 | 1 |
| Loading | 7 (S042–S048) | 4 | 1 | 2 |
| End-to-End | 9 (S049–S057) | 4 | 3 | 2 |
| **Total** | **57** | **25** | **20** | **12** |
