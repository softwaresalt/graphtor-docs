---
title: "Pipeline SourceRecord must derive kind/url from plan.sources, not source_id"
description: "All sources registered as kind=local and url=source_id when SourceRecord was constructed without looking up the PlannedSource"
problem_type: "logic_error"
category: "best-practices"
component: "src/pipeline/mod.rs"
root_cause: "SourceRecord constructed inline with hardcoded kind='local' and url=source_id, ignoring the Source variant in plan.sources"
resolution_type: "code_fix"
severity: "critical"
message: "SourceRecord with kind=local and url=source_id registered for Git repository"
file_path: "src/pipeline/mod.rs"
citations:
  - "docs/exec-plans/2026-04-29-pipeline-orchestration-plan.md"
  - "https://github.com/softwaresalt/graphtor-docs/pull/7"
tags:
  - "pipeline"
  - "source-record"
  - "metadata"
  - "graph-db"
---

## Problem

When implementing the pipeline orchestrator, `SourceRecord` nodes were inserted into CozoDB
for every source in the plan. However, the construction used the `source_id` string as both
the `url` and hardcoded `kind="local"` for all sources — even Git repositories. This meant
Git repos would be stored as local sources with a path-like URL, corrupting graph metadata.

The symptom only surfaces at query time or during graph traversal; the pipeline itself runs
without error, making this a silent data quality bug.

## Root Cause

The `SourceRecord` was being constructed inside a match arm on `SourceOutcome::Success`
without access to the original `PlannedSource`. The code used:

```rust
SourceRecord {
    source_id: source_id.clone(),
    url: source_id.clone(),   // wrong: source_id ≠ url
    kind: "local".to_string(), // wrong: hardcoded for all sources
    name: source_id.clone(),
    synced_at: None,
}
```

The `AcquisitionPlan` carries `plan.sources: Vec<PlannedSource>` with the full
`Source::Git(GitSource { url, id, .. })` or `Source::Local(LocalSource { path, id, .. })`
variants, but these were not consulted.

## Resolution

Build a `HashMap<&str, &PlannedSource>` keyed on `ps.source.id()` before the source loop.
Introduce a `build_source_record(ps: &PlannedSource) -> SourceRecord` helper:

```rust
fn build_source_record(ps: &PlannedSource) -> SourceRecord {
    match &ps.source {
        Source::Git(git) => SourceRecord {
            source_id: git.id.clone(),
            url: git.url.clone(),
            kind: "git".to_string(),
            name: git.id.clone(),
            synced_at: None,
        },
        Source::Local(local) => SourceRecord {
            source_id: local.id.clone(),
            url: local.path.to_string_lossy().into_owned(),
            kind: "local".to_string(),
            name: local.id.clone(),
            synced_at: None,
        },
    }
}
```

Then in the `SourceOutcome::Success` arm, look up via the index and call the helper:

```rust
let record = source_index
    .get(source_id.as_str())
    .map(|ps| build_source_record(ps))
    .unwrap_or_else(|| SourceRecord { /* fallback */ });
```

## Prevention

- Any time a DB node is upserted from pipeline output, trace the data back to the
  original configuration struct (e.g., `Source::Git`, `Source::Local`) rather than
  using string IDs as proxy values.
- Include a SourceRecord metadata assertion in integration tests: verify that a Git
  source lands in the DB with `kind="git"` and the actual repo URL.
- Code review checklist: when constructing records with `kind` and `url` fields, flag
  any hardcoded string literals and ask "where does this value come from in the config?"
