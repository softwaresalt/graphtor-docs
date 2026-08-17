---
type: compaction-report
date: 2026-08-16
target: memory
agent: Stage
context: "Consolidates the 2026-08-16 dark-Stage 047-S/048-S intermediate checkpoints; records the PR #96 review-remediation cycle"
source_files:
  - "docs/archive/memory/2026-08-16/dark-stage-970-5d98-serve-followups-memory.md"
  - "docs/archive/memory/2026-08-16/dark-stage-review-outcome-checkpoint.md"
preserved:
  - "docs/memory/2026-08-16/dark-stage-session-complete-memory.md"
tags:
  - stage
  - dark-mode
  - compaction
  - read-only-serve
  - serve-discovery
---

# Compaction Report — 2026-08-16 Dark-Stage (047-S / 048-S)

## Trigger

The mandatory `compact-context` step runs at Stage session and phase completion;
it is not gated solely on the 40-file / 500 KB threshold. The original session
record wrongly skipped it on a file-count rationale. PR #96 review flagged that
gap, so this report consolidates the two intermediate 2026-08-16 checkpoints and
records the follow-up remediation.

## Action

* Consolidated (verbose originals moved to `docs/archive/memory/2026-08-16/`):
  * `dark-stage-970-5d98-serve-followups-memory.md` — intake, scope, and
    read-only code grounding.
  * `dark-stage-review-outcome-checkpoint.md` — plan-review and adversarial-review
    outcome.
* Preserved in place as the authoritative handoff for the queued 047-S / 048-S
  shipments: `dark-stage-session-complete-memory.md`.

## Consolidated session summary

### Stash disposition

| Stash | Kind | Disposition |
|---|---|---|
| 970AE45A | spike | archived — spike findings + feature 054-F (Group A) |
| 5D98DBCC | feature | archived — external-path REJECTED (Principle III/IV); reshaped into 054-F |
| B88E37BF | task | archived — task 055.001-T (Group B) |
| 5868A7C5 | task | archived — task 055.002-T (Group B) |
| 5905CDEE | bug | NEW active (deferred) — symlink-swap TOCTOU, future spike |
| F1CE20EC | feature | NEW active (deferred) — Option C shared serve + true F6 fix |

### Shipments produced (queued, not claimed)

* **047-S** "Read-only serve guarantee honesty (F2/F6)" — priority medium; run
  first. 054-F → 054.001-T (A1 code) → 054.002-T (A2 docs); A2 blocks-on A1.
* **048-S** "Serve auto-discovery follow-ups (PR90 deferrals)" — priority low; run
  second. 055-F → 055.001-T (B1 streaming memory reduction) and 055.002-T (B2
  alias evaluation); B1 and B2 independent.

### Review consensus (Stage planning gate)

Four independent cross-model reviewers (anthropic claude-opus-4.8, google
gemini-3.1-pro, openai gpt-5.6-sol, xai grok-4.5) plus a 3-reviewer
post-remediation re-review. reject-external-path: HIGH-confidence PASS. Both plans
FAILed round 1 and PASSed after one remediation cycle:

* Plan A error: overloaded `is_engine_enforced_readonly()` to `access_mode ==
  ReadOnly` (would make `open_sqlite_readonly` falsely report engine enforcement).
  Remediated: predicate keeps `guard.is_some()`; repository-wide honest-surface
  sweep; F6 documented as a residual, qualified for one owning guard (same- or
  cross-process).
* Plan B error: traversal short-circuit broke fail-closed-on-walk-error and could
  escalate a partially-unreadable source from read-only to read-write posture.
  Remediated: full error-observing walk retained; only the `O(document-count)`
  `Vec` removed via a streaming boolean over a shared reusable matcher;
  deterministic ordering seam + differential test.

### Incidental repair (logged)

`.backlogit/archive/013-S.md` had pre-existing malformed YAML blocking the
mandatory Step 0.1 index sync. Repaired minimally (relocated `shipped_at` / `pr` /
`merge_commit` under `custom_fields`). Operational unblocking within Stage backlog
authority — not product-scope expansion.

## PR #96 review-remediation cycle (follow-up Stage session)

Bounded review-remediation on PR #96 (head `chore/stage-047-S`). Copilot review
raised 7 actionable threads + 2 suppressed findings; all 9 corrected in the
Stage-owned decision / plan / backlog / memory artifacts (no source or config
code). Highlights:

* Deliberation B decision + risks: replaced the traversal short-circuit with the
  full error-observing streaming refactor and an eligible-file-before-later-error
  regression invariant (matches the corrected Plan B body).
* Plan B: Requirements Trace now points to the shared compiled matcher
  (aggregate warning semantics preserved); security-sensitive behavior marked
  **present**; `Requires plan hardening: yes`; `## Plan Hardening` section added
  preserving the fail-closed full-walk error observation.
* Spike + Plan A: F6 wording re-scoped from "single-process serving" to one
  owning `DataStore` guard (two independent same-process guards also reproduce the
  ordering window).
* Stash `5905CDEE`: reframed from a TOCTOU-vulnerable path re-check to an
  identity-bound / no-follow retained-handle restoration (fail closed where a
  handle cannot be retained).

A fresh multi-persona adversarial post-remediation re-review (>= 3 independent
cross-model reviewers) cleared all HIGH/MEDIUM P0/P1 findings. Details recorded in
`dark-stage-session-complete-memory.md`.

## Traceability

Every consolidated original is retained byte-for-byte under
`docs/archive/memory/2026-08-16/`. No content deleted.
