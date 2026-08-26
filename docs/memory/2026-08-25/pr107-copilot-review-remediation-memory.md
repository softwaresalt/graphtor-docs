---
doc_type: memory
title: "Stage — PR #107 Copilot review remediation (4 comments)"
source: stage-agent-session
date: 2026-08-25
branch: chore/stage-dark-security-pipeline
status: superseded
---

# Stage Checkpoint — PR #107 Copilot Review Remediation

> [!WARNING]
> **SUPERSEDED (2026-08-25).** This entire PR #107 Copilot-review remediation
> checkpoint is retained for history only. Do NOT rely on anything below. It presents
> `cap-std` Windows / beneath-root behavior as **proven** and recommends a
> **path-based / Unix-only rustix fallback**, and it scores Principles III/IV as
> **PASS (gated on U6)** — all of these are obsolete overclaims. Under current
> authority, `cap-std`'s exact root / intermediate / leaf beneath-root semantics and
> its Rust-1.75 MSRV compatibility are **UNPROVEN and gated**: any unproven safe-API
> or MSRV claim MUST produce a **BLOCKED** feasibility outcome (U7 `059.007-T` /
> U8 `059.008-T`), keeping Principles III/IV **NOT-PASSED**. There is **no silent
> fallback, no path-based chmod/deletion fallback, and no in-crate `unsafe`
> fallback** — a BLOCKED gate halts production work rather than degrading to an
> unsafe or path-based substitute. Authority is exclusively the U7/U8 feasibility
> gates and the eleven-task U1-U11 DAG in
> `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md`, the deliberation
> `docs/decisions/2026-08-24-store-toctou-nofollow-handle-deliberation.md`, and the
> durable handoff `docs/memory/2026-08-25/stage-store-toctou-nofollow-memory.md`.

Report-only Stage remediation of four Copilot review comments on PR #107, using
**planning/backlog artifacts only**. No product source/config edits, no builds,
no shipment claim/execute, no PR/GitHub interaction, no thread resolution, no
commit. Unrelated untracked files preserved.

## Comments addressed

### Comment #1 — shipment ordering (thread PRRT_kwDORiB5E86b-4Q0)
Replaced prose-only ordering in `.backlogit/queue/051-S.md` with backlogit-native
`blocks` dependency edges:
- `051-S` depends-on `050-S`
- `049-S` depends-on `051-S`

Verified cycle-free queue order **050-S → 051-S → 049-S**; frontmatter/index
consistent after `backlogit_sync_index` (502 items indexed).

### Comment #4 — intermediate-directory swap TOCTOU (thread PRRT_kwDORiB5E86b-4SR)
Final-component `O_NOFOLLOW`/`OPEN_REPARSE_POINT` does not prevent an
intermediate-directory swap after `validate_path`. Created a new **gating**
bounded task:

- **`059.006-T` (U6)** under `059-F`, priority high — directory-identity /
  containment-safe opener.
- **Design selected:** **cap-std beneath-root, directory-handle-relative
  component walk/open.** cap-std resolves each path component relative to an open
  directory handle, refusing symlink/reparse/`..`/absolute escape on both
  platforms (Unix `openat2` `RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS` via
  already-transitive rustix 1.1.4; Windows handle-relative `NtCreateFile` wrapped
  safely). `#![forbid(unsafe_code)]` preserved (unsafe FFI lives inside the
  dependency). No unsafe, no path-based chmod fallback.
- **Documented fallback** (only if no cap-std release supports MSRV 1.75):
  Unix-only hand-rolled rustix `openat` walk + a separate safe Windows design.
- **Dependency wiring (cycle-free):** `059.006-T` depends-on `059.001-T`;
  `059.002-T`/`059.003-T`/`059.004-T`/`059.005-T` each depend-on `059.006-T` — so
  U2/U3/U4/U5 cannot complete without U6.
- **Shipment membership:** added `059.006-T` to `051-S` manifest (now
  `059-F, 059.001-T … 059.006-T`).
