---
title: "Post-merge closure session for PR #118 (startup checkpoint recovery)"
description: "Records the mandatory Ship post-merge closure protocol executed for merged PR #118: merge confirmation, closure branch creation, quality-gate re-verification, closure artifact, compound-refresh review, and P-020 compact-context invocation, stopping at the closure PR readiness gate pending explicit operator approval"
source: "docs/memory/2026-09-04/post-merge-closure-pr-118-session-memory.md"
doc_type: "memory"
date: "2026-09-04"
agent: "ship"
---

## Outcome

Completed mandatory post-merge closure (Ship agent Step 6) for merged PR
#118 (`chore(startup): fix startup checkpoint recovery and quarantine
legacy checkpoints`), merge commit `255020e14df99767549253d56ec3d53aa0b2bbd7`.
No shipment was claimed, created, or mutated at any point — this was a
standalone chore-branch PR outside the backlogit shipment pipeline.
Shipments `048-S` and `049-S` were read-only-verified as unchanged
(`archived`/`archived_status: active` and `queued`, respectively) and were
not touched. A dedicated `post-merge/startup-checkpoint-recovery` branch was
created from freshly-fetched `origin/main` for all closure artifacts; no
commit landed directly on `main`. Session stopped at the closure PR
readiness gate — merge approval was not sought or given, per the operator's
explicit instruction that main-PR approval does not carry to a closure PR.

## Session scope

Single continuous closure session. No prior Ship-owned active checkpoint
existed (`backlogit checkpoint list` showed 9 records, all `stage`-owned and
already `resolved`) — normal zero-candidate startup, no crash-recovery
needed.

## Timeline

1. **Merge Confirmation Gate**: `gh pr view 118 --json state,mergedAt,mergeCommit`
   → `state: MERGED`, `mergedAt: 2026-09-05T00:06:18Z`,
   `mergeCommit.oid: 255020e14df99767549253d56ec3d53aa0b2bbd7` (matches the
   SHA supplied for this session). `git fetch origin main` +
   `git merge-base --is-ancestor 255020e origin/main` → exit 0.
2. **Hook/index hygiene** (session-start protocol): polled
   `backlogit hooks poll --consumer-id ship` (1166 unacked generic
   create/update-artifact events, none of the `post_merge_closure` /
   `feature_review_ready` special signal types, none referencing this
   PR/branch) and acknowledged through `seq: 1166`. Ran `backlogit sync`
   (`INDEX_SYNC_OK`, 521 artifacts indexed).
3. **Shipment applicability check**: confirmed via `backlogit search` (zero
   hits) and direct `backlogit get 048-S` / `backlogit get 049-S` that no
   shipment covers this PR. Ran
   `autoharness gate pipeline-topology --mode agent --phase ambient
   --shipment ...` (rejected — agent mode always requires `--shipment`) then
   `--mode manual --phase ambient` (no shipment needed) → `PASS`,
   `worktree_topology: WORKTREE_TOPOLOGY_OK`,
   `active_shipment_invariant.active_shipment_ids: []`. Recorded
   `shipment-reconcile` as non-applicable with full rationale in the closure
   artifact.
4. **Branch protocol**: `git checkout main` → `git pull` (fast-forwarded
   12 commits to `255020e`) → `git checkout -b
   post-merge/startup-checkpoint-recovery`. `git worktree list --porcelain`
   confirmed single worktree throughout (P-016).
5. **Quality-gate re-verification on the merge commit**: `cargo check
   --all-targets` (8.52s, clean), `cargo fmt --all -- --check` (clean),
   `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` (clean),
   `cargo test` (all binaries + doc-tests green, 0 failed), `cargo audit`
   (plain invocation reports 1 pre-existing vulnerability
   `RUSTSEC-2026-0041` + 6 unmaintained-crate advisories — all pre-dating
   this PR, already tracked in `audit.toml`/task `013.008-T`; the exact
   CI-equivalent `--ignore ... --deny warnings` invocation from
   `.github/workflows/ci.yml` exits 0 clean). `gh pr checks 118` confirmed
   all 3 CI checks (`build`, `detect code changes`, `pipeline topology
   gate`) passed at merge.
6. **Content review**: read the full PR diff from the true branch base
   (`git merge-base 5ba7fbe 176a198` = `5ba7fbe`) to `176a198` to build an
   accurate summary — confirmed no `src/`/`Cargo.*` changes; changes limited
   to `start.sh`/`start.ps1`, `.mcp.json`, `.autoharness/config.yaml`,
   `.gitignore`, `.github/copilot/settings.local.json`,
   `scripts/deploy-harness.*`, two new ad hoc git-diagnostic scripts, and
   `.backlogit/` checkpoint quarantine + stash + memory/compound artifacts
   already committed within the PR itself.
7. **Read the 4 pre-existing P-021 stash entries** (`CCAC612D`, `578B8678`,
   `BAD41DF2`, `8AFB7B3A`) captured during the PR's own lifecycle — all
   still `active`, none touched (read-only lookup only, per Role Boundary).
8. **compound-refresh review**: checked the two `docs/compound/` entries
   most relevant to this PR's changes
   (`checkpoint-schema-and-lifecycle-controls-2026-09-03.md`,
   `mcp-json-workspacefolder-camelcase-2026-08-24.md`). Both remain accurate
   and complete; classified **keep** for both, no edits made. The
   `.mcp.json` env-binding gap this PR fixed is a recurrence of the same
   root cause the second entry already documents (its Prevention guidance
   was followed correctly again) — not a new distinct pattern warranting a
   new entry.
9. **Runtime-verification / operational-closure**: initially (incorrectly)
   recorded `runtime-verification: N/A` on the theory that `runtime_surfaces`
   was all `false` in `workspace-profile.yaml`. **Corrected during the
   closure PR's own local review**: `runtime_surfaces` and
   `runtime_validation.validator_manifest` are different sections — the
   latter defines a required `cli` surface (`cli-status`,
   `mcp-stdio-startup` probes, `mcp-client-smoke` manual checkpoint) that
   this PR's launcher/`.mcp.json` changes squarely touch. Ran both required
   automated probes against the merge commit: `cli-status` PASS;
   `mcp-stdio-startup` started cleanly with no startup-phase error but its
   exit-on-stdin-EOF result does not literally satisfy the manifest's
   declared clean-exit signal (the probe sends no `initialize` request, so
   this is a known probe-methodology limitation — not evidence of a
   regression — pending an initialize-sending harness, tracked as PR #114).
   Also ran an isolated verbatim-logic replay of the
   `start.ps1` cwd-anchoring/`.env.local`-override fix (PASS). Could not
   perform the required `mcp-client-smoke` manual checkpoint (no live MCP
   client in this session) — recorded as deferred per the validator
   manifest's own documented fallback. Both open items cap this at
   `READY_WITH_CONDITIONS`. Updated the closure artifact's frontmatter and
   releasability section accordingly (`status: READY_WITH_CONDITIONS`).
   Wrote the full `operational-closure` artifact:
   `docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md`
   (`compaction: pending` at time of writing).
10. **Doc/knowledge graduation review**: grepped `docs/ARCHITECTURE.md` and
    `AGENTS.md` for `start.sh`/`start.ps1`/`checkpoint` references — nothing
    stale found; neither file touched. `docs/configuration.md` was already
    correctly updated within PR #118 itself.
11. **Source-artifact retirement check**: no covering feature/chore item
    exists for this PR, so no `custom_fields.source_stash_id` /
    `source_deliberation_id` to read — recorded as not-applicable, not
    skipped silently.
12. Wrote this memory checkpoint.

## Next steps (this session, immediately following)

* Invoke `compact-context` with `target: all` (P-020, mandatory per merge).
  Expected candidates: the two 2026-09-04 PR-118-lifecycle memory files
  (`pr-118-readiness-copilot-remediation-memory.md`,
  `pr-118-cycle4-circuit-breaker-halt-memory.md`) — both part of the now
  fully-completed, merged PR #118 work, no external citations found via
  `git grep`. **Explicitly excluding** both 2026-09-03 files
  (`checkpoint-quarantine-recurrence-controls-memory.md`,
  `checkpoint-resolution-and-049s-topology-blocker-memory.md`) from
  compaction: the second is cited by the resolution file's own reference,
  and — more importantly — the resolution file documents *open, unresolved*
  work (the `049-S` topology blocker) that a future session needs intact
  and undisturbed, not a completed unit eligible for compaction. This
  memory file (the one you are reading) is also excluded — it describes
  this closure session itself, which is not yet complete pending closure PR
  review/approval.
* Update this closure artifact's `compaction` frontmatter field to `done`
  (or `degraded` if compact-context fails — non-blocking either way) after
  the invocation completes.
* Run `backlogit sync` again for closure-index hygiene.
* Stage the closure artifact + this memory + compaction outputs, commit
  with conventional-commit format + emoji footer + Copilot trailer, push
  `post-merge/startup-checkpoint-recovery`, and open a dedicated closure PR
  to `main` with a current-HEAD `## Local Review Readiness` block.
