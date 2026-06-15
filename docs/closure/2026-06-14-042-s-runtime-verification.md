---
date: 2026-06-14
slug: 042-s-runtime-verification
shipment: 042-S
surface: cli
mode: manual
status: PASS
merge_commit: 7c6250b8ae7b7304bc1294dd06ab38d11a717df9
owner: copilot
---

# Runtime Verification — 042-S Docline Markdown Ingestion Pivot

## Verification Target

Verify the shipped runtime surfaces for the docline-markdown ingestion pivot:

* docline v1 frontmatter parsing stays accepted while malformed frontmatter still fails closed
* acquisition and config planning remain local-only after retiring Git/URL/PDF/DOCX/HTML paths
* explicit database targets continue to behave correctly without a discovered registry
* namespaced `source_path` identity still prevents duplicate-path corruption
* staged v4 migration preflight still protects existing data on invalid or duplicate inputs
* the MCP manifest surface remains aligned with the shipped tool contract

## Preconditions

* PR `#69` merged at `7c6250b8ae7b7304bc1294dd06ab38d11a717df9`
* Shipment `042-S` archived on branch `post-merge/042-docline-markdown-ingestion-pivot`
* Closure verification ran from a clean post-merge worktree rooted at `origin/main`

## Commands Attempted

```text
cargo test --test parse_frontmatter_test
cargo test --test acquire_plan_test
cargo test --test explicit_db_target_no_registry_test
cargo test --test mcp_manifest_test
cargo test --test pipeline_duplicate_source_path_test
cargo test --test sync_v4_preflight_test
```

## Expected Behavior

* YAML frontmatter detection remains stable for LF/CRLF inputs and malformed delimiters fail safely
* local acquisition planning remains the only supported runtime path
* `serve`/`status` with `--db-path` continue to fail closed without inventing a phantom registry
* duplicate or stolen `source_path` values do not corrupt stored chunks
* v4 migration preflight aborts before destructive rebuild when candidate inputs are invalid
* MCP manifest tooling remains complete and sorted

## Observed Behavior

All targeted runtime verification commands passed on the merged code:

* `cargo test --test parse_frontmatter_test` — 14 passed
* `cargo test --test acquire_plan_test` — 5 passed
* `cargo test --test explicit_db_target_no_registry_test` — 2 passed
* `cargo test --test mcp_manifest_test` — 6 passed
* `cargo test --test pipeline_duplicate_source_path_test` — 4 passed
* `cargo test --test sync_v4_preflight_test` — 6 passed

Additional closure validation also passed:

* `cargo fmt --all -- --check`
* `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`
* `cargo test --all-targets`

## Evidence

* frontmatter parsing remains stable across LF/CRLF and malformed-delimiter cases
* local-only acquisition planning still rejects the retired remote/runtime acquisition paths
* explicit-db no-registry behavior still reaches the v4 gate instead of fabricating registry state
* duplicate-path regression coverage still preserves pre-existing data
* v4 migration preflight still blocks destructive rebuild on invalid or duplicate docline inputs
* MCP manifest coverage still reports the expected tool set without unexpected names

## Advisory Note

Raw local `cargo audit` (without CI's `--ignore` suppressions) still reports
the existing high-severity `RUSTSEC-2026-0041` path via `lz4_flex`, already
tracked by blocked backlog task `013.008-T`. It also reported unmaintained
transitive dependency warnings, which were captured for later triage in stash
entry `964597B1`.

## Verdict

**PASS**

## Recommended Next Action

Carry this verification result into the post-merge closure PR for shipment
`042-S`.
