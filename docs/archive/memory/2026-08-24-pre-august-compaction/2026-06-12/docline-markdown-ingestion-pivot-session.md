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
* Remediated the adversarial-review findings on PR #69 and pushed:
  * `4e5e40905fdaf925e4775ca62798a3613e735157` — `fix(sync): use map_or for legacy-state mtime`
  * `d01fab7d76d300812ce164f11c53f642a9e9c109` — `fix(sync): harden pivot rebuild safety`
* Hardened the docline pivot by rejecting non-canonical `source_path` values, blocking `--no-embed` during destructive pivot-era rebuilds, and persisting sync state after successful regular full syncs
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
* `cargo audit --ignore RUSTSEC-2026-0041 --ignore RUSTSEC-2026-0008` passed with only pre-existing upstream unmaintained warnings
* `gh pr checks 69` reports `build` passing on commit `d01fab7d76d300812ce164f11c53f642a9e9c109`

## Branch and backlog state

* Current branch: `feat/042-docline-markdown-ingestion-pivot`
* PR #69 remains open against `main`
* Worktree is clean after pushing the hardening fixes
* `042-F` is marked `done`
* `042-S` remains `active` because the PR is not yet merged
* `stash@{0}` remains preserved and untouched

## Next steps

* Keep PR #69 in review until merge approval is granted
* Track the remaining upstream `cozo` dependency advisories separately from the docline pivot shipment
