---
title: "Post-042-S Unmaintained Transitive Audit Advisory Triage Plan"
status: draft
source_deliberation: "docs/decisions/2026-06-18-unmaintained-transitive-audit-advisories-deliberation.md"
source_stash_id: "964597B1"
created: 2026-06-18
revised: 2026-06-18
review_attempt: 1
---

# Post-042-S Unmaintained Transitive Audit Advisory Triage Plan

## Objective

Bring the `cargo audit` gate back to a clean, signal-bearing state after the
042-S dependency-tree change by triaging the five unmaintained-crate advisories
per the deliberation decision (Option B):

1. Attempt to remove `number_prefix` (RUSTSEC-2025-0119) by upgrading `indicatif`.
2. Suppress the remaining unmaintained advisories via the established two-place
   pattern (`audit.toml` rationale + `ci.yml --ignore`), each with a rationale and
   a 2026-09-18 follow-up review date.
3. Add `--deny warnings` to the CI audit step so the ignore list becomes an
   explicit allowlist that fails CI on any NEW unmaintained/unsound advisory.
4. Drop the obsolete `--ignore RUSTSEC-2026-0008` (git2) entry, since git2 is no
   longer in the dependency tree.

## Scope

### In scope

* `Cargo.toml` / `Cargo.lock`: optional `indicatif` (and possibly `hf-hub`) bump.
* `audit.toml`: documented ignore entries with rationale + review date.
* `.github/workflows/ci.yml`: matching `--ignore` flags + `--deny warnings`.

### Out of scope

* **Remediating** RUSTSEC-2026-0041 (lz4_flex) is owned by the separate blocked
  task `013.008-T` and is out of scope here. RUSTSEC-2026-0008 (git2) is already
  resolved (git2 absent from `Cargo.lock`); this plan does NOT remediate git2 — it
  only performs the in-scope hygiene of removing git2's now-dead `--ignore`/audit
  entry as part of the same audit-config edit (see Unit 2).
* Upgrading cozo / candle / tokenizers.
* Migrating CI from `cargo audit` to `cargo-deny` (future path; see compound
  learning `cargo-audit-workspace-config-limitation.md`).
* Any library/source/test code change. This work is config + dependency-manifest
  only.

## Problem Frame (technical)

`cargo audit` on the current `Cargo.lock` reports 1 vulnerability
(lz4_flex RUSTSEC-2026-0041, already ignored) and **5 unmaintained warnings**:

| Advisory | Crate | Verified transitive path | Upstream blocker |
|---|---|---|---|
| RUSTSEC-2025-0056 | adler 1.0.2 | `miniz_oxide 0.7.4 ← swapvec 0.3.0 ← cozo 0.7.6` | cozo pins swapvec 0.3.0 |
| RUSTSEC-2025-0141 | bincode 1.3.3 | `swapvec 0.3.0 ← cozo` + `fast2s 0.3.1 ← cozo` | cozo pins swapvec/fast2s (bincode 1.x) |
| RUSTSEC-2025-0057 | fxhash 0.2.1 | `jieba-rs 0.6.8 ← cozo` | cozo pins jieba-rs 0.6.8 |
| RUSTSEC-2025-0119 | number_prefix 0.4.0 | `indicatif 0.17.11 ← graphtor-core` (direct) + `indicatif 0.17.11 ← hf-hub 0.3.2` | indicatif 0.17 uses number_prefix; hf-hub 0.3.2 pins indicatif 0.17 |
| RUSTSEC-2024-0436 | paste 1.0.15 | `tokenizers 0.20.4 ← graphtor-core` + `gemm-* ← candle-core 0.8.4` | candle/tokenizers/gemm pin paste 1.x |

The CI step currently runs `cargo audit --ignore RUSTSEC-2026-0041 --ignore
RUSTSEC-2026-0008` with **no `--deny warnings`**, so the 5 unmaintained warnings
are silent noise rather than a gated allowlist.

## Requirements Trace

| Requirement (from deliberation) | Implementation action | Unit |
|---|---|---|
| Determine per-advisory upgrade-vs-suppress | Attempt indicatif bump (number_prefix); confirm others are cozo/candle-locked | Unit 1 |
| Suppress with rationale + review date | Add documented `audit.toml` entries + CI `--ignore` flags | Unit 2 |
| Make suppression meaningful | Add `--deny warnings` (allowlist gate) | Unit 2 |
| Remove obsolete git2 ignore | Drop `--ignore RUSTSEC-2026-0008` from CI + audit.toml | Unit 2 |
| Keep `013.008-T` separate | No backlog change here; flagged for operator | n/a (Stage report) |

