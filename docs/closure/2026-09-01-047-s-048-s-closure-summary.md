---
type: compaction-report
date: 2026-09-01
target: closure
slug: 047-s-048-s-closure-summary
source_count: 6
source_bytes: 44532
archived_bytes: 44784
archive_root: docs/archive/closure/2026-09-01-047-s-048-s-compaction
shipments: ["047-S", "048-S"]
---

# Closure Compaction — 047-S / 048-S (2026-09-01)

## Trigger

Copilot review on PR #116 (thread `PRRT_kwDORiB5E86eRfKU`) correctly identified
that `target: all` compaction cannot limit closure assessment to the
just-closed `051-S` release unit's own record. Six closure artifacts dated
`2026-08-17`, belonging to shipments `047-S` and `048-S`, are both (a)
`> threshold_days` old (15 days as of 2026-09-01, over the 14-day default)
and (b) tied to fully complete, archived features/chores — `047-S` and
`048-S` (feature `055-F`) are both present only in `.backlogit/archive/`,
with no live queue entry — satisfying compact-context Phase 2's closure
rule ("Feature or chore is complete AND more than `threshold_days` old")
without exception. This pass compacts them for real.

## Candidates Compacted (6 of 6 assessed, 6 compacted, 0 excluded)

| Original | Shipment | Archived to |
|---|---|---|
| `2026-08-17-047-s-post-merge-closure.md` | `047-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-047-s-post-merge-closure.md` |
| `2026-08-17-047-s-release-observability-evidence.md` | `047-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-047-s-release-observability-evidence.md` |
| `2026-08-17-serve-auto-discovery-followups-closure.md` | `048-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-closure.md` |
| `2026-08-17-serve-auto-discovery-followups-compound-refresh.md` | `048-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-compound-refresh.md` |
| `2026-08-17-serve-auto-discovery-followups-post-merge-closure.md` | `048-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-post-merge-closure.md` |
| `2026-08-17-serve-auto-discovery-followups-runtime-verification.md` | `048-S` | `docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-runtime-verification.md` |

No exclusion was applied to any of the six candidates. Every one of them
qualifies squarely under the closure rule above; no Behavioral Constraint
("never delete files," "never compact active-item checkpoints," "preserve
the most recent checkpoint for each completed task") withholds a closure
record — those constraints govern memory/checkpoint compaction, and this
group's shipments carry no open manifest member.

**Preserved under a Behavioral Constraint, not left out of scope**:
`docs/memory/2026-08-17/047-s-session-closure-memory.md` and
`docs/memory/2026-08-17/048-s-session-closure-memory.md` were assessed
against compact-context Phase 2's memory rules (they qualify under "part of
a completed feature or chore," since both `047-S` and `048-S` are fully
archived). They are **not compacted**, per the skill's own Behavioral
Constraint: "Preserve the most recent checkpoint for each completed task."
Both files are Ship-authored (no `agent:` frontmatter override; their body
text is written in first-person Ship session-report style), so Ship has
authority to compact them and this exclusion is a genuine content-based
one, not a Role Boundary substitute.

