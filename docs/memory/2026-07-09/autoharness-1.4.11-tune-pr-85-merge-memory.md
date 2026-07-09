---
title: "Autoharness 1.4.11 harness tune + PR #85 merge"
date: 2026-07-09
release_unit: "chore/autoharness-tune-2026-07-08"
status: "merged"
---

## Summary

Upgraded the installed autoharness harness baseline from 1.4.5 to 1.4.11.
Escalated from a routine version-check into a full harness reconciliation
across ~90 tracked artifacts after discovering significant drift (including a
new P-014–P-017 governance framework the operator explicitly required be
adopted in the same PR/branch).

## Key Decisions

* Full adoption of P-014 (local-review-readiness merge gate), P-015
  (single-artifact shipment closure), P-016 (no parallel branch/worktree
  execution), P-017 (dark factory autonomy contract) — operator explicitly
  overrode an initial "defer to separate PR" instinct.
* Verified every `backlogit_*` tool reference against the live MCP registry
  rather than trusting naming-convention inference; fixed 7 mismatches.
* Installed the genuinely missing `.autoharness/backlog-registry.yaml` after
  the operator's intuition twice correctly challenged an initial
  "intentional absence" conclusion.

## Files Changed (final state)

* `.autoharness/harness-manifest.yaml` / `workspace-profile.yaml` — JSON→YAML
  migration, 14 agent path corrections, overlay path fixes.
* `.autoharness/config.yaml` — added `model_routing.adversarial_review.alt_provider`
  / `alt_family` (left empty/unconfigured).
* `.autoharness/backlog-registry.yaml` — newly installed (was missing).
* `.github/agents/.ship.agent.md`, `.stage.agent.md`, `_orchestrator.agent.md` —
  re-rendered from current templates, backlogit tool names corrected.
* `.github/agents/subagents/*.agent.md` — 14 files path-corrected, 4 new
  reviewer-persona agents installed that `review/SKILL.md` already referenced
  but were never installed.
* `AGENTS.md`, `.github/copilot-instructions.md`,
  `.github/instructions/constitution.instructions.md` — restored P-016/P-017
  references, added missing `graphtor-docs` and `adversarial-review` overlay
  sections.
* `.markdownlint.json` — `MD025.front_matter_title` disabled (root-cause fix,
  see compound entry).
* `.gitignore` — added `.autoharness/staging/`.

## Verification Performed

* `autoharness verify-workspace`: 0 blockers, 0 warnings, all 90 artifacts
  unchanged (final state).
* `verify-harness` multi-model adversarial review: 3 parallel reviewers,
  4 CRITICAL/MAJOR findings remediated (missing agents, missing overlay
  docs, stale manifest paths).
* Functional smoke test: `.Stage`, `.Ship`, `_Orchestrator` invoked read-only,
  all passed, confirmed backlogit tool-name fixes work in practice.
* markdownlint-cli2 (via public npm registry — corporate proxy returns
  E401): 0 errors on all touched files.

## PR #85 Lifecycle

* Created against `main` with a `## Local Review Readiness` block
  (`READY_WITH_FOLLOWUPS`, P0=0/P1=0).
* Copilot shadow review raised 5 findings post-creation:
  1. Doubled slash in `tests//integration/` / `tests//contract/` path
     guidance (`harness-architect/SKILL.md`).
  2. 8 empty template placeholders for `alt_review_provider`/
     `alt_review_family` (`adversarial-review.agent.md`).
  3. `plugin.json` `agents` list pointing at nonexistent files — repointed
     at `_orchestrator`, `.ship`, `.stage`.
  4. `model_family` mismatch (`.stage.agent.md` 4.6 vs `_orchestrator`'s
     tier3 default 4.8) — reconciled to 4.8.
  5. Misleading "files matching" wording in `review/SKILL.md`'s Security
     Reviewer routing clause — reworded to "content matching these
     patterns".
* All 5 fixed in commit `58b7195`, replies posted citing the fix commit,
  all 5 threads resolved via GraphQL `resolveReviewThread`.
* PR body's Local Review Readiness block updated to reference the post-fix
  HEAD before merge.
* Merge blocked by branch-protection `REVIEW_REQUIRED` (Copilot only
  commented, did not formally approve). Operator explicitly confirmed
  using `gh pr merge --admin` to bypass. Merged via merge commit
  `9047746` at 2026-07-09T08:02:00Z.

## Deliberately Deferred

* 14 pre-existing MD041 violations in untouched skill files.
* `model_routing.adversarial_review.alt_provider`/`alt_family` scaffolded
  but left unconfigured.
* Pre-existing `.autoharness/config.yaml` schema blockers (custom
  `graphtor-docs` capability pack not in schema enum; `model_routing`
  nested-object vs. schema's plain-string expectation).
* 2 MINOR overlay-coherence gaps: `learnings-researcher.agent.md` and
  `skill-search/SKILL.md` don't yet reference engram tools.

## Post-Merge Closure Notes

* No backlogit shipment/chore item was created for this work (it was
  ad-hoc operator-directed tuning, not routed through the Stage→Ship
  backlog pipeline) — `shipment-reconcile` does not apply; confirmed no
  matching shipment manifest exists for PR #85.
* `docs/memory/` at 36 files / ~109 KB — below `compact-context` mandatory
  thresholds (40 files / 500 KB); no compaction triggered this session.
* 3 compound learnings captured in `docs/compound/workflow-issues/`:
  MD025 front-matter-title false positive, harness-manifest stale-path
  silent-skip, install-harness backlog-registry step gap.

## Next Steps

* None outstanding for this release unit. Local `main` is synced to merge
  commit `9047746`. Feature branch `chore/autoharness-tune-2026-07-08`
  left in place (not deleted) per earlier operator preference during merge.
