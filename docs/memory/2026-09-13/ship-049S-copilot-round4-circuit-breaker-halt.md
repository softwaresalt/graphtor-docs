# Ship 049-S — Copilot review-comment-fix circuit breaker halt

**Session**: P-017 dark-factory execution of shipment `049-S`, Ship-owned, `merge_approval_pre_authorized: true`, `admin_fallback_pre_authorized: false`.

## State at halt

- Shipment `049-S`: claimed, all 8 manifest tasks (`056.020-T`, `056.022-T`,
  `056.023-T`, `056.021-T`, `056.001-T`, `056.002-T`, `056.003-T`, `056.019-T`)
  implemented and `done`. Not yet archived/closed — merge has not happened yet.
- Branch: `chore/fix-mcp-serve-initialize-handshake-regression`. HEAD:
  `643448bd41d2fdf3d8e34a91f81f0e942ca2b671`.
- PR: #120 (`https://github.com/softwaresalt/graphtor-docs/pull/120`), open,
  base `main`.
- Standard multi-persona review: complete pre-PR. All P0/P1 fixed or
  P-021-captured (stash `6C174AA9`, `ECD56875`).
- Adversarial multi-model review: complete pre-PR (report at
  `docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md`).
  0 HIGH-confidence consensus findings; all MEDIUM/LOW fixed or acknowledged.
- Copilot code review (repository ruleset `copilot_code_review.review_on_push:
  true` re-arms review on every push):
  - **Round 1**: 7 findings — fixed in `180b00f`, replied + resolved via GraphQL.
    1 additional finding captured as P-021 stash `AC2E02C0` (out of scope per
    `056.021-T`'s own reviewed same-user TOCTOU threat model).
  - **Round 2**: 8 findings — fixed in `3ac04eb`, replied + resolved via GraphQL.
  - **Round 3**: 6 findings — fixed in `643448b`, replied + resolved via GraphQL.
  - **Round 4**: **3 NEW findings, NOT fixed** — surfaced after the round-3
    push, all targeting round-3's own new code:
    1. `tools/mcp-probe/src/process.rs:369` (thread `PRRT_kwDORiB5E86h1-3Q`) —
       `validate_evidence_output_path` still permits any absolute destination
       (e.g. outside the probe workspace); wants a trusted-probe-root-relative
       canonicalization + symlink/reparse-escape rejection.
    2. `tools/mcp-probe/src/exact_cli.rs:1014` (thread `PRRT_kwDORiB5E86h1-3c`) —
       the round-3 `version_probe_failure_reason`/second identity probe is
       recorded but never enforced against the existing fail-closed identity
       rule.
    3. `tools/mcp-probe/src/process.rs:155` (thread `PRRT_kwDORiB5E86h1-3m`) —
       `bounded_wait_after_kill`'s bool result is discarded at its call sites;
       wants the teardown status returned/recorded as an explicit outcome
       field instead of silently swallowed.
  - All three round-4 findings pass the P-021 C1 same-contract-surface test
    (they are direct completions of code introduced in round 3, not new
    design/feature scope) — they are **in-scope**, not P-021-deferrable.

## Why this is a halt, not a 4th auto-fix round

The Ship circuit breaker table specifies:

> Review comment fix cycles | 3 | Present PR with remaining unresolved
> comments listed for operator

Rounds 1, 2, and 3 already consumed all 3 permitted review-comment-fix
cycles. Round 4 would be a 4th cycle. Per the circuit breaker and the
operator's own execution requirement #4 ("Repeat within cycle limits"),
Ship halts the auto-fix loop here rather than silently starting a 4th
round, and presents the PR with the 3 remaining unresolved round-4
comments for explicit operator direction (fix now / accept and continue /
stop).

This is **not** a P-021 stop (findings are in-scope) and **not** a P-014
merge-readiness failure by itself — it is the dedicated review-comment-fix
cycle-limit breaker.

## CI status (informational)

- `detect code changes` — **pass** (the only check listed in the repo's
  `PR-Required` ruleset `required_status_checks`).
- `build` — **pass** (3m26s, HEAD `643448b`).
- `pipeline topology gate` — **fail**, but confirmed via
  `gh api repos/.../rulesets/13816903` to be **outside**
  `required_status_checks` (not a required check; the ruleset requires only
  `detect code changes`). Failure token is the exact known false positive
  the operator pre-authorized an override for: `PREDECESSOR_NOT_SHIPPED`
  naming numeric predecessor `048-S`, at CI `--mode ci --phase ambient`
  (the automated server-side backstop job, not one of the agent-invoked
  `--mode agent`/`--mode manual` phases the operator's override syntax
  covers). Non-blocking for merge; documented, not silently ignored.

## Not yet done (blocked on operator decision)

- Round-4 fix / resolve.
- §1.9 local-review-readiness re-check, P-018 Copilot-review-completion
  gate, P-009 merge-strategy guardrail, merge itself.
- Post-merge closure (shipment-reconcile safe-close, runtime-verification,
  operational-closure, compound-refresh, mandatory P-020 compact-context).

## Follow-ups already recorded (unaffected by this halt)

- Stash `6C174AA9` (medium/chore, Stage-owned): pre-existing MSRV/globset
  break.
- Stash `ECD56875` (high/feature, Stage-owned, requires deliberation):
  `--allow-all-tools` technical-enforcement gap.
- Stash `AC2E02C0` (medium/chore, Stage-owned, requires deliberation):
  Windows-ACL/security-descriptor enforcement gap.