**Correction (2026-09-01, PR #116 round-3 remediation)**: the original text
here additionally and incorrectly claimed these were the *sole* remaining
live checkpoint for each shipment. `docs/memory/2026-08-16/dark-stage-session-complete-memory.md`
also names both `047-S` and `048-S` (a Stage handoff memory recording both
shipments as "ready for Ship," predating the two 2026-08-17 files). The
correct, narrower claim is that the two 2026-08-17 files are each the *most
recent* checkpoint for their shipment — the 2026-08-16 file is earlier, not
later, so it does not disturb that ordering. The 2026-08-16 file itself is
a **separate, Role-Boundary-excluded candidate**: its frontmatter declares
`agent: Stage`, and it qualifies under Phase 2's completed-work rule on
content grounds (both shipments it names are now fully archived) but is
**not compacted** because Ship's Continuity row unconditionally forbids
mutating another agent's checkpoint or memory — a different, ownership-based
exclusion from the "most recent checkpoint" rule applied to the two
2026-08-17 files above. Recorded as a follow-up handoff for a future Stage
session, not acted on here.

## Consolidated Record

### 047-S — Read-Only Serve Guarantee Honesty

Resolved PR #90 deferrals F2/F6: corrected an overstated read-only serve
guarantee across `src/db/store.rs` (`is_engine_enforced_readonly`,
`open_engine_readonly`, `open_sqlite_readonly`, `EngineReadonlyGuard`
rustdocs, new `ENGINE_READONLY_OPEN_LOG_MESSAGE` constant) and
`docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`,
without changing any guard runtime behavior
(`EngineReadonlyGuard::lock`/`Drop` byte-identical to pre-shipment `main`;
`is_engine_enforced_readonly()` continues to return exactly
`self.engine_readonly_guard.is_some()`). Merged as PR #97, commit
`704b95a6c1e2930079d6f3a602ab66e9682d4916` (merge-commit strategy, P-009),
archived via single-artifact safe-close (protected set empty).

* **Verified**: live CLI smoke test (`cargo build --release` + `serve
  --read-only`) confirmed the qualified log line renders character-for-character
  as the new `ENGINE_READONLY_OPEN_LOG_MESSAGE` constant. Quality gates all
  green post-merge (`fmt`, `clippy --pedantic`, 555+ tests, `cargo audit`
  with the `RUSTSEC-2026-0249` allowlist addition).
* **Healthy/failure signals**: healthy = qualified log line renders as a
  single coherent line, no operator confusion; failure = line fails to
  render, is misread as reintroducing an unconditional guarantee, or a
  future log-scraping rule keyed on `"filesystem lock active"` silently
  stops matching.
* **Monitoring**: manual observation only (single-developer, local-only
  tool, no dashboard/alerting). Owner `@softwaresalt`.
* **Post-deploy observation window (residual, unresolved)**: defined as the
  first 10 `ReadOnly`-posture `serve` starts or 14 calendar days post-merge
  (i.e. by 2026-08-31), whichever came first. As of the 2026-08-17 closure
  record only 1 start had been observed and the window was recorded
  **open**; no later record closing it (healthy/degraded/rolled-back) has
  been found in this repository as of this compaction pass (2026-09-01).
  This compaction does **not** fabricate a close-out outcome — it preserves
  the open status as an honest residual, tracked by stash follow-up items
  `9CEC208C`, `C365AB98`, `3FFE51B4`, `B883681D`, `B8C0851E` (see the
  archived PR #97 description reference for full per-item disposition).
* **Rollback**: text-only revert to a shorter-but-still-qualified message
  (never back to the original unconditional wording); zero effect on guard
  runtime behavior either way.
* **Releasability status at closure**: `READY_WITH_CONDITIONS`.

### 048-S — Serve Auto-Discovery Follow-Ups (feature `055-F`)

Resolved two more PR #90 deferrals in `src/workspace/serve_discovery.rs`:
an additive `graphtor_core::acquire::FileFilter` public API consumed by a
refactored `filter_files` (`055.001.001-ST`), and an O(1)-memory streaming
`stream_ingestible` classifier reusing that shared `FileFilter`
(`055.001.002-ST`) — fail-closed-on-walk-error and no-short-circuit
invariants preserved and verified. `055.002-T` (served-alias
canonicalization) concluded a documented no-op: existing canonical-path
dedup already sufficient. Merged as PR #101 (feature), commit
`ac8847b85ce2cea53a8f739530b35d3f6ea2ede4`, and PR #102 (post-merge
closure), commit `0cf49a81d5471026d17c81ea09db0d92f569a94b`, both
merge-commit strategy. Shipment `048-S` closed via safe-close (protected
set empty — feature `055-F` fully covered by its 4 manifest descendants,
no cascade op used); confirmed via `git status --short -- ".backlogit/"`
showing only the expected archive rename + hooks-queue append.

* **Verified**: runtime verification `PASS` — CLI/manual subprocess
  invocation against the real compiled binary confirmed the `serve`
  startup posture-resolution log and aggregate exclusion-warning log across
  three scenarios (ingestible, excluded-only, zero-candidate), plus a
  platform-independent seam test for the later-walk-error regression case
  (Windows cannot simulate unreadable subtrees via ACLs; the existing
  `#[cfg(unix)]`-gated sibling test covers real-filesystem confirmation on
  Linux CI). No BLOCKED prerequisites.
* **Healthy/failure signals**: classification set identical to
  pre-refactor; fail-closed on any walk error (never promotes a
  partially-unreadable source from `ReadOnly` to `Generation`); aggregate
  "all files excluded" warning parity preserved.
* **Monitoring**: no feature flags/migrations/rollout gates; same-process
  behavior-preserving refactor plus one additive library API.
* **Post-deploy follow-up (residual)**: stash item `8C2E313D` — observe the
  next 3 local `serve` startups (or 24h) and record the outcome per the
  monitoring plan. No later record of this item's disposition has been
  found in this repository as of this compaction pass; preserved as an
  open residual, not fabricated closed.
* **Knowledge graduation**: two new `docs/compound/` entries captured
  (`tracing-envfilter-wrong-crate-target-2026-08-17.md`,
  `workflow-issues/git-commit-powershell-embedded-quotes-2026-08-17.md`);
  four existing related entries reviewed and kept as-is (no overlap
  requiring consolidation). No `docs/ARCHITECTURE.md`, `AGENTS.md`,
  `docs/design-docs/`, or `docs/product-specs/` updates were required.
* **Source-artifact cleanup**: both tasks' `source_stash_id`s
  (`B88E37BF`, `5868A7C5`) were already absent from the active stash
  (consumed by Stage at harvest time, prior to this shipment's Ship
  session) — no action was needed then, and none is needed now.
* **Releasability status at closure**: `READY`. This was the last shipment
  in the P-017 activation scope (`970AE45A`, `5D98DBCC`, `B88E37BF`,
  `5868A7C5`) — all four stash IDs confirmed consumed.

## Follow-Up Items (residual, carried forward — not created or mutated by this compaction pass)

* `9CEC208C`, `C365AB98`, `3FFE51B4`, `B883681D`, `B8C0851E` — `047-S`
  post-deploy observation window and related follow-ups (open status
  preserved, per above).
* `8C2E313D` — `048-S` post-deploy observation window close-out (open
  status preserved, per above).

These stash IDs are recorded here purely as a read-only pointer for a
future Stage/operator session; this compaction pass creates, edits, or
archives no stash entry (Ship's Role Boundary forbids stash mutation,
P-010).

## Cross-Reference Reconciliation

Every tracked citation in a document Ship is authorized to edit was
rewritten to the corresponding `docs/archive/closure/2026-09-01-047-s-048-s-compaction/`
path, so the traceable path from this summary back to the original verbose
artifacts (and from every other Ship-authored tracked citing document) is
preserved, per the compact-context skill's "traceable path" constraint. Of
the **7 tracked external citing documents** identified, **5 were updated**:

* `docs/memory/2026-08-17/047-s-session-closure-memory.md`
* `docs/memory/2026-08-17/048-s-session-closure-memory.md`
* `docs/memory/compacted/2026-08-17-047-s-memory-compaction.md`
* `docs/memory/compacted/2026-08-17-048-s-memory-compaction.md`
* `docs/archive/memory/2026-08-17/047-s-build-checkpoint-pre-pr.md`

Plus internal cross-references among the six archived files themselves
(moved together into the same archive subdirectory; mutual citations
rewritten to the new path so they remain valid after the move).

**The other 2 of the 7 were deliberately NOT updated, and remain stale**:

* `docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-decided-plan.md`
* `docs/exec-plans/2026-08-16-serve-auto-discovery-followups-decided-plan.md`

Round 1 of this PR's remediation (`324bb37`) originally rewrote these two
decided-plans' citations, but that was a Ship Planning-row Role Boundary
violation — Ship has no authority to create or modify a plan artifact, even
for a citation-only, factually-accurate edit. Round 6 reverted both files
byte-for-byte to their pre-PR (`aa7e8ac`) state. Both decided-plans
therefore still cite the six pre-compaction `docs/closure/2026-08-17-*`
paths this pass archived — a real, known-stale reference, not a hidden
one — and updating them is Stage's Planning authority, not Ship's.
Recorded as a documentation-only follow-up handoff for a future Stage
session (full detail in the paired closure record's "Follow-up handoff
(P-020 compaction, stale decided-plan citations...)" section).

**A third, separate known exception** (not one of the 7 tracked citing
documents above — a Stage-owned artifact, not a document Ship authored or
could ever have been authorized to rewrite): `.backlogit/stash.jsonl` line
7 (the live stash entry `8C2E313D`) contains a prose reference to
`docs/closure/2026-08-17-serve-auto-discovery-followups-closure.md` by its
pre-compaction path. `.backlogit/` is a Stage-owned artifact; Ship's Role
Boundary permits no stash-entry mutation of any kind (P-010), so this
reference is left untouched. **This is a known stale pointer**: the
referenced file has not been deleted, but the stash entry's exact
pre-compaction path no longer resolves — following that path literally
returns a dead link, because the file was relocated (not removed) to
`docs/archive/closure/2026-09-01-047-s-048-s-compaction/2026-08-17-serve-auto-discovery-followups-closure.md`.
A reader who encounters this stash entry should be directed to this
consolidated summary, or directly to the archived file at the path above,
rather than to the stash entry's original (now-dead) path.

**In total, three stale-reference exceptions exist across this PR, all
known and none hidden**: the two decided-plans above, and this one
Stage-owned stash entry.

## Result

6 closure records compacted into this 1 consolidated summary; 6 files
archived to `docs/archive/closure/2026-09-01-047-s-048-s-compaction/`
(44,532 bytes at time of relocation; 44,784 bytes as currently archived,
reflecting the added cross-reference annotations from the reconciliation
above — source size and archived size are distinct measurements, not a
discrepancy); 0 files deleted; 7 tracked external citing documents
identified, of which **5 were updated** (Ship-authorized — see above) and
**2 (the two decided-plans) were deliberately left stale**, still citing
their original pre-compaction paths, because updating a plan artifact's
citations is Stage's Planning authority, not Ship's — recorded as a
follow-up handoff above (plus the six archived files' own mutual
cross-references, all updated); 2 memory checkpoints (Ship-authored)
assessed and preserved under the "most recent checkpoint for each
completed task" Behavioral Constraint (not compacted); 1 memory checkpoint
(`docs/memory/2026-08-16/dark-stage-session-complete-memory.md`,
Stage-authored) assessed as a qualifying content candidate but excluded
under Ship's Role Boundary (Continuity row — Ship may not mutate another
agent's checkpoint or memory), recorded as a follow-up handoff for Stage,
not compacted; **3 known stale-reference exceptions in total** (the two
decided-plans, plus 1 known Stage-owned `.backlogit/stash.jsonl`
exception) documented above.
