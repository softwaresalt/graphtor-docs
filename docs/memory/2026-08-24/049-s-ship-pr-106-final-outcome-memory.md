---
type: session-memory
agent: ship
date: 2026-08-24
branch: chore/stage-049-S
final_head: 37e9acf851a21f63326ef394d86b9ba756397fb6
pr: 106
feature: 056-F
shipment: 049-S
scope: pr-106-lifecycle-handoff
status: complete
---

# Ship PR #106 lifecycle handoff — final session outcome

Bounded Ship session scoped strictly to making PR #106 ready for
operator-approved merge. Did **not** claim shipment `049-S`, implement any of
its eight evidence tasks, merge the PR, checkout `main`, or begin post-merge
closure, per explicit operator instruction.

## Final state

* Final HEAD: `37e9acf851a21f63326ef394d86b9ba756397fb6`.
* Commits added this session (on top of Stage's unpushed `178c54c`):
  * `1af5239` — `.mcp.json` `${workspace_folder}` to `${workspaceFolder}` fix
    (validated against VS Code's MCP config reference) plus initial Ship
    session memory.
  * `7dee9c7` — `#[allow(clippy::unused_async_trait_impl)]` on
    `src/mcp/server.rs`'s `#[tool_handler]` impl, fixing a CI `build` failure
    caused by CI's floating `stable` Rust toolchain advancing to 1.98.0
    mid-session (pre-existing, unrelated to this PR's diff; reproduced and
    verified via `rustup update stable`).
  * `37e9acf` — added `#[allow(unknown_lints)]` before the version-specific
    allow, after an automated Copilot review correctly flagged that the new
    lint postdates the crate's 1.75 MSRV; verified under Rust 1.97.0 and
    1.98.0. Rust 1.75.0 itself could not be used for end-to-end verification
    due to an unrelated, pre-existing `Cargo.lock` issue (`globset 0.4.18`
    requires `edition2024`; confirmed untouched by this PR, last changed by
    an unrelated older commit `a47f9ef`).
* GitHub PR state: `mergeable=MERGEABLE`, `mergeStateStatus=CLEAN`,
  `state=OPEN`. CI (`build`, `detect code changes`) both `pass`.
* Repository ruleset `PR-Required` requirements all satisfied:
  `required_status_checks=["detect code changes"]` (pass),
  `required_review_thread_resolution=true` (75/75 threads resolved),
  `required_approving_review_count=0`, `allowed_merge_methods=["merge"]`.
  Repo settings confirm `allow_merge_commit=true`,
  `allow_squash_merge=false`, `allow_rebase_merge=false` (P-009 satisfied).

## Review outcome

Five review-skill personas (Constitution, Correctness, Maintainability,
Learnings Researcher, Template Integrity) reviewed the full
`origin/main...HEAD` diff in report-only mode: unanimous `P0=0, P1=0`. Six P2
follow-ups recorded in the PR body (none block merge; none affect the active
`049-S` 8-task manifest): a pre-existing `.gitignore`/`.mcp.json` tracking
inconsistency, exec-plan historical-narrative bloat, permanent CI investment
in the one-shot probe crate, a missing Stage memory checkpoint for the
`056.021`-`056.025-T` task split, missing `description` markers on
`056.026/027/028-T` (independently reproduced via
`backlogit_get_item(section="description")` erroring, and independently
confirmed by an automated Copilot review), and the pre-existing Cargo.lock
MSRV drift discovered this session. Outcome: `READY_WITH_FOLLOWUPS`.

## Bot-thread triage

75 total review threads; **0 unresolved, 0 human-authored** at session end.
27 were unresolved at session start (spanning the PR's full review history,
not just the newest six); each was independently re-verified against current
HEAD content (not assumed from prior narrative), replied to with the specific
resolving content/commit, and resolved via GraphQL. A subsequent automated
Copilot review — triggered by this repository's `PR-Required` ruleset
(`copilot_code_review: review_on_push: true`), independent of the
`request_copilot_review` MCP tool, which was degraded all session — surfaced
5 more threads: 3 reconfirmed the `056.026/027/028-T` marker-gap P2 already
tracked, and 2 caught real issues in this session's own fix commits (stale
PR-scope text, and the `unknown_lints` MSRV gap), both fixed and
replied/resolved.

## Shadow review

`request_copilot_review` (MCP) failed consistently across 4 attempts across
the session (timeouts / `context deadline exceeded`); a lightweight read-only
GitHub MCP probe (`pull_request_read`) confirmed the server itself, not just
this operation, was degraded. A REST-based reviewer-add fallback was
attempted and confirmed ineffective (`reviewRequests` stayed empty),
consistent with this repository's own documented guidance that CLI/REST
reviewer flags do not reliably address Copilot's special reviewer identity.
Recorded as unavailable via the manual path — but the ruleset's automatic
`review_on_push` policy independently triggered real reviews on each push,
which were fully triaged (see above).

## Compound learnings captured

* `docs/compound/workflow-issues/clippy-allow-unknown-lints-msrv-guard-2026-08-24.md`
  — new: suppressing a clippy lint added after a crate's declared MSRV needs
  `#[allow(unknown_lints)]` alongside the version-specific allow.
* `docs/compound/workflow-issues/mcp-json-workspacefolder-camelcase-2026-08-24.md`
  — new: VS Code MCP config env interpolation requires exact camelCase
  `${workspaceFolder}`; snake_case variants fail silently.
* Upvoted two existing entries
  (`clippy-useless-conversion-ci-rust-version-skew-2026-05-01.md`,
  `clippy-pedantic-map-unwrap-or-ci-vs-local-2026-04-30.md`) — this session's
  CI failure is a third real-world reconfirmation of the same
  floating-toolchain-drift root cause; the "pin CI toolchain" prevention
  advice in both remains unaddressed in `ci.yml`.

## Preserved, untouched

`.backlogit/checkpoints/checkpoint-20260822-090657.json`,
`.backlogit/checkpoints/checkpoint-20260822-092508.json`, `.backlogit/runtime/`,
and the 10 root helper scripts. Verified via `git status --short` at every
checkpoint this session; none staged, edited, or deleted.

## Out of scope, not done (by design)

Did not claim shipment `049-S`, implement any of its 8 tasks, merge PR #106,
checkout `main`, or begin post-merge closure. Did not edit
`.backlogit/queue/056.026-T.md`, `056.027-T.md`, or `056.028-T.md` (the
missing-marker fix is Stage-owned backlog-content work, outside Ship's Role
Boundary for this session) — tracked as an explicit PR-body follow-up
instead. Did not touch `.gitignore`, the exec-plan's historical-narrative
bloat, or the pre-existing `Cargo.lock`/MSRV-1.75 drift (all pre-existing,
out of scope, documented as follow-ups).

## Handoff / next steps

PR #106 is ready for operator review and merge-commit approval. On approval,
a separate Ship session should: merge with a merge commit (not squash/rebase,
already confirmed available), then — only after explicit merge — proceed
with `049-S` claim and implementation in a fresh, correctly-scoped session.
Recommended non-blocking follow-ups for a future Stage session: wrap the
`description` sections in `056.026-T`/`056.027-T`/`056.028-T`; investigate/fix
the pre-existing `Cargo.lock` MSRV-1.75 resolution failure
(`globset 0.4.18`/`edition2024`); consider pinning the CI Rust toolchain
(`ci.yml`) to stop recurring toolchain-drift clippy failures (three
occurrences now documented in `docs/compound/workflow-issues/`).
