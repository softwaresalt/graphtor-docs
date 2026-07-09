# autoharness Tuning Report — 2026-07-08

**Workspace**: `c:\Source\GitHub\graphtor` (graphtor-docs)
**Branch**: `chore/autoharness-tune-2026-07-08` (created from `main`; do not commit tuning output directly to `main`)
**autoharness package**: already at latest PyPI release, `1.4.11` — no pip upgrade required
**Manifest version before tuning**: `1.4.5` (recorded `tuned_at: 2026-05-20`)
**Manifest version after tuning**: `1.4.11`

## Summary

Full adoption of the current upstream autoharness template set, including the new P-014
rewrite and new P-015/P-016/P-017 governance policies, per explicit operator direction.
`verify-workspace` reports **0 blockers, 0 warnings, all 85 tracked artifacts `unchanged`**
(new baseline established).

## Applied Changes

### 1. Manifest/profile format migration (breaking — required for `verify-workspace`/tune to work at all)

- Converted `.autoharness/harness-manifest.json` → `harness-manifest.yaml` and
  `workspace-profile.json` → `workspace-profile.yaml` (the CLI hardcodes `.yaml`; originals
  backed up to `.autoharness/backups/2026-07-08/`).
- Fixed a stale `workspace_path` in the profile (`D:\Source\GitHub\graphtor-docs` →
  `C:\Source\GitHub\graphtor`).
- Bumped `autoharness_version` to `1.4.11` and corrected `autoharness_home`.

### 2. Path/template corrections for 14 renamed agent files

Upstream renamed several installed agent files; the manifest still pointed at the old
locations. Updated manifest `path`/`template` fields and confirmed content parity:

- `.github/agents/orchestrator.agent.md` → `.github/agents/_orchestrator.agent.md`
- `.github/agents/ship.agent.md` → `.github/agents/.ship.agent.md`
- `.github/agents/stage.agent.md` → `.github/agents/.stage.agent.md`
- `.github/agents/research/learnings-researcher.agent.md` → `.github/agents/subagents/learnings-researcher.agent.md`
- `.github/agents/security-sentinel.agent.md` → `.github/agents/subagents/security-sentinel.agent.md`
- `.github/agents/review/*.agent.md` (9 files) → `.github/agents/subagents/*.agent.md`

### 3. Full governance framework adoption (P-014 rewrite + new P-015/P-016/P-017)

Adopted upstream's new local-review-first PR merge model and dark-factory autonomy
framework:

- **`.github/policies/workflow-policies.md`** — rewrote P-014 (local review readiness is
  now the authoritative merge gate; Copilot/GitHub-hosted review is optional advisory
  shadow review), added **P-015** (single-artifact shipment closure, no cascade
  `ship_shipment`), **P-016** (no parallel branch/worktree execution), and **P-017**
  (dark factory autonomy contract). Amendment-log dates: preserved `2026-05-20` for
  pre-existing P-001–P-013 rows; used `2026-07-08` for the P-014 rewrite and new
  P-015/P-016/P-017 rows.
- **`.github/instructions/github-pr-automation.instructions.md`** — rewrote to the
  local-review-first / shadow-review model with the §1.9 Pre-Merge Review Readiness gate.
- **`.github/instructions/concurrency.instructions.md`** — adopted P-016-aware wording
  (branch/worktree boundary section).
- **`.github/instructions/constitution.instructions.md`** and **`AGENTS.md`** — restored the
  "Single active implementation branch/worktree" and "Dark factory mode (P-017)"
  Development Workflow bullets; fixed stale `.github/agents/review/` → `.github/agents/subagents/`
  references in the deprecated-agents table.
- **`.github/instructions/agent-intercom.instructions.md`** — added the "Dark Factory
  Visibility Protocol" section and the `output-timestamps.instructions.md` cross-reference.
- **`.github/instructions/output-timestamps.instructions.md`** — installed (new artifact;
  timestamp-format authority referenced by agent-intercom).
- **`.github/instructions/release-observability.instructions.md`** — added the
  "Releasability Evidence Contract" section.
- **`.github/instructions/adversarial-review.instructions.md`** and
  **`.github/agents/subagents/adversarial-review.agent.md`** — added "Alternate Model
  Provider Support" and "Post-Remediation Re-Review". Added a new
  `model_routing.adversarial_review.alt_provider` / `alt_family` section to
  `.autoharness/config.yaml` (both empty — no alternate provider configured yet; the docs
  read sensibly in that state and can be filled in later to enable it).
- **Skills fully re-rendered** to the current local-review-first / P-014–017-aware
  templates: `pr-lifecycle`, `review`, `runtime-verification`, `operational-closure`,
  `shipment-reconcile`, `harness-architect`, `build-feature`.
