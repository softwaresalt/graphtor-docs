---
title: "Stage staging-review remediation — dark security pipeline shipments 050-S / 051-S / 049-S"
type: session-memory
agent: stage
date: 2026-08-25
branch: chore/stage-dark-security-pipeline
shipments:
  - 050-S
  - 051-S
  - 049-S
status: complete
tags:
  - stage
  - staging-review
  - remediation
  - dark-mode
  - security
  - residual-risk
---

## Task

Bounded Stage staging-review remediation of the valid local findings for the
three queued dark-scope shipments, preserving order **050-S → 051-S → 049-S**.
Planning/backlog artifacts only — no source/config changes, no builds, no
shipment claim, no PR/Ship invocation. All unrelated/untracked files preserved.

## Remediations applied (files / backlog fields changed)

1. **050-S (vscode pip auto-approve hardening plan).** Added a complete
   `## Constitution Check` to
   `docs/exec-plans/2026-08-24-vscode-pip-autoapprove-hardening-plan.md` mapping
   Principles I–XI, with an explicit config-only risk (`ActionRisk: low`) and
   git-revert rollback note. No other change to that plan.
2. **051-S memory Markdown conformance.** Added a frontmatter `title:` field to
   `docs/memory/2026-08-25/stage-store-toctou-nofollow-memory.md` (repo memory
   convention: `title:` in frontmatter, no H1) to satisfy the first-heading
   Markdown rule.
3. **051-S store TOCTOU plan + task acceptance strengthened.** In
   `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md` and tasks
   `059.001-T` / `059.002-T` / `059.004-T`:
   * **Reparse breadth (A1):** resolved the `FILE_ATTRIBUTE_REPARSE_POINT`
     ambiguity by explicitly **adopting the broader fail-closed policy** (refuse
     ANY reparse-point entry), justified because a precise name-surrogate test
     needs the reparse tag via `unsafe` `DeviceIoControl(FSCTL_GET_REPARSE_POINT)`
     (precluded by `#![forbid(unsafe_code)]` at MSRV 1.75) and
     `FileType::is_symlink()` is path-based (re-introduces TOCTOU). Added U5 delta
     (e) regression for a legitimate non-redirecting reparse file; documented the
     intentional Unix/Windows breadth asymmetry.
   * **MSRV verification (A2):** U1 acceptance now REQUIRES
     `cargo +1.75.0 check --all-targets` (pinned-MSRV) for the new direct deps
     (`libc`, `windows-sys`); the 1.75.0 toolchain is installed locally.
   * **Unix read access (A3):** U2 now requires Unix `OpenOptions` to set
     `.read(true)` alongside `custom_flags(O_NOFOLLOW)` (O_NOFOLLOW is a modifier;
     an access mode is mandatory); write access deliberately not requested.
   * **cargo-tree before/after (A4):** U1 acceptance now requires a BEFORE/AFTER
     `cargo tree -d` (+ `-i windows-sys` / `-i libc`) comparison, not a
     post-edit-only snapshot.
   * Added a dated report-only Plan Review addendum (attempt 2, PASS advisory:
     P0=0 P1=0 P2=4-resolved P3=0).
4. **049-S commit field + durable readiness.**
   * Cleared the premature `commit: 642c3e4` from `.backlogit/queue/049-S.md`
     (queued shipments must not carry an implementation commit; `642c3e4` was a
     Stage docs-reconcile commit, not a 049-S implementation). Traceability
     preserved via a `049-S` backlog log comment (prior full SHA
     `642c3e44e5a467b9a16ae875c4e613854df61cf3`). Index synced; `commit` now null.
   * Added `## Stage Readiness Evidence (durable — 2026-08-25)` to
     `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
     so Ship can verify the 8-task manifest **without** relying on the now-frozen
     **MERGED** PR #106 body. Manifest UNCHANGED:
     `[056.020-T, 056.022-T, 056.023-T, 056.021-T, 056.001-T, 056.002-T,
     056.003-T, 056.019-T]`; covering feature `056-F` excluded (P-015). All eight
     verified `queued` on 2026-08-25.

## Order rationale (dark scope preserved)

050-S → 051-S → 049-S is preserved. **050-S executes first** because the live
`.vscode/settings.json` still contains `"pip": true` (line 7) — the exact blanket
terminal auto-approval (substring-match RCE-at-install) risk 050-S removes; that
active risk is why the config hardening leads the pipeline.

## Residual-risk classification (operator-visible)

* **Duplicate `056.026-T` create events in `hooks_queue.jsonl`** (two
  `create_artifact` events: seq 1001 @ 2026-08-23T19:16:13Z and seq 1027 @
  2026-08-23T20:44:27Z). These are **outside this staging diff** and do **not**
  block 050-S / 051-S / 049-S. Preserved as an operator-visible residual;
  append-only history is **not** rewritten. No new work created.
* **Live `.vscode/settings.json` `"pip": true`** remains the exact risk 050-S
  will fix and is the reason 050-S executes first (see order rationale). Left
  untouched by Stage (config mutation is Ship's boundary).
* **Unrelated broken references in archived `054`/`055` families** (present under
  `.backlogit/archive/054*` and `055*`) are **outside dark scope**. Not touched;
  no new follow-up created (none opened unless an existing follow-up already
  covers them).

## Validation

* Focused Stage plan-review conducted directly (contained evidence surface);
  recorded as advisory addenda in the two plans. Markdown validated via
  `backlogit_docs_lint` (see session output). No builds run (Stage boundary).

## Blockers

None. 050-S, 051-S, 049-S remain **queued** (not claimed/executed). Order
050-S → 051-S → 049-S preserved. Untracked/unrelated files untouched. Not
committed (per task instruction).
