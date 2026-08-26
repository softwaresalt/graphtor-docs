---
session_type: stage
date: 2026-06-18
chore_id: 043-C
shipment_id: 043-S
deliberation_id: 002-DL
source_stash_id: 964597B1
status: complete
---

# Stage Memory — 043-C Post-042-S Unmaintained Transitive Audit Advisory Triage

## ID Correction (2026-06-19)

> This Stage session minted the chore as `035-C`, which collided with the
> already-archived feature `035-F` ("Remove non-functional Editor::Copilot MCP
> config path", shipped via `026-S`) because backlogit's per-type level-1 counter
> handed `chore` a number the `feature` namespace already owned. During the 043-S
> Ship session the collision was detected and the chore + its two task children
> were renumbered to the collision-free shared number `043` via direct file edits
> + `backlogit sync` (bare-ID CLI mutations are unsafe while a duplicate exists).
> The references below have been updated to the corrected IDs:
> `035-C → 043-C`, `035.001-T → 043.001-T`, `035.002-T → 043.002-T`. The archived
> `035-F` feature and its children are unrelated and were left untouched. See
> `docs/compound/backlogit-level1-id-collision-across-parent-types.md` for the RCA.

## Step Checklist

- [x] Step 0.0 — Tool Availability Gate
  `.autoharness/backlog-registry.yaml` absent → no MCP registry. `backlogit`
  CLI v1.2.0 on PATH with `.backlogit/` workspace = the intended operating mode.
  Shipments supported via `backlogit shipment`. `TOOL_OK: backlogit (CLI)`.
- [x] Step 0.1 — Index Sync  `backlogit sync` → indexed 386 artifacts. `INDEX_SYNC_OK`.
- [x] Step 0 — Operator visibility  Headless (Orchestrator-invoked); intercom/engram
  MCP tool surface not callable from this agent → visibility degraded, proceeded
  locally with explicit repo evidence (prescribed fallback).
- [x] Step 1 — Triage & classification  Single targeted stash entry `964597B1`
  classified **task-shaped**. Verified all 5 advisories + transitive paths via
  `cargo audit` and `Cargo.lock`/`cargo tree`.
- [x] Step 1.5 — Contextual grouping  Single-entry fallback (operator targeted
  964597B1) → solo group with synthesized covering chore. `013.008-T` evaluated
  for grouping and **kept separate** (see decision below).
- [x] Step 1.8 — Learnings retrieval  Hit: `docs/compound/cargo-audit-workspace-config-limitation.md`
  (**high confidence**) — two-place suppression pattern (audit.toml documents; CI
  `--ignore` enforces; cargo audit 0.22 does not read audit.toml). Folded into plan.
- [x] Step 2 — Deliberation  `002-DL` + `docs/decisions/2026-06-18-unmaintained-transitive-audit-advisories-deliberation.md`.
  Decided Option B.
- [x] Step 3 — Planning  `docs/archive/plans/2026-08-24-pre-august-compaction/2026-06-18-unmaintained-transitive-audit-advisories-plan.md`.
  Step 3.2: `Requires plan hardening: yes` (CI security-audit gate) → plan-harden
  appended `## Plan Hardening`.
- [x] Step 4 — Plan review  Scope Boundary Auditor (independent) + inline persona
  lenses → **PASS** (all findings P3; 2 P3 refinements applied: git2 out-of-scope
  wording, Unit 1 cascade ceiling). `plan-review-attempt: 1 PASS`.
- [x] Step 5 — Harvest  Chore `043-C` → tasks `043.001-T`, `043.002-T`. Dependency
  edge `043.002-T → 043.001-T (blocks)` recorded.
- [x] Step 5.5 — Shipment assembly  `043-S` queued; items parent-first/dep-order
  `[043-C, 043.001-T, 043.002-T]`. Verified via `shipment get`.
- [x] Step 5.6 — Archive consumed stash  `964597B1` → archived. Stash now empty.
- [x] Step 6 — Summary  Presented to Orchestrator.

## Triage Decisions (per advisory)

| Advisory | Crate | Decision |
|---|---|---|
| RUSTSEC-2025-0056 | adler | Ignore + document (locked behind cozo 0.7.6 → swapvec → miniz_oxide 0.7.4) |
| RUSTSEC-2025-0141 | bincode | Ignore + document (locked behind cozo: swapvec + fast2s) |
| RUSTSEC-2025-0057 | fxhash | Ignore + document (locked behind cozo → jieba-rs 0.6.8) |
| RUSTSEC-2025-0119 | number_prefix | Attempt indicatif upgrade first (direct dep); ignore + document if infeasible |
| RUSTSEC-2024-0436 | paste | Ignore + document (deep via candle 0.8.4 / gemm / tokenizers 0.20.4) |

Plus: add `--deny warnings` (allowlist gate); drop obsolete `--ignore RUSTSEC-2026-0008`
(git2 absent from Cargo.lock); preserve `RUSTSEC-2026-0041` (lz4_flex, owned by 013.008-T).
All suppressions carry a **2026-09-18** review date.

## 013.008-T — Kept SEPARATE (with operator flag)

Verified: git2 (RUSTSEC-2026-0008) already resolved (absent from Cargo.lock, removed
by 042-S Git-path retirement); lz4_flex (RUSTSEC-2026-0041) still present and still
semver-locked behind cozo 0.7.6 → swapvec 0.3.0 → lz4_flex ^0.10 — blocker NOT cleared.
Its remaining advisory is a genuine 8.2-high vulnerability, different in kind/timeline
from the 5 informational warnings. **Operator-attention flag:** narrow `013.008-T` to
lz4_flex-only and correct its stale `blocked_reason` ("candle vector search APIs not yet
stable" → "cozo 0.7.6 pins swapvec 0.3.0 → lz4_flex ^0.10; awaiting cozo swapvec 0.4+").
Intentionally NOT modified here (out of stash 964597B1 scope; it is a blocked item).

## Next Step / Handoff

Hand **shipment 043-S** to Ship. No Ship work performed (no branch, build, CI, or PR).
Pre-existing unrelated working-tree edits (`.autoharness/config.yaml`,
`.github/agents/*.agent.md`) intentionally left untouched and excluded from staging commit.