- **Core pipeline agents fully re-rendered**: `.github/agents/.ship.agent.md`,
  `.github/agents/.stage.agent.md`, `.github/agents/_orchestrator.agent.md` (these were
  missed in the first pass because the manifest's old paths pointed at nonexistent files;
  the diff tool never actually compared their real content until the path fix was
  cross-checked against upstream's P-016/P-017 references).
- **Review persona agents fully re-rendered**: `agent-native-parity-reviewer`,
  `architecture-strategist`, `concurrency-reviewer`, `constitution-reviewer`,
  `scope-boundary-auditor`, `security-lens-reviewer`, `security-reviewer`,
  `technology-reviewer`, plus `learnings-researcher` and `security-sentinel`.
- **New prompts installed**: `feature-flow.prompt.md` (sequential default),
  `feature-flow-parallel.prompt.md` (P-016-compliant planning overlap — now installable
  since P-016 is adopted), `feature-flow-dark.prompt.md` (P-017 dark-factory entrypoint —
  now installable since P-017 is adopted).

**Caveat on inferred backlogit operation names**: `.ship.agent.md` and `.stage.agent.md`
reference several backlogit MCP operations not previously documented in this workspace's
attached instruction set (checkpoint save/get/list/resolve, shipment claim/create/add-to,
hook event poll/ack). These were rendered using the `backlogit_<verb>_<noun>` naming
convention already confirmed elsewhere (e.g., `backlogit_create_item`,
`backlogit_move_item`, `backlogit_archive_item`). **Verify these exact tool names against
the live backlogit MCP tool registry** (e.g., via `backlogit_export_command_map` or
`backlogit_get_metadata_catalog`) before relying on them operationally.

**Caveat on inferred backlogit operation names — RESOLVED**: `.ship.agent.md` and
`.stage.agent.md` originally referenced several backlogit MCP operations inferred from
naming convention rather than confirmed. This was verified against the live backlogit
tool registry via `backlogit_get_metadata_catalog` and `backlogit_export_command_map`,
and 4 incorrect references were found and corrected:

| File | Wrong (as first written) | Corrected to |
|---|---|---|
| `.ship.agent.md` | `backlogit_save_checkpoint` | `backlogit_create_checkpoint` |
| `.ship.agent.md` | `backlogit_get_queue` (for shipment lookup) | `backlogit_list_shipments` |
| `.stage.agent.md` | `backlogit_save_checkpoint` | `backlogit_create_checkpoint` |
| `.stage.agent.md` | `backlogit_get_queue` (for shipment lookup) | `backlogit_list_shipments` |
| `.stage.agent.md` | `backlogit_update_item` (for adding shipment items) | `backlogit_add_to_shipment` |
| `.stage.agent.md` | `backlogit_get_item` (for reading back a shipment) | `backlogit_get_shipment` |
| `_orchestrator.agent.md` | `backlogit_get_queue` (for active/queued shipment checks) | `backlogit_list_shipments` |

All other inferred names (`backlogit_ack_hook_events`, `backlogit_poll_hook_events`,
`backlogit_claim_shipment`, `backlogit_create_shipment`, `backlogit_cleanup_checkpoints`,
`backlogit_get_checkpoint`, `backlogit_list_checkpoints`, `backlogit_resolve_checkpoint`,
`backlogit_sync_index`, `backlogit_archive_item`, `backlogit_move_item`,
`backlogit_create_item`, `backlogit_search_items`, `backlogit_get_item`,
`backlogit_ship_shipment`) matched the live registry exactly. A final cross-check of every
`backlogit_*` reference across all touched files against the full confirmed 58-tool
registry found zero remaining unknown names.

### 4. Other genuine content fixes (unrelated to the governance framework)

- **`.github/instructions/circuit-breaker.instructions.md`** — added the missing "Cooldown
  and Auto-Reset (Optional)" section.
- **`.github/skills/harness-doctor/SKILL.md`** — fixed the manifest-path reference
  (`.json` → `.yaml`) and adopted live `autoharness version` CLI resolution instead of a
  stale baked-in version string.
- **`.github/instructions/mcp-server.instructions.md`** — updated "Project Structure" to
  match the actual current `src/` tree.
- **`.github/instructions/graphtor-docs.instructions.md`** — added the missing
  `GRAPHTOR_EMBED_MODEL_DIR` paragraph.

### 5. Manifest reconciliation

- Recomputed SHA-256 checksums for all 85 tracked artifacts against actual current content.
- Adopted current `.autoharness/config.yaml` as-is (operator-customized model routing,
  capability packs, etc. — plus the new `adversarial_review` section above) and refreshed
  `config_hash`.
- Final `verify-workspace --json`: **0 blockers, 0 warnings, all 85 artifacts `unchanged`,
  0 remaining new-artifact candidates.**

### 6. Housekeeping

- Added `.autoharness/staging/` to `.gitignore` (disposable `verify-workspace` output).

## Pre-Existing Schema Findings (not introduced by this tuning run)

`verify-workspace` surfaces `strict_schema_blockers` against `.autoharness/config.yaml`
that predate this session:

- `capability_packs` includes `graphtor-docs`, which is not in the current
  `harness-config.schema.json` enum. This is a legitimate workspace-specific capability
  pack the schema enum doesn't yet accommodate.
- `model_routing.tier1` / `tier2` / `tier3` / `orchestrator` / (now also)
  `adversarial_review` are nested objects but the schema expects a plain string. The
  richer object form is what the installed agent/skill templates actually consume
  (`TIER_1_FAMILY`, `TIER_1_PROVIDER`, etc. all resolve correctly from it), so this looks
  like the schema lagging the config convention rather than the config being wrong.

Neither finding blocks harness operation (`blockers: []`); both are flagged for awareness
only.

## Next Steps

1. Review the diff on `chore/autoharness-tune-2026-07-08` and open a PR against `main`.
2. Decide whether to configure `model_routing.adversarial_review.alt_provider`/`alt_family`
   to activate the new alternate-reviewer-provider capability, or leave it disabled.
3. Consider updating `harness-config.schema.json` upstream (or removing the custom
   capability pack) to resolve the pre-existing `strict_schema_blockers`.
