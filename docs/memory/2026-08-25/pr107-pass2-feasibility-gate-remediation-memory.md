---
doc_type: memory
title: "Stage — PR #107 pass-2 remediation (cap-std overclaims → U7 feasibility gate)"
source: stage-agent-session
date: 2026-08-25
branch: chore/stage-dark-security-pipeline
status: superseded
---

# Stage Checkpoint — PR #107 Pass-2 Feasibility-Gate Remediation

> **SUPERSEDED (2026-08-25).** This entire pass-2 record is obsolete and is retained
> for history only. Everything below is superseded, including: its shipment manifest
> ending at U7 (`U1`-`U7`), its `U1`-before-`U7` ordering, its "U1-U7 final authority"
> claim, its five-obligation U7 model, and any name-based or directory-relative
> transient-sidecar **deletion** claim. Do NOT rely on anything in this file. Current
> authority is the durable handoff
> `docs/memory/2026-08-25/stage-store-toctou-nofollow-memory.md`, together with the
> eleven-task U1-U11 DAG in
> `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md` (pass 6 addendum)
> and `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`. U7 and
> U8 are now separate three-scenario feasibility gates with U7 depending on nothing;
> fail-closed transient-sidecar **retention** (U3) stands unless same-identity deletion
> is proven.

Report-only, Stage-phase remediation of a second round of PR #107 reviewer
blockers on the `src/db/store.rs` TOCTOU feature (`059-F` / shipment `051-S`).
**No product source/config/build/PR actions. NOT committed. Untracked temp files
preserved.**

## Core change

Removed the overclaim that `cap-std` is already proven to deliver the exact
root/intermediate/leaf beneath-root semantics and Rust-1.75 compatibility.
Introduced a NEW bounded, test-first **feasibility/evidence gate** task
`059.007-T` (U7) that must PASS before U6 builds on `cap-std`, or return BLOCKED.

## Final manifest — shipment 051-S

`059-F`, `059.001-T`, `059.002-T`, `059.003-T`, `059.004-T`, `059.005-T`,
`059.006-T`, `059.007-T` (U7 newly added). Shipment sequencing preserved:
`050-S → 051-S → 049-S` (`051-S` blocks-depends on `050-S`).

## Intra-shipment DAG (cycle-free, verified)

```text
U7 (059.007-T) ← U1 (059.001-T)
U6 (059.006-T) ← U1, U7
U2 (059.002-T) ← U1, U6
U3 (059.003-T) ← U2, U6
U4 (059.004-T) ← U2, U6
U5 (059.005-T) ← U3, U4, U6
```

## U7 (059.007-T) — five proof obligations (safe APIs only, MSRV 1.75)

1. Atomic workspace-root directory-handle bootstrap with no-follow/no-reparse
   semantics — Unix safe `OpenOptions` `O_DIRECTORY|O_NOFOLLOW` read; Windows safe
   `OpenOptions` `FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT` +
   attribute rejection + share/access flags; retained for the `DataStore` lifetime.
   Explicit threat model: attacker may write **inside** the workspace root but not
   its trusted parent; root handle bootstrapped once from the trusted parent.
2. Conversion to/from `cap_std::fs::Dir`/`File` (or the selected safe capability
   API), no in-crate `unsafe`.
3. Component walk refuses **intermediate** symlink/junction swaps.
4. Final **in-bounds leaf** reparse/symlink refused (not merely escape-prevented).
5. Compiles under Rust 1.75 **and** decides the `cap_std::fs::File` vs
   `std::fs::File` boundary (into_std vs capability-file helper signatures).

BLOCKED outcome ⇒ Principles III/IV remain NOT-PASSED; no vague `unsafe` and no
path-based fallback is an acceptable substitute.

## Wording/claims changed

- **Overclaims removed** from U6/U1, plan Decisions/Risks/Constitution Check, and
  the deliberation doc: `cap-std` is now a **candidate** whose semantics/MSRV are
  **proven by U7 before adoption**.
- **Principles III/IV** honestly downgraded from `PASS (gated on U6)` to
  `NOT-PASSED (provisional; gated on U7 PASS + U6)` in the plan; deliberation +
  `059-F` addenda match.
- **U3 transient sidecar cleanup** now relative to the retained U6 root `Dir`
  handle (`symlink_metadata` + `remove_file`); "path-based residual acceptable"
  claim removed.
- **Deterministic reparse predicate** (U2/U5): target-independent literal
  `0x0000_0400` + pure `should_refuse_reparse(u32)` compiled/tested on Linux CI;
  production Windows code MUST call it; `#[cfg(windows)]` assertion that
  `0x0000_0400 == FILE_ATTRIBUTE_REPARSE_POINT`.
- **U4** reworded to the **intentionally broader any-reparse-point policy** (not
  "matching `is_reparse_point` breadth").
- **File boundary** decided in U7, inherited by U6/U2 (no late ambiguity).
- **Continuous MSRV evidence** required on U1: a dedicated Rust 1.75 CI check
  during implementation (or a proven equivalent repository gate); Stage does not
  alter the workflow now.
- **references** frontmatter added to `059.003-T` and `059.004-T`.
- Historical plan addendum B1 marked **superseded-by-C1** to remove a stale
  `PASS (gated on U6)` reading.

## Files modified (all uncommitted)

- `.backlogit/queue/059.007-T.md` (NEW), `059-F.md`, `059.001-T.md` …
  `059.006-T.md`
- `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md`
- `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`

## Verification

- `backlogit_sync_index` → 503 indexed, no error; DAG cycle-free.
- `backlogit_docs_lint` on both edited docs → `valid:true, violations:0`.
  (Corpus-wide 367 violations are pre-existing and out of scope; the deliberation
  doc's own two findings — missing `doc_type`/`source` — were fixed while editing.)
- Focused security/Rust/correctness review: **P0=0, P1=0**; 2 minor findings
  fixed in-artifact (U2 Unix `.read(true)` restored; B1 addendum marked superseded).

## Next actor (Ship)

Run U1 → **U7 (feasibility gate) test-first**; only if U7 records PASS proceed to
U6 → U2–U5. Add the dedicated Rust 1.75 CI check during implementation. Do **not**
claim Principles III/IV until U7 PASS **and** U6 lands.
