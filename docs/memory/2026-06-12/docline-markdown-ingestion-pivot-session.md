---
title: "Session memory — docline markdown ingestion pivot"
description: "Checkpoint after pivoting graphtor-docs toward docline-standardized Markdown ingestion on feature branch 042."
date: "2026-06-12"
branch: "feat/042-docline-markdown-ingestion-pivot"
status: "active"
---

## Completed work

* Staged feature `042-F` and shipment `042-S`, plus the supporting deliberation and implementation plan
* Promoted the pinned docline contract artifacts from `stash@{0}^3` into tracked repo paths without touching `stash@{0}`
* Pivoted graphtor-docs to local standardized-Markdown ingestion with embedded contract validation
* Removed legacy PDF, DOCX, Git, URL, and HTML ingestion/runtime branches
* Added migration gates, namespaced logical document identity, explicit registry behavior, and updated regression coverage
* Committed the feature branch as:
  * `49bbce136b3354ff69ee431605ac83f23ab4c807` — `feat(ingest): pivot graphtor-docs to docline markdown`
  * `85dda9a82814db6ce9f69ce6268f1819c6bac576` — `docs: document docline markdown ingestion pivot`
  * `217424249d1c4c3d64d843188f10f42ee39d6fcf` — `docs(harness): stage 042-F markdown ingestion pivot`

## Key decisions

* Graphtor now treats docline v1 frontmatter as the single supported ingestion contract
* Local source registries remain, but the runtime no longer supports `git` or `url` source types
* Contract `source_path` is treated as the canonical logical document path, namespaced by internal source identity
* Missing registries fail closed instead of triggering broad workspace auto-discovery or runtime stub creation
* The contract schema is embedded from tracked artifacts so installed binaries and dev builds validate identically

## Validation outcome

* `cargo fmt --all -- --check` passed
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` passed
* `cargo test --all-targets` passed
* `cargo audit` still reports the pre-existing unrelated advisory `RUSTSEC-2026-0041` through `cozo -> lz4_flex`

## Branch and backlog state

* Current branch: `feat/042-docline-markdown-ingestion-pivot`
* Worktree was clean before this memory checkpoint
* `042-F` is marked `done`
* `042-S` remains `active` because the work is committed locally but not yet PR'd or merged
* `stash@{0}` remains preserved and untouched

## Next steps

* Commit this memory checkpoint on the feature branch
* Push `feat/042-docline-markdown-ingestion-pivot` to the remote
* Open a PR for shipment `042-S` when ready for review
