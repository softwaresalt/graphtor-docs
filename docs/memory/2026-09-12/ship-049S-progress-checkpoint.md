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

3. **056.023-T** -- DONE, committed (`e89147b` code, `7dcc9b9` backlog
   archival). Wired `src/evidence.rs` into the crate (`pub mod evidence;`
   in `lib.rs`; `serde_json = "1.0"` added to `Cargo.toml`, its `zmij`
   transitive dep independently confirmed a legitimate real serde_json
   1.0.151 dependency via the crates.io API, not a supply-chain
   anomaly). `evidence.rs`: redaction helpers (`redact_argv`/
   `redact_env`/`redact_json_value`), `FrameKind`/`FrameEvent`/
   `InitializeCorrelation`/`EvidenceSummary`, `LineReassembler`,
   `CollectorState`/`process_line`, `EvidenceCollector` (own dedicated
   bounded channel + correlator thread, fully decoupled from
   transport's delivery thread/channel; `new`/`new_with_capacity`/
   `hook`/`finalize`), `evidence_summary_to_json` (manual
   `serde_json::json!` construction, no `Serialize` derive),
   `write_evidence_output` (atomic write). `process.rs`'s `run_wrapper`
   now constructs an `EvidenceCollector`, attaches its hook to the
   duplex pump, and calls `finalize()`/`write_evidence_output()` on
   every teardown path (including deadline teardown); `WrapperOutcome`
   gained `evidence_valid`/`evidence_write_error`.
   - `tests/evidence_test.rs`: 10 new tests (redaction pure functions,
     fragmented/interleaved initialize correlation via direct hook
     calls, mismatched-id non-correlation, channel-saturation
     invalidation without affecting real forwarding, a panicking
     `CopyHook`'s isolation, no-raw-frame-persistence, two true
     end-to-end `run_wrapper` checks).
   - Two test-design bugs found and fixed during red/green: (a) a
     `redact_json_value` test fixture used the key `"credentials"`,
     which itself substring-matches the sensitive-key check and
     blanket-redacts the whole object instead of recursing (renamed to
     `"auth_info"` -- this is intentional over-redaction behavior, not
     a code bug); (b) an interleaved-correlation test spliced a
     complete frame's bytes into the middle of a still-fragmented
     line's byte stream, which a single-stream `LineReassembler` cannot
     treat as two frames -- restructured to keep each direction's byte
     stream internally contiguous while still interleaving across
     directions.
   - All gates green (10 evidence + 6 process + 6 transport = 22 tests
     at this point): `cargo +1.75.0 check|test|build|clippy -D warnings
     -D clippy::pedantic`, `cargo fmt --check`, `cargo audit` (clean, 32
     deps), plus a `graphtor-core` `cargo check`.

4. **056.021-T** -- DONE, committed (`8ea6cf7` code, `0d58d0a` backlog
   archival). Built `tools/mcp-probe/src/workspace.rs`:
   `McpServerEntrySpec` (caller-supplied entry_name/wrapper_exe/
   inner_exe/inner_args -- this module never reads the real
   `.mcp.json`), `ProbeWorkspace`, `WorkspaceError`
   (`AlreadyExists`/`ReparsePoint`/`ContainmentEscape`/`Io`/`Json`),
   `create_probe_workspace(repo_root, nonce, entry)` and
   `remove_probe_workspace`.
   - Design correction reached before writing code: control and
     treatment `.mcp.json` wrapper args must be **byte-identical**,
     including the same `--evidence-output` path and same
     `--run-nonce` value (not per-leg-distinct) -- the two legs run
     sequentially and share one evidence_output file; treatment adds
     **only** a `cwd` key (the canonicalized repo root) to its JSON
     object.
   - Fail-closed exclusive creation: pre-check the exact nonce leaf via
     `symlink_metadata` (any existing entry -> `ReparsePoint` if a
     junction/symlink, else generic `AlreadyExists`; never reused or
     followed), then `fs::create_dir`, then a post-creation
     `canonicalize` + `starts_with` containment re-check against the
     canonical repo root -- applied to both the shared `logs/probe`
     parent chain (defends against a tampered ancestor component) and
     the freshly created leaf.
   - Ancestor fixture: `ancestor/.mcp.json` holds deliberately
     syntactically-invalid JSON (a loud, unambiguous sentinel);
     `ancestor/run/.mcp.json` holds a valid wrapper config with its own
     separate evidence_output (a distinct Gate-1 config-discovery
     proof, not part of the control/treatment cwd contrast).
   - Confirmed via `main.rs`'s own doc comment that 056.001-T (not
     056.021-T) owns the `exact-cli` subcommand; 056.021-T's "wiring"
     is `pub mod workspace;` plus doc-comment updates in `lib.rs`/
     `main.rs`, no new CLI surface.
   - `tests/workspace_test.rs`: 6 new tests -- layout, byte-identical
     control/treatment args except `cwd`, exclusive-creation rejection,
     ancestor/ancestor-run fixture content, and **two real-junction
     rejection scenarios** via `cmd /C mklink /J` (leaf-level
     pre-existing junction -> `ReparsePoint`; ancestor-component
     redirect, e.g. `logs` itself as a junction pointing outside the
     fake repo root -> `ContainmentEscape`/`ReparsePoint`), each run
     against a fresh temp-dir fake repo root so no test touches this
     crate's own working tree or the real repo's `logs/probe/`.
   - Also fixed one pre-existing `clippy::assigning_clones` pedantic
     finding in `evidence.rs` (newly surfaced by the current default
     `stable` toolchain, rustc/cargo 1.98.0, not introduced by this
     task) -- `pending_initialize_request_id = id.clone()` ->
     `.clone_from(&id)`.
   - All gates green (28 tests total: 10 evidence + 6 process + 6
     transport + 6 workspace) under **both** the default stable
     (1.98.0) and the `+1.75.0` MSRV toolchain: `cargo check|test|
     clippy --all-targets -D warnings -D clippy::pedantic`, `cargo fmt
     --check`, `cargo audit` (clean), plus a `graphtor-core` `cargo
     check`.

## Remaining work (not yet started)

- 056.001-T (exact_cli.rs, real Copilot CLI differential probe; one-shot
  classification; may end `done` or `blocked`) -- consumes 056.021-T's
  `ProbeWorkspace`/`McpServerEntrySpec` and 056.022-T's `wrapper`
  subcommand.
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
