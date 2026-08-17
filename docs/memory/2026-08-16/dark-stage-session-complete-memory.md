---
title: "Dark-Stage session complete — shipments 047-S / 048-S ready for Ship"
type: session-memory
date: 2026-08-16
agent: Stage
mode: P-017 dark-factory (DARK_MODE_ACTIVE)
status: complete
tags:
  - stage
  - dark-mode
  - session-end
  - handoff
---

## Outcome

Full Stage pipeline executed over the four scoped stash entries. Two queued
shipments produced (not claimed). No PRs created/pushed (Stage boundary honored).
Intercom unavailable throughout — degraded local visibility logged; all events
recorded here instead of broadcast.

## Stash disposition (traceability map)

| Stash | Kind | Disposition | Became |
|---|---|---|---|
| 970AE45A | spike | archived (consumed) | spike findings + feature 054-F (Group A) |
| 5D98DBCC | feature | archived (consumed) | external-path REJECTED (III/IV); reshaped into 054-F; deferred F1CE20EC/5905CDEE |
| B88E37BF | task | archived (consumed) | task 055.001-T (Group B) |
| 5868A7C5 | task | archived (consumed) | task 055.002-T (Group B) |
| 5905CDEE | bug | NEW active (deferred) | symlink-swap TOCTOU — future spike |
| F1CE20EC | feature | NEW active (deferred) | Option C shared-serve + true F6 fix — needs operator go |

## Shipments (queued, not claimed) — safe execution order

* **047-S** "Read-only serve guarantee honesty (F2/F6)" — priority medium (first).
  * 054-F (covering) → 054.001-T (A1 code, honest surfaces + characterization tests) → 054.002-T (A2 docs). Dep: 054.002-T blocks-on 054.001-T.
* **048-S** "Serve auto-discovery follow-ups (PR90 deferrals)" — priority low (second).
  * 055-F (covering) → 055.001-T (B1 code, streaming memory reduction) , 055.002-T (B2 alias evaluation). B1 and B2 independent.

Recommended order: 047-S before 048-S (reliability/correctness priority). The two
shipments are technically independent (store.rs vs serve_discovery.rs) — no hard
cross-shipment dependency was encoded because none exists.

## Review consensus

4 independent cross-model reviewers (anthropic/google/openai/xai) + 3-reviewer
post-remediation re-review. reject-external-path: HIGH-confidence PASS. Both plans
FAILed round 1 (my initial designs were flawed: Plan A wrongly overloaded
is_engine_enforced_readonly(); Plan B short-circuit broke fail-closed → posture
escalation) and PASSed after one remediation cycle. All HIGH/MEDIUM P0/P1 resolved
before shipment creation. Residual items are P3 advisories folded into the plans.
Full record in each plan's appended `## Plan Review` and in
`dark-stage-review-outcome-checkpoint.md`.

## Incidental repair (logged)

`.backlogit/archive/013-S.md` had pre-existing malformed YAML that blocked the
mandatory Step 0.1 index sync. Repaired minimally (relocated shipped_at/pr/
merge_commit under custom_fields). Operational unblocking of a mandatory gate,
within Stage backlog authority — not product-scope expansion.

## Commit / dirty state

Commit (Stage-owned): docs/decisions (3), docs/exec-plans (2), docs/memory
(2026-08-16, 3), .backlogit/{archive/013-S.md, archive/stash.jsonl, stash.jsonl,
hooks_queue.jsonl, queue/047-S,048-S,054-F,054.001-T,054.002-T,055-F,055.001-T,
055.002-T}. Not pushed.

Left uncommitted (stowaway, per contract): .autoharness/config.yaml,
.github/agents/{.ship,.stage,_orchestrator}.agent.md, .gitignore,
.vscode/settings.json. Also left untracked: .backlogit/runtime/ (ephemeral hook
consumer checkpoint).

## Hard blocks / notes for Ship

* None blocking. Both shipments are ready to claim.
* 047-S is security-adjacent (read-only contract). Ship should run the full quality
  gate; task 054.001-T requires the is_engine_enforced_readonly() call-site audit
  and must NOT overload the predicate.
* 048-S task 055.001-T is safety-critical behavior-preservation (classifier gates
  read-only vs read-write posture) — the full fail-closed walk MUST be retained.
