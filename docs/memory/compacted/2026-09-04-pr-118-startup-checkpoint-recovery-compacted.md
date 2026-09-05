---
type: compaction-report
date: 2026-09-04
target: memory
slug: pr-118-startup-checkpoint-recovery-compacted
source_count: 2
archive_root: docs/archive/memory/2026-09-04
---

# Memory Compaction — PR #118 Startup Checkpoint Recovery (2026-09-04)

## Trigger

Ship's mandatory P-020 post-merge closure `compact-context` invocation
(`target: all`), run after merge of PR #118 (`255020e14df99767549253d56ec3d53aa0b2bbd7`)
and after writing this closure session's own memory checkpoint
(`docs/memory/2026-09-04/post-merge-closure-pr-118-session-memory.md`).
Per the mandatory-invocation/threshold-gated-selection contract, the
just-merged PR's own lifecycle memory is the intended default candidate: it
is now part of a completed unit of work (Phase 2, "part of a completed
feature or chore"), regardless of age (both files are 0 days old — the
completed-work rule applies independent of the `threshold_days` age gate).

## Candidates Compacted (2 of 2 assessed, 2 compacted, 0 excluded from this pair)

| Original | Archived to |
|---|---|
| `docs/memory/2026-09-04/pr-118-readiness-copilot-remediation-memory.md` | `docs/archive/memory/2026-09-04/pr-118-readiness-copilot-remediation-memory.md` |
| `docs/memory/2026-09-04/pr-118-cycle4-circuit-breaker-halt-memory.md` | `docs/archive/memory/2026-09-04/pr-118-cycle4-circuit-breaker-halt-memory.md` |

`git grep` confirmed zero external citations to either original path from
`docs/compound/`, `docs/closure/`, `docs/exec-plans/`, `docs/decisions/`, or
`docs/design-docs/` — no cross-reference reconciliation was needed outside
the two files' own mutual citation (fixed below). Both files were
Ship-authored (`agent: "ship"` frontmatter on the readiness file;
first-person Ship session-report style throughout the cycle4 file, no
`agent:` override to another owner) — Ship has authority to compact both.

**Two related files in the same date directory were deliberately NOT
compacted** (not part of this pair, and not otherwise eligible):

* `docs/memory/2026-09-03/checkpoint-quarantine-recurrence-controls-memory.md`
  — its own "Open work" note is superseded by the sibling file below, but it
  is cross-referenced *by* that sibling (which is itself excluded), and
  compacting it in isolation would orphan that citation without a
  compelling benefit. Preserved as-is.
* `docs/memory/2026-09-03/checkpoint-resolution-and-049s-topology-blocker-memory.md`
  — documents **live, unresolved** work (the `049-S` topology-gate blocker
  against archived `048-S`'s missing `shipped` lifecycle event). This is
  explicitly **not** "part of a completed feature or chore" — the blocker
  remains open as of this compaction pass. Compacting or archiving this file
  would risk a future Stage/operator session losing the operational detail
  needed to remediate `049-S`. Excluded under the Behavioral Constraint
  "never compact checkpoints for active work" (extended here to active
  *investigation* state, not merely active backlog-item status), consistent
  with the precedent set in the 2026-09-01 047-S/048-S compaction record for
  excluding still-relevant content on non-age grounds.
* This closure session's own new memory file
  (`docs/memory/2026-09-04/post-merge-closure-pr-118-session-memory.md`) is
  also excluded: it describes the closure session that produced this very
  compaction pass, which is not yet complete (awaiting closure-PR review and
  operator merge approval).

## Consolidated Record

### PR #118 Lifecycle — Local Review, Copilot Shadow-Review Remediation, Merge-Approval Gate, Cycle-4 Circuit Breaker

**Decisions made**:

* No shipment claimed or mutated (`048-S`/`049-S` untouched) — chore-branch-only
  PR lifecycle, per explicit operator instruction.
* Local adversarial review (3-reviewer consensus, report-only) on the base
  diff returned `READY_WITH_FOLLOWUPS`, P0=0/P1=0; one out-of-scope finding
  (ad hoc git-diagnostic scripts) captured as stash `CCAC612D` per P-021 C2
  rather than fixed in-branch.
* Discovered a repository ruleset (`rules/branches/main`, id `13816903`) that
  auto-engages Copilot code review on every push and requires all review
  threads resolved before merge — the classic branch-protection API alone
  (404, "not protected") is insufficient to conclude Copilot/thread-resolution
  is not engaged; the newer Rulesets API must also be checked. Recorded as a
  compound-learning candidate if this pattern recurs (not separately
  captured as a compound entry — noted here for traceability only).
* Four consecutive Copilot review-fix rounds occurred across the PR's
  lifecycle. Round 4 exceeded the Ship agent's 3-cycle circuit-breaker limit
  for review-comment fix cycles; per that directive, the two round-4
  findings were initially left unresolved and presented to the operator.
* On resumption, the operator authorized extending the review-fix budget to
  a 4th cycle. Both round-4 findings were investigated against the actual
  repository layout (not accepted at face value) and confirmed to require
  editing files that do not exist in this repository's version-controlled
  source tree at all (`scripts/start.ps1.tmpl` / `scripts/start.sh.tmpl` —
  present only in the externally-versioned, pip-installed/vendored
  `autoharness` tool project, confirmed via `git ls-files`,
  `git check-ignore -v`, and absence of a `.git` directory in the
  pip-install location). This fails the P-021 C1 same-contract-surface test
  categorically (not same-repo-different-surface, but a different
  externally-versioned project entirely) — resolved via P-021 C2
  defer-capture (stash entries `BAD41DF2`, `8AFB7B3A`) instead of a
  code/template edit, since none was possible within this PR's commit.
* Both round-4 threads were replied-to (citing the deferred entry IDs) and
  resolved only after those replies were posted, per the capture-first
  ordering. `mergeStateStatus` moved `BLOCKED` → `CLEAN`;
  `autoharness gate copilot-review 118 --enforcement auto` →
  `SATISFIED: PASS`.
* Applied 4 genuine in-scope Copilot findings directly across the PR's
  earlier rounds (`.mcp.json` backslash path; missing `${workspaceFolder}`
  env bindings; `.mcp.json`/`start.sh` sync-removal inconsistency; a
  stale-tense compound-doc paragraph) — all within files this branch itself
  modified, confirmed in-scope per P-021 C1/C3.
* Did not fix a 5th finding (stash `CCAC612D`'s own `Kind: chore` text vs.
  its actual `kind: task` field) — editing a captured stash entry is outside
  Ship's create-only stash authority (Role Boundary, single-write capture
  invariant); flagged for Stage's triage of `CCAC612D` instead.
