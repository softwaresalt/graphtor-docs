# autoharness Tuning Report — 2026-07-08

**Workspace**: `c:\Source\GitHub\graphtor` (graphtor-docs)
**Branch**: `chore/autoharness-tune-2026-07-08` (created from `main`; do not commit tuning output directly to `main`)
**autoharness package**: already at latest PyPI release, `1.4.11` — no pip upgrade required
**Manifest version before tuning**: `1.4.5` (recorded `tuned_at: 2026-05-20`)
**Manifest version after tuning**: `1.4.11`

## Summary

The pip package was already current. The real drift was in the *installed workspace harness*:
the manifest/profile were in a legacy JSON format the current CLI can no longer read, several
agent files had been renamed upstream without the manifest being updated, and a handful of
instruction/skill files were missing small, genuinely new content. Upstream templates also
introduced a large new governance framework (P-014 rewrite + new P-015/P-016/P-017 policies)
that was deliberately **not** adopted — see "Excluded — Requires Separate Decision" below.

## Applied Changes

### 1. Manifest/profile format migration (breaking — required for any future `verify-workspace`/tune to work)

- Converted `.autoharness/harness-manifest.json` → `harness-manifest.yaml` and
  `workspace-profile.json` → `workspace-profile.yaml` (the CLI hardcodes `.yaml`; originals
  backed up to `.autoharness/backups/2026-07-08/`).
- Fixed a stale `workspace_path` in the profile (`D:\Source\GitHub\graphtor-docs` →
  `C:\Source\GitHub\graphtor`).
- Bumped `autoharness_version` to `1.4.11` and corrected `autoharness_home`.

### 2. Path/template corrections for 14 renamed agent files

Upstream renamed several installed agent files; the manifest still pointed at the old locations.
Updated manifest `path`/`template` fields to match reality (no file moves were needed — the
correct files already existed):

- `.github/agents/orchestrator.agent.md` → `.github/agents/_orchestrator.agent.md`
- `.github/agents/ship.agent.md` → `.github/agents/.ship.agent.md`
- `.github/agents/stage.agent.md` → `.github/agents/.stage.agent.md`
- `.github/agents/research/learnings-researcher.agent.md` → `.github/agents/subagents/learnings-researcher.agent.md`
- `.github/agents/security-sentinel.agent.md` → `.github/agents/subagents/security-sentinel.agent.md`
- `.github/agents/review/*.agent.md` (9 files) → `.github/agents/subagents/*.agent.md`

### 3. Genuine content fixes (small, low-risk, unrelated to the new governance framework)

- **`.github/instructions/circuit-breaker.instructions.md`** — added the missing "Cooldown and
  Auto-Reset (Optional)" section present in the current template.
- **`.github/skills/harness-doctor/SKILL.md`** — fixed the manifest-path reference
  (`.json` → `.yaml`) and adopted the improved Phase 2 version-check logic (resolves the
  *current* autoharness version live via `autoharness version` instead of comparing against a
  stale baked-in string — this directly fixes the exact stale-version problem this tuning run
  diagnosed).
- **`.github/instructions/mcp-server.instructions.md`** — updated the "Project Structure" listing
  to match the actual current `src/` tree (previously missing `lib.rs`, `lock.rs`, `cli/`, `db/`,
  `embed/`, `mcp/`, `parse/`, `pipeline/`, `query/`, `sync/`, `workspace/`).
- **`.github/instructions/graphtor-docs.instructions.md`** — added the missing paragraph
  documenting the `GRAPHTOR_EMBED_MODEL_DIR` environment variable / `.env.local` convention.
