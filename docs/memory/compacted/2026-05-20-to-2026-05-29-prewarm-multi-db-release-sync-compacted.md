---
title: "Compacted memory — 2026-05-20 to 2026-05-29 prewarm, multi-db, and release-sync work"
description: "Dense summary of completed staging, shipping, and closure checkpoints compacted from the late-May shipment wave."
date: "2026-06-12"
source_window: "2026-05-20 to 2026-05-29"
status: "compacted"
---

## Covered work

This compaction summarizes completed or superseded memory artifacts for:

* shipment handoff and gate coordination around `026-S` and `027-S`
* feature `037-F` / shipment `028-S` for prewarm sync progress and telemetry
* feature `038-F` / shipment `029-S` for multi-database file support
* features `039-F` and `040-F` staged for runtime hardening and source-registry normalization
* shipment `030-S` closure follow-up
* shipment `031-S` closure follow-up
* shipment `032-S` / feature `041-F` release sync hardening staging, review, and ship memory
* shipment `041-S` post-merge closure memory

## Durable decisions

* Keep Stage and Ship release units isolated; do not dispatch a queued shipment while an earlier Ship release unit is still active or blocked on merge approval
* Treat prewarm as a dedicated CLI workflow with stderr progress plus machine-readable telemetry instead of overloading existing sync output
* Split multi-database work into separate concerns:
  * runtime and locking hardening
  * source-registry normalization and duplicate-intake controls
* Preserve parity between CLI, status, and internal runtime surfaces when changing shared data-shape contracts
* Keep release-sync hardening scoped to shared embedding resolution and operator-visible progress rather than widening MCP/runtime scope
* Treat stale or unavailable Copilot re-review as a merge-gate problem, not an excuse to silently continue past the policy boundary

## Key implementation outcomes

### Prewarm workflow

* Added a dedicated `prewarm` subcommand with progress callbacks and JSONL telemetry
* Introduced shared sync progress reporting patterns that later informed follow-on sync UX work
* Kept the implementation dependency-light and validation-first

### Multi-database support

* Added optional per-source database routing
* Refactored runtime/store surfaces so sync, serve, status, and prewarm could operate across discovered databases
* Tightened status-shape consistency and backward compatibility around sync-state paths

### Source-registry and runtime hardening

* Staged source-registry normalization and duplicate-intake preflight as a separate feature from runtime hardening
* Reinforced the need to keep duplicate-detection, config validation, and runtime loading behavior aligned

### Release sync hardening

* Added a shared embedding resolver path used by sync, serve, and prewarm
* Added operator-visible sync progress while keeping stdout/JSON contracts parseable
* Carried explicit review findings forward into follow-up plan revisions before shipment

## Files and surfaces that changed in the covered wave

Commonly touched areas across the compacted checkpoints:

* `src/main.rs`
* `src/sync/`
* `src/config/`
* `src/db/`
* `src/cli/`
* `tests/prewarm_progress_test.rs`
* `tests/sync_progress_test.rs`
* `tests/embedding_resolver_parity_test.rs`
* multi-database and status-related integration tests
* plan, deliberation, and closure artifacts under `docs/decisions/`, `docs/exec-plans/`, and `docs/closure/`

## Recurrent failure modes and lessons

* Copilot review freshness on the current PR head remained a recurring operational gate
* Shared output surfaces needed explicit parity tests whenever JSON or status shape changed
* Runtime changes that affected multiple commands were safer when staged as narrow, dependency-ordered units
* Progress and identity changes required explicit documentation updates in the same release unit to avoid drift

## Outcome summary

* The late-May wave completed prewarm progress/telemetry, multi-database support, and release-sync hardening as separate but related release units
* Closure artifacts and queue state from those units are preserved elsewhere in the repo; this compacted file replaces the need to keep the verbose session-level memory checkpoints in `docs/memory/`
* The original verbose checkpoints are archived under `docs/archive/memory/` with their relative paths preserved
