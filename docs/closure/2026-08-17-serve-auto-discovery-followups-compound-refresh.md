---
title: "Compound Refresh — Shipment 048-S"
description: "Review of docs/compound/ entries related to shipment 048-S's tracing-capture and PowerShell-quoting discoveries"
date: 2026-08-17
scope: "recent"
mode: "propose"
shipment: "048-S"
---

## Entries Reviewed

| Entry | Classification | Evidence |
|---|---|---|
| `docs/compound/tracing-callsite-interest-cache-parallel-test-race.md` | **keep** | Still accurate and distinct. This shipment's new entry (`tracing-envfilter-wrong-crate-target-2026-08-17.md`) explicitly cross-references and distinguishes itself from this one: interest-cache race is probabilistic/intermittent; the new crate-target-mismatch bug is deterministic/100%-reproducible. No overlap requiring consolidation. |
| `docs/compound/best-practices/envfilter-suppress-noisy-crate-2026-05-04.md` | **keep** | Distinct issue (WARN-vs-ERROR level-string off-by-one semantics for suppressing a noisy dependency crate) from the new entry's issue (EnvFilter directive naming the wrong crate entirely, causing total event loss). No overlap. |
| `docs/compound/workflow-issues/gh-pr-body-powershell-backtick-conflict-2026-04-29.md` | **keep** | Same general class of PowerShell-quoting-conflicts-with-CLI-argument problem as this shipment's new `git-commit-powershell-embedded-quotes-2026-08-17.md` entry, but a different specific trigger (backtick vs. embedded double-quote) and different CLI command (`gh pr create --body` vs. `git commit -m`). Both entries independently pattern-match against different literal error messages a future session might see, so kept separate rather than consolidated; the new entry cross-references this one and explicitly generalizes the file-based-argument workaround. |
| `docs/compound/best-practices/shared-status-type-binary-library-2026-05-06.md` | **keep** (not modified) | Related topic (binary/library crate boundary patterns) but addresses a different concern (a shared status type) than this shipment's `FileFilter` additive-API pattern. Reviewed for overlap; none found significant enough to warrant consolidation into a single "crate boundary" mega-entry at this time. |

## New Entries Added This Shipment

* `docs/compound/tracing-envfilter-wrong-crate-target-2026-08-17.md` — EnvFilter directive must
  target the crate a call-site is actually compiled into, not an analogous donor module's crate.
* `docs/compound/workflow-issues/git-commit-powershell-embedded-quotes-2026-08-17.md` — `git
  commit -m` with embedded double-quoted text breaks PowerShell argument parsing; use `-F` with a
  file instead.

## Follow-Up Items

None. All reviewed entries remain accurate, distinct, and worth keeping as-is; no consolidation,
replacement, or deletion was warranted.
