---
title: "Triage Post-042-S Unmaintained Transitive Audit Advisories"
description: "Per-advisory triage of 5 unmaintained-crate RUSTSEC advisories reaching the tree via cozo/indicatif/tokenizers"
topic: "cargo audit unmaintained transitive dependency hygiene after 042-S closure"
depth: "standard"
decision_status: "decided"
promoted_to: "plan"
linked_artifacts:
  - "docs/archive/plans/2026-08-24-pre-august-compaction/2026-06-18-unmaintained-transitive-audit-advisories-plan.md"
source_stash_id: "964597B1"
tags:
  - "ci"
  - "security-audit"
  - "dependency-hygiene"
  - "deliberation-outcome"
---

## Problem Frame

After the `042-S` shipment ("Standardize graphtor-docs on docline Markdown
ingestion") retired the PDF/DOCX/Git/URL acquisition paths, the dependency tree
changed. `cargo audit` now reports **five unmaintained-crate advisories**
(informational warnings, not vulnerabilities) reaching the tree transitively:

| Advisory | Crate | Transitive path (verified via `cargo audit` / `cargo tree`) |
|---|---|---|
| RUSTSEC-2025-0056 | adler 1.0.2 | `miniz_oxide 0.7.4 ← swapvec 0.3.0 ← cozo 0.7.6` |
| RUSTSEC-2025-0141 | bincode 1.3.3 | `swapvec 0.3.0 ← cozo 0.7.6` **and** `fast2s 0.3.1 ← cozo 0.7.6` |
| RUSTSEC-2025-0057 | fxhash 0.2.1 | `jieba-rs 0.6.8 ← cozo 0.7.6` |
| RUSTSEC-2025-0119 | number_prefix 0.4.0 | `indicatif 0.17.11 ← graphtor-core` (direct) **and** `indicatif 0.17.11 ← hf-hub 0.3.2` |
| RUSTSEC-2024-0436 | paste 1.0.15 | `tokenizers 0.20.4 ← graphtor-core` **and** `gemm-* ← candle-core 0.8.4` |

The triage must decide, per advisory, whether the transitive dependency can be
**upgraded/removed**, or whether it must be **suppressed** via the established
two-place pattern (`audit.toml` rationale + `ci.yml --ignore` flag) with a
documented rationale and a follow-up review date.

### Who cares and why

* CI maintainers: keep the `cargo audit` gate signal-bearing and noise-free.
* Security posture: ensure suppressions are deliberate, documented, and time-boxed
  — not silent drift.

### Constraints

* Quality gates must stay green: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit`.
* Constitution: TDD, no `.unwrap()`/`.expect()` in library code, conventional
  commits, merge-commit policy. (This work is config-only; no library code.)
* Established pattern (compound learning
  `cargo-audit-workspace-config-limitation.md`): cargo audit 0.22 does **not**
  read `audit.toml`; suppression is applied via `--ignore` flags in
  `ci.yml`. `audit.toml` is documentation-of-record.

### Out of scope

* RUSTSEC-2026-0041 (lz4_flex, genuine 8.2-high vuln) and the now-resolved
  RUSTSEC-2026-0008 (git2) — owned by the **separate** blocked task `013.008-T`.
* Migrating CI from `cargo audit` to `cargo-deny` (a larger, separate decision).
* Upgrading cozo / candle / tokenizers themselves.

## Research Findings

* **Current `cargo audit` state (verified locally):** 1 vulnerability
  (lz4_flex RUSTSEC-2026-0041, already `--ignore`d in CI) + **5 unmaintained
  warnings** (the advisories above). CI presently runs
  `cargo audit --ignore RUSTSEC-2026-0041 --ignore RUSTSEC-2026-0008` with **no
  `--deny warnings`** — so the 5 unmaintained warnings do not currently fail CI;
  they are silent noise in the audit output.
* **RUSTSEC-2026-0008 (git2) is fully resolved:** `git2` is **absent from
  `Cargo.lock`** (only a stale registry cache copy remains). The 042-S task
  `042.013-T` ("Retire Git acquisition path and shared abstractions") removed the
  `git2` dependency entirely. The `ci.yml`/`audit.toml` `--ignore RUSTSEC-2026-0008`
  is now obsolete.
* **All cozo-rooted advisories share one blocker:** cozo 0.7.6 pins
  `swapvec 0.3.0`, `fast2s 0.3.1`, and `jieba-rs 0.6.8`, which pin
  `miniz_oxide 0.7.4`/`adler`, `bincode 1.x`, and `fxhash` respectively. None can
  be upgraded without cozo publishing a release that bumps these. This is the same
  upstream blocker as lz4_flex (RUSTSEC-2026-0041).
* **paste** is a proc-macro pulled deeply by `candle-core 0.8.4`, the `gemm-*`
  family, and `tokenizers 0.20.4`. No adopted successor exists across these crates;
  upgrading candle is a major, risky change (and is the subject of an unrelated
  blocked stream).
* **number_prefix is the one upgrade-candidate:** `indicatif` is a **direct**
  dependency (`indicatif = "0.17"`). Newer `indicatif` releases dropped the
  `number_prefix` dependency. However `hf-hub 0.3.2` also pulls `indicatif 0.17`,
  so eliminating `number_prefix` may also require bumping `hf-hub` (and verifying
  candle's hf-hub integration still builds). Feasibility must be verified with a
  build, not assumed.
* **Prior art (compound):** `cargo-audit-workspace-config-limitation.md` documents
  the exact two-place suppression pattern this triage must follow.

## Options Evaluated

### Option A: Suppress all 5 via `--ignore` only (cosmetic)

Add 5 `--ignore` flags to CI + 5 documented entries to `audit.toml`. Do not add
`--deny warnings`.

* **Pros:** Minimal change; matches existing pattern exactly.
* **Cons:** Without `--deny warnings`, the `--ignore` list is **cosmetic** — it
  only silences the 5 known IDs; any NEW unmaintained/unsound advisory still
  prints as a warning and still does not fail CI. The suppression list does not
  become a real allowlist; future drift stays invisible.
* **Effort:** Low. **Fit:** Partial — satisfies the literal ask but leaves the
  gate weak.

### Option B: Suppress all 5 + add `--deny warnings` (allowlist gate) + attempt number_prefix upgrade

Attempt the `indicatif` bump first to drop `number_prefix`. Suppress whatever
remains via `audit.toml` + CI `--ignore`, each with rationale + review date
(2026-09-18). Add `--deny warnings` so the ignore list becomes an explicit
allowlist and any NEW unmaintained/unsound advisory fails CI and forces re-triage.

* **Pros:** Makes suppression meaningful (allowlist, not cosmetic); strengthens
  the security gate rather than weakening it; time-boxed re-triage; removes one
  advisory outright if the upgrade is feasible.
* **Cons:** Slightly larger CI change; the `indicatif` upgrade may prove
  infeasible (then number_prefix is suppressed like the rest).
* **Effort:** Low–Medium. **Fit:** Strong — keeps the audit gate signal-bearing.

### Option C: Migrate CI to `cargo-deny`

Replace `cargo audit` with `cargo-deny`, which reads `audit.toml` natively.

* **Pros:** `audit.toml` `[advisories]` activates automatically; richer policy.
* **Cons:** Larger change; new tool in CI; out of scope for a triage task.
* **Effort:** High. **Fit:** Poor for this scope — deferred.

## Trade-off Comparison

| Criterion | Option A | Option B | Option C |
|---|---|---|---|
| Effort | Low | Low–Medium | High |
| Gate strength after change | Weak (cosmetic) | Strong (allowlist) | Strong |
| Matches existing pattern | Yes | Yes (+`--deny`) | No (replaces tool) |
| Scope fit (triage task) | Yes | Yes | No |
| Future-drift visibility | No | Yes | Yes |

## Decision

**Adopt Option B.**

Per-advisory triage outcome:

| Advisory | Crate | Decision | Rationale |
|---|---|---|---|
| RUSTSEC-2025-0056 | adler | **Ignore + document** | Semver-locked behind cozo 0.7.6 → swapvec 0.3.0 → miniz_oxide 0.7.4. Upgrade requires cozo release. |
| RUSTSEC-2025-0141 | bincode | **Ignore + document** | Locked behind cozo (swapvec + fast2s pin bincode 1.x; bincode 2.0 is breaking). |
| RUSTSEC-2025-0057 | fxhash | **Ignore + document** | Locked behind cozo → jieba-rs 0.6.8. |
| RUSTSEC-2025-0119 | number_prefix | **Attempt upgrade first; ignore + document if infeasible** | `indicatif` is a direct dep; newer indicatif drops number_prefix, but hf-hub 0.3.2 also pins indicatif 0.17. Verify by build; fall back to suppression. |
| RUSTSEC-2024-0436 | paste | **Ignore + document** | Deep proc-macro via candle 0.8.4 / gemm / tokenizers 0.20.4; no adopted successor; candle upgrade out of scope. |

Each suppression carries a rationale and a **2026-09-18 follow-up review date**.
CI gains `--deny warnings` so the ignore list is an explicit allowlist. The
obsolete `--ignore RUSTSEC-2026-0008` (git2) is dropped from CI and `audit.toml`
as part of the same edit, since git2 is no longer in the tree.

## Rejected Alternatives

* **Option A** rejected: cosmetic suppression leaves the gate unable to catch new
  unmaintained advisories.
* **Option C** rejected: tool migration is a separate, larger decision beyond this
  triage's scope (captured as a future path in the compound learning).

## Decision on related blocked task `013.008-T` (kept SEPARATE)

`013.008-T` ("Upgrade cozo/git2 deps to clear audit advisories") covers
RUSTSEC-2026-0041 (lz4_flex) and RUSTSEC-2026-0008 (git2). Verification shows:

* **git2 (RUSTSEC-2026-0008) is already resolved** — git2 was removed from the
  tree by 042-S; the advisory no longer appears in `cargo audit`.
* **lz4_flex (RUSTSEC-2026-0041) is still present** and still semver-locked behind
  cozo 0.7.6 → swapvec 0.3.0 → lz4_flex ^0.10. The blocker has **not** cleared.

Because its remaining open advisory is a genuine **8.2-high vulnerability** that is
still blocked on an upstream cozo release — categorically different from these
5 informational unmaintained warnings, and shippable on a different timeline —
`013.008-T` is **kept separate** and is NOT grouped into this shipment. (The prior
042 Stage session likewise deferred it as an unrelated blocked item.)

## Unresolved Questions / Operator Attention

* **`013.008-T` is partially obsolete and has a stale blocker.** git2 is resolved,
  so the task should be **narrowed to lz4_flex only**, and its `blocked_reason`
  ("candle vector search APIs not yet stable") corrected to the true blocker
  ("cozo 0.7.6 pins swapvec 0.3.0 → lz4_flex ^0.10; awaiting cozo swapvec 0.4+").
  This backlog-hygiene edit is **flagged for the operator** and intentionally NOT
  performed here (it is outside stash 964597B1's scope).
* The obsolete `--ignore RUSTSEC-2026-0008` removal is folded into this work's CI
  edit (safe: git2 is gone, so it can never re-trigger).

## Risks and Mitigations

* **Risk:** `indicatif` bump breaks candle/hf-hub integration. **Mitigation:**
  gate the bump on green `cargo build/clippy/test`; revert and suppress if it
  breaks (number_prefix then joins the ignore list).
* **Risk:** `--deny warnings` causes CI to fail on a not-yet-suppressed advisory.
  **Mitigation:** ensure every advisory in current `cargo audit` output is either
  upgraded away or in the `--ignore` allowlist before merging; verify locally with
  the exact CI command.
* **Risk:** suppressions become permanent silent drift. **Mitigation:** every
  entry carries a 2026-09-18 review date and a named upstream blocker.