## Implementation Units

### Unit 1 — Verify `number_prefix` upgrade feasibility via `indicatif` bump

* **Domain:** dependency / manifest (`Cargo.toml`, `Cargo.lock`).
* **Execution posture:** spike / investigation-first.
* **Changes:**
  1. Bump `indicatif` to the latest 0.17-compatible (or newer) release that drops
     the `number_prefix` dependency; if a newer major is required, evaluate whether
     `hf-hub` must also bump and whether candle's hf-hub integration still builds.
  2. Run `cargo tree -i number_prefix` to confirm whether `number_prefix` is gone.
  3. If the bump cleanly removes `number_prefix` with all gates green, keep it. If
     it forces an incompatible `hf-hub`/candle change or otherwise breaks the
     build, **revert** the manifest change and record that `number_prefix` must be
     suppressed like the other cozo/candle-locked advisories.
  * **Cascade ceiling (scope guard):** the bump must stay confined to
    `Cargo.toml` + `Cargo.lock`. If removing `number_prefix` requires any
    source-code adaptation to a changed `indicatif`/`hf-hub` API, or pulls in a
    candle-incompatible `hf-hub` major, **revert** and treat number_prefix as
    suppress-only. Do not let Unit 1 grow beyond its 2-file manifest boundary.
* **Files:** `Cargo.toml`, `Cargo.lock` (≤2 files).
* **Verification / acceptance:**
  * `cargo tree -i number_prefix` shows the crate **absent**, OR the task log
    records the exact blocking constraint that makes the upgrade infeasible.
  * If a bump is kept: `cargo build --all-targets`, `cargo clippy --all-targets --
    -D warnings -D clippy::pedantic`, `cargo test --all-targets`,
    `cargo fmt --all -- --check` all pass.
  * The outcome (removed vs. must-suppress) is recorded so Unit 2 knows whether
    `RUSTSEC-2025-0119` belongs in the ignore allowlist.

### Unit 2 — Suppress remaining unmaintained advisories in `audit.toml` + CI; add allowlist gate

* **Domain:** config (`audit.toml`, `.github/workflows/ci.yml`).
* **Execution posture:** characterization-first (run `cargo audit` before and
  after to characterize the exact warning set).
* **Depends on:** Unit 1 (Unit 1 decides whether `number_prefix` is in the set).
* **Changes:**
  1. In `audit.toml [advisories] ignore`, add documented entries for
     `RUSTSEC-2025-0056` (adler), `RUSTSEC-2025-0141` (bincode),
     `RUSTSEC-2025-0057` (fxhash), `RUSTSEC-2024-0436` (paste), and
     `RUSTSEC-2025-0119` (number_prefix — **only if** Unit 1 could not remove it).
     Each entry records: the transitive path, the named upstream blocker, and a
     `Review: 2026-09-18` follow-up date.
  2. Remove the obsolete `RUSTSEC-2026-0008` (git2) entry from `audit.toml` and its
     `--ignore` flag from CI — git2 is no longer in `Cargo.lock`. Keep
     `RUSTSEC-2026-0041` (lz4_flex), which is still present and owned by `013.008-T`.
  3. In the `ci.yml` audit step, add a matching `--ignore RUSTSEC-...` flag for
     every advisory in `audit.toml`'s ignore list, and append `--deny warnings`.
  4. Keep the `audit.toml` header note accurate (cargo audit 0.22 does not
     auto-discover `audit.toml`; CI applies `--ignore`).
* **Files:** `audit.toml`, `.github/workflows/ci.yml` (2 files).
* **Verification / acceptance:**
  * Running the exact CI command (`cargo audit --ignore <each-id> --deny warnings`)
    against the current `Cargo.lock` exits 0 with **zero** reported warnings.
  * `audit.toml` has a rationale + `2026-09-18` review date for each newly added
    advisory; the obsolete git2 entry is removed; lz4_flex entry preserved.
  * The `ci.yml` `--ignore` set is exactly the `audit.toml` ignore set, plus
    `--deny warnings`.

## Dependency Graph

```
Unit 1 (indicatif/number_prefix feasibility)  ──▶  Unit 2 (suppress + allowlist gate)
```

No cycles. Unit 1 must complete first because its outcome determines whether
`RUSTSEC-2025-0119` is in Unit 2's ignore list.

## Decisions and Rationale

