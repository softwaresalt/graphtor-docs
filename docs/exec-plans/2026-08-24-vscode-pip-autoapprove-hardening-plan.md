---
title: "Implementation Plan: Harden .vscode/settings.json pip auto-approval (9CEC208C)"
description: "Replace the blanket chat.tools.terminal.autoApprove pip:true entry with a narrowly-scoped anchored regex (matchCommandLine:true), or remove it if no concrete pip command is required, restoring least-privilege for the terminal auto-approve allow-list"
doc_type: "exec-plan"
source: "docs/decisions/2026-08-24-vscode-pip-autoapprove-hardening-deliberation.md"
stash_ids:
  - "9CEC208C"
deliberation_id: "003-DL"
tags:
  - security
  - config
  - vscode
  - auto-approve
  - hardening
  - chore
---

## Problem Frame

`.vscode/settings.json` defines `chat.tools.terminal.autoApprove`, the VS Code
terminal auto-approve allow-list used during AI agent sessions. It currently
holds four entries:

1. `/^python \.scripts/clone_ms_docs_repos\.py https://api\.github\.com/orgs/MicrosoftDocs ms-docs\.txt 2>&1$/` → `{ approve: true, matchCommandLine: true }`
2. `"pip": true`  ← **the defect**
3. `/^python \.scripts/clone_ms_docs_repos\.py https://api\.github\.com/orgs/MicrosoftDocs ms-docs\.txt$/` → `{ approve: true, matchCommandLine: true }`
4. `/^python \.scripts/generate_clone_scripts\.py$/` → `{ approve: true, matchCommandLine: true }`

Entries 1, 3, 4 are anchored regexes gated by `matchCommandLine: true` — they
only auto-approve one exact command line each. Entry 2, the bare string key
`"pip"` with value `true`, is matched as a substring against the command and
therefore auto-approves **any** command line containing `pip` (e.g.
`pip install <attacker-package>`). Because pip executes arbitrary build-backend
/ `setup.py` code at install time, this is effectively blanket remote-code-execution
approval inside agent sessions. Confirmed pre-existing (predates the 047-S
stowaway carry-over per `git show 33bbb37:.vscode/settings.json`).

This plan restores least-privilege: no terminal command is auto-approved unless
it matches a specific, reviewed, anchored pattern.

## Requirements Trace

| Source requirement | Implementation action |
|---|---|
| No blanket `"pip": true` (or bare-substring) entry remains | Remove the `"pip": true` key from the auto-approve map |
| Any retained pip auto-approval is an anchored regex + `matchCommandLine: true` | If a concrete, reviewed pip command line is required, add it as one anchored `/^...$/` key mirroring entries 1/3/4; otherwise omit (Option-A fallback) |
| File remains valid JSON; three existing patterns preserved byte-for-byte | Edit only the `pip` entry; re-validate JSON after the edit |
| General `pip install <anything>` is NOT auto-approved | Verify resulting allow-list has no substring/prefix pip entry |

## Implementation Units

### Unit 1 (config) — Replace blanket pip auto-approval with least-privilege allow-list entry

* **Domain:** config (single file, single logical change) — width-isolated.
* **Files affected:** `.vscode/settings.json` (1 file).
* **Change:** Remove the `"pip": true` key from `chat.tools.terminal.autoApprove`.
  Determine whether the documented `.scripts/*.py` clone workflow actually
  requires a specific pip command auto-approved:
  * If **yes**: add exactly one anchored `/^<exact reviewed pip command line>$/`
    key with `{ "approve": true, "matchCommandLine": true }`, mirroring entries
    1/3/4.
  * If **no** (expected — the documented workflows invoke `python .scripts/*.py`,
    not `pip`): remove the entry outright (Option-A fallback) and add no
    replacement.
* **Verification:**
  * `.vscode/settings.json` parses as valid JSON (e.g. `python -c "import json, pathlib; json.loads(pathlib.Path('.vscode/settings.json').read_text())"` or an editor/JSON linter).
  * The three existing anchored patterns are byte-for-byte unchanged (diff review).
  * No bare-substring / prefix `pip` key remains; a hypothetical
    `pip install requests` command line does not match any auto-approve entry.
