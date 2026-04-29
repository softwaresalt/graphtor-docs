# Feature Specification: Source Registry & Acquisition

**Feature Branch**: `003-source-management`  
**Created**: 2026-03-14  
**Status**: Draft  
**Input**: User description: "Source Registry and Acquisition: sources.yaml parsing with serde_yaml, Git repository cloning via git2 crate (shallow depth=1), local directory source scanning, and include/exclude glob pattern filtering. Manages the developer's personal documentation registry."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Acquire Git Documentation Repositories (Priority: P1)

A developer has configured one or more Git repository sources in their `sources.yaml` file. They run the acquisition command and the system clones each repository using a minimal shallow fetch (depth of 1) to conserve disk space. Repositories are organized into directories named by their source ID. If a repository has already been cloned from a previous run, the system skips it rather than re-cloning, making the operation safe to repeat.

**Why this priority**: Git repositories are the primary documentation source for most developers. Without the ability to clone repos, the entire downstream pipeline (parsing, embedding, loading) has no input. This is the critical path for the system's core value proposition.

**Independent Test**: Can be fully tested by providing a `sources.yaml` with Git source entries, running the acquisition, and verifying that repositories are cloned to the expected directories with shallow depth, that re-running skips existing clones, and that failures in one source do not prevent cloning of others.

**Acceptance Scenarios**:

1. **Given** a `sources.yaml` with a Git source entry specifying a valid repository URL and branch, **When** the system acquires sources, **Then** the repository is cloned into a directory named by the source's ID under the data root, using a shallow fetch with depth of 1.
2. **Given** a Git source that was already cloned in a previous acquisition run, **When** the system acquires sources again, **Then** it detects the existing local directory and skips the clone operation for that source, logging a skip message.
3. **Given** a `sources.yaml` with three Git sources where the second source has an unreachable URL, **When** the system acquires sources, **Then** the first and third sources are cloned successfully, the second source produces an error with the URL and failure reason, and a summary reports 2 successes and 1 failure.
4. **Given** a Git source with a specified branch name, **When** the system clones the repository, **Then** only the specified branch is fetched (not all branches).

---

### User Story 2 - Index Local Documentation Directories (Priority: P1)

A developer has documentation files stored in local directories on their machine (e.g., personal notes, internal team docs, downloaded reference material). They add these directories to `sources.yaml` as local sources. The system scans each directory recursively, discovering all files that match the configured patterns. The developer sees a list of discovered files available for downstream processing.

**Why this priority**: Local directories are the second primary source type. Many developers keep personal documentation, notes, or offline copies that are not in Git repositories. Supporting local sources makes the system useful even without network access.

**Independent Test**: Can be fully tested by creating a temporary directory tree with various files, configuring it as a local source in `sources.yaml`, and verifying that the system discovers all expected files while respecting the directory structure.

**Acceptance Scenarios**:

1. **Given** a `sources.yaml` with a local source pointing to an existing directory containing markdown files at various nesting depths, **When** the system scans the source, **Then** it discovers all markdown files recursively and reports them as available for processing.
2. **Given** a local source pointing to a directory that does not exist, **When** the system attempts to scan it, **Then** it produces an error identifying the missing path and continues processing any remaining sources.
3. **Given** a local source directory containing hundreds of files, **When** the system scans it, **Then** it logs progress at INFO level (total files discovered) and per-file details at DEBUG level.

---

### User Story 3 - Filter Files Using Include/Exclude Patterns (Priority: P2)

A developer wants to process only specific documentation files from a large repository or directory. They configure include patterns (e.g., `**/*.md`) to select files and exclude patterns (e.g., `**/drafts/**`, `**/CHANGELOG.md`) to reject files they do not want indexed. The system applies these patterns to the file list discovered from each source, producing a filtered set of files ready for the ingestion pipeline.

**Why this priority**: Without filtering, the system would process every file in a repository — including changelogs, CI configs, generated docs, and other non-documentation content. Filtering is essential for practical use with large repositories, but the acquisition and scanning features deliver value even without filtering (by processing all files).

**Independent Test**: Can be fully tested by providing a known set of file paths and various include/exclude pattern combinations, then verifying the filtered output matches expectations for each combination.

**Acceptance Scenarios**:

1. **Given** a source with include pattern `**/*.md`, **When** the file list is filtered, **Then** only files with the `.md` extension are included and all other files are excluded.
2. **Given** a source with include pattern `docs/**/*.md` and exclude pattern `**/internal/**`, **When** the file list is filtered, **Then** markdown files under `docs/` are included except those under any `internal/` subdirectory.
3. **Given** a source with no include or exclude patterns configured, **When** the file list is filtered, **Then** all files are included (no filtering is applied).
4. **Given** a source with an exclude pattern that matches every file matched by the include pattern, **When** the file list is filtered, **Then** the result is an empty file set and a warning is logged indicating no files matched after filtering.
5. **Given** a source with multiple include patterns (e.g., `**/*.md`, `**/*.txt`), **When** the file list is filtered, **Then** files matching ANY include pattern are included (union), and files matching ANY exclude pattern are then removed.

