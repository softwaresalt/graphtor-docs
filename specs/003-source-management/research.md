# Research: Source Registry & Acquisition

**Feature**: 003-source-management
**Date**: 2026-03-14

## R1: Git Cloning Strategy with `git2`

**Decision**: Use `git2::Repository::clone` with `FetchOptions` configured for shallow depth=1 and single-branch fetch.

**Rationale**: The `git2` crate is the Constitution-mandated Git library (no shell-out to system git). Shallow clones minimize disk usage. Single-branch fetch avoids downloading unnecessary refs.

**Alternatives considered**:
- `std::process::Command` calling system `git` — rejected: violates Constitution tech stack, requires system git installation
- `gix` (gitoxide pure-Rust) — rejected: less mature than `git2`, more complex API for basic cloning

**Key implementation notes**:
- Use `RepoBuilder` with `FetchOptions` to set depth and branch
- `git2` handles SSH and HTTPS via system credential helpers (`CredentialType::SshKey`, `CredentialType::UserPassPlaintext`)
- For "skip-if-exists" check: verify target directory exists AND contains a `.git` directory
- Non-existent branch: `git2` will return an error from `fetch` — catch and convert to `GraphtorError::Pipeline`

## R2: Recursive Directory Scanning

**Decision**: Use the `walkdir` crate for recursive file enumeration.

**Rationale**: `walkdir` provides a single, well-tested iterator for recursive directory traversal with cross-platform support, symlink handling, and depth control. The stdlib `std::fs::read_dir` requires manual recursion and error handling.

**Alternatives considered**:
- Manual recursion with `std::fs::read_dir` — rejected: more error-prone, no built-in symlink cycle detection
- `ignore` crate (ripgrep's file walker) — rejected: heavier than needed, brings .gitignore semantics we don't want

**Key implementation notes**:
- Use `WalkDir::new(path).follow_links(false)` to avoid symlink cycles
- Collect only regular files (skip directories and symlinks)
- Sort results for deterministic ordering (Constitution §III)

## R3: Glob Pattern Filtering Strategy

**Decision**: Use the existing `globset` dependency to compile include/exclude patterns into `GlobSet` matchers.

**Rationale**: `globset` is already in `Cargo.toml` (used by `config/validation.rs`). It compiles multiple patterns into an optimized automaton for efficient matching.

**Alternatives considered**:
- `glob` crate — rejected: single-pattern matching, no set optimization
- Manual pattern matching — rejected: error-prone, reimplements existing functionality

**Key implementation notes**:
- Compile include patterns into one `GlobSet`, exclude into another
- Match against relative paths (relative to the source root, not the data root)
- Empty include set → match all files (FR-009)
- Empty exclude set → exclude nothing (FR-010)
- Include first, then exclude (FR-008)

## R4: Error Handling for Acquisition

**Decision**: Add a new `GraphtorError::Acquire` variant for acquisition-specific errors, or use the existing `Pipeline` variant with stage="acquire".

**Rationale**: The existing `Pipeline` variant with `stage: "acquire"` fits naturally. Adding a new variant would be cleaner for matching but increases enum size. Since acquisition errors always have a source ID context, a dedicated variant with `source_id` field is preferable.

**Final decision**: Use existing `Pipeline` variant for now. The `stage` field conveys the acquisition context. Source ID can be included in the `message` field. If downstream consumers need to match specifically on acquisition errors, a dedicated variant can be added later.

**Key implementation notes**:
- `git2::Error` → `GraphtorError::Pipeline { message: ..., stage: "acquire" }`
- `walkdir::Error` → `GraphtorError::Pipeline { message: ..., stage: "acquire" }`
- `std::io::Error` → `GraphtorError::Io` (already covered)
- All errors include the source ID in the message string

## R5: Data Root Directory Management

**Decision**: Auto-create the data root directory if it does not exist (clarification from Stage 2).

**Rationale**: Standard practice for developer tools. Failing on a missing directory creates unnecessary friction.

**Key implementation notes**:
- Use `std::fs::create_dir_all(data_root)` at the start of acquisition
- Validate the created path against the allowed root (path security)
- Default data root: `.graphtor-data/` relative to the workspace (configurable)

## R6: New Dependencies Justification

| Crate | Version | Purpose | Constitution Justification |
|-------|---------|---------|---------------------------|
| `git2` | latest | Git cloning operations | Explicitly listed in Constitution tech stack table |
| `walkdir` | latest | Recursive directory traversal | ~500 LOC, no transitive deps beyond `same-file`. stdlib alternative requires manual recursion. |

Both dependencies are well-established, actively maintained, and align with the Lightweight Footprint principle.