* Run local review + the §1.9/P-018 readiness gates for the **closure PR**
  itself (a fresh gate pass — the original PR #118 approval does not carry
  over).
* Stop at merge-ready. Per explicit operator instruction, a distinct closure
  PR approval is required before merge; do not invent or assume approval.
  Remain on the closure branch awaiting that approval.

## Decisions

* Do not claim, create, or mutate any shipment for this closure (no
  shipment exists for this PR's scope; fabricating one is a P-010
  violation).
* Do not touch `048-S`/`049-S` in any way beyond read-only verification.
* Do not compact or otherwise edit
  `checkpoint-resolution-and-049s-topology-blocker-memory.md` — it documents
  live, unresolved cross-shipment work that must remain discoverable and
  intact for whichever future session remediates the `049-S` blocker.
* Do not create, edit, or archive any of the 4 pre-existing P-021 stash
  entries — cite them read-only in the closure Follow-Up Handoff section.
* ~~Treat the launcher scripts and `.mcp.json`/config changes as
  non-runtime-surface (dev-tooling only) rather than inventing a
  runtime-verification probe.~~ **Superseded — see Addendum below.** This
  was wrong: `runtime_validation.validator_manifest` is a separate,
  independently-required contract from the `runtime_surfaces` booleans, and
  it does apply to this PR's launcher/`.mcp.json` changes.