* **Execution posture:** characterization-first — read current file, capture the
  exact existing patterns, then apply the minimal edit and re-validate. No source
  compilation involved; this is a config-only change so `cargo` gates are N/A
  except as a no-op sanity build if desired.

## Dependency Graph

Single implementation unit; no intra-plan dependencies. No dependency on other
backlog items, and explicitly isolated from E86A6E56, 5905CDEE, 8C2E313D, 049-S,
and unrelated stash entries.

## Decisions and Rationale

* **Prefer anchored regex + `matchCommandLine: true`, fall back to removal.**
  Matches the operator recommendation and the in-file convention; removal is the
  correct endpoint when no concrete pip command needs auto-approval, avoiding a
  speculative pattern.
* **Classify as a chore.** Security/config hygiene with no net-new product
  capability, shipped as one coordinated release unit.
* **Keep the diff minimal.** Touch only the `pip` entry to avoid regressions in
  the three reviewed patterns.

## Risks and Caveats

* **Silent dependence on pip auto-approval:** a workflow may have relied on the
  blanket entry. Mitigation: documented workflows use `python .scripts/*.py`;
  if a concrete pip command surfaces during implementation, re-add it as one
  anchored pattern.
* **JSON breakage / accidental edit of other patterns.** Mitigation: JSON
  re-validation and byte-for-byte diff review of the preserved patterns are
  acceptance criteria.
* **Scope creep** into other settings/security items. Mitigation: scope frozen
  to 9CEC208C.

## Plan Hardening Signals (REQUIRED)

* Public API, schema, or contract change — **absent**. No code contract changes.
* Security, auth, permission, or compliance-sensitive behavior — **present**.
  This change narrows an auto-approval trust boundary that governs terminal
  command execution in agent sessions; the whole point is a security posture
  improvement.
* Migration, backfill, destructive data/config action, or irreversible step —
  **absent**. The edit is a small, fully reversible config change (revert via git).
* External integration, operator checkpoint, or external dependency — **absent**.
* High runtime, rollout, or rollback risk — **absent**. Editor-only config; no
  runtime service, no deploy, trivial git revert.

`Requires plan hardening: yes` (security-sensitive signal present).

## Runtime Verification and Closure

* **Runtime surface changed?** No application runtime surface (no CLI/API/UI/jobs).
  The only "surface" is the VS Code editor auto-approve behavior, which is
  developer-environment config, not a shipped product runtime.
* **Runtime verification:** confirm, after the edit, that VS Code no longer
  auto-approves a generic `pip ...` command (the auto-approve map contains no
  substring/prefix pip key), and that the three documented `.scripts/*.py`
  patterns still auto-approve as before. This is an inspection-level check on the
  resulting JSON, not a build/test run.
* **Operational closure:** rollback trigger = any documented clone workflow
  breaks due to an unexpected approval prompt → revert the single-file change via
  git. Ownership: the single developer/operator. Validation window: next agent
  session that exercises the `.scripts/*.py` clone workflow. No monitoring
  system applies; record the closure check as a manual inspection item.

## Plan Hardening

**Hardening required?** Yes — triggered by the security/permission-sensitive
signal (this change narrows a terminal-command auto-approval trust boundary).
Confirmed materially relevant: the entry governs which terminal commands run
without a human in the loop during AI agent sessions.

**Learnings and instructions consulted:**

* `docs/compound/` — searched for VS Code auto-approve / terminal-approval /
  pip hardening prior art; no relevant learnings (low-confidence retrieval).
* `.github/instructions/constitution.instructions.md` — Principle VII
  (Destructive Command Approval) and the workspace security posture: terminal
  auto-approval must be an explicit, reviewed allow-list, not a blanket grant.
* `.github/instructions/strict-safety.instructions.md` — used its
  ProposedAction / ActionRisk / ActionResult vocabulary below.

**Protected invariants:**

* The three existing anchored `.scripts/*.py` auto-approve patterns remain
  byte-for-byte unchanged.
* `.vscode/settings.json` remains valid JSON.
* After the change, no terminal command is auto-approved unless it matches a
  specific anchored regex with `matchCommandLine: true`.

**Risky actions (strict-safety vocabulary):**

