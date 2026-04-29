# Behavioral Scenarios: Source Registry & Acquisition

**Branch**: `003-source-management` | **Date**: 2026-03-14
**Source**: spec.md, plan.md, data-model.md

## Scenario Matrix

### Acquisition Planning

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S001 | Plan – Happy Path | Plan with one Git source needing clone | A SourceConfig with one Git source and the data root has no directory for that source ID | An acquisition plan is created | The plan contains one PlannedSource with action CloneGit and the correct target_dir | FR-001, FR-002 | P1 |
| S002 | Plan – Happy Path | Plan with one local source | A SourceConfig with one local source pointing to an existing directory | An acquisition plan is created | The plan contains one PlannedSource with action ScanLocal | FR-005 | P1 |
| S003 | Plan – Happy Path | Plan with mixed sources | A SourceConfig with 2 Git sources and 1 local source; one Git dir already exists | An acquisition plan is created | Plan has 1 CloneGit, 1 SkipGit, 1 ScanLocal; total_clone=1, total_skip=1, total_scan=1 | FR-001, FR-003, FR-005 | P1 |
| S004 | Plan – Skip | Already-cloned Git source detected | A Git source whose target directory exists and contains a `.git` subdirectory | An acquisition plan is created | The source is planned as SkipGit | FR-003 | P1 |
| S005 | Plan – Data Root | Data root auto-created | The configured data root directory does not exist | An acquisition plan is created | The data root directory is created automatically before planning proceeds | FR-021 | P1 |
| S006 | Plan – Path Security | Data root outside allowed root rejected | The data root resolves to a path outside the allowed root | An acquisition plan is created | A PathViolation error is returned | FR-017 | P1 |
| S007 | Plan – Edge | Empty source config | A SourceConfig with zero sources | An acquisition plan is created | The plan has an empty sources list with all counts at 0 | — | P2 |

### Git Source Acquisition

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S008 | Git – Happy Path | Clone a Git repository with shallow fetch | A Git source with a valid HTTPS URL and branch "main" | The source is acquired | The repository is cloned to `data_root/<source_id>/` with depth=1 and only the specified branch | FR-001, FR-002, FR-004 | P1 |
| S009 | Git – Happy Path | Clone respects specified branch | A Git source with branch "release/v2" | The source is acquired | Only the "release/v2" branch is fetched, not "main" or other branches | FR-004 | P1 |
| S010 | Git – Skip | Skip already-cloned repository | A Git source whose target directory already exists with `.git` | Acquisition is executed | The clone is skipped, a skip message is logged at INFO level, and the source reports as Skipped | FR-003, FR-018 | P1 |
| S011 | Git – Error | Unreachable URL | A Git source with URL "https://example.com/nonexistent.git" | Clone is attempted | A Pipeline error is returned with the source ID and failure reason; other sources continue | FR-015, FR-020 | P1 |
| S012 | Git – Error | Non-existent branch | A Git source with a valid URL but branch "does-not-exist" | Clone is attempted | A Pipeline error is returned identifying the source ID and invalid branch name; other sources continue | FR-020 | P1 |
| S013 | Git – Fault Isolation | One of three Git sources fails | Three Git sources where the second has an unreachable URL | All sources are acquired | Sources 1 and 3 succeed, source 2 fails, summary reports 2 successes and 1 failure | FR-015 | P1 |
| S014 | Git – Logging | Clone progress logged | A Git source is being cloned | Clone operation runs | INFO log emitted at start ("cloning source 'X' from URL") and completion ("cloned source 'X': N files") | FR-018 | P2 |
| S015 | Git – Edge | SSH URL format | A Git source with URL "git@github.com:org/repo.git" | Acquisition is attempted | The system accepts the SSH URL format and attempts the clone (success depends on local SSH config) | FR-011 | P2 |
| S016 | Git – Edge | Target directory exists but no .git | A directory named by the source ID exists but has no `.git` subdirectory | Acquisition plan is created | The source is treated as needing clone (directory state is ambiguous) | FR-003 | P2 |

