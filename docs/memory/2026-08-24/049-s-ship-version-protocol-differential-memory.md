---
type: session-memory
agent: ship
date: 2026-08-24
branch: chore/stage-049-S
final_head: 102728f0453a9445b458648e98be8bca0632cd32
pr: 106
feature: 056-F
shipment: 049-S
scope: version-protocol-differential-planning-correction
---

# Ship PR #106 — version/protocol differential planning correction

Second operator follow-up in the same session: new evidence (reverting the
local Copilot CLI to the last stable build removes the failure; a newer MCP
protocol version is reportedly in play) plus an explicit, direct instruction
to "make the smallest planning correction within PR-readiness scope,
commit/push it, rerun exact-HEAD review, and refresh readiness."

## Role Boundary decision (revised from the prior turn)

The prior turn declined to edit deliberation/plan content, citing Ship's
NON-NEGOTIABLE Role Boundary. This turn, the operator gave a second,
increasingly explicit, direct instruction naming the exact action
("make the smallest planning correction... commit/push it") rather than a
general request to "flag/fix" wording. Given: (a) the correction is narrow
and evidence/hypothesis-framing only — it changes no task's scope,
dependencies, acceptance-criteria substance, or shipment membership; (b) the
operator explicitly and repeatedly authorized this specific action, framed
as PR-readiness work; (c) Ship cannot literally "redirect to Stage" as a
separate invokable agent in this environment; and (d) leaving materially
important, operator-verified evidence undocumented in the actual plan
artifacts (rather than only in the transient PR body) was judged a bigger
risk than the narrow Role Boundary exception — proceeded to edit the
deliberation, exec-plan, and `056.001-T.md` directly, under explicit operator
authorization, with the exception documented transparently in the PR body,
commit messages, and this memory record.

## Commits

* `755139a` — `docs(mcp): add version/protocol differential evidence
  requirements`. Recorded the operator's stable-vs-affected observation in
  the deliberation's H3 row; rebalanced H0 confidence (High→Medium) and H3
  confidence (Low→Medium); removed the "H0 is the leading hypothesis"
  declarations (both occurrences) in favor of a process-only sequencing
  rationale; added explicit client/toolchain protocol-version-negotiation
  framing to H3-A; softened an "already newest MCP revision"/"not a
  credible cause" claim in Evidence Gathered; added evidence-capture
  requirements (exact CLI version/build/hash for stable + affected builds;
  client-offered/server-negotiated `protocolVersion`/capabilities; no
  hard-coded protocol version) to `056.001-T.md` and the parallel exec-plan
  `### T0 —` section; added a new Open Questions bullet. A dedicated
  Correctness Reviewer sub-agent pass on this exact diff, run BEFORE the
  commit, found `P0=0, P1=0, P2=0, P3=3`; two of three P3s were fixed in the
  same commit.
* `102728f` — `fix(mcp): bound T0 run-count and allow absent protocol
  negotiation`. A subsequent automated Copilot review (triggered by this
  repo's `review_on_push` ruleset policy) caught two genuine logical defects
  in `755139a`'s new text: (1) requiring a server-negotiated `protocolVersion`
  unconditionally on "both legs" is impossible on the affected leg, since the
  entire premise of the investigation is that the server may exit before any
  `initialize` response exists; (2) adding an optional second (stable-build)
  classification pass without updating the surrounding "exactly one
  pair"/"single pair" run-count language created a direct contract conflict.
  Fixed both: an absent negotiated value is now an explicit first-class
  captured outcome (not an unmet requirement), and the run-count bound is
  explicit and consistent (at most one affected-build pair plus at most one
  stable-build pair, never more) across `056.001-T.md`'s acceptance-criteria,
  description, and every parallel reference in the exec-plan (T0 section,
  Constitution Check summary, retained-nodes list).

## Verification

* Backlogit parses `056.001-T.md` correctly after both edits
  (`backlogit_get_item` section extraction succeeds; `backlogit_sync_index`
  reports 492 artifacts, unchanged count).
* Manual `grep`-equivalent sweeps after each commit confirmed no stale
  "leading hypothesis" or "the single pair"/"exactly one control" references
  remained anywhere in the three edited files.
* CI green on both commits (`build`, `detect code changes` both `pass`).
* All bot-review threads from both automated review rounds (3 threads on
  `755139a`, 3 more on `102728f`, plus the pre-existing 73) replied to with
  the specific fixing commit and resolved via GraphQL. Final state: 79/79
  resolved, 0 unresolved, 0 human-authored, `mergeStateStatus=CLEAN`.

## Out of scope, not done (per operator instruction)

Did not broaden into `049-S` implementation. Did not claim shipment `049-S`.
Did not merge, did not checkout `main`. PR #106 remains ready for operator
review and merge-commit approval at HEAD `102728f0453a9445b458648e98be8bca0632cd32`.