* Final `## Local Review Readiness` block confirmed `READY_WITH_FOLLOWUPS`
  (P0=0/P1=0) at head `f2d42474b272084aa77eaeaef6021b515ca5e4dc` after round
  3; readiness was re-confirmed again after round 4's defer-capture
  resolution at a later head, per the §1.9 gate's re-run-on-HEAD-advance
  rule.
* Did not merge. Remained on `fix/graphtor-startup-checkpoint-recovery`
  throughout; never checked out `main` mid-lifecycle.

**Files modified across the full PR lifecycle** (see the closure artifact,
`docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md`,
for the authoritative merged-diff summary): `start.sh`, `start.ps1`,
`.mcp.json`, `.autoharness/config.yaml`, `.gitignore`,
`docs/configuration.md`, `.backlogit/checkpoints/*` (quarantine),
`.backlogit/stash.jsonl` (4 P-021 captures), plus session memory and
compound-learning files.

**Key learnings**:

* GitHub repository rulesets (`/rules/branches/{branch}`) can auto-engage
  Copilot review and thread-resolution requirements even when the classic
  branch-protection API reports "not protected" (404) — both APIs must be
  checked to determine true merge-gating behavior.
* `-F`/`--field` (not `-f`) is required for `gh api graphql` to support
  `@file` magic when loading a GraphQL query from a file.
* A stash entry's own text can itself become a Copilot review finding
  (`Kind: chore` vs. `kind: task` field mismatch) — Ship cannot fix this
  because editing a captured stash entry is outside its create-only
  authority; this is a durable pattern worth remembering if it recurs.
* When a Copilot finding asks for a change to a `.tmpl` file, verify with
  `git ls-files` whether that file is actually tracked in *this*
  repository before assuming it is same-repo, in-scope work — it may
  belong to a separate, externally-versioned tool project entirely (as it
  did here, for the `autoharness` package's own template directory).

**Failed approaches / dead ends**: none recorded — the 4-round Copilot
cycle was a normal review-remediation progression, not a failed approach;
each round's findings were either fixed (rounds 1–3, in-scope) or correctly
identified as out-of-scope and deferred (round 4).

**Outcome**: PR #118 merged (`255020e14df99767549253d56ec3d53aa0b2bbd7`) with
all CI checks green, all review threads resolved, `SATISFIED: PASS` on the
P-018 gate, and `READY`/`READY_WITH_FOLLOWUPS` local review readiness
recorded at each re-verified HEAD. Full post-merge closure completed
separately — see
`docs/closure/2026-09-04-pr-118-startup-checkpoint-recovery-post-merge-closure.md`.

## Result

2 memory files compacted into this 1 consolidated summary; 2 files archived
to `docs/archive/memory/2026-09-04/` (byte-preserved, only 2 internal
cross-reference paths corrected — the cycle4 file's citation of the
readiness file's new location, and the readiness file's own frontmatter
`source` field); 0 files deleted; 0 external tracked citing documents found
(none needed updating); 2 sibling 2026-09-03 memory files and this closure
session's own new memory file reviewed and explicitly preserved (not
compacted), per the rationale above.
