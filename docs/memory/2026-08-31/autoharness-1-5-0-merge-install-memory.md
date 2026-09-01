---
type: session-memory
title: "Merge-install autoharness 1.4.11 -> 1.5.0 via reconstructed-baseline three-way merge"
timestamp: "2026-08-31T17:07:12Z"
date: "2026-08-31"
agent: "auto-mergeinstall"
skill: "direct (install-harness bypassed in favor of a custom three-way merge)"
status: "complete; pushed, PR not yet opened"
branch: "chore/autoharness-merge-install-1.5.0"
commit: "efe1b5f"
---

## Outcome

Merge-installed autoharness **1.5.0** over the workspace's **1.4.11** harness.
108 files changed: 55 refreshed, 13 new, 2 renamed.

| Metric | Before | After |
|---|---|---|
| Blockers | 3 | 0 |
| Migration proposals | 4 | 1 (ambiguous / manual-review) |
| Uninstalled templates | 20 | 0 |
| Managed artifacts | 90 | 109 |

## Why a hand-built merge instead of the install-harness skill

`autoharness verify-workspace` renders a **preview-only** staging tree. That render
does not know the values a previous install's LLM resolved, so it regresses
agent-authored content back to `{{PLACEHOLDER}}` (96 unresolved on first run).
Copying staging output into the workspace would have degraded the harness.

The install path was therefore replaced with a genuine three-way merge:

1. `pip install --target %TEMP%\ah1411 autoharness==1.4.11` to reconstruct a real
   merge base.
2. Recover the original variable map by aligning each installed artifact against
   its template with `difflib.SequenceMatcher` and reading `replace` opcodes
   (152 resolved, 2 hand-fixed conflicts).
3. Render 1.4.11 (base) and 1.5.0 (upstream) with that same map — both to zero
   unresolved placeholders.
4. Line-merge base/local/upstream over base coordinates.

Base reproduced the installed harness exactly for 60/100 artifacts, which is the
evidence that the recovered map was faithful; the remaining drift was real local
modification.

## The critical merge rule

Capability-pack **overlay sections** (in `constitution.instructions.md`,
`copilot-instructions.md`, `AGENTS.md`) are authored by the installer LLM at
install time — the registry only names an `overlay_instruction` file per pack.
They exist in installed files but in **no template**, so a naive merge reads them
as "local noise" and drops them.

Fix: for **pure-insertion regions** (`i1 == i2` on both sides) keep upstream
lines *and* local-only lines; only true rewrites let upstream win. Conflicts fell
7 -> 4, and the adversarial-review / graphtor-docs overlays, the `MD025`
markdownlint rule, and the shipment-reconcile evidence-source table were all
verified preserved.

## Decisions worth remembering

* **Agent renames** `.ship`/`.stage` -> `_ship`/`_stage` were done with `git mv`
  so history follows. Their 1.4.11 templates had different names, so a
  `BASE_RENAMES` map was needed or the merge silently skipped them.
* **Escalation config ambiguity**: 1.5.0 fails closed when both the legacy flat
  `escalation` block and nested `stage.escalation`/`ship.escalation` are present.
  Legacy keys were set to empty strings.
* **`ping-loop.prompt.md`** lost its upstream template. Dropped from the manifest
  (now workspace-local) rather than deleted.
