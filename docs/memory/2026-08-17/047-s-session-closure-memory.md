---
type: session-memory
date: 2026-08-17
shipment: 047-S
context: "Ship agent session under P-017 dark-factory mode — full pipeline for 047-S, both PRs merged"
---

# Session Closure Memory — 047-S (2026-08-17)

## Scope

Depth-1 Ship agent session under P-017 dark-factory mode. Claimed and
completed shipment `047-S` ("Read-only serve guarantee honesty F2/F6") end
to end: implementation PR, merge, post-merge closure PR, merge. `048-S`
was explicitly NOT started or claimed per operator instruction.

## Outcome Summary

* **PR #97** (`feat/054-f-readonly-serve-guarantee-honesty`) — implementation.
  Merged as `704b95a6c1e2930079d6f3a602ab66e9682d4916` (2-parent merge
  commit, P-009 compliant). 6-reviewer standard+adversarial review (0
  P0/P1). 4 rounds of Copilot shadow review (operator-elevated to
  blocking; 20/21 files reviewed each round, 0 primary comments
  throughout): round 1 raised 9 suppressed comments (5 fixed for a genuine
  precision gap, 2 matched already-stashed config-drift follow-ups, 2
  addressed via a CI-disclosure subsection); round 2 raised 4 comments (1
  fixed, 2 recurring config-drift disclosed, 1 addressed elsewhere); round
  3 raised 2 comments, both fixed; round 4 raised 10 comments with a
  non-convergent count (9→4→2→10) and was dispositioned as follow-ups
  (`3FFE51B4`, `B8C0851E`) rather than a 4th fix cycle, per the 3-cycle cap
  and because F6 is an explicitly documented best-effort residual, not a
  target for exhaustive formal precision.
* **PR #98** (`post-merge/054-f-readonly-serve-guarantee-honesty`) —
  post-merge closure. Merged as `bf241ab0351fd7546de0d7ec3833088e47e47a33`
  (2-parent merge commit, P-009 compliant). 3-reviewer proportionate
  review (0 P0/P1). 4 Copilot review passes across 3 pushed commits:
  pass 1 (at `77d82ff`) created 5 real threaded comments (all fixed in
  `9708ba6`, replied to, resolved via GraphQL); pass 2 (at `9708ba6`,
  18:41) surfaced 5 suppressed findings (stale readiness HEAD, an
  EnvFilter self-consistency gap, a "5 vs 6 surfaces" undercount in two
  files, and an over-specific `#2874` trigger-condition claim — all fixed
  or hedged in `387135f`); pass 3 (at the same `9708ba6`, 18:47, a
  separate review pass this checkpoint's earlier draft omitted) surfaced
  4 more suppressed findings — 1 was the same EnvFilter gap already fixed
  by pass 2's remediation, and 3 were genuinely new (an "astronomically
  unlikely" retry-reliability overstatement, an incorrect `docs/memory/`
  file-count delta, and a premature `READY` releasability status while the
  post-deploy observation window was still open) — all 3 new findings were
  fixed as part of this same session-closure checkpoint's own PR; pass 4
  (at `387135f`) generated zero new comments (one suppressed comment was a
  stale-body-read timing artifact, already correct by the time it posted).

  *Correction*: an earlier draft of this checkpoint stated "3 substantive
  fix cycles applied across 5 code/doc precision corrections each round"
  for PR #97 — this conflated the 4 distinct per-round counts (5, 1, 2,
  and 10-deferred) into a single number; the breakdown above is the
  accurate one, matching PR #97's own final readiness record.
* **This PR (`#99`, `chore/session-closure-047-s`) itself went through 3
  Copilot review passes.** Pass 1 (at `de749a8`) raised 4 findings: 2 real
  accuracy gaps in this checkpoint (both corrected above), 1 timing
  artifact (a scope-disclosure line that was already correct by the time
  the review ran), and 1 substantive discovery — a second, previously
  unprocessed Copilot review pass on PR #98 at commit `9708ba6`, whose 3
  new findings were fixed directly in the affected already-merged files
  (`docs/compound/tracing-callsite-interest-cache-parallel-test-race.md`,
  `docs/memory/compacted/2026-08-17-047-s-memory-compaction.md`,
  `docs/closure/2026-08-17-047-s-post-merge-closure.md`). Pass 2 (at
  `8a7ac50`) raised 3 more findings: a stale file-count claim in this PR's
  own description ("Four-file" corrected to the actual count once this
  file's own edit was included), a timing-artifact restatement of the
  already-corrected readiness HEAD, and a genuine cross-document
  consistency gap — `docs/closure/2026-08-17-047-s-release-observability-evidence.md`
  still declared unconditional `READY` while the post-merge closure doc
  had just been corrected to `READY_WITH_CONDITIONS` for the same open
  observation window; aligned both documents. Pass 3 (at `a91cf8c`) raised
  4 more findings, all genuine: a self-contradiction my own pass-2 fix
  introduced (saying the window "cannot close on the merge date itself"
  when its own criteria say 10 starts *or* 14 days, so it theoretically
  could — reworded in both closure docs to state the actual reason it was
  open: only 1 start observed, 14 days not elapsed); and a follow-up-item
  classification mismatch between this checkpoint (2 hygiene / 1 stowaway
  / 2 F6) and the already-merged PR #98 compaction report (which had said
  2 hygiene / 2 stowaway / 1 F6) — corrected the compaction report to
  match the verified 2/1/2 split; and a restatement in this checkpoint's
  own "Key Technical Learnings" bullet of the oversimplified `#2874`
  trigger model that the compound doc itself had already corrected —
  reworded to preserve the same registration-ordering qualification.