- **Plan updates** in `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md`:
  cap-std in U1 (config domain, MSRV + `cargo tree -d` checks); new U6 unit
  section; Dependency Graph; Requirements Trace row; Decisions (cap-std
  rationale); Risks (+4); Constitution Check III/IV → **PASS (gated on U6)**, VI
  cap-std row; Plan Hardening dependency signal; risky-actions (+2); protected
  invariant #4; runtime verification; target scenarios; rollback trigger; new
  report-only plan-review addendum (2026-08-25) → **attempt 3 / PASS**.

### Comment #2 — deterministic breadth test (thread PRRT_kwDORiB5E86b-4RR)
Windows non-name-surrogate reparse integration fixture may always skip. Added a
deterministic normal-CI unit test of the broader fail-closed policy:
- **U5 delta (f):** pure predicate `should_refuse_reparse(file_attributes: u32)
  -> bool` (= `file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0`), extracted in
  U2, tested with fabricated attribute-bit inputs — no filesystem, no privilege.
- Real reparse fixture (delta (e)) kept as **optional integration** coverage with
  explicit executed/skipped reporting.
- Updated tasks `059.005-T` (delta (f) + acceptance) and `059.002-T` (predicate
  extraction note). Safe Rust 1.75, forbid unsafe maintained.

### Comment #3 — stale SHA authority (thread PRRT_kwDORiB5E86b-4Rw)
In `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`:
- Removed the pinned stale `a5503919` staging snapshot HEAD everywhere (0 remain).
- Neutralized **all** "Current staging authority" claims (0 remain).
- Made the live **PR #107 `## Local Review Readiness` current-HEAD record plus
  Ship's mandatory fresh current-HEAD local review** the authoritative dynamic
  gate; the durable Stage evidence section is now an explicit
  manifest/disposition cross-check only, **not** a review-currency or
  merge-readiness authority.
- Commit-specific evidence labelled historical; deliberately did **not** pin a new
  current-HEAD SHA (would itself go stale). Ship verification checklist item 2
  now points at the PR #107 readiness record.

## Files / items / dependencies / manifests changed

**Backlog (via backlogit MCP):**
- Created `059.006-T`.
- Deps added (`blocks`): `059.006-T`←`059.001-T`; `059.002-T`←`059.006-T`;
  `059.003-T`←`059.006-T`; `059.004-T`←`059.006-T`; `059.005-T`←`059.006-T`;
  `051-S`←`050-S`; `049-S`←`051-S`.
- `051-S` manifest += `059.006-T`.
- Updated `059.001-T`, `059.002-T`, `059.005-T`.
- Comments appended (actor `stage`) to `059.006-T` and `051-S`.

**Docs:**
- `docs/exec-plans/2026-08-24-store-toctou-nofollow-handle-plan.md` (comments #4, #2).
- `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md` (comment #3).

## Review / lint

- **Plan review (report-only, focused multi-persona):** store-toctou plan
  addendum **attempt 3 / PASS**; findings B1 (P1 intermediate-dir) and B2 (P2
  deterministic test) resolved in-artifact. No unresolved P0/P1.
- **`backlogit_docs_lint` (authoring, `docs/exec-plans`):** 5 violations, **all
  pre-existing** in files not touched this session
  (`2026-08-16-readonly-serve-guarantee-hardening-decided-plan.md`,
  `2026-08-16-serve-auto-discovery-followups-decided-plan.md`,
  `2026-08-24-pre-august-completed-plan-compaction.md`). Both edited files:
  **0 findings**.

## Blockers / open items
- **cap-std MSRV 1.75 compatibility** is a **Ship-time** `cargo +1.75.0
  check --all-targets` verification, not confirmed in this planning session. The
  plan documents the Unix-only rustix fallback + separate safe Windows design if
  no cap-std release supports 1.75.

## Next steps (for Ship)
- Execute 059-F per the gated graph (U6 before U2/U3/U4/U5); confirm cap-std MSRV;
  run fresh current-HEAD local review recorded in PR #107 `## Local Review
  Readiness`.