* Deferred security bug 5905CDEE (symlink TOCTOU) is real; triage when convenient.
* compact-context: INVOKED (mandatory at session/phase completion — not
  threshold-gated; the earlier "skipped: under 40-file/500KB trigger" rationale
  was wrong and was corrected during PR #96 review). Consolidated the two
  2026-08-16 intermediate checkpoints into
  `docs/memory/compacted/2026-08-16-dark-stage-047-048-compacted.md`; verbose
  originals archived under `docs/archive/memory/2026-08-16/`. This session-complete
  memory is preserved in place as the active 047-S/048-S handoff.

## Second Copilot review cycle (2026-08-16, cycle 2 of <=3) — 048-S plan/backlog

PR #96 (staging PR, HEAD ad1b836) received a SECOND Copilot review with 5
suppressed actionable comments (no live threads), all on the 048-S release unit.
Remediated in Stage-owned artifacts only (no source/config/cargo/branch/PR):

| # | Finding | Fix |
|---|---|---|
| 1 | plan wrongly marked public-API impact "absent" — shared matcher crosses the binary/library crate boundary | plan now marks it present (additive): the `graphtor-docs` binary classifier reuses a NEW additive `FileFilter` (new/is_match) public API in the `graphtor_core` library `acquire` module; `filter_files` refactored to consume it (single source of truth); SemVer-minor; rustdoc + library unit test + differential test added |
| 2 | no post-deploy observation despite read-only vs read-write posture gating | added a bounded manual Post-Deploy Observation Window (owner, per-source baseline, bidirectional rollback trigger, revert procedure, window-close outcome) |
| 3 | task 055.001-T acceptance criteria lacked exact warning cases | AC5 now carries all four: excluded-only = 1 aggregate warning; ingestible = 0; walk-error = 0 (fail-closed); zero-candidate = 0 |
| 4 | feature 055-F risk mislabeled | priority stays low; body carries ActionRisk moderate (security-sensitive) — misclassification promotes to read-write Generation |
| 5 | deliberation classified B/B1 as uniformly low-risk | Decision + Risks + Trade-off table now classify B/B1 moderate/security-sensitive (B2 low); blast-radius statement corrected for the `filter_files` library refactor |

Coherence: task 055.001-T (and sibling 055.002-T) priority aligned to low; the task
carries a decomposition note (split into 055.001.001-ST library-first +
055.001.002-ST binary if it exceeds the 2-hour rule); all revert/rollback language
reconciled with a possible two-commit B1 decomposition.

Adversarial post-remediation review: 3 independent reviewers, cross-model (openai
gpt-5.6-sol Correctness, anthropic claude-opus-4.8 Constitution, google
gemini-3.1-pro Scope) plus a focused re-review pass. Consensus: NO HIGH/MEDIUM
P0/P1; all 5 findings ADDRESSED; Stage boundary honored. Two internal remediation
passes used (pass 2 fixed a re-review-surfaced revert/blast-radius contradiction).
One LOW-confidence P3 advisory remains (optional upfront task pre-split) —
recorded and accepted; both final reviewers deemed the decomposition note
sufficient.

Validation: backlogit doctor pass on 055-F / 055.001-T / 055.002-T; index synced;
docs-lint shows only pre-existing repo-wide frontmatter gaps (doc_type/source),
none introduced by these edits.

Commit (this cycle, Stage-owned only): docs/exec-plans/2026-08-16-serve-auto-discovery-followups-plan.md,
docs/decisions/2026-08-16-serve-auto-discovery-followups-deliberation.md,
.backlogit/queue/{055-F,055.001-T,055.002-T}.md. Not pushed. Stowaways
(.autoharness/config.yaml, .github/agents/{.ship,.stage,_orchestrator}.agent.md,
.gitignore, .vscode/settings.json) plus .backlogit/runtime/ left uncommitted per
contract. Shipment 048-S manifest unchanged (still 055-F, 055.001-T, 055.002-T).

Note for Ship: 055.001-T now requires an additive `graphtor_core::acquire` public
API (`FileFilter`) reused by both the classifier and a refactored `filter_files`;
red-first for the new API, characterization-first for the classifier; consider the
documented library/binary subtask split if the combined change exceeds 2 hours.

## Third Copilot review cycle (2026-08-16, cycle 3 of <=3, FINAL) — 047-S harvest traceability + 054-F priority

PR #96 (staging PR, HEAD 69b9ed0e) received a THIRD Copilot pass: no live threads,
two suppressed actionable findings, both on the 047-S / Group A release unit and its
harvest traceability. Remediated in Stage-owned artifacts only (no source/config/
cargo/branch/PR/push):

| # | Finding | Fix |
|---|---|---|
| 1 | `.backlogit/archive/stash.jsonl` consumed entries lacked machine-readable harvest metadata; 054/055 targets lacked `source_stash_id` | Set the four archived source entries to `reason: harvested` + `harvested_artifact_id` (canonical form, matches E8F043DD->053-F / 2D49BDDF->052.001-T), and added the reverse `source_stash_id(s)` to each target frontmatter |
| 2 | `.backlogit/queue/054-F.md` lacked `priority` | Added `priority: medium`, consistent with tasks 054.001-T / 054.002-T (medium) and shipment 047-S (medium) |

Exact harvest-link representation (bidirectional; committed files are the source of
truth):

* stash.jsonl (stash -> target): `"reason":"harvested","harvested_artifact_id":"<id>"`
  for 5D98DBCC->054-F, 970AE45A->054-F, B88E37BF->055.001-T, 5868A7C5->055.002-T.
* frontmatter (target -> stash): 054-F `custom_fields.source_stash_ids: [5D98DBCC,
  970AE45A]` (plural, matches 037-F); 055.001-T `source_stash_id: B88E37BF`;
  055.002-T `source_stash_id: 5868A7C5` (singular, matches 052.001-T). Bumped
  `updated_at` on the three edited items to reflect the edit (canonical:
  016-F / 037-F / 053-F all carry updated_at > created_at). No execution dependencies
  invented; existing parent links unchanged.

Validation: index synced (449 items). `backlogit_query_sql` proves 054-F
priority=medium + source_stash_ids=["5D98DBCC","970AE45A"]; 055.001-T
source_stash_id=B88E37BF; 055.002-T source_stash_id=5868A7C5; all three updated_at >
created_at. Targeted `backlogit_doctor` PASS on 054-F / 055.001-T / 055.002-T;
workspace doctor shows only the pre-existing, unrelated orphan 013.008-T.
stash.jsonl re-validated: 50/50 lines valid JSON, LF + trailing newline preserved,
exactly 4 lines changed. Note: the durable db tables `stash_links` / `stash_entries`
(inside gitignored `.backlogit/backlogit.db`) reflect only the two originally
harvest_stash'd task links (B88E37BF, 5868A7C5); the committed truth is the
frontmatter + archive JSONL — the same shape 037-F's plural sources (3FE2DDFB /
0D214027) rely on. No supported, committable operation records a stash_link for an
archived entry into an existing item, and the db cache must not be hand-edited.