* Both branches carried the six authorized stowaway files
  (`.autoharness/config.yaml`, `.github/agents/.ship.agent.md`,
  `.github/agents/.stage.agent.md`, `.github/agents/_orchestrator.agent.md`,
  `.gitignore`, `.vscode/settings.json`) from stash
  `4791882bad8291bae5d26cc0a096d37c25e54cc4`, applied via `git stash apply`
  (not pop) on the feature branch, reviewed as explicit PR scope, and
  committed as 2 coherent commits (`7f5de00`, `27b84b5`). The other 3
  pre-existing stashes (`4539d57c...`, `18f40d4f...`, `0b694d99...`) were
  never touched.
* Runtime verification was performed live: built the release binary, ran
  `serve --read-only` against a throwaway docline-valid fixture workspace,
  and confirmed the exact qualified log wording in real process output.
* Shipment `047-S` archived via safe-close (protected set empty — full
  single-shipment archive, no cascade). `048-S` verified untouched
  (`status: queued`) both before and after this session.
* 5 follow-up items stashed for Stage triage, spanning 3 distinct kinds
  (not all F6-related — an earlier draft of this checkpoint incorrectly
  grouped all 5 as F6 residuals): `9CEC208C` (pre-existing
  `.vscode/settings.json` blanket `pip` auto-approve — workspace hygiene,
  unrelated to this shipment), `C365AB98` (pre-existing `.gitignore`
  duplicate `.engram/` entry — workspace hygiene, unrelated to this
  shipment), `3FFE51B4` (model-routing config drift inherited from the
  authorized stowaway content — stowaway-related, not F6), `B883681D`
  (optional cosmetic `main.rs` F6 cross-reference — F6-related), and
  `B8C0851E` (optional further F6 wording refinement distinguishing
  overlapping vs. purely-sequential guard lifetimes — F6-related).

## Key Technical Learnings (already captured durably)

* `docs/compound/tracing-callsite-interest-cache-parallel-test-race.md` —
  new compound entry: `tracing-core` callsite `Interest` caching can starve
  a log-capture test of ANY output (not just wrong content) under parallel
  `cargo test` when sibling tests share a callsite — the precise trigger
  requires the subtler `tokio-rs/tracing#2874` registration-ordering
  preconditions (a `Dispatch` existing but not yet active when another
  thread touches the callsite), not simply "any no-subscriber touch
  poisons the process." Fix: `EnvFilter` (empirical partial improvement,
  not a forcing mechanism) + `rebuild_interest_cache()` + bounded retry
  (mitigation, not a guarantee).
* `docs/compound/cargo-audit-workspace-config-limitation.md` — appended:
  `RUSTSEC-2026-0249` (`smartstring`, a `cozo` transitive dependency) had
  to be added to the audit allowlist mid-session; this pattern (new
  advisories requiring allowlist maintenance) recurs and is now documented.
* GitHub repository ruleset discovery: this repo gates merges via a
  `PR-Required` ruleset (`required_review_thread_resolution: true`,
  `copilot_code_review.review_on_push: true`), not classic branch
  protection — `branches/main/protection` returns 404 but merges are still
  blocked by unresolved Copilot review threads. Direct pushes to `main` are
  blocked by the same ruleset (`target: branch`, `enforcement: active`),
  which is why this session-closure memory file required its own minimal
  PR rather than a direct commit.

## Process Notes

* A major GitHub-wide outage (incident `zkxwbgr0cnmx`) occurred mid-session,
  causing transient CI/Copilot-review failures; distinguished from
  code-caused failures by reproducing failures directly against `main` and
  cross-referencing `githubstatus.com` incident timing. Handled with
  patient circuit-breaker-compliant backoff rather than rapid retries.
* Intercom was unavailable for the full session (per the dark-mode
  contract); all work proceeded as safe, non-destructive, evidence-gated
  work with no operator-approval-dependent destructive actions attempted.
* No admin fallback was attempted or needed — both merges completed via
  the normal merge path once each PR's local/adversarial review, CI, and
  thread-resolution gates were satisfied.

## Final State

* `main` at `bf241ab0351fd7546de0d7ec3833088e47e47a33`, clean working tree,
  single worktree.
* `047-S` archived; `048-S` untouched and ready for Orchestrator/Stage to
  pick up next per the queued pipeline.