* **ProposedAction:** In Ship's implementation step, edit
  `.vscode/settings.json` to remove the blanket `"pip": true` and either add one
  anchored pip regex or leave it removed.
  * `targets`: `.vscode/settings.json` (single developer-environment config file)
  * `change_kind`: local config edit
  * `ActionRisk`: **low** — non-destructive, fully reversible via `git revert` /
    checkout; no runtime service, data, or shared contract touched. The change is
    risk-*reducing* (tightens an over-broad grant).
  * `rollback`: revert the single-file change in git.
  * `approval_required`: the staging step is operator-approved; the code/config
    edit itself is performed and reviewed by Ship under its normal review gate.
  * `ActionResult`: `planned` (execution deferred to Ship).

**Reinforced verification (carried into Ship runtime verification):**

* Precheck: capture the exact current auto-approve map before editing.
* Post-edit: (a) JSON validity check; (b) byte-for-byte diff confirming only the
  `pip` entry changed; (c) negative check — a representative `pip install <pkg>`
  command line matches no remaining auto-approve entry.

**Reinforced closure (carried into Ship operational closure):**

* Rollback trigger: any documented `.scripts/*.py` clone workflow breaks due to
  an unexpected approval prompt → git-revert the change.
* Owner: single developer/operator. Validation window: next agent session that
  exercises the clone workflow. No monitoring system applies; closure is a manual
  inspection item.

**Unresolved operator decisions blocking safe execution:** none. The only open
question (whether to anchor a specific pip command or remove outright) is a
low-risk implementation-time determination for Ship, not a blocker — both
endpoints satisfy the security invariant.

<!-- plan-hardened: yes -->

## Plan Review

**Gate decision: PASS.**

**Plan hardening:** Required (security/permission-sensitive signal present) and
satisfied — a `## Plan Hardening` section is present with protected invariants,
a `ProposedAction` classified at `ActionRisk: low`, and reinforced
verification/closure detail carried forward for Ship.

Reviewed against the always-on and triggered persona lenses (conducted directly;
the plan's evidence surface is a single config file fully contained in one read,
so persona subagent dispatch was not warranted):

* **Constitution Reviewer** — Aligns with Principle VII (Destructive Command
  Approval) and the workspace security posture: the change tightens an
  over-broad terminal auto-approval grant toward an explicit reviewed allow-list.
  Correctly respects the Stage Role Boundary by deferring the actual config edit
  to Ship. No violations. (no findings)
* **Rust Reviewer** — Not applicable; no Rust code, type signatures, or error
  handling in scope (config-only change). (no findings)
* **Scope Boundary Auditor** — Scope frozen to 9CEC208C; single width-isolated
  config unit; isolation from E86A6E56, 5905CDEE, 8C2E313D, 049-S, and unrelated
  stash is explicit. No scope creep or YAGNI. (no findings)
* **Learnings Researcher** — No compound prior art contradicts or supersedes the
  plan (low-confidence retrieval). (no findings)
* **Architecture Strategist** — No architectural impact; editor/environment
  config only, no module boundaries or dependency chains affected. (no findings)
* **Security Lens Reviewer** (triggered — trust-boundary/permission change) —
  Endorses removing the blanket substring grant and the negative-verification
  check. **P3 (advisory):** if Ship opts to add a replacement pip regex, it MUST
  be fully anchored (`^...$`) with regex metacharacters escaped and
  `matchCommandLine: true`, mirroring entries 1/3/4, so no prefix/substring
  bypass reintroduces the hole. Already covered by the plan's acceptance criteria
  and verification; recorded here for Ship awareness. **P3 (advisory):** confirm
  no other bare-substring auto-approve keys exist in the map after the edit.

**Findings summary:** P0 = 0, P1 = 0, P2 = 0, P3 = 2 (advisory, already covered
by plan acceptance criteria).

**Runtime verification & closure:** Present and adequate for a config-only
change — JSON validity, byte-for-byte preservation of existing patterns, and a
negative auto-approval check; git-revert rollback trigger with owner and
validation window recorded.

No P0/P1/P2 findings — proceed to harvest.

<!-- plan-review-attempt: 1 -->
<!-- plan-review-verdict: PASS -->