Adversarial review: 3 independent cross-model reviewers — Correctness (anthropic
claude-sonnet-4.6), Backlog/Schema-integrity (google gemini-3.1-pro), Scope/
Constitution (anthropic claude-opus-4.8). Consensus: NO HIGH/MEDIUM P0/P1; all PASS.
Correctness + schema returned NO FINDINGS. Scope PASSed with a P1-HIGH
commit-discipline caution (the six stowaways are NOT gitignore-protected -> stage
explicit paths only) — honored via explicit-path staging + post-stage `git status`
verification — and a P3-MEDIUM audit-trail note (updated_at not bumped) which was
remediated in one internal pass. Stage Role Boundary honored; P-009 / P-010 / P-016
all clear.

Commit (this cycle, Stage-owned only): `.backlogit/archive/stash.jsonl`,
`.backlogit/queue/{054-F,055.001-T,055.002-T}.md`, and this memory file. Not pushed.
Stowaways (`.autoharness/config.yaml`, `.github/agents/{.ship,.stage,_orchestrator}.agent.md`,
`.gitignore`, `.vscode/settings.json`) and `.backlogit/runtime/` left untouched and
uncommitted per contract; pre-existing gitignored `.*.lock` files left in place (not
Stage-created). Shipment manifests 047-S / 048-S unchanged. Cycle 3 of 3 is the final
permitted Copilot review-fix cycle.