* **Two-place suppression (audit.toml + CI):** required because cargo audit 0.22
  does not read `audit.toml` (compound learning). audit.toml stays as
  documentation-of-record and as the cargo-deny-ready forward path.
* **`--deny warnings`:** converts the ignore list from cosmetic noise-silencing
  into a real allowlist; new unmaintained/unsound advisories then fail CI and
  force a re-triage instead of drifting silently. This strengthens the gate.
* **Attempt number_prefix upgrade:** it is the only advisory whose crate is reached
  through a **direct** dependency (`indicatif`), so an upgrade is plausible and
  worth a verified attempt before suppressing.
* **Drop git2 ignore now:** safe because git2 is gone from the tree; leaving a dead
  `--ignore` flag is misleading.
* **Keep `013.008-T` separate:** its open advisory (lz4_flex) is a genuine
  high-severity vulnerability still blocked on an upstream cozo release —
  different in kind and timeline from these informational warnings.

## Risks and Caveats

* **indicatif bump breaks candle/hf-hub:** mitigated by gating on green
  build/clippy/test and reverting on failure (number_prefix then suppressed).
* **`--deny warnings` fails CI on an un-ignored advisory:** mitigated by verifying
  the exact CI command locally so every current advisory is either upgraded away or
  in the allowlist before merge.
* **Suppressions become permanent drift:** mitigated by the 2026-09-18 review date
  and named upstream blocker on every entry.

## Plan Hardening Signals (REQUIRED)

* public API, schema, or contract change — **absent** (config + manifest only; no
  library/source/test changes; no public surface change).
* security, auth, permission, or compliance-sensitive behavior — **present
  (compliance-sensitive)**: the change modifies the CI **security-audit gate**.
  The change strengthens the gate (adds `--deny warnings`) and only suppresses
  documented informational advisories with a time-boxed review; it does not weaken
  authz or expose data. The genuine vulnerability (lz4_flex) stays tracked
  separately and is not silenced here.
* migration, backfill, destructive/irreversible action — **absent** (all edits are
  reversible config/manifest changes; revert restores prior gate behavior).
* external integration, operator checkpoint, external dependency — **present
  (low)**: Unit 1 touches external crate versions, but the outcome is verified by
  build/test and reverted on failure.
* high runtime, rollout, or rollback risk — **absent** (no runtime surface change;
  rollback is a git revert of config/manifest).

Conclude: **Requires plan hardening: yes** — because the change touches the CI
security-audit gate (compliance-sensitive surface), a brief hardening pass records
the gate-integrity invariant, verification, and rollback explicitly before review.

## Runtime Verification and Closure

* **Runtime surface changed:** none at application runtime. The only "runtime"
  affected is **CI** (the audit gate) and the local dependency graph.
* **Verification before absorbed:** the exact CI command
  (`cargo audit --ignore <ids> --deny warnings`) exits 0 with zero warnings on the
  merged `Cargo.lock`; full quality-gate suite green.
* **Operational closure:** the 2026-09-18 review date is the validation window;
  the named upstream blockers (cozo swapvec 0.4+, candle, hf-hub 0.4+) are the
  re-triage triggers; ownership stays with CI maintainers. Rollback = revert the
  config/manifest commit, restoring the prior audit invocation.

## Plan Hardening

**Hardening required:** Yes — the change modifies the CI **security-audit gate**
(`cargo audit` invocation), a compliance-sensitive surface. This pass records the
protected invariant, risky actions, verification, and rollback explicitly so the
plan-review gate can confirm the gate is strengthened, not weakened.

### Risk triggers and protected invariants

* **Trigger:** editing the CI audit step and the suppression allowlist.
* **Invariant 1 (no real vulnerability silenced):** only *unmaintained*
  (informational) advisories are added to the ignore allowlist. The genuine
  high-severity vulnerability `RUSTSEC-2026-0041` (lz4_flex) remains owned and
  tracked by `013.008-T` and is NOT newly suppressed by this work (its pre-existing
  ignore stays, unchanged in intent).
* **Invariant 2 (gate net-stronger):** `--deny warnings` is added so the ignore
  list becomes an explicit allowlist; the post-change gate fails on any NEW
  unmaintained/unsound advisory. The change must not reduce what CI catches.
* **Invariant 3 (no dead suppressions):** the obsolete `RUSTSEC-2026-0008` (git2)
  ignore is removed because git2 is absent from `Cargo.lock`; verified via
  `cargo tree`/`Cargo.lock` inspection before removal.

### Risky actions