- **`.github/prompts/feature-flow.prompt.md`** — installed (new optional prompt, approved by
  operator; developer-friendly alias for the Orchestrator's standard sequential pipeline).

### 4. Manifest reconciliation (baseline refresh)

- Recomputed SHA-256 checksums for all 82 tracked artifacts against their actual current content
  and wrote them back into the manifest as the new baseline.
- Adopted the current `.autoharness/config.yaml` content as-is (it is intentionally
  operator-customized — model routing, capability packs, etc.) and refreshed `config_hash` /
  `profile_hash` to match.
- Added a manifest entry for the newly-installed `feature-flow.prompt.md`.
- Re-ran `autoharness verify-workspace --json`: **0 blockers, 0 warnings, all 82 tracked
  artifacts report `unchanged`.**

### 5. Housekeeping

- Added `.autoharness/staging/` to `.gitignore` (disposable `verify-workspace` output; was
  previously untracked and un-ignored).

## Excluded — Requires Separate Decision

While diffing every tracked artifact against its current template, a **large new governance
framework** was discovered upstream that this tuning run deliberately did **not** adopt, because
it changes real operating policy rather than fixing drift:

- **P-014 rewrite** — demotes GitHub Copilot review from the primary merge gate to an optional
  "advisory shadow review," replacing it with a **local review readiness** gate as the
  authoritative merge condition.
- **P-015 (new)** — Single-Artifact Shipment Closure (no cascade `ship_shipment`).
- **P-016 (new)** — No Parallel Branch/Worktree Execution (forbids parallel implementation
  worktrees workspace-wide, with a narrow Stage spike/research exception).
- **P-017 (new)** — "Dark Factory Autonomy Contract": an explicit bounded autonomous
  Stage → Ship execution and merge-approval mode, with its own trigger phrases, telemetry
  events, and visibility protocol.

Files that carry pieces of this framework and were **left unchanged**:
`.github/policies/workflow-policies.md`, `.github/instructions/constitution.instructions.md`
(one bullet under "Development Workflow"), `.github/instructions/github-pr-automation.instructions.md`,
`.github/instructions/concurrency.instructions.md`, `.github/instructions/agent-intercom.instructions.md`
(the "Dark Factory Visibility Protocol" section, which also references a not-yet-installed
`output-timestamps.instructions.md`), `.github/instructions/release-observability.instructions.md`
(the "Releasability Evidence Contract" section), and the skill files
`pr-lifecycle`, `review`, `runtime-verification`, `operational-closure`, `shipment-reconcile`,
`harness-architect`, `build-feature` (all reference the local-review-first / P-014 language).

**Recommendation**: run the `deliberate` skill (or an explicit conversation) to decide whether to
adopt this framework before the next tuning pass — it is a policy change, not a bug fix.

## Other Advisory Items (not installed)

- **`feature-flow-parallel.prompt.md`** — explicitly requires P-016 compliance; not installed
  because P-016 was not adopted (see above). Revisit if/when P-016 is adopted.
- **`feature-flow-dark.prompt.md`** — requires explicit P-017 opt-in; not installed.
- **Adversarial-review "Alternate Model Provider Support" + "Post-Remediation Re-Review"** —
  genuinely new, separate optional capabilities (`ALT_REVIEW_PROVIDER`/`ALT_REVIEW_FAMILY` are
  not configured for this workspace). Not adopted; would need new `config.yaml` fields if wanted.

## Pre-Existing Schema Findings (not introduced by this tuning run)

`verify-workspace` surfaced `strict_schema_blockers` against `.autoharness/config.yaml` that
predate this session (this config content was never touched):

- `capability_packs` includes `graphtor-docs`, which is not in the current
  `harness-config.schema.json` enum (`agent-intercom`, `agent-engram`, `backlogit`,
  `browser-verification`, `continuous-learning`, `strict-safety`, `release-observability`,
  `adversarial-review`). This appears to be a legitimate workspace-specific capability pack the
  schema enum doesn't yet accommodate.
- `model_routing.tier1` / `tier2` / `tier3` / `orchestrator` are nested objects
  (`{model, model_family, reasoning_effort, model_provider}`) but the schema expects a plain
  string. The richer object form is what the installed agent/skill templates actually consume
  (`TIER_1_FAMILY`, `TIER_1_PROVIDER`, etc. all resolve correctly from it), so this looks like
  the schema lagging the config convention rather than the config being wrong.

Neither finding blocks harness operation (`blockers: []`); both are flagged for awareness only.
`config.yaml` was intentionally left untouched since it is operator-customized.

## Next Steps

1. Review the diff on `chore/autoharness-tune-2026-07-08` and open a PR against `main`.
2. Decide on the P-014/P-015/P-016/P-017 governance framework adoption (separate from this PR
   recommended, given its scope).
3. Consider updating `harness-config.schema.json` upstream (or removing the custom capability
   pack) to resolve the `strict_schema_blockers` above.