---

### User Story 4 - Re-run Acquisition Safely (Idempotent Operation) (Priority: P2)

A developer runs the acquisition command multiple times — perhaps after adding a new source, or after a previous run was interrupted. The system handles repeated execution gracefully: already-cloned Git repositories are skipped, already-scanned local directories are re-scanned (since local files may have changed), and the final output reflects the current state of all configured sources. No data is duplicated or corrupted by repeated runs.

**Why this priority**: Idempotency is a core architectural principle (Constitution §V). Developers must be able to safely re-run acquisition without worrying about side effects. However, this is a property of the acquisition implementation rather than a new user-facing feature.

**Independent Test**: Can be fully tested by running acquisition twice on the same configuration and verifying that the second run produces identical results, skips existing Git clones, and does not create duplicate directories or corrupt existing data.

**Acceptance Scenarios**:

1. **Given** a successful previous acquisition run, **When** the system runs acquisition again with the same `sources.yaml`, **Then** all Git sources report "skipped (already exists)" and local sources are re-scanned, with the final file list matching the first run.
2. **Given** a previous acquisition run where one Git source failed, **When** the system runs again, **Then** it retries the failed source while skipping the already-cloned sources.
3. **Given** a `sources.yaml` that was modified to add a new source since the last run, **When** acquisition runs, **Then** only the new source is cloned/scanned while existing sources are skipped or re-scanned.

---

### User Story 5 - Validate Source Registry Before Processing (Priority: P2)

A developer creates or modifies their `sources.yaml` and wants to verify it is correct before running the full acquisition pipeline. The system validates all source definitions upfront — checking URL format for Git sources, path existence for local sources, and glob syntax for all include/exclude patterns — and reports ALL validation errors at once rather than failing on the first error. This allows the developer to fix all issues in a single edit cycle.

**Why this priority**: Upfront validation prevents wasted time on partial acquisition runs that fail midway. It is valuable but depends on the parsing (FG-001) and acquisition (stories 1-2) being in place first.

**Independent Test**: Can be fully tested by providing `sources.yaml` files with various invalid entries (bad URLs, missing paths, invalid globs) and verifying that all errors are collected and reported together.

**Acceptance Scenarios**:

1. **Given** a `sources.yaml` with a Git source containing an invalid URL (e.g., missing scheme, no host), **When** the system validates sources, **Then** it reports the specific URL validation error with the source ID and the invalid URL.
2. **Given** a `sources.yaml` with a local source pointing to a path that does not exist, **When** the system validates sources, **Then** it reports the missing path with the source ID and the non-existent path.
3. **Given** a `sources.yaml` with an invalid glob pattern (e.g., unclosed bracket `[*.md`), **When** the system validates sources, **Then** it reports the glob syntax error identifying which pattern is invalid and in which source.
4. **Given** a `sources.yaml` with three sources where two have validation errors, **When** the system validates, **Then** it reports both errors in a single validation pass (not stopping at the first error) and indicates that 1 of 3 sources is valid.
5. **Given** a fully valid `sources.yaml`, **When** the system validates sources, **Then** it confirms that all sources passed validation with a summary of source count and types.

---

### Edge Cases

- What happens when a Git source URL uses SSH authentication (`git@github.com:...`) versus HTTPS?
- How does the system handle a Git repository where the specified branch does not exist?
- What happens when a local source path is a symlink pointing outside the allowed root?
- How does the system handle a `sources.yaml` with zero sources defined?
- What happens when the disk runs out of space during a Git clone operation?
- How does the system handle extremely large repositories (tens of thousands of files) during file enumeration?
- What happens when a local directory becomes inaccessible (permissions change) between validation and scanning?
- How does the system handle glob patterns with platform-specific path separators (forward vs backslash)?
- What happens when two different sources (one Git, one local) have the same source ID?
- How does the system behave when the data root directory does not exist at acquisition time?

## Clarifications

### Session 2026-03-14

