---
title: "Source Registry Normalization and Duplicate-Intake Preflight"
type: deliberation
status: decided
stash_id: 4BEEF41A
depth: standard
promote_to: plan
created: 2026-05-23
---

# Source Registry Normalization and Duplicate-Intake Preflight

## Problem Frame

Graphtor-docs currently uses a single `sources.yaml` for all documentation source
definitions. As the operator routes different content domains to different databases,
the single file becomes unwieldy and risks duplicate file paths being ingested into
multiple databases — wasting storage and producing confusing search results.

The operator wants:
1. Multiple source files following a `*.sources.yaml` naming convention
2. Consistent schema across all source files (including explicit `database` field)
3. A preflight check that detects duplicate file/path intake across databases
4. Operator alerting before ingestion to allow override or adjustment

### Who cares

The operator managing multiple documentation domains with dedicated databases.

### Constraints

* Must be backward-compatible with existing single `sources.yaml`
* Must maintain the existing config validation (duplicate IDs, glob patterns, etc.)
* Schema must require `database` field in multi-file mode (can default to `graph.db`)
* Preflight check must run before any ingestion begins
* Must work with the existing CLI sync flow

### Success criteria

* `graph.sources.yaml`, `powerbi.sources.yaml`, etc. are each independently valid
* `graphtor-docs sync` discovers and parses all `*.sources.yaml` files
* Duplicate path detection works across all source files/databases
* Operator receives a clear warning listing duplicates with affected databases
* Operator can override (proceed anyway) or abort to fix

### Scope boundaries

* OUT: Auto-merging or deduplicating content already ingested
* OUT: Changing the database routing mechanism itself
* IN: Multi-file discovery and loading in `src/config/`
* IN: Cross-file validation for duplicate paths
* IN: CLI preflight reporting and override prompt
* IN: Schema enforcement (database field required in multi-file mode)

## Research Findings

### Current architecture

* `src/config/source.rs`: `SourceConfig` with `pub sources: Vec<Source>`
* `src/config/validation.rs`: validates duplicate source IDs, formats, globs, database names
* `src/config/mod.rs`: loads from a single path (`.graphtor/config/sources.yaml`)
* Each `Source` variant (Git, Local, Url) has an optional `database` field
* The `database` field defaults to `None` (uses `graph.db`)

### File pattern conventions

The operator proposes `*.sources.yaml` (e.g. `graph.sources.yaml`,
`powerbi.sources.yaml`). This is clean, discoverable, and avoids collision with
other YAML config files in `.graphtor/config/`.

### Validation gaps

* No cross-file duplicate path detection exists
* No enforcement that `database` is explicitly declared
* No preflight check before sync begins

## Options

### Option A: Multi-File Discovery with Unified Validation

Extend `SourceConfig` loading to glob `*.sources.yaml` from the config directory.
Merge all sources into a unified `Vec<Source>` and run validation (including new
cross-file duplicate detection) on the merged set. Require `database` field when
multiple source files are present.

* **Pros**: Clean separation; each domain gets its own file; unified validation catches cross-file conflicts; backward-compatible (single `sources.yaml` still works)
* **Cons**: Need to handle load-order determinism; slightly more complex config loading
* **Effort**: Medium
* **Fit**: High — directly addresses all requirements

### Option B: Source File Includes (YAML Anchors/References)

Keep a single `sources.yaml` but support `!include` directives that pull in
domain-specific sub-files.

* **Pros**: Single entry point; familiar YAML pattern
* **Cons**: Non-standard YAML; requires custom deserializer; harder to validate per-file; doesn't naturally enforce per-domain schema
* **Effort**: Medium-High
* **Fit**: Low — adds complexity without the clean domain separation the operator wants

### Option C: Directory-Based Source Registry

Each source gets its own YAML file in a `sources.d/` directory.

* **Pros**: Maximum granularity; easy to add/remove individual sources
* **Cons**: Too granular for the operator's mental model (they think in domain groups); harder to see which sources share a database; no natural grouping
* **Effort**: Medium
* **Fit**: Low — operator explicitly wants domain-grouped files

## Decision

**Chosen: Option A — Multi-File Discovery with Unified Validation**

Rationale:
1. Directly matches the operator's mental model (one file per domain/database)
2. Backward-compatible: single `sources.yaml` continues to work as before
3. Unified validation catches cross-file conflicts in one pass
4. The `*.sources.yaml` glob is simple and deterministic (alphabetical load order)
5. Schema enforcement (require `database` field) is straightforward in multi-file mode

### Rejected alternatives

* **Option B**: Non-standard YAML extensions add fragility without benefit
* **Option C**: Too granular; doesn't match the operator's domain-grouped model

### Risks and mitigations

| Risk | Mitigation |
|---|---|
| Breaking change for existing `sources.yaml` | Support both: lone `sources.yaml` works; `*.sources.yaml` pattern is additive |
| Load-order affecting behavior | Alphabetical sort; document that source IDs must be globally unique |
| Performance of cross-file path comparison | O(n²) on source paths is fine for typical counts (<100 sources) |

### Unresolved questions

* Should `sources.yaml` (without the naming pattern) be deprecated in favor of
  `graph.sources.yaml`? Recommend: support both, document migration path.
* Should the duplicate-intake alert block sync entirely or just warn? Recommend:
  block by default, `--force` flag to override.
