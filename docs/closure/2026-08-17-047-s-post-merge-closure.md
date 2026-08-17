---
date: 2026-08-17
slug: 047-s-post-merge-closure
shipment: 047-S
mode: post-merge
status: READY
owner: copilot
---

# Post-Merge Closure — 047-S Read-Only Serve Guarantee Honesty

PR [`#97`](https://github.com/softwaresalt/graphtor-docs/pull/97) merged
shipment `047-S` at `704b95a6c1e2930079d6f3a602ab66e9682d4916` (merge commit,
merge-commit strategy per Constitution Principle XI / P-009).

## Summary of the Change

Resolved PR #90 deferrals **F2** and **F6** by correcting an overstated
read-only serve guarantee across every in-code and documentation contract
surface, without changing any guard runtime behavior:

* `src/db/store.rs` — corrected rustdoc on `is_engine_enforced_readonly`,
  `open_engine_readonly`, `open_sqlite_readonly`, and the
  `EngineReadonlyGuard` struct/field docs; new
  `ENGINE_READONLY_OPEN_LOG_MESSAGE` constant qualifies the startup log.
* New characterization test, landed RED-first (Constitution Principle II).
* `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
  — corrected "Read-only serve hardening" section.
* `docs/closure/2026-08-17-047-s-release-observability-evidence.md` —
  release-observability evidence for the startup-log runtime-surface change.
* CI audit-allowlist maintenance (disclosed, necessary scope addition):
  `RUSTSEC-2026-0249` (`smartstring`, unmaintained, direct `cozo` dependency)
  added to the existing suppression pattern in `.github/workflows/ci.yml` /
  `audit.toml`.
* Two authorized "stowaway" commits (model-routing config, workspace
  hygiene) carried over from a prior session at explicit operator request.

`EngineReadonlyGuard::lock`/`Drop` bodies are byte-identical to pre-shipment
`main`; `is_engine_enforced_readonly()` continues to return exactly
`self.engine_readonly_guard.is_some()`.

## Invariants to Preserve

* App-level `AccessMode::ReadOnly` remains the sole authoritative read-only
  guarantee (unchanged).
* `is_engine_enforced_readonly()` meaning unchanged — `guard.is_some()`, not
  overloaded to `access_mode == ReadOnly`.
* `EngineReadonlyGuard::lock`/`Drop` runtime behavior unchanged (permission
  capture/restore, non-empty-sidecar preservation, symlink containment).
* No external-path capability introduced; `validate_path` containment
  unaltered (Constitution Principle III/IV).

## Validator Evidence (Runtime Verification)

Runtime surface touched: a single `tracing::info!` log line, emitted once
per `DataStore::open_engine_readonly` call at `serve` startup for a
`ReadOnly`-posture database.

**Manual/CLI validator** — ran the release binary built from the merged
`main` (`cargo build --release`, `Finished release profile in 1m 23s`)
against an isolated, throwaway workspace under `target/` (workspace-contained
per Principle IV, cleaned up after verification):

1. `graphtor-docs sync --config sources.yaml` against a single docline-valid
   markdown fixture — synced 1 file, 1 chunk, 0 errors, confirming the merged
   `main` binary builds and runs the full sync pipeline cleanly.
2. `graphtor-docs serve --read-only --config sources.yaml` — forced
   `ReadOnly` posture, confirmed via log:
   `resolved serve posture discovered_count=1 generation_count=0
   readonly_count=1`, then:

   ```text
   opened engine-enforced read-only SQLite DataStore (filesystem lock
   active: protection is robust only if no independent guard ever overlaps
   this guard's lifetime on the file; any such overlap - same- or
   cross-process - leaves protection best-effort for the rest of this
   guard's life, even once the overlapping guard drops - see F6)
   ```

   This is character-for-character the `ENGINE_READONLY_OPEN_LOG_MESSAGE`
   constant, confirming the qualified wording is emitted correctly by a real
   process end-to-end (not merely by the unit test's capture harness). The
   process then exited on the expected "MCP server failed to start /
   connection closed: initialize request" condition, since no MCP client
   was attached to complete the STDIO handshake — expected and irrelevant to
   this verification.

**Verdict**: `PASS`. No manual checkpoints required (no OAuth/payment/email/
external-service flows in scope). No blocked prerequisites.

## Pre-Deploy Audits

* No feature flag, migration, schema, or cross-service dependency involved.
* `cargo build --release` succeeded from merged `main`.
* Full local quality gates (fmt, clippy pedantic, test, audit) all green on
  the merge commit (re-verified locally after merge — see Quality Gates
  below).

## Deployment / Rollout Path

Merge-only. `graphtor-docs` is a single-developer, local-only CLI/MCP
server (no hosted deployment, no release binary distribution pipeline
gated on this change). The next operator run of `cargo build --release` (or
`install`) picks up the corrected wording automatically.

## Post-Deploy Checks

See the release-observability evidence
(`docs/closure/2026-08-17-047-s-release-observability-evidence.md`) for the
full owner/baseline/window/threshold/rollback record. Summary:

* **Owner**: `@softwaresalt`.
* **Observation window**: first 10 `ReadOnly`-posture `serve` starts, or 14
  calendar days post-merge, whichever comes first.
* **Healthy signal**: the qualified log line renders as a single coherent
  line; no operator confusion reported (there is no other consumer of this
  log line today).
* **Failure signal**: the log line fails to render, is misread as
  reintroducing an unconditional guarantee, or (if ever added) a
  log-scraping rule keyed on `"filesystem lock active"` silently stops
  matching.
* **Rollback trigger**: any failure signal above observed during the window.
* **Rollback procedure**: text-only revert to a *shorter but still-qualified*
  message (never the original unconditional wording — see the evidence doc
  for why). Zero effect on guard runtime behavior either way.

## Risky Action Record

| ProposedAction | ActionRisk | ActionResult |
|---|---|---|
| Correct overstated read-only contract wording across code/doc surfaces (log constant, `is_engine_enforced_readonly`/`open_engine_readonly`/`open_sqlite_readonly` rustdocs, `EngineReadonlyGuard` struct/field docs, the design doc) | moderate (security-adjacent wording on a read-only guarantee; no runtime/behavior change) | applied |
| Add `RUSTSEC-2026-0249` to the CI audit allowlist (pre-existing, unrelated advisory blocking merge) | low (documentation of an already-accepted transitive-dependency risk class; matches 6 existing precedents) | applied |
| Carry 2 pre-authorized stowaway commits onto this branch/PR | low (config/workspace-hygiene only, explicitly operator-authorized) | applied |
| Archive shipment `047-S` via safe-close (not the cascade op) | low (single-artifact archival; protected set was empty — full-feature shipment) | applied, verified no cascade |

## Healthy Signals

* `cargo build --release` succeeds from `main`.
* `serve --read-only` emits the qualified log line verbatim.
* All quality gates green on the merge commit.

## Failure Signals

* Any of the Failure Threshold conditions in the release-observability
  evidence doc.
* A future `cargo audit` run reporting `RUSTSEC-2026-0249` as a *newly
  actionable* (not just unmaintained) advisory — re-triage per its `Review:
  2026-09-18` date in `audit.toml`.

## Monitoring Plan

Manual observation only (no dashboard/alerting exists or is warranted for
this single-developer, local-only tool) — see release-observability
evidence doc for the full plan. `audit.toml` review dates (`2026-09-18`)
serve as the periodic re-triage mechanism for the unmaintained-crate
suppressions.

## Validation Window

10 `ReadOnly` `serve` starts or 14 days post-merge (see above).

## Owner

`@softwaresalt` (sole maintainer).

## Quality Gates (Re-Verified Post-Merge)

| Gate | Result |
|---|---|
| `cargo build --release` (merged `main`) | ✅ `Finished release profile [optimized] in 1m 23s` |
| `cargo fmt --all -- --check` | ✅ clean (verified throughout the PR lifecycle) |
| `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` | ✅ clean |
| `cargo test --all-targets` | ✅ 555+ tests, 0 failures |
| `cargo audit` (with updated allowlist) | ✅ pass — 1 vulnerability + 6 unmaintained-crate advisories, all explicitly allowlisted (see `audit.toml`); not "clean" in the sense of zero findings, but a successful gate over accepted, documented supply-chain risk |
| CI (`build` job on PR #97, final HEAD `b1d3f05`) | ✅ pass |

## Releasability Evidence

| Evidence | Status |
|---|---|
| Monitoring plan | Manual observation (documented, proportionate to risk) |
| Pre-deploy audit | N/A — no migration/flag/cross-service dependency; verified build |
| Runtime verification | `PASS` — live CLI smoke test confirms exact log wording |
| Post-deploy observation window | Defined: 10 starts / 14 days, owner `@softwaresalt` |
| Rollback trigger + procedure | Defined: text-only revert, honest fallback only |
| Risky actions | All recorded above, `ActionResult: applied` |

**Releasability status**: `READY`.

## Cross-References

* `docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md`
* `docs/decisions/2026-08-16-readonly-serve-cross-process-coordination-spike.md`
* `docs/decisions/2026-08-16-shared-external-readonly-databases-deliberation.md`
* `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
* `docs/closure/2026-08-17-047-s-release-observability-evidence.md`
* Follow-up items stashed: `9CEC208C`, `C365AB98`, `3FFE51B4`, `B883681D`,
  `B8C0851E` (see PR #97 description for full disposition of each).
