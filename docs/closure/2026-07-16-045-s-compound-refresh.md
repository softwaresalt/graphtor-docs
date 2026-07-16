---
type: compound-refresh
date: 2026-07-16
scope: recent
context: "Post-merge closure of shipment 045-S (consumption-first graphtor); PRs #90 + #91 merged"
mode: apply
---

# Compound Refresh — Post 045-S

## Summary

Reviewed the compound entries most relevant to the 045-S shipment and the two
merged remediation PRs (#90 code, #91 closure). 045-S primarily **added** new
runtime surfaces (read-only serve auto-discovery, minimal install/uninstall,
doctor footprint detection, reparse-point containment) rather than altering code
documented by existing entries, so most of the library remained accurate. One
workflow entry drifted relative to the reply/resolve flow exercised across 14
review waves and was updated; one genuinely new security learning was captured.

## Entries Reviewed and Classified

| Entry | Classification | Evidence |
|---|---|---|
| `best-practices/github-resolve-review-threads-graphql-2026-05-02.md` | **update** | 14 review waves on PRs #90/#91 used the GraphQL `addPullRequestReviewThreadReply` reply-by-thread-ID path + PowerShell quoting discipline; added both to the entry. |
| `github-pr-copilot-review-reply-ids-2026-05-05.md` | **keep** | REST reply-by-numeric-ID path documented here is still valid; the GraphQL path is a complementary alternative, not a replacement. No drift. |
| `workflow-issues/gh-pr-body-powershell-backtick-conflict-2026-04-29.md` | **keep** | `--body-file` pattern used verbatim this session for both PR bodies; still accurate. |
| `best-practices/post-merge-closure-switch-main-2026-05-20.md` | **keep** | Closure followed switch-to-main-after-merge; still accurate. |
| `best-practices/atomic-lock-file-write-cleanup-2026-04-30.md` | **keep** | Related to the atomic-0600 temp-file create in 045-S but distinct (lock file vs mcp config); not superseded. |
| Remaining ~50 entries (pipeline/pdf/cozo/rmcp/ci) | **keep** | Out of 045-S scope; no contradicting evidence surfaced. |

## New Learning Captured

| New entry | Rationale |
|---|---|
| `best-practices/reparse-point-fail-closed-containment-2026-07-16.md` | Hard-won across PR #90 review waves 5–10: the `is_reparse_point` guard (`src/path/security.rs`) that rejects a symlink/junction workspace root before write/mutate at the serve scan-root, install, and uninstall boundaries — a reusable containment pattern not previously in the library. |

## Files Changed by This Refresh

- Updated: `docs/compound/best-practices/github-resolve-review-threads-graphql-2026-05-02.md`
- Created: `docs/compound/best-practices/reparse-point-fail-closed-containment-2026-07-16.md`
- Created: this report

## Follow-Up

None requiring manual review. No entries marked stale; no consolidations or
deletions performed.