* `ProposedAction`: bump `indicatif` (Unit 1). `ActionRisk`: low — reversible
  manifest edit; **no approval required**; gated on green build/clippy/test;
  `ActionResult`: either `number_prefix` removed (verified by `cargo tree -i`) or
  reverted with a recorded infeasibility note.
* `ProposedAction`: add `--deny warnings` + ignore allowlist to `ci.yml` (Unit 2).
  `ActionRisk`: low–medium — could fail CI if any current advisory is not in the
  allowlist; **no approval required** but **must be verified locally** with the
  exact CI command before merge. `ActionResult`: `cargo audit --ignore <ids>
  --deny warnings` exits 0 with zero warnings.
* `ProposedAction`: remove `RUSTSEC-2026-0008` ignore. `ActionRisk`: low —
  safe only because git2 is gone; `ActionResult`: audit output unchanged for git2
  (no such advisory present).

### Added verification depth

* Environment precheck: confirm `git2` absent from `Cargo.lock` before dropping its
  ignore; confirm the full current advisory set with `cargo audit` (no `--ignore`).
* Target scenario: run the **exact** post-change CI command locally; require exit 0
  and zero warnings.
* Full quality-gate suite (`fmt`, `clippy -D warnings -D pedantic`, `test`) green
  after the Unit 1 manifest change.

### Rollback, monitoring, ownership

* **Rollback:** `git revert` the config/manifest commit restores the prior audit
  invocation and dependency graph. No data or schema migration is involved.
* **Monitoring / validation window:** the `2026-09-18` review date on every ignore
  entry; re-triage when cozo (swapvec 0.4+), candle, or hf-hub (0.4+) publish.
* **Owner:** CI maintainers.

### Learnings and instructions consulted

* `docs/compound/cargo-audit-workspace-config-limitation.md` — two-place
  suppression pattern (audit.toml documents; CI `--ignore` enforces).
* `.github/instructions/ci-security.instructions.md`,
  `.github/instructions/constitution.instructions.md` (config-only change; no
  library code, so `.unwrap()`/TDD constraints do not apply to these edits).

### Unresolved operator decisions

* None block execution. One backlog-hygiene flag (narrow `013.008-T` to
  lz4_flex-only; fix its stale `blocked_reason`) is recorded for the operator and
  is intentionally out of this plan's scope.

## Plan Review

**Gate decision: PASS** (all findings P3/advisory; hardening present and complete).

Plan hardening was **required** (CI security-audit gate touched) and is
**satisfied** — the `## Plan Hardening` section records the gate-integrity
invariants, `ProposedAction`/`ActionRisk` classifications, verification depth, and
rollback path.

### Persona findings

| Persona | Verdict | Notes |
|---|---|---|
| Constitution Reviewer | PASS | Config/manifest-only change; no library code, so TDD / no-`.unwrap()` constraints do not apply to these edits. Conventional-commit + merge-commit policy apply at Ship execution time. No violations. |
| Rust Reviewer | PASS | No type signatures or error handling introduced. The `indicatif` bump (Unit 1) is gated on green `build`/`clippy -D warnings -D pedantic`/`test`; revert-on-break keeps the crate sound. |
| Scope Boundary Auditor | ADVISORY (all P3) | `--deny warnings` and `indicatif` upgrade judged JUSTIFIED in-scope; git2 ignore-removal is in-scope hygiene. Two P3 refinements (git2 out-of-scope wording; Unit 1 cascade ceiling) **addressed** in this revision. Boilerplate-heaviness P3 left as-is (defensible for a security-gate change). |
| Learnings Researcher | PASS | Plan correctly applies compound learning `cargo-audit-workspace-config-limitation.md` (two-place suppression: audit.toml documents, CI `--ignore` enforces). No contradiction with prior art. |
| Architecture Strategist | PASS | No architectural impact; no module/coupling change. |
| Security Lens Reviewer (triggered) | PASS | Change strengthens the audit gate (`--deny warnings` → explicit allowlist); only documented informational advisories suppressed, each time-boxed (2026-09-18); the real vulnerability (lz4_flex) stays tracked separately; no authz change, no data exposure, no secrets. |

### Severity summary

* P0: 0  P1: 0  P2: 0  P3: 4 (2 addressed, 2 acknowledged-advisory).

### Runtime verification / closure check

Verification (exact CI command exits 0 with zero warnings; full quality-gate
suite green) and closure (2026-09-18 review date, named upstream blockers, CI-maintainer
ownership, git-revert rollback) are present and adequate for a CI-gate change.

**Proceed to harvest.**

<!-- plan-review-attempt: 1 PASS -->
