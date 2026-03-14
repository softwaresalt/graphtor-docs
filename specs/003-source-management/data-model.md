# Data Model: Source Registry & Acquisition

**Feature**: 003-source-management
**Date**: 2026-03-14

## Entities

### SourceAction (enum)

Represents the resolved action for a single source in the acquisition plan.

| Variant | Description |
|---------|-------------|
| `CloneGit` | Git source needs to be cloned (no local directory exists) |
| `SkipGit` | Git source already cloned (local directory exists with `.git`) |
| `ScanLocal` | Local source needs to be scanned |

### PlannedSource

A single source with its resolved action, used to build the acquisition plan.

| Field | Type | Description |
|-------|------|-------------|
| `source` | `Source` (from FG-001) | The original source definition |
| `action` | `SourceAction` | What to do with this source |
| `target_dir` | `PathBuf` | Resolved local directory path for this source |

### AcquisitionPlan

The full plan of what needs to happen across all sources.

| Field | Type | Description |
|-------|------|-------------|
| `data_root` | `PathBuf` | Resolved data root directory |
| `sources` | `Vec<PlannedSource>` | Ordered list of sources with actions |
| `total_clone` | `usize` | Count of sources needing clone |
| `total_skip` | `usize` | Count of sources being skipped |
| `total_scan` | `usize` | Count of local sources to scan |

### AcquiredSource

A single source after acquisition (successful).

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `String` | Source identifier (from config) |
| `source_type` | `SourceType` | Git or Local |
| `local_dir` | `PathBuf` | Directory containing acquired files |
| `discovered_files` | `Vec<PathBuf>` | All files found before filtering |

### SourceType (enum)

| Variant | Description |
|---------|-------------|
| `Git` | Cloned from a Git repository |
| `Local` | Scanned from a local directory |

### FilteredFileSet

Files from a source after glob filtering.

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `String` | Source identifier |
| `original_count` | `usize` | Files before filtering |
| `filtered_count` | `usize` | Files after filtering |
| `files` | `Vec<PathBuf>` | Selected file paths (relative to source root) |

### SourceOutcome (enum)

Per-source result after the acquisition attempt.

| Variant | Fields | Description |
|---------|--------|-------------|
| `Success` | `FilteredFileSet` | Source acquired and filtered successfully |
| `Skipped` | `source_id: String` | Git source already existed, skipped |
| `Failed` | `source_id: String, error: String` | Acquisition failed with error |

### AcquisitionResult

Aggregate result of the full acquisition process.

| Field | Type | Description |
|-------|------|-------------|
| `outcomes` | `Vec<SourceOutcome>` | Per-source results |
| `total_sources` | `usize` | Total sources attempted |
| `succeeded` | `usize` | Sources successfully acquired |
| `skipped` | `usize` | Sources skipped (already exist) |
| `failed` | `usize` | Sources that failed |
| `total_files` | `usize` | Total files after filtering across all sources |

### ValidationError

A single validation error for one source.

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `String` | Which source has the error |
| `field` | `String` | Which field failed (url, path, include, exclude) |
| `message` | `String` | Human-readable error description |

### ValidationReport

Aggregate validation results.

| Field | Type | Description |
|-------|------|-------------|
| `errors` | `Vec<ValidationError>` | All validation errors found |
| `valid_count` | `usize` | Number of sources that passed validation |
| `total_count` | `usize` | Total sources checked |

## Relationships

```text
SourceConfig (FG-001)
  └── Vec<Source>
        └── PlannedSource (action + target_dir)
              └── AcquisitionPlan
                    └── AcquiredSource (after clone/scan)
                          └── FilteredFileSet (after glob)
                                └── SourceOutcome
                                      └── AcquisitionResult (aggregate)
```

## State Transitions

```text
Source (from config)
  ├── validate() → ValidationReport
  └── plan() → PlannedSource
        ├── CloneGit → clone → AcquiredSource → filter → FilteredFileSet → Success
        ├── SkipGit → Skipped
        └── ScanLocal → scan → AcquiredSource → filter → FilteredFileSet → Success
                                                                       └── Failed (on error)
```

## Validation Rules

1. **URL format**: Git source URLs must contain `://` (HTTPS) or match `git@host:path` (SSH)
2. **Path existence**: Local source paths must exist and be directories
3. **Glob syntax**: All include/exclude patterns must compile with `globset::Glob::new()`
4. **Path security**: All resolved paths must pass `validate_path()` (FG-001)
5. **Data root**: Must be auto-created if missing, validated against allowed root
