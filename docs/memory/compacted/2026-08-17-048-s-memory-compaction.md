---
type: compaction-report
date: 2026-08-17
target: memory
context: "Final dark-factory closure-only compaction — 048-S shipment complete, build checkpoint superseded by closure artifacts"
---

# Compaction Report — 048-S (2026-08-17)

## Trigger

Final closure-only `compact-context` pass across the completed P-017
dark-factory activation scope (stash `970AE45A`/`5D98DBCC`/`B88E37BF`/`5868A7C5`),
run after both `047-S` and `048-S` were independently merged and archived with
no open PRs or queued shipments remaining. `047-S`'s own build checkpoint was
already compacted during its shipment closure
(`docs/memory/compacted/2026-08-17-047-s-memory-compaction.md`); `048-S`'s
build checkpoint had not yet been compacted, so this pass performs that
compaction now.

## Consolidated Summary

Shipment `048-S` ("Serve auto-discovery follow-ups — PR90 deferrals B88E37BF
+ 5868A7C5") completed in full: feature `055-F` with execution container
`055.001-T` and its two subtasks (`055.001.001-ST` library `FileFilter` API,
`055.001.002-ST` streaming classifier refactor), plus `055.002-T` (alias
canonicalization evaluation, concluded documented no-op). RED-first TDD was
directly observed for both subtasks (`12 passed; 9 failed` → `21 passed; 0
failed` for `FileFilter`; `0 passed; 7 failed` → `44 passed; 0 failed` for
`stream_ingestible`). Full quality gate sequence green (fmt, clippy pedantic,
362 lib + 215 bin tests, audit with the CI-equivalent allowlist). 7-persona
cross-model adversarial review resolved one genuine P1 (a proactive
`stream_ingestible` performance micro-optimization that caused
`format_candidate_count` to under-count once a match was found — reverted;
independently confirmed negligible real benefit) and empirically refuted one
self-disclosed low-confidence P1 (a `clippy::option_if_let_else` concern —
confirmed via an explicit extra-flag clippy run that the lint is not actually
enabled by this repository's `-D clippy::pedantic` gate, since it also fires
on 12 pre-existing unrelated call sites). Two P2 findings were fixed (a
dangling cross-reference to a never-created release-observability file,
retargeted to the actual closure doc; a `tracing` target-name note added for
the crate-boundary-driven warning-target change) and one P2 was confirmed
pre-existing/out-of-scope (duplicated format-alias normalization, verified
byte-for-byte unmodified via `git show`). Copilot shadow review (elevated to
blocking for the session) ran clean after one fix cycle on each of PR #101
(feature, 4 comments fixed) and PR #102 (post-merge closure, 2 comments
fixed). Merged as PR #101
(`ac8847b85ce2cea53a8f739530b35d3f6ea2ede4`) and post-merge closure PR #102
(`0cf49a81d5471026d17c81ea09db0d92f569a94b`), then safely archived
(single-artifact safe-close, protected set empty — full-feature shipment).

Key decisions and learnings (full detail retained in
`docs/closure/2026-08-17-serve-auto-discovery-followups-closure.md`,
`docs/closure/2026-08-17-serve-auto-discovery-followups-runtime-verification.md`,
`docs/closure/2026-08-17-serve-auto-discovery-followups-post-merge-closure.md`,
`docs/closure/2026-08-17-serve-auto-discovery-followups-compound-refresh.md`,
and the two new `docs/compound/` entries this shipment produced):

* An additive `graphtor_core::acquire::FileFilter` public API (SemVer-minor)
  was required — not merely an internal refactor — because the classifier
  lives in the `graphtor-docs` binary crate while the shared compiled matcher
  lived only privately in the `graphtor_core` library crate.
* The full error-observing `WalkDir` traversal was deliberately retained (no
  traversal short-circuit) to preserve the fail-closed contract that gates
  read-only vs read-**write** `Generation` serve posture — a rejected
  short-circuit alternative would have silently escalated a
  partially-unreadable source to read-write.
* Served-alias canonicalization (`055.002-T`) concluded a documented no-op:
  the existing canonical-path `BTreeSet` dedup already handles union assembly,
  shared-alias collapse, and outside-alias rejection with no gap found.
* A `tracing::EnvFilter` test helper initially targeted the wrong crate name
  (`graphtor_core` instead of `graphtor_docs`, since `serve_discovery.rs`
  compiles into the binary crate) — caught and fixed within the same task;
  captured as a new, distinct compound learning
  (`docs/compound/tracing-envfilter-wrong-crate-target-2026-08-17.md`) separate
  from the pre-existing callsite-interest-cache-race entry.
* A PowerShell git-commit-message quoting pitfall was captured as a second new
  compound learning
  (`docs/compound/workflow-issues/git-commit-powershell-embedded-quotes-2026-08-17.md`).
* One follow-up item stashed for Stage triage: `8C2E313D` (post-deploy
  observation window close-out — 3 local `serve` startups or 24h, whichever
  first; asynchronous, non-blocking, owned by the developer).
* The P-017 dark-factory activation scope (`970AE45A`, `5D98DBCC`, `B88E37BF`,
  `5868A7C5`) is now fully closed across both shipments.

## Action

* Archived the build-phase checkpoint
  `docs/memory/2026-08-17/048-s-build-checkpoint-implementation-complete.md`
  (superseded by the post-merge closure artifacts above, which are the
  durable, more complete record) to `docs/archive/memory/2026-08-17/`.
* Left `docs/memory/2026-08-17/048-s-session-closure-memory.md` in place — it
  is the final session-closure record for `048-S` and is not superseded by
  anything more complete.
* Did not touch `docs/memory/2026-08-16/dark-stage-session-complete-memory.md`:
  its reference to the (now-archived) plan path is a historical statement of
  what a specific Stage commit contained, not a forward-navigation link, so
  rewriting it would misrepresent that commit's actual historical contents.

## Result

* One superseded build checkpoint archived to `docs/archive/memory/`, and
  this compaction report added to `docs/memory/compacted/` — a like-for-like
  move plus one durable summary, not a net reduction in `docs/memory/`
  content.
* Durable record for `048-S` now lives in `docs/closure/` (closure, runtime
  verification, post-merge closure, and compound-refresh artifacts) and
  `docs/compound/` (2 new learnings), per the Durable Knowledge Layout
  convention.
* Companion action in this same pass: the two completed, reviewed exec-plans
  behind `047-S` and `048-S`
  (`docs/archive/plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md`
  and `docs/archive/plans/2026-08-16-serve-auto-discovery-followups-plan.md`)
  were consolidated into decided-plans and archived; see
  `docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-decided-plan.md`
  and `docs/exec-plans/2026-08-16-serve-auto-discovery-followups-decided-plan.md`.