- Q: When a Git source specifies a branch that doesn't exist on the remote, what should the system do? → A: Report error with source ID and branch name, skip that source, continue processing others (consistent with fault isolation in FR-015).
- Q: When the configured data root directory doesn't exist at acquisition time, what should the system do? → A: Auto-create the data root directory and proceed with acquisition (standard practice for developer tools).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST clone Git sources using shallow fetch (depth of 1) to minimize disk usage and clone time. If a clone operation fails partway, the system MUST clean up the partial directory to avoid false skip-if-exists on retry.
- **FR-002**: System MUST organize cloned Git repositories into directories named by the source's ID under a configurable data root directory.
- **FR-003**: System MUST skip cloning a Git source if a local directory with a `.git` subdirectory already exists for that source ID (idempotent acquisition). A directory without `.git` is treated as needing a fresh clone.
- **FR-004**: System MUST fetch only the branch specified in the source configuration, not all branches.
- **FR-005**: System MUST recursively scan local source directories to discover all files.
- **FR-006**: System MUST apply include glob patterns to select files from a source's discovered file list (union of all include patterns).
- **FR-007**: System MUST apply exclude glob patterns to remove files from the included set (union of all exclude patterns).
- **FR-008**: System MUST process include patterns before exclude patterns — a file must match at least one include pattern AND not match any exclude pattern to be selected.
- **FR-009**: When no include patterns are configured for a source, the system MUST include all files by default.
- **FR-010**: When no exclude patterns are configured for a source, the system MUST not exclude any files.
- **FR-011**: System MUST validate Git source URLs for format correctness before attempting to clone — HTTPS URLs must contain a scheme and host (e.g., `https://host/path`); SSH URLs must match the `git@<host>:<path>` format.
- **FR-012**: System MUST validate local source paths for existence before attempting to scan.
- **FR-013**: System MUST validate all glob pattern syntax before attempting to apply patterns.
- **FR-014**: System MUST collect and report ALL validation errors across all sources in a single pass rather than stopping at the first error.
- **FR-015**: System MUST continue processing remaining sources when acquisition of one source fails (fault isolation).
- **FR-016**: System MUST produce a summary after acquisition reporting: total sources processed, successes, failures, files discovered, and files after filtering.
- **FR-017**: System MUST validate that all source paths (both local source paths and cloned repository paths) resolve within the allowed root directory, rejecting any path that escapes the boundary.
- **FR-018**: System MUST log acquisition progress at INFO level (source starts, completions, summaries) and per-file details at DEBUG level.
- **FR-019**: System MUST support a dry-run mode that validates sources and reports what would be acquired without actually cloning repositories or scanning directories.
- **FR-020**: When a Git source specifies a branch that does not exist on the remote repository, the system MUST report an error identifying the source ID and invalid branch name, skip that source, and continue processing remaining sources.
- **FR-021**: When the configured data root directory does not exist at acquisition time, the system MUST auto-create it before proceeding with acquisition.

### Key Entities

- **AcquisitionPlan**: A resolved plan of which sources need to be acquired, derived from the parsed `sources.yaml`. Contains the list of sources with their resolved state (needs-clone, already-exists, needs-scan).
- **AcquiredSource**: Represents a single source after acquisition — contains the source ID, source type (Git or local), the local directory path, and the list of discovered files before filtering.
- **FilteredFileSet**: The set of files from a source after include/exclude glob patterns have been applied. Contains the source ID, the original file count, the filtered file count, and the list of selected file paths.
- **AcquisitionResult**: The outcome of the full acquisition process across all sources — contains per-source results (success with file list, or failure with error), and aggregate counts.
- **ValidationReport**: A collection of all validation errors found across all sources, with per-source error details and a validity summary.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All valid Git sources in `sources.yaml` are cloned with shallow depth to their designated directories with zero manual intervention.
- **SC-002**: Re-running acquisition on an unchanged `sources.yaml` completes without re-cloning any existing Git sources and produces the same file list as the initial run.
- **SC-003**: All valid local directory sources are scanned recursively with 100% of matching files discovered.
- **SC-004**: Include/exclude glob patterns correctly filter the file list — the filtered set contains only files matching at least one include pattern and no exclude patterns.
- **SC-005**: A `sources.yaml` with multiple validation errors produces a single report listing all errors across all sources, enabling the developer to fix everything in one edit.
- **SC-006**: A failure in one source (unreachable URL, missing path) does not prevent successful acquisition of other configured sources.
- **SC-007**: Acquisition of a repository with 1,000+ files completes file enumeration and filtering within 5 seconds (excluding network clone time).
- **SC-008**: Dry-run mode reports planned actions for all sources without performing any filesystem or network operations.

## Assumptions

- The foundation layer (FG-001) provides the `SourceConfig`, `GitSource`, `LocalSource`, and error types that this feature builds upon.
- `sources.yaml` is UTF-8 encoded and has already been parsed by the FG-001 configuration parser into typed structures.
- The data root directory (where cloned repos are stored) is configurable and defaults to a `.graphtor-data/` directory relative to the workspace.
- Git sources use either HTTPS or SSH URLs; authentication is handled by the developer's existing Git credential configuration (no credential management in scope).
- Glob patterns use standard globbing syntax compatible with the `globset` crate conventions (e.g., `**/*.md`, `!` prefix is NOT used — exclusion is handled by separate exclude pattern lists).
- The system runs on the developer's local machine with filesystem access to all configured local source paths.

## Out of Scope

- **Configuration parsing**: Parsing `sources.yaml` into typed structures (covered by FG-001: Rust Foundation).
- **Error type definitions**: Core error hierarchy and types (covered by FG-001: Rust Foundation).
- **Incremental sync**: Detecting changed files and performing surgical re-ingestion (covered by FG-008: Incremental Sync).
- **Markdown parsing**: Parsing or processing the content of acquired files (covered by FG-003: Markdown Parser).
- **Git credential management**: Storing, caching, or prompting for Git credentials.
- **Repository updates**: Pulling new commits from previously cloned repositories (covered by FG-008: Incremental Sync).
- **Network proxy configuration**: Configuring HTTP/SOCKS proxies for Git clone operations.
