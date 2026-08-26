---
type: stage-checkpoint
timestamp: 2026-08-24T23:24:00-07:00
agent: Stage
session: 049-S bounded reassessment / narrowing
shipment: 049-S
feature: 056-F
status: complete
outcome: no-change
---

# Stage Reassessment — Shipment 049-S (MCP serve initialize-handshake regression)

## Scope of this session

Bounded Stage reassessment of the existing **queued** shipment `049-S`
("Fix MCP serve initialize-handshake regression (Copilot CLI OS error 232)").
Goal: determine the smallest coherent, executable release unit that addresses
the stated regression without bundling unrelated or stale work.

Read-only for everything else. Newly queued Stage shipments `050-S` and `051-S`
and all stash entries were left untouched. No source/config changes, no builds,
no branches/worktrees/PRs, no Ship invocation.

## Decision: LEAVE 049-S UNCHANGED

`049-S` is **already** the smallest coherent, executable release unit for the
evidence-first remediation of the OS error 232 regression. It is the plan's
explicitly re-decomposed (2026-08-23) **PHASE 1 "evidence foundation +
parity-safe always-on diagnostics"** unit.

### Manifest (before == after)

| # | Task | Role |
|---|------|------|
| 1 | 056.020-T | Probe crate: std-only core sync transport (byte proxy) |
| 2 | 056.022-T | Probe crate: process spawn/teardown (RAII), wrapper subcommand |
| 3 | 056.023-T | Probe crate: observer seam + redacted evidence capture |
| 4 | 056.021-T | Probe crate: isolated workspace + control/treatment fixtures |
| 5 | 056.001-T | T0 exact-CLI differential serve probe; ordered cause classification |
| 6 | 056.002-T | Independent green out-of-process serve handshake test driver |
| 7 | 056.003-T | Always-on `cmd_serve` diagnostics seam + `mcp_serve_ready` (production) |
| 8 | 056.019-T | Sole H3-B terminal cause adjudicator (fan-in of 056.001 + 056.003) |

Umbrella feature `056-F` is excluded from the manifest (protected covering
feature per **P-015**).

### Why it is already minimal and coherent

- **Dependency-closed / self-contained.** All dependency edges are internal to
  the 8 tasks; the two endpoints (056.020, 056.002) have no upstream and the
  terminal (056.019) depends only on internal members. Verified via
  `backlogit_get_dependencies`.
- **Two spines converge on one deliverable.** probe+T0
  (`056.020 -> 056.022 -> 056.023 -> 056.021 -> 056.001`) and independent green
  driver+diagnostics (`056.002 -> 056.003`) both converge at the sole H3-B
  terminal `056.019`. The unit's acceptance IS the trusted exact-Copilot
  cause-selection record (056.001 ordered causes + 056.019 H3-B classification)
  plus the always-on diagnostics seam.
- **Removing any task breaks executability.** Dropping any probe-crate task
  starves 056.001; dropping 056.002 starves 056.003 and the 056.019 fan-in;
  dropping 056.003 removes the always-on seam and a 056.019 dependency; dropping
  056.001/056.019 removes the classification/terminal. Any removal yields a
  non-executable partial that cannot close 049-S.
- **Correctly excludes downstream work** (would be premature/unrelated):
  - PHASE 1.5 CI job `056.028` — cannot be authored until the `tools/mcp-probe/`
    crate lands via the 049-S probe tasks.
  - PHASE 2 remedy tasks — `selection:pending`, gated on T0/056.019 evidence.
    Bundling now would violate the selection gate and be premature (root cause
    not yet evidenced).
  - PHASE 3 acceptance/docs — `056.004-T` (T4), `056.012-T`, `056.013-T`.

## Focused scope/plan review (PASS)

- **Width isolation:** OK — 4 probe-crate widths + exact-cli + out-of-process
  driver + production `cmd_serve` diagnostics + H3-B adjudication; each task a
  single width.
- **2-hour rule:** OK — each is one bounded task.
- **Atomic milestone:** OK — probe crate builds/self-tests green; driver green;
  diagnostics green with stdout-parity proof; ordered-cause + H3-B record.
- **Schema/CLI/docs coupling:** Not a blocker for this unit. `056.003-T`
  explicitly defers all docs to `056.012-T` (outside 049-S). The type/transport
  discriminator reconciliation and CLI/docs surfaces (`056.026-T`/`056.027-T`/
  `056.013-T`) are PHASE 2/3, correctly outside 049-S.
- **P-001 / P-016:** Respected — one release unit in flight, single
  implementation worktree; sequential execution enforced by selection gate +
  one-shipment-at-a-time, not by fake cross-cause edges.
- **Staleness / drift:** None. All 8 tasks updated 2026-08-23 and consistent
  with the latest `056-F` DoD and exec-plan PHASE 1 manifest; shipment manifest
  matches the plan's named 8-task manifest exactly (no drift).

## Removed / deferred items and their state

None removed. No manifest mutation performed, so no items required conversion to
queued backlog follow-ups. All non-manifest 056-F tasks remain `queued`
follow-ups outside 049-S with their existing labels/dependencies (unchanged).

## Execution order (relative to 050-S / 051-S)

Under P-001 (one release unit in flight) and P-016 (single implementation
worktree), sequential:

1. `050-S` — Harden VS Code terminal auto-approve allow-list (security, high)
2. `051-S` — store.rs identity-bound no-follow TOCTOU fix (security, high)
3. `049-S` — evidence foundation (this shipment)
4. PHASE 1.5 — `056.028` standalone-probe CI job (Stage-assembled after 049-S closes)
5. PHASE 2 — T0/056.019-selected remedy shipment(s)
6. PHASE 3 — acceptance/docs (`056.004`/`056.012`/`056.013`)

Security shipments 050-S/051-S go first per operator direction. No overlap
between 049-S members and 050-S (`057.001-T`) or 051-S (`059.001..005-T`).

## Follow-up recommendations

- After 049-S closes, Stage assembles the PHASE 1.5 unit (`056.028`) **before**
  flipping any remedy family from `selection:pending` to `selection:selected` or
  creating any remedy shipment.
- Fail-closed H3-B: if `056.019-T` returns **INCONCLUSIVE**, it moves to
  `blocked`, blocks 049-S closure, and REQUIRES a **named new bounded Stage
  follow-up** (or operator adjudication) — it is never marked done and never
  closes 049-S in that state.

## Blockers

None. Reassessment complete. Shipment left queued and unmodified; no shipment
executed this session (Stage role boundary — Ship owns execution).

## Artifacts changed this session

- `049-S` backlogit log — appended Stage reassessment comment.
- backlogit agent memory — `stage-049S-reassessment-2026-08-24`.
- backlogit checkpoint — `.backlogit/checkpoints/checkpoint-20260825-062811.json`.
- This memory file.
