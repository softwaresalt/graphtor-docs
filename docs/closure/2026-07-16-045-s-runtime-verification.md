---
date: 2026-07-16
slug: 045-s-runtime-verification
shipment: 045-S
mode: runtime-verification
status: PASS
owner: copilot
---

# Runtime Verification — 045-S Consumption-first graphtor

PR `#90` merged shipment `045-S` at
`479ac2b0e8deb66d036ab3c4eb8b79b272f501bc`.

Verification ran the release binary built from merged `main` against the
affected CLI surfaces (`install`, `doctor`, `uninstall`) in an isolated
throwaway workspace under `target/` (workspace-contained per Principle IV).

## Build

| Command | Result |
| --- | --- |
| `cargo build --release` (merged main) | ✅ `Finished release profile in 4m 07s`, exit 0 |

## Affected-Surface Checks

### Minimal install (consumption-first default)

`graphtor-docs install` created **only**:

* `.graphtor/` root directory (empty — no ingestion subdirs)
* `.mcp.json` with the registered `graphtor-docs` serve entry

Confirmed **absent**: `bin/`, `data/`, `cache/`, `config/`, `logs/`,
`sources.yaml`, copied binary. Output surfaced the consumption-first guidance
(drop a `.db` into `.graphtor/` and `serve`; `--with-ingestion` for the full
scaffold). ✅

### Doctor (minimal workspace)

`graphtor-docs doctor` reported **all PASS**, exit 0, with consumption-first
explanatory messages for each un-created ingestion subdir and the
serve-via-PATH binary resolution. No `Fail`. Confirms wave-9 `detect_footprint`
Minimal-classification fix (shared `config/` excluded from ingestion footprint)
holds at runtime. ✅

### Uninstall (minimal)

`graphtor-docs uninstall --confirm` enumerated **all** destructive mutations
before acting (F5): the MCP config prune list and the now-empty `.graphtor/`
root removal (F4). Both `.mcp.json` and `.graphtor/` were removed cleanly. ✅

### With-ingestion install + uninstall (full scaffold)

`graphtor-docs install --with-ingestion` created the full scaffold
(`bin/`, `data/`, `cache/`, `config/`, `logs/`, `sources.yaml`) with correct
next-steps guidance; `uninstall --confirm` removed the entire footprint. ✅

## Result

Runtime verification is **PASS** for the consumption-first install, doctor
footprint classification, and enumerated uninstall surfaces. All observed
behavior matches the shipped 045-S contract.
