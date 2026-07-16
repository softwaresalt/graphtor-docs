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

## Phase-1 Trust-Boundary Surfaces (auto-discovery, read-only posture, status)

The shipment's primary trust-boundary changes — dropped-database
auto-discovery, read-only posture classification, `serve` served-set gating, and
`status` — were exercised through the shipped integration and unit suites run
against the release binary, plus a live discovery smoke:

| Surface | Evidence | Result |
| --- | --- | --- |
| Read-only `serve` posture gating | `cargo test --release --test serve_posture_gating_test` | ✅ 8 passed |
| `status` on a discovered database | `cargo test --release --test db_status_test` | ✅ 4 passed |
| `status` multi-database discovery | `cargo test --release --test status_multi_db_test` | ✅ 9 passed |
| Auto-discovery + read-only classification (dropped `.db`, dedup union, junction rejection, explicit-db read-only, `..`-escape rejection, empty-config stays read-only) | `cargo test --release --bin graphtor-docs serve_discovery` | ✅ 31 passed |
| Live dropped-`.db` discovery | minimal `install` → dropped `.graphtor/graph.db` → `status --json` | ✅ auto-discovery located the dropped path and opened it (cozo rejected the deliberately non-database placeholder content — expected, confirms the discovery→open path reaches the real db loader) |

These 52 deterministic checks cover the read-only posture classification and
dropped-database auto-discovery logic that install/doctor/uninstall smoke alone
does not reach. The long-running `serve` STDIO loop itself was not started
interactively; its served-set gating and posture classification are covered by
the posture-gating and `serve_discovery` suites above.

## Result

Runtime verification is **PASS** for shipment `045-S`. The consumption-first
install, doctor footprint classification, and enumerated uninstall surfaces
behave as designed, and the Phase-1 trust-boundary surfaces (dropped-database
auto-discovery, read-only posture classification, and `status`) are verified via
the shipped integration and unit suites plus a live discovery smoke. All
observed behavior matches the shipped 045-S contract.
