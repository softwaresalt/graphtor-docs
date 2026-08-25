---
title: "Blanket pip auto-approval in .vscode/settings.json (9CEC208C)"
description: "Lightweight security deliberation on replacing the blanket chat.tools.terminal.autoApprove pip:true entry with a narrowly-scoped, reviewed regex allow-list so arbitrary pip command lines are no longer auto-approved in AI agent sessions"
doc_type: "decision"
topic: "Harden the VS Code terminal auto-approve allow-list so pip commands require operator review instead of blanket auto-approval"
depth: "lightweight"
decision_status: "decided"
promoted_to: "docs/exec-plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md"
stash_ids:
  - "9CEC208C"
linked_artifacts:
  - "docs/exec-plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md"
source: "stash:9CEC208C"
tags:
  - security
  - config
  - vscode
  - auto-approve
  - supply-chain
  - hardening
---

## Problem Frame

Stash bug `9CEC208C` (priority high) reports a **pre-existing** security
weakness in `.vscode/settings.json`. The `chat.tools.terminal.autoApprove`
map contains a blanket `"pip": true` entry. Unlike the sibling entries — which
are anchored regex patterns with `matchCommandLine: true` that only match one
specific reviewed command line — the bare `"pip": true` key auto-approves **any**
terminal command line that references `pip` without operator review.

Concretely, this creates an arbitrary-package-installation and
code-execution path for an AI agent session: `pip install <attacker-package>`,
`pip install --index-url <hostile-mirror> ...`, or any `pip download`/
`pip install -e .` variant would be auto-approved and run without a human in
the loop. Because pip executes arbitrary `setup.py` / build backend code at
install time, blanket auto-approval is effectively blanket remote code
execution approval.

**Provenance (confirmed in the stash report):** `git show
33bbb37:.vscode/settings.json` shows `"pip": true` predates the 047-S
stowaway carry-over. The 047-S stowaway only removed stale `.specify/speckit`
entries; it did not introduce or touch `"pip": true`. This is therefore an
independent, pre-existing hygiene/security defect, not a regression from 047-S.

**Who cares and why:** the single developer/operator running AI agent sessions
in this workspace. The whole point of the auto-approve list is to skip prompts
for _specific, reviewed, low-risk_ commands; a blanket `pip` entry defeats that
intent and silently widens the trust boundary.

### Constraints and requirements

* Security: no terminal command should be auto-approved unless it matches a
  specific, reviewed pattern (principle of least privilege / explicit allow-list).
* Consistency: the fix must match the existing convention already used in the
  same file — anchored regex keys with `"approve": true, "matchCommandLine": true`.
* Non-destructive to legitimate workflow: existing intentionally-reviewed pip
  invocations (if any are actually needed) may be re-added as narrow anchored
  patterns; today there is no evidence any specific pip command is required for
  the documented `.scripts/*.py` workflows, so removing the blanket is safe.
* Valid JSON: `.vscode/settings.json` must remain parseable and preserve the
  other three auto-approve patterns unchanged.

### Success criteria

* `chat.tools.terminal.autoApprove` no longer contains a blanket `"pip": true`
  (or any bare-substring) entry.
* Any retained pip auto-approval is expressed as an anchored regex with
  `matchCommandLine: true`, equivalent in shape to the existing
  `.scripts/*.py` patterns.
* The file remains valid JSON and the three existing python-script patterns are
  byte-for-byte preserved.
* A general `pip install <anything>` command line is NOT auto-approved after the
  change (verified by inspecting the resulting allow-list semantics).

### Out of scope

* Auditing or changing the three existing `.scripts/*.py` auto-approve patterns.
* Broader review of every other VS Code / agent setting or capability-pack
  config (tracked separately if needed).
* The unrelated isolated stash items E86A6E56, 5905CDEE, 8C2E313D, C365AB98,
  3FFE51B4, and shipment 049-S — explicitly excluded from this session.
* Any source-code, build, or CI change. This is a single config-file edit.

## Research Findings

* **Current file state** (`.vscode/settings.json`): the auto-approve map holds
  four entries — three anchored `matchCommandLine: true` regex patterns for
  `python .scripts/clone_ms_docs_repos.py ...` and
  `python .scripts/generate_clone_scripts.py`, plus the outlier bare
  `"pip": true`.
* **VS Code semantics:** in `chat.tools.terminal.autoApprove`, a string key with
  a boolean value is matched as a substring/prefix against the command by
  default; only entries with `matchCommandLine: true` and a `/regex/` key are
  evaluated as anchored command-line regexes. So `"pip": true` matches any
  command containing `pip`, which is exactly the over-broad behavior reported.
