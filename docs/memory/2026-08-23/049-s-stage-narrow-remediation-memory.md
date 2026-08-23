---
type: session-memory
agent: stage
date: 2026-08-23
branch: chore/stage-049-S
base_head: 10f05bd594e30095d74ba2af03c8d16e4f0c082b
feature: 056-F
shipment: 049-S
scope: narrow-remediation
---

# Stage narrow remediation — 056-F / 049-S (MCP serve OS error 232)

Planning/backlog/docs only. No Rust/source/build/PR/merge work (full local build
non-applicable — recorded). Applied eight adjudicated corrections from a fresh
report-only review of HEAD `10f05bd`, then ran a fresh 4-persona report-only plan
review and remediated its consensus P1/P2 findings.

## New tasks created (via backlogit)
- `056.026-T` — Reconcile generated MCP config `type`/`transport` discriminator
  (UNCONDITIONAL generation; depends only on `056.019-T`; not-needed when no
  mismatch). Labels `phase:remedy`,`cause:discriminator`,`selection:pending`.
- `056.027-T` — Deliver reconciled discriminator to existing installs (depends on
  `056.026-T` + `056.018-T`; composes `056.017`/`056.018`; NOT `056.008`/`056.009`;
  operator-approval-gated destructive mutation). Same labels.
- `056.028-T` — Dedicated standalone-probe CI job (`tools/mcp-probe/` separate
  manifest/lockfile). Depends on `056.020-T`; `phase:evidence`,`cause:probe-ci`;
  outside `049-S`; in T4 fan-in. Stage creates, Ship implements.

Task count now 28 (`056.001-T`..`056.028-T`).

## Corrected managed-config DAG (delivery split)
`056.019→056.017`; `056.017→056.008` (cwd) parallel to `056.017→056.024→056.018`
(recovery); `{056.008,056.018}→056.009`. Discriminator: `056.019→056.026`;
`{056.026,056.018}→056.027`. `056.017`/`056.024`/`056.018` activation predicates
broadened to include discriminator existing-install delivery so no delivery task
depends on a prerequisite that closed not-needed. Edge changes: `056.008` dep
`056.024`->`056.017`; `056.018` dep `056.008`->`056.024`; `056.009` += `056.008`;
`056.013` += `056.027`; T4 (`056.004`) += `056.026`/`056.027`/`056.028`; REMOVED
`056.001->056.002` (driver is T0-agnostic).

## Selection gate (correction 3)
Every `phase:remedy` task carries `selection:pending` + `cause:<family>` (16
tasks). Stage flips selected families to `selection:selected` after `049-S`
closes, then creates the remedy shipment. Required pre-shipment SQL gate defined
in DoD + plan. P-001/P-016 do NOT perform selection (plan corrected). Unselected
stay `queued`+`selection:pending`, no shipment membership (NOT status `blocked`).

## H3-B fail-closed (correction 1)
`056.019-T`: conclusive B1/proven-B2 -> `done` + close `049-S`; INCONCLUSIVE ->
`blocked` + blocks `049-S` + named new bounded Stage follow-up. `056.019` records
its own status only; Stage Final Assembly dispositions siblings under H3-B2.
Aligned across DoD, plan T-H3-B, decision sections 5/6/Open Questions.

## Fresh review verdict (4 personas, report-only)
Constitution=ADVISORY(0/0), Architecture=PASS(0/0), Correctness=FAIL(0/4 P1),
Agent-Native=FAIL(0/2 P1). Consensus P1s remediated in one cycle: (1) Verification
Commands step 5 `/mcp show` wire-field demand + `-split ' '`; (2) recent Plan
Review bullet still calling inconclusive terminal; (3) `056.019` H3-B2 mutating
sibling status vs selection gate; (4) selection gate lacked concrete check; (5)
`056.002` read-only control unbounded + parity-control circular with `056.003`.
P2s remediated: `056.027` approval gate (+Constitution VII/Plan Hardening),
`056.028` implement-vs-plan-only contradiction (+stale duplicate description
repair). Residual = P3 advisories only. No unresolved P0/P1.

## Six current Copilot PR-thread mapping (read-only; PR/threads NOT edited)
1. H3-B inconclusive closability -> RESOLVED by content (fail-closed everywhere).
2. Managed-config/discriminator coupling -> RESOLVED (split `056.026`/`056.027`,
   corrected DAG, activation broadening).
3. Selection vs P-001 claim -> RESOLVED (explicit selection-gate labels + required
   query; plan stops claiming P-001 selects).
4. `/mcp show` vs wire-field parity -> RESOLVED (CLI-visible only; wire fields via
   `056.002` control; Verification step 5 + decision section 6 aligned).
5. Stale monolithic strings (halt/return-to-T0, raw-frame replay) -> RESOLVED.
6. Duplicate hook events (`hooks_queue.jsonl`) -> OUTSTANDING BY DESIGN: append-only
   benign history, no supported removal; NOT "fixed" by deleting history. Operator
   note retained; disposition stated. Never hand-edited.

## Validation
metadata/WIT confirmed (statuses queued/active/done/blocked; shipment blocks deps
supported); `backlogit_sync_index` OK; `doctor` clean except pre-existing
unrelated `013.008-T` orphan (out of scope); `049-S` membership unchanged (8
tasks); all 16 remedy tasks `selection:pending`; graph acyclic (61 edges); no
stale strings; `git diff --check` clean (CRLF-only notices). Full Rust build
non-applicable (planning-only).

## Preserved (NOT committed)
`.mcp.json`, `.backlogit/checkpoints/*.json`, `.backlogit/runtime/`,
`exec_commands.ps1`, `run_git_commands.sh`, `temp_git_commands.sh`.

## Next steps
Ship: run fresh current-HEAD local review; execute `049-S` evidence unit; after
T0 classification, apply the selection gate and assemble the selected remedy
shipment(s); PHASE 3 via the Stage Final Assembly Protocol.