### Local Source Scanning

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S017 | Local – Happy Path | Scan directory with nested markdown files | A local source pointing to a directory with .md files at depths 0, 1, 2, and 3 | The source is scanned | All .md files at all depths are discovered and returned as paths | FR-005 | P1 |
| S018 | Local – Happy Path | Scan discovers all file types | A local source pointing to a directory with .md, .txt, .yaml, and .rs files | The source is scanned | All files (regardless of extension) are discovered before filtering | FR-005 | P1 |
| S019 | Local – Ordering | Results are deterministically sorted | A local source directory with files in random filesystem order | The source is scanned twice | Both scans return files in identical sorted order | FR-005 | P1 |
| S020 | Local – Error | Non-existent directory | A local source pointing to a directory that does not exist | The source is scanned | A Pipeline error is returned with the source ID and missing path; other sources continue | FR-012, FR-015 | P1 |
| S021 | Local – Path Security | Source path outside allowed root | A local source path that resolves outside the allowed root | The source is scanned | A PathViolation error is returned | FR-017 | P1 |
| S022 | Local – Symlinks | Symlinks are not followed | A local directory containing a symlink to another directory | The source is scanned | The symlink target's files are NOT included in results; only regular files are returned | FR-005 | P2 |
| S023 | Local – Logging | Scan progress logged | A local source with 100 files | The source is scanned | INFO log: "scanning local source 'X': 100 files discovered"; DEBUG log: per-file paths | FR-018 | P2 |
| S024 | Local – Edge | Empty directory | A local source pointing to an existing but empty directory | The source is scanned | An empty file list is returned (not an error) | FR-005 | P2 |
| S025 | Local – Edge | Permission denied on subdirectory | A local directory where one subdirectory has restricted permissions | The source is scanned | Files in accessible subdirectories are discovered; an error or warning is emitted for the restricted subdirectory | FR-015, FR-018 | P3 |

### Glob Pattern Filtering

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S026 | Filter – Happy Path | Include only markdown files | Files [a.md, b.txt, c.rs], include pattern `**/*.md` | Files are filtered | Only [a.md] is returned | FR-006 | P1 |
| S027 | Filter – Happy Path | Include multiple patterns (union) | Files [a.md, b.txt, c.rs], include patterns [`**/*.md`, `**/*.txt`] | Files are filtered | [a.md, b.txt] are returned | FR-006 | P1 |
| S028 | Filter – Happy Path | Exclude removes from included set | Files [a.md, drafts/b.md, c.md], include `**/*.md`, exclude `**/drafts/**` | Files are filtered | [a.md, c.md] are returned; drafts/b.md excluded | FR-006, FR-007, FR-008 | P1 |
| S029 | Filter – Precedence | Include before exclude | A file matching both an include and an exclude pattern | Files are filtered | The file is EXCLUDED (exclude wins after include) | FR-008 | P1 |
| S030 | Filter – Default | No include patterns means all files | Files [a.md, b.txt], no include patterns, no exclude patterns | Files are filtered | All files [a.md, b.txt] are returned | FR-009, FR-010 | P1 |
| S031 | Filter – Default | No exclude patterns means nothing excluded | Files [a.md, b.md], include `**/*.md`, no exclude patterns | Files are filtered | [a.md, b.md] — all matching includes returned | FR-010 | P1 |
| S032 | Filter – Warning | All files excluded | Files [a.md, b.md], include `**/*.md`, exclude `**/*.md` | Files are filtered | Empty list returned and a WARN log is emitted | FR-006, FR-007 | P2 |
| S033 | Filter – Edge | Path-specific pattern | Files [docs/guide.md, api/ref.md], include `docs/**/*.md` | Files are filtered | Only [docs/guide.md] returned | FR-006 | P1 |
| S034 | Filter – Edge | Case sensitivity | Files [README.md, readme.md], include `**/*.md` | Files are filtered | Both files are included (glob matching is case-sensitive by default on Linux, insensitive on Windows) | FR-006 | P2 |