* **Prior learnings:** the compound library (`docs/compound/`) contains no
  prior art on VS Code auto-approve hardening; matches for "pip" are unrelated
  Python/pipeline code learnings. Low-confidence retrieval — no reusable pattern.
* **Existing convention is the template:** the fix should reuse the exact shape
  already present in the file (anchored regex + `approve: true` +
  `matchCommandLine: true`), so it is consistent and self-documenting.
* **No existing backlog item** covers this concern (query-first check found only
  "pipeline" false positives), so this is not a duplicate.

## Options Evaluated

### Option A: Remove the blanket entry entirely

Delete the `"pip": true` key, leaving only the three anchored python-script
patterns.

* **Pros:** simplest, smallest diff, maximal safety, zero residual pip
  auto-approval surface. No evidence any specific pip command is currently needed.
* **Cons:** if some future workflow legitimately needs a specific pip command
  auto-approved, it must be added back later (cheap to do).
* **Effort:** low.
* **Fit:** strong — fully satisfies least-privilege; slightly conservative.

### Option B: Replace with a narrowly-scoped anchored regex (operator recommendation)

Replace `"pip": true` with an anchored `/^.../` regex key carrying
`"approve": true, "matchCommandLine": true`, scoped to the exact, reviewed pip
command line(s) that are actually needed — mirroring the existing
`.scripts/*.py` patterns.

* **Pros:** preserves a documented, reviewed pip workflow while closing the
  arbitrary-command hole; matches the file's established convention; explicit and
  auditable.
* **Cons:** requires knowing the exact command line(s) to anchor; if none is
  genuinely required today, the anchored pattern would be speculative.
* **Effort:** low.
* **Fit:** strong — directly matches the operator's stated recommendation and the
  in-file convention.

### Option C: Leave as-is / accept risk

Keep `"pip": true`.

* **Pros:** none beyond zero effort.
* **Cons:** unmitigated arbitrary-package-install / RCE path in agent sessions.
* **Effort:** none.
* **Fit:** rejected — violates the workspace security posture and the whole
  purpose of an explicit allow-list.

## Trade-off Comparison

| Criterion | Option A (remove) | Option B (scoped regex) | Option C (as-is) |
|---|---|---|---|
| Security posture | Strongest | Strong | Unacceptable |
| Matches file convention | N/A (removes) | Yes | N/A |
| Preserves any needed pip workflow | No (re-add later) | Yes (if a real command exists) | Yes (unsafely) |
| Diff size / risk | Smallest | Small | None |
| Alignment with operator recommendation | Partial | Full | None |

## Decision

**Chosen direction: Option B — replace the blanket `"pip": true` with a
narrowly-scoped anchored regex pattern (`matchCommandLine: true`), equivalent in
shape to the existing `.scripts/*.py` auto-approve patterns** — with an explicit
Option-A fallback: **if implementation review finds that no specific pip command
line is actually required by the documented workflows, remove the entry
entirely rather than inventing a speculative pattern.**

This matches the operator's recommendation in the stash report, honors the
existing in-file convention, and restores least-privilege for the terminal
auto-approve surface. The covering work is classified as a **chore** (security /
config hygiene, no net-new product capability) that ships as one coordinated
release unit.

## Rejected Alternatives

* **Option C (leave as-is):** rejected — leaves an arbitrary-package-install /
  code-execution path auto-approved in AI agent sessions.
* **Option A as the primary path:** demoted to a fallback rather than the primary
  choice, because the operator explicitly recommended a scoped-regex replacement;
  Option A remains the correct outcome only if no concrete pip command line needs
  auto-approval.

## Unresolved Questions

* Is there any concrete, currently-needed pip command line that must remain
  auto-approved (which anchored regex to write), or is outright removal
  (Option A fallback) the right endpoint? Ship's implementation/review step
  should confirm against the documented `.scripts/*.py` clone workflow before
  finalizing the pattern.

## Risks and Mitigations

* **Risk:** removing/narrowing pip auto-approval introduces an approval prompt
  for a workflow that silently depended on it. **Mitigation:** the three
  documented workflows use `python .scripts/*.py`, not `pip`; if a specific pip
  command surfaces during implementation, re-add it as one anchored pattern.
* **Risk:** malformed JSON or accidental change to the three existing patterns.
  **Mitigation:** acceptance criteria require valid JSON and byte-for-byte
  preservation of the existing patterns; Ship validates by re-reading the file.
* **Risk:** scope creep into other settings/security items. **Mitigation:**
  session scope frozen to 9CEC208C; isolation from E86A6E56, 5905CDEE, 8C2E313D,
  049-S, and unrelated stash recorded explicitly.
