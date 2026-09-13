# Ship 049-S dark-factory run — round-2 Copilot remediation checkpoint

**Session**: P-017 dark factory execution of shipment 049-S ("Fix MCP serve
initialize-handshake regression / Copilot CLI OS error 232").
**Config route**: claude-sonnet-5 / anthropic / xhigh.
**Timestamp**: 2026-09-12 (round 2 remediation complete).

## Status at this checkpoint

- Shipment 049-S claimed; all 8 manifest tasks (056.020-T, 056.022-T,
  056.023-T, 056.021-T, 056.001-T, 056.002-T, 056.003-T, 056.019-T)
  implemented/done.
- Standard 9-persona review complete (2 P-021 captures: stash `6C174AA9`,
  `ECD56875`).
- Adversarial multi-model review complete, findings remediated.
- PR #120 open: `chore/fix-mcp-serve-initialize-handshake-regression` ->
  `main`.
- Copilot review round 1 (7 findings on initial PR HEAD): all fixed,
  replied, resolved. Commit `180b00f`. One out-of-scope finding (Windows
  ACL enforcement) captured as P-021 deferred entry stash `AC2E02C0`.
- Copilot review round 2 (8 NEW findings on `180b00f`): all classified,
  fixed, replied, resolved. Commit `3ac04eb`, pushed.
  - Threads A, B, C, E, F, G, H: in-scope bug fixes (all in code this
    shipment itself introduced in round 1 or earlier).
  - Thread D: known `PREDECESSOR_NOT_SHIPPED`/048-S false positive,
    already covered by the operator's dag-readiness override
    authorization. No code/backlog change; reply + resolve only.
  - No new P-021 captures needed in round 2 (thread D is a tooling
    false-positive disposition, not a scope-expansion proposal).

## Round-2 fixes (commit `3ac04eb`)

1. `exact_cli.rs::persist_outcome_json` — owner-only (0o600) temp file
   mode (thread A).
2. `exact_cli.rs::identity_failure_message` (new) — fail-closed identity
   enforcement ahead of Gate-1 branching (thread B).
3. `main.rs::run_exact_cli_subcommand` — fails closed (nonzero exit) on
   persistence failure, still prints JSON first (thread C).
4. `tests/common/serve_driver.rs::shutdown` — `try_wait()` OS-error
   branch now attempts kill()+wait() before joining drain threads
   (thread E).
5. `exact_cli.rs::leg_connected` — now also requires
   `!stdout_truncated` (thread F).
6. `exact_cli.rs::gate1_proves_ancestor_merge` (new) — precise
   H3-B-candidate gating; every other Gate-1 failure shape yields
   `blocked` (thread G).
7. `transport.rs::run_duplex_pump` — bounded `DELIVERY_DRAIN_BUDGET`
   (250ms) poll/join replacing unconditional detach; new
   `PumpOutcome::delivery_drain_incomplete` flag; `evidence.rs`'s new
   `note_transport_delivery_drain_incomplete` wired into `process.rs`
   before `finalize()` (thread H).
8. `.backlogit/queue/049-S.md` (thread D) — no change; reply/resolve
   citing dag-readiness + audit-log evidence only.

Regression tests added: 6 `gate1_proves_ancestor_merge` unit tests, 7
`identity_failure_message` unit tests, 1
`persist_outcome_json_fails_closed_when_the_workspace_root_does_not_exist`
unit test, 2 new `exact_cli_test.rs` integration tests, 2 new
`evidence.rs` unit tests, 2 new `transport_test.rs` integration tests.

## Verification (all clean)

- mcp-probe: `cargo check/clippy --all-targets -D warnings -D
  clippy::pedantic` (added one justified
  `#[allow(clippy::struct_excessive_bools)]` on `PumpOutcome` — 4
  genuinely orthogonal outcome flags, no state-machine relationship),
  `cargo fmt --all -- --check`, `cargo test` (47+10+5+7+8+9 = passing,
  0 failures), `cargo audit` with CI's exact RUSTSEC allowlist (clean).
- Root workspace: `cargo check/clippy/fmt --check/test --all-targets`,
  all clean (includes `tests/common/serve_driver.rs` thread-E fix,
  verified via `serve_handshake_driver_test`/`serve_preflight_test`).

## Next steps

1. Re-run `autoharness gate copilot-review 120` against commit
   `3ac04eb` — expect possible round 3 (Copilot re-arms on every push,
   confirmed empirically after round 1's push).
2. If round 3 surfaces new findings: classify (P-021 C1), fix, reply,
   resolve — same cycle, within the 3-cycle review-comment-fix circuit
   breaker (this would be cycle 3 of 3 if round 3 has any in-scope
   fixes needing code changes; present remaining findings to operator if
   the bound is hit).
3. If Copilot review reports `SATISFIED`/no unresolved threads: proceed
   to full local quality gates (already clean as of this commit), CI
   monitoring (`fix-ci` if needed), §1.9 pre-merge readiness gate, P-018
   gate re-verify, P-009 merge-strategy guardrail, present merge
   readiness to operator (dark-mode pre-authorized), execute
   merge-commit-only merge.
4. Full Step 6 post-merge closure sequence remains pending: Merge
   Confirmation Gate -> Release Closure Completion Gate ->
   `post-merge/{feature_slug}` branch -> `shipment-reconcile` (pre ->
   safe-close -> post) -> `runtime-verification` ->
   `operational-closure` -> `compound-refresh` -> mandatory P-020
   `compact-context` -> backlog index resync -> closure PR (own §1.9
   gate + operator approval) -> return to `main`.

## Follow-ups recorded for Stage (handoff only, not created by Ship)

- Stash `6C174AA9` — MSRV/globset break (pre-existing, captured during
  standard review).
- Stash `ECD56875` — allowlist/sandbox safety gate (captured during
  standard review).
- Stash `AC2E02C0` — Windows ACL enforcement for control/treatment
  workspace directories (P-021 deferred scope expansion, round-1 thread
  4).

No new stop conditions encountered. The operator's pre-authorized
dag-readiness override continues to correctly and narrowly cover only
the `PREDECESSOR_NOT_SHIPPED`/048-S false positive (round-2 thread D was
the same token/scope, handled per the pre-authorization).

## In-scope continuity evidence (do not delete)

`docs/memory/2026-09-10/stage-049S-darkrun-confirm-memory.md` — Stage's
prior confirmation that 049-S is unchanged and ready. Left untouched.