## Addendum — Closure PR #119 local review remediation (same session)

The closure PR (`#119`) was created, and a 3-reviewer adversarial local
review (report-only) was run against its own diff before presenting it as
merge-ready. First pass at HEAD `4d5ea72` returned `READY_WITH_FOLLOWUPS`
(2 LOW-confidence wording nits, fixed directly at `e526222`). After PR
creation, GitHub's Copilot shadow-review (auto-triggered by the same
repository ruleset that governed PR #118) surfaced **5 substantive,
actionable findings** against the closure artifact's own content — not
wording nits, but factual/completeness corrections:

1. Runtime-verification was wrongly marked `N/A` — corrected per the
   Decisions-list strikethrough above; the required automated probes were
   run against the merge commit, `mcp-client-smoke` recorded as an
   outstanding condition (`READY_WITH_CONDITIONS`).
2. Same incorrect `N/A` claim mirrored in this memory file (§9 above) —
   corrected in place.
3. The closure's `.env.local` history claim ("`start.sh` did not load it at
   all") was factually wrong — the pre-fix Bash launcher did load it, with
   a skip-if-already-set guard; `25a1290` removed that guard. Corrected.
4/5. The closure understated the Copilot review-round count (said 4; the
   verified GraphQL history shows 7 total review submissions — 5 with new
   actionable comments, 2 clean/informational). Corrected in both the
   closure artifact and the (already-compacted) memory summary, and a new
   Follow-Up Handoff item was added for a previously undiscovered residual
   finding surfaced by this investigation: 6 merged
   `.backlogit/archive/checkpoints/*.disposition.json` files publicly
   record a specific Windows domain account identifier (redacted here —
   see the merged files themselves for the literal value, not repeated in
   this documentation) — Copilot's own final pre-merge review flagged this
   as 6 suppressed comments that were never individually surfaced as an
   actionable thread during PR #118's lifecycle. This closure cannot
   rewrite already-merged history to redact it; recorded as a Stage
   follow-up instead.