### Source Validation

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S035 | Validate – Happy Path | All sources valid | A SourceConfig with 2 valid Git sources and 1 valid local source | Sources are validated | ValidationReport has 0 errors, valid_count=3, total_count=3 | FR-011, FR-012, FR-013 | P1 |
| S036 | Validate – URL | Invalid HTTPS URL rejected | A Git source with URL "not-a-url" (no scheme, no host) | Source is validated | A validation error is reported for the source ID with field "url" | FR-011 | P1 |
| S037 | Validate – URL | Valid SSH URL accepted | A Git source with URL "git@github.com:org/repo.git" | Source is validated | No validation error for the URL | FR-011 | P1 |
| S038 | Validate – Path | Non-existent local path rejected | A local source with path "/nonexistent/directory" | Source is validated | A validation error is reported for the source ID with field "path" | FR-012 | P1 |
| S039 | Validate – Glob | Invalid glob syntax rejected | A source with include pattern `[invalid` | Source is validated | A validation error is reported identifying the pattern and source | FR-013 | P1 |
| S040 | Validate – Collect All | Multiple errors reported together | Three sources: first has invalid URL, second has invalid path, third is valid | Sources are validated | Report contains 2 errors (from sources 1 and 2) and valid_count=1 | FR-014 | P1 |
| S041 | Validate – Edge | Valid HTTPS URL accepted | A Git source with URL "https://github.com/org/repo.git" | Source is validated | No validation error for the URL | FR-011 | P1 |
| S042 | Validate – Path Security | Local path escapes allowed root | A local source with a path using `..` traversal to escape the allowed root | Source is validated | A validation error or PathViolation is reported | FR-017 | P1 |

### Acquisition Result & Summary

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S043 | Result – Happy Path | Full acquisition summary | 3 sources: 1 Git cloned (50 files after filter), 1 Git skipped, 1 local scanned (30 files after filter) | Acquisition completes | AcquisitionResult: succeeded=2, skipped=1, failed=0, total_files=80 | FR-016 | P1 |
| S044 | Result – Mixed | Summary with failures | 3 sources where 1 fails | Acquisition completes | AcquisitionResult: succeeded=2, skipped=0, failed=1; total_files counts only successful sources | FR-015, FR-016 | P1 |
| S045 | Result – Logging | Summary logged at INFO | Acquisition completes | Summary is logged | INFO message: "acquisition complete: N sources (M succeeded, K skipped, J failed), T files" | FR-016, FR-018 | P1 |

### Dry-Run Mode

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S046 | DryRun – Happy Path | Dry-run reports planned actions | A valid SourceConfig with 2 Git and 1 local source | Dry-run is executed | Report shows what would be cloned/scanned but no filesystem or network operations occur | FR-019 | P2 |
| S047 | DryRun – Validation | Dry-run validates first | A SourceConfig with validation errors | Dry-run is executed | Validation errors are reported; no actions are planned for invalid sources | FR-019 | P2 |

### Idempotency

| ID | Category | Scenario | Given | When | Then | FR | Priority |
|----|----------|----------|-------|------|------|----|----------|
| S048 | Idempotent – Git | Second run skips existing clones | A successful first acquisition run | Acquisition runs a second time with same config | All Git sources report Skipped; no network calls made | FR-003 | P1 |
| S049 | Idempotent – Local | Second run re-scans local dirs | A successful first acquisition run | Acquisition runs a second time with same config | Local sources are re-scanned (files may have changed); file list reflects current state | FR-005 | P1 |
| S050 | Idempotent – Added Source | New source added between runs | First run acquires 2 sources; sources.yaml updated to add a third | Second acquisition runs | Only the new source is cloned/scanned; existing sources are skipped/re-scanned | FR-003 | P2 |

## Summary

| Category | Count | P1 | P2 | P3 |
|----------|-------|----|----|----|
| Acquisition Planning | 7 | 5 | 1 | 0 |
| Git Source Acquisition | 9 | 5 | 3 | 0 |
| Local Source Scanning | 9 | 4 | 3 | 1 |
| Glob Pattern Filtering | 9 | 6 | 2 | 0 |
| Source Validation | 8 | 6 | 0 | 0 |
| Acquisition Result & Summary | 3 | 3 | 0 | 0 |
| Dry-Run Mode | 2 | 0 | 2 | 0 |
| Idempotency | 3 | 2 | 1 | 0 |
| **Total** | **50** | **31** | **12** | **1** |
