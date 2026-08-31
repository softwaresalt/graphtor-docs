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

## Next steps

1. Open a PR from `chore/autoharness-merge-install-1.5.0`.
2. Decide whether to wire `scripts/ci-topology-check.sh` into CI (installed
   additively, not referenced by any workflow).
3. Decide whether `.github/prompts/ping-loop.prompt.md` should be deleted now
   that no 1.5.0 content references it.
4. Consider adopting the new `pre-push-quality-gates` hook (installed, opt-in;
   profile sets `pre_push_gates: [format, lint, test]`).