All 5 findings were fixed directly in a follow-up commit (in-scope,
same-contract-surface corrections to this closure's own content — P-021
C1/C3), pushed, and the corresponding review threads replied to and
resolved. See the closure artifact's updated Summary/Validator
Evidence/Releasability/Follow-Up sections for the corrected content.

## Addendum 2 — Closure PR #119 second Copilot review round (same session)

Pushing the round-1 fix commit (`a150d72`) re-armed Copilot review, which
surfaced **6 further actionable findings** at the new HEAD:

1/2/3. **The round-1 fix itself republished the exposed domain-account
   identifier** — in the closure artifact (two locations) and in this
   memory file's own Addendum text — increasing its public visibility
   rather than reducing it. Corrected by redacting the literal value in all
   three locations and pointing instead to the merged `.disposition.json`
   files themselves as the source of truth, deliberately not creating any
   further searchable copy of it in documentation.
4. **The compacted summary also republished the identifier** (line ~164 of
   the P-020 compacted file) — same redaction applied there.
5. **The `mcp-stdio-startup` probe result was recorded as an unconditional
   PASS, but the manifest's declared success signal requires a clean exit
   on stdin EOF, and the probe actually exited with an error (exit code 2,
   `connection closed: initialize request`)** — a genuine factual
   correction, not a wording nit. Investigated against
   `.autoharness/workspace-profile.yaml`'s validator manifest and the
   already-cited exec-plan: the manifest's own note explains this probe
   sends no `initialize` request and therefore cannot distinguish a
   regression from normal handshake behavior, so the observed exit is a
   known probe-methodology limitation (identical on any build run this
   way) rather than evidence PR #118 introduced a regression — but it is
   NOT a literal PASS of the manifest's declared signal either. Rewrote the
   closure artifact's probe row, verdict, and releasability
   evidence/status to state this precisely (still `READY_WITH_CONDITIONS`,
   now with two explicit conditions instead of one), and added a 7th
   Follow-Up Handoff item pointing at the missing initialize-sending
   harness (tracked as PR #114 in the manifest's own note).
6. **The PR body's `## Local Review Readiness` block was stale** —
   advertised `READY` at reviewed HEAD `e526222` while the closure artifact
   already recorded `READY_WITH_CONDITIONS` at a later HEAD. **Confirmed
   corrected**: the PR body was updated via `gh pr edit` to the
   then-current HEAD
   (`8e484f4`, after the second review round's own follow-up fixes landed),
   with a full history of both local-review passes and both Copilot
   shadow-review rounds — independently re-verified via
   `gh pr view 119 --json body` showing `Reviewed HEAD: 8e484f4...` live on
   the PR, not merely planned. (This HEAD was itself superseded by a later
   commit — see Addendum 3 below, which also states the PR body's
   subsequent update status explicitly, to avoid repeating this same
   staleness pattern.)

All 6 findings were investigated before fixing (per the same
investigate-before-fixing discipline as Addendum 1) and fixed directly
in-scope (P-021 C1/C3) in a second follow-up commit, pushed, and the
corresponding review threads replied to and resolved.

## Addendum 3 — Closure PR #119 third Copilot review round + local-review re-pass (same session)

A second local adversarial review pass (against the `e526222..ada4cf7`
diff, where `ada4cf7` is the second follow-up commit referenced in
Addendum 2 above) returned `READY_WITH_FOLLOWUPS` (2 LOW-confidence, MINOR
advisory nits confined to the compacted memory file's pre-existing
round-count narrative — neither introduced by the corrective diff itself).
Both were fixed directly (in-scope, low-effort wording reconciliation),
committed as `8e484f4`, and pushed. The PR body was then updated
(`gh pr edit`) to the then-current HEAD (`8e484f4`) with the full local-
and shadow-review history.

Pushing `8e484f4` re-armed Copilot review a third time, surfacing **3
further findings**:

1. **A reintroduced N/A-adjacent contradiction**: the Summary's own
   "zero product runtime surface touched" sentence (describing the absence
   of Rust source changes) contradicted the already-corrected Validator
   Evidence section. Fixed by distinguishing "no Rust/binary implementation
   changed" from "no runtime surface touched" — the latter is false; the
   launcher/`.mcp.json` changes do touch the required `cli` surface.
2. **Post-deploy observation window incorrectly declared closed**: the
   `release-observability` capability pack (confirmed installed via
   `.github/instructions/release-observability.instructions.md`) requires
   an explicit, bounded observation window (duration + owner) for
   runtime-affecting work, not a "no window" declaration — especially with
   `mcp-client-smoke` still outstanding. Corrected: opened an explicit
   14-day (or until `mcp-client-smoke` completes, whichever first) window,
   owner `@softwaresalt`, tied to the same Follow-Up Handoff item as the
   outstanding manual checkpoint.
3. **A stale self-referential claim in this very memory file** (Addendum 2,
   item 6): at the time that sentence was written (during the `ada4cf7`
   commit), the PR body had not yet actually been updated — the sentence
   described a still-pending action in a way that could be misread as
   already complete. By the time this third Copilot round ran, the PR body
   update genuinely had landed (see above), so this was a timing artifact,
   not a live inaccuracy; the wording was nonetheless sharpened to state
   the completion definitively and cite independent re-verification.

All 3 findings were investigated, confirmed genuine (not false positives —
finding 1 was a real reintroduced contradiction; finding 2 reflected a real
gap against an actually-installed capability pack's contract; finding 3 was
a legitimate wording-precision gap), and fixed directly in-scope (P-021
C1/C3) in a third follow-up commit, pushed, and the corresponding review
threads replied to and resolved.

## Addendum 4 — Final local-review pass and chronology cleanup (same session)

A third local adversarial review pass (against the `8e484f4..d62f6d5` diff)
returned `READY_WITH_FOLLOWUPS`: one HIGH-confidence (3/3 reviewer)
consensus finding, plus 4 LOW-confidence unique advisory nits. All three
of the pass's specifically-targeted verification questions (runtime-surface
wording, Validation Window three-way consistency, Addendum 3 narration
accuracy) were independently confirmed correct by all 3 reviewers.

The consensus finding: Addendum 2/3 above described `8e484f4` as "the
final HEAD" and asserted a re-verified PR-body update, but this file's own
Addendum 3 later documented that `8e484f4` was itself superseded by
`d62f6d5` without ever stating the PR body's corresponding update status —
a self-referential chronology gap in exactly the failure class this
document's own remediation history had already been correcting. Fixed
directly: reworded "final HEAD" to "then-current HEAD" in both Addendum 2
and Addendum 3, explicitly anchored `ada4cf7` as "the second follow-up
commit," and this addendum itself now states the PR body's update status
for the current HEAD (below) rather than leaving it implicit.

Also fixed: a "should" → "MUST" rigor-language gap in the closure
artifact's Validation Window / Follow-Up Handoff item 6 outcome-recording
language, matching the release-observability instructions' own imperative
wording for recording an observation window's outcome at close.

**Accepted as follow-ups, not fixed in this closure** (LOW-confidence,
single-reviewer, non-blocking per the review's own priority ordering):
consolidating the Pre-Deploy Audits section into an explicit
release-observability checklist format; consolidating the Monitoring Plan
section into an explicit structured checklist; and a debatable wording-nit
about whether "timing artifact" adequately characterizes a since-corrected
premature claim. None of these affect the substantive releasability
claims (runtime verification, validation window, releasability evidence
all independently confirmed internally consistent by all 3 reviewers), so
this closure does not expand further to address them — recorded here for
traceability, not as a Follow-Up Handoff item (they are about this
closure's own document structure, not a future code/config action).

Both fixes were committed as a fourth follow-up commit and pushed. The PR
body was updated (`gh pr edit`) to reflect this commit as the current
reviewed HEAD, closing the chronology gap the consensus finding
identified — this addendum is written after that update, not before it,
specifically to avoid repeating the same premature-claim pattern a fourth
time.

This is the fourth and final round of review-driven correction in this
closure session (3 Copilot shadow-review rounds + this final local-review
pass, all now resolved); per Ship's own review-fix-cycle circuit breaker,
this closure treats this as the natural stopping point rather than
continuing to iterate.