* **10 deliberate skips**: `.github/workflows/ci.yml` (project-owned Rust CI with
  `schedule` + `cargo audit` beats generic scaffolding), `.env.local` (holds
  `TAVILY_API_KEY`), browser-verification + continuous-learning instructions
  (packs disabled), technology-go/python/typescript, and 3 backlog.md scaffolding
  files (workspace uses backlogit's own `config.yaml`/`stash.jsonl`/`registry.yaml`).

## Behavior change to watch

`shipment-reconcile/SKILL.md` step 8: upstream replaced the local Path A/B
evidence procedure with a `move --status shipped` -> `archive` -> verify
`archived_status` sequence plus `RECONCILE_FAIL_SHIPMENT_RECORD_PROVENANCE`.
The per-item Path A/B discipline in step 4 and the evidence-source table survived.

## Verification performed

Pipeline-topology gate PASS; skill-search returns the new `doc-review` skill;
8 PowerShell + 8 bash scripts parse clean; markdownlint 0 issues on 36
instruction files; 0 stale `.ship`/`.stage` references; manifest, profile, and
config all schema-valid; remaining 75 `{{...}}` confirmed template-intended
runtime fill-ins (each file's count is at most its own template's count).

## Known non-issues

* 2 P1 portability warnings on `_orchestrator.agent.md` are byte-identical to the
  upstream 1.5.0 template (false positives on its documented `~/.autoharness/`
  resolution-order fallback).
* `verify-workspace` still reports 67 unresolved placeholders — that is its own
  preview render, not the installed files.
* MD041 fires on 15 `SKILL.md` files that open with `##`. Pre-existing: 14 such
  files at HEAD before this change. Upstream template convention; not a regression.

## Follow-up: agent-intercom pack removal (`d6dd31d` + `87fc508`)

The operator asked to remove `agent-intercom` from the installed capability
packs. No autoharness subcommand manages pack selection, so this was done
manually.

**Scope determination.** 37 files mentioned `intercom`. Comparing installed
reference counts against the 1.5.0 templates showed near-exact parity
(`_ship` 29/30, `_stage` 13/13, `_orchestrator` 11/11,
`copilot-instructions.md` 9/9, `AGENTS.md` 8/8, `constitution` 5/5, skills
identical). Diffing `AGENTS.md` against its template confirmed the
`### Capability Overlay — agent-intercom` block sits at the *same line numbers*
upstream, and `copilot-instructions.md` documents all nine packs — including
ones this workspace never enabled.

**Conclusion: those references are template-native conditional guards**
("When the `agent-intercom` capability pack is installed…"). They self-disable
via the pack list and were deliberately left untouched — editing them would
diverge from upstream and force a manual conflict on every future
tune/merge-install.

**Actual removal surface (5 files):**

* `.autoharness/config.yaml` — dropped from `capability_packs`
* `.autoharness/harness-manifest.yaml` — dropped from `capability_packs`,
  `capability_pack_overlays`, and the artifact entry
* `.autoharness/workspace-profile.yaml` — dropped from
  `harness_recommendations.capability_packs`, the recommendation rationale, and
  the `agent_intercom:` detection block
* deleted `.github/instructions/agent-intercom.instructions.md` (pack overlay
  instruction)
* deleted `.github/prompts/ping-loop.prompt.md` (intercom-only; upstream
  removed it in their own `ping-loop-removal-acp-consolidation` work, so it had
  no 1.5.0 template and was already unmanaged)

**Stale-justification note.** The profile claimed the pack was recommended
`because: agent-intercom MCP server configured in .vscode/mcp.json`. That file
does not exist, and root `.mcp.json` registers only
`backlogit, engram, context7, tavily, github`. The detection block asserting
`detected: true, mcp_configured: true` was therefore factually wrong and was
removed rather than merely deselected.

**Verification.** `profile_hash` / `config_hash` recomputed; both YAML files
re-validated against their JSON schemas; `verify-workspace` reports blockers 0,
warnings 2 (the known upstream `_orchestrator` false positives), rendered
artifacts 109 → 108, and — the decisive signal — **0 uninstalled templates**,
confirming the deselection propagated so the renderer no longer expects the
intercom instruction file. `gate pipeline-topology` PASS.

### Gotcha: PowerShell parse error silently split the commit

The first commit attempt used a bash heredoc (`git commit -F - <<'EOF'`).
PowerShell has no heredoc, and the resulting **parse** error aborts the entire
script block *before any statement runs* — so the `git add` on the preceding
line never executed. The two deletions were already staged by `git rm`, so
`git commit` still succeeded and produced `d6dd31d` containing **only the
deletions**. The branch was briefly in a worse state than either endpoint: the
instruction file was gone while the pack was still listed as enabled.

Caught by re-checking `git status` after the "done" report; fixed in `87fc508`.

**Lesson:** a PowerShell parse error is all-or-nothing for the whole block —
never assume earlier commands in a failed block ran. After any commit, verify
with `git show --stat <sha>` and `git show HEAD:<file>` rather than trusting a
clean-looking working tree, since staged-but-uncommitted and
never-staged changes look identical in a filtered `git status`.

## Next steps

1. Open a PR from `chore/autoharness-merge-install-1.5.0`.
2. Decide whether to wire `scripts/ci-topology-check.sh` into CI (installed
   additively, not referenced by any workflow).
3. ~~Decide whether `.github/prompts/ping-loop.prompt.md` should be deleted.~~
   Done in `d6dd31d` alongside the agent-intercom pack removal.
4. Consider adopting the new `pre-push-quality-gates` hook (installed, opt-in;
   profile sets `pre_push_gates: [format, lint, test, build]`).
5. `copilot_review.enforcement` stays `auto` (max wait 900s) — operator
   confirmed on 2026-08-31 that P-018 blocking on unresolved Copilot threads is
   the desired behavior. No change required.
