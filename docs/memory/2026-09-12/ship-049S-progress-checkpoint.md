# Ship 049-S dark-run progress checkpoint

**Session**: P-017 dark factory run, Ship owner, shipment 049-S.
**Branch**: `chore/fix-mcp-serve-initialize-handshake-regression` (from `main` @ b2e82fa).
**Route**: claude-sonnet-5/anthropic/xhigh.

## Status

- Shipment 049-S claimed (`active`). All 8 manifest tasks activated by
  backlogit's own claim cascade (side effect, not individually invoked by
  Ship for 7 of the 8). Covering feature 056-F's status also rolled up to
  `active` as a backlogit-internal side effect of the claim -- 056-F was
  NOT added to the manifest and no Ship operation targeted it directly.
- Legacy `pipeline-topology` gate's `PREDECESSOR_NOT_SHIPPED` (048-S) false
  positive overridden twice so far (pre_claim, post_claim) via the
  operator-audited `--force` path. Must reapply at each `lifecycle` phase
  checkpoint before build-final/PR-creation/safe-close.
- **Discovered backlogit 1.10.1 behavior**: moving a task to `status: done`
  immediately relocates its file from `.backlogit/queue/` to
  `.backlogit/archive/` (rename), well before the shipment itself is
  closed or a merge SHA exists. This matches the Ship role-boundary's
  "archived-current-delivery finalization" path for `backlogit_update_item`.
  **Decision**: defer ALL per-task `--commit {sha}` evidence writes to
  Step 6 post-merge `shipment-reconcile` safe-close, once the real merge
  SHA exists, rather than writing an interim feature-branch SHA now.

## Completed tasks (this session)

1. **056.020-T** -- DONE, committed (`ec6a274`). Built
   `tools/mcp-probe/` standalone crate: `src/transport.rs`
   (`run_duplex_pump`, bounded non-blocking `CopyHook` delivery seam,
   bounded stderr drain, deadline signaling), `src/lib.rs` (lib target
   `mcp_probe` exposing `transport`), `src/main.rs` (thin bin with hidden
   `__echo`/`__block` self-test subcommands), `tests/transport_test.rs`
   (6 black-box integration self-tests using
   `env!("CARGO_BIN_EXE_mcp-probe")`).
   - All gates green: `cargo +1.75.0 check|test|build|clippy -D warnings
     -D clippy::pedantic`, `cargo fmt --check`.
   - **Real deadlock bug found and fixed during red/green**: the
     deadline path unconditionally joined the stderr-drain thread even
     on timeout; a wedged child's stderr never closes until reaped by
     the caller (after the function returns), so this could hang
     forever. Fixed by detaching (not joining) the stderr thread
     specifically on the `timed_out` path.
   - Also had to restructure lib+bin split mid-task: a `#[cfg(test)]`
     unit test's `std::env::current_exe()` resolves to the `cargo test`
     harness binary, not the crate's real `main()` dispatch -- fixed by
     moving self-tests to a genuine integration test using
     `CARGO_BIN_EXE_mcp-probe`.

2. **056.022-T** -- DONE (moving to `done` immediately after this
   checkpoint commit). Built `tools/mcp-probe/src/process.rs`:
   `ENV_INHERITANCE_SENTINEL_VAR`, `ChildGuard` (RAII kill+wait, sole
   kill authority, reaps on `Drop` including panic/unwind), the
   observe-only `ProcessObserver` trait + `ProcessIdentity`,
   `is_ambiguous_match` (fail-closed: same-second start-time match is
   NEVER promoted to confirmed identity), `SysinfoProcessObserver`
   (production impl, `default-features = false` on the `sysinfo` dep to
   drop the `multithread`/`rayon` feature, which required rustc 1.80+
   and broke the crate's 1.75 pin), `WrapperArgs`/`parse_wrapper_args`,
   `WrapperConfig` (test-only `pump_deadline`), `WrapperOutcome`, and
   `run_wrapper` (composes 056.020-T's transport unchanged). Wired into
   `main.rs`'s `wrapper` subcommand and a new hidden `__exit <code>`
   self-test helper (deterministic non-zero exit-code passthrough,
   distinct from `__echo`'s always-0 exit).
   - `tests/process_test.rs`: 6 black-box tests across the 3 mandated
     scenarios -- normal completion + back-to-back runs + non-zero exit
     preservation + panic/unwind reap (`ChildGuard` via
     `catch_unwind`); deadline/error teardown (wedged `__block` inner,
     bounded pump deadline, confirms no exit code + prompt teardown);
     deterministic pid-reuse ambiguity + residual-descendant surfacing
     via a fake `ProcessObserver` test double (no real OS timing
     dependency).
   - All gates green: `cargo +1.75.0 check|test|build|clippy -D warnings
     -D clippy::pedantic`, `cargo fmt --check`, `cargo audit --file
     tools/mcp-probe/Cargo.lock` (clean, 21 deps scanned).
   - Fixes applied during clippy pedantic pass: `# Errors` doc sections
     on `ChildGuard::wait`/`parse_wrapper_args`;
     `#[allow(clippy::module_name_repetitions)]` on `ProcessIdentity`/
     `ProcessObserver` (deliberate, clearer names); replaced
     `bool::then(..)` inside `filter_map` with `filter().map()` in
     `child_pids`.

## Remaining work (not yet started)

- 056.023-T (evidence.rs, serde_json, observer/correlator, finalized
  lockfile + audit gate)
- 056.021-T (workspace.rs, isolated workspace/config fixtures,
  containment check)
- 056.001-T (exact_cli.rs, real Copilot CLI differential probe; one-shot
  classification; may end `done` or `blocked`)
- 056.002-T (out-of-process serve handshake test driver, main crate,
  independent of chain 1)
- 056.003-T (serve_preflight.rs hardening in main crate, depends on
  056.002-T)
- 056.019-T (H3-B terminal adjudication, depends on 056.003-T +
  056.001-T; may legitimately end `blocked`, which would halt 049-S
  closure)
- Standard multi-persona review + adversarial multi-model review before
  PR
- PR creation, Copilot review cycle, CI, merge (merge commit only, no
  admin fallback)
- Post-merge: shipment-reconcile (pre -> safe-close -> post),
  runtime-verification, operational-closure, compound-refresh, mandatory
  P-020 compact-context, index resync

## Key files

- `.github/skills/shipment-reconcile/SKILL.md` -- read through line ~500
  (Pre-Mode). Safe-Close Mode (535-979) and Cascade Close Sub-Procedure
  (1021-1242) MUST be (re-)read in full before Step 6 closure.
- `src/main.rs` (main crate) lines ~2370-2660: `open_serve_databases` /
  `cmd_serve`, target for 056.003-T's `ServePreflightExit` seam and
  `mcp_serve_ready` event (call site for `rmcp::serve_server` at ~line
  2646).
