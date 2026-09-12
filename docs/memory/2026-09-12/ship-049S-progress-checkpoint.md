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

5. **056.001-T** -- DONE, committed (`0bd8c92` code, `d575e6a` backlog
   archival). Built `tools/mcp-probe/src/exact_cli.rs` (~1500 lines incl.
   18 unit tests): `ExactCliArgs`/`parse_exact_cli_args`, `fnv1a_64` +
   `CopilotIdentity`/`identify_copilot`, `CaptureSink`,
   `spawn_piped_child`/`pump_and_reap` (reused `ChildGuard` +
   `run_duplex_pump` one layer up, over the exact Copilot CLI child
   itself -- never a new pump/teardown), `SentinelObservation`/
   `watch_sentinel_inheritance`/`spawn_sentinel_watcher` (new,
   independent, read-only `sysinfo::Process::environ()` watcher --
   `WrapperOutcome.sentinel_inherited` is never persisted by `main.rs`
   and `EvidenceSummary` has no sentinel field, so this could not be
   satisfied by reading the wrapper's evidence file without touching
   056.022-T/056.023-T-owned modules, which the task's own AC forbids),
   `Gate1Outcome`/`run_gate1` (`mcp get --json` ancestor-isolation
   proof, zero AI cost), `Leg`/`LegOutcome`/`run_leg`, `PassOutcome`/
   `run_pass`, `classify_pass` (ordered H0a-retained / no-reproduction /
   unresolved / unexpected-asymmetry), `ExactCliOutcome`/
   `outcome_to_json`/`run_exact_cli`. Wired `pub mod exact_cli;` into
   `lib.rs` and the `exact-cli` subcommand into `main.rs` (the one
   final wiring edit the AC permits).
   - Design bugs fixed before compiling clean: `Stdio::null()` doesn't
     satisfy `run_duplex_pump`'s `.take()` requirement on
     `child.stdin`/`stdout` (switched to `Stdio::piped()` +
     `io::empty()` incoming); an `E0382` borrow-of-moved-value in the
     original two-match `pump_and_reap` (consolidated into one `match`
     computing `(timed_out, exit_code)` together).
   - Pedantic clippy fixes (default stable, 1.98.0): `map().unwrap_or_else()`
     -> `map_or_else()` in `CaptureSink::into_bytes`; a doc-list
     indentation lint on `spawn_and_capture`'s doc comment.
   - MSRV-only (`+1.75.0`) clippy surfaced 3 `module_name_repetitions`
     findings in this task's own new items (`ExactCliArgs`,
     `ExactCliOutcome`, `run_exact_cli`) -- fixed with the same
     `#[allow(clippy::module_name_repetitions)]` + rationale-comment
     pattern already established in `evidence.rs`/`process.rs`. The
     same MSRV clippy pass also surfaced 4 pre-existing
     `module_name_repetitions` findings in `workspace.rs`
     (`WorkspaceError`, `ProbeWorkspace`, `create_probe_workspace`,
     `remove_probe_workspace`) that are **NOT** newly introduced by
     this task, are **not** flagged by the newer stable (1.98.0)
     clippy CI actually runs (`.github/workflows/ci.yml` only runs
     `stable` toolchain clippy/fmt/test/audit -- there is no MSRV
     clippy job in CI), and touching `workspace.rs` is outside
     056.001-T's declared file ownership. Left untouched; recorded here
     as a known, non-blocking, MSRV-clippy-only latent finding for a
     future Stage-triaged cleanup task if ever desired. `cargo check`/
     `cargo test` both pass clean under `+1.75.0` for the whole crate
     (MSRV compile/test compatibility, the actually-required contract,
     is intact).
   - `cargo audit` clean (no new dependency added).
   - **Real-world run executed** (the core acceptance criterion) via
     `cargo +1.75.0 run --manifest-path tools/mcp-probe/Cargo.toml --
     exact-cli --copilot-exe C:\Tools\copilot.exe --repo-root
     C:\Source\GitHub\graphtor --inner-exe C:\Tools\bun.exe --inner-arg
     .copilot/graphtor-mcp-shim.cjs --inner-arg graphtor-docs
     --inner-arg serve --entry-name graphtor-docs --leg-deadline-secs 60
     --gate1-deadline-secs 20` against the real installed Copilot CLI
     (`GitHub Copilot CLI 1.0.84-3`, content hash `375c12c42318bea6`,
     144861984 bytes) and the real production `bun.exe` +
     `.copilot/graphtor-mcp-shim.cjs`.
     - **Gate 1: PASSED.** The CLI logged `Warning: skipping workspace
       MCP config "...\ancestor\.mcp.json" because it is malformed`
       (the owned 056.021-T sentinel-invalid ancestor fixture) and
       resolved the requested entry's `sourcePath` to the nested
       child's own `.mcp.json`, not the ancestor -- proving
       nearest-config-wins isolation with **zero** AI-model cost.
     - **`h3_b_candidate: false`** (Gate 1 passed; no ancestor merge or
       both-legs-foreign result) -- so the Gate-1 branch to
       `H3-B-candidate`/forward-to-056.019-T does **not** apply.
     - **Wrapper handoff parity confirmed on disk**: control and
       treatment `.mcp.json` wrapper args are byte-identical (same
       `--inner-exe`, `--inner-arg .copilot/graphtor-mcp-shim.cjs`
       `graphtor-docs` `serve`, same `--evidence-output`/`--run-nonce`);
       treatment alone adds `"cwd": "\\?\C:\Source\GitHub\graphtor"`.
     - **Sentinel-inheritance observation: OBSERVED** on both legs (a
       direct child matching this binary's own path was seen carrying
       `MCP_PROBE_ENV_INHERITANCE_SENTINEL` in its OS-reported
       environment) -- confirms the new `sysinfo`-based watcher works
       against the real process tree on this host and gives 056.006-T a
       positive env-inheritance selection signal.
     - **Ordered cause classification: `unresolved`** -- "Cause not
       resolved by cwd alone for build 'affected': neither leg reached
       a connected, initialize-correlated state (control
       last_mcp_status=Some("failed"), treatment
       last_mcp_status=Some("connected"))." The wrapper's own
       JSON-RPC frame capture recorded exactly one event on each leg
       (`server/discover`, not `initialize`) and `initialize_correlation:
       null` on both legs, so even the treatment leg's "connected"
       session-level status did not satisfy the compound
       connected-AND-initialize-correlated bar `classify_pass` requires
       before declaring `H0a-retained`. This is honest, as-observed
       evidence, recorded without a rerun (one-shot contract) and
       without modifying the classifier to make the result look
       cleaner.
     - **Terminal: `done`** (not `blocked`) -- evidence capture itself
       was never blocked; this is a fully conclusive, if unresolved,
       one-shot classification.
     - **Stable-build comparison: not run.** Investigated
       `C:\Tools\copilot.exe.old-26912-*`/`*-30316-*` (no `--version`
       output at all) and the versioned
       `%LOCALAPPDATA%\github-copilot-sdk\cli\{1.0.71,1.0.73,1.0.80}\
       copilot.exe` caches (each independently reports the same live
       `1.0.84-3` version string when invoked directly, despite
       distinct content hashes and distinct folder version labels --
       strongly suggesting these are thin self-relaunching/self-updating
       launchers rather than genuinely pinned historical binaries). No
       last-known-stable build could be identified with reasonable
       confidence, so per the AC's own conditional wording ("when a
       last-known-stable Copilot executable is available"), only the
       required single affected-build pair was run.
     - Full JSON output persisted transiently under
       `logs/probe/exact-1789250503062413000-35436/` (gitignored,
       per-design scratch workspace; `evidence.json` +
       `control/`/`treatment/wrapper-evidence-snapshot.json` +
       `ancestor/` fixture all inspected and confirmed correct). No
       lingering `mcp-probe.exe`/orphaned processes after the run.
   - **Consequence for 056.019-T**: 056.019-T's own first disposition
     rule applies directly -- "If T0 emitted neither an H3-B cwd cause
     nor an `H3-B-candidate` from a Gate-1 ancestor-config merge, move
     to `done` with `not-needed: H3-B / isolated-config mechanism not
     evidenced`." Since `h3_b_candidate=false` and 056.001-T's
     classification vocabulary never emits a distinct "H3-B cwd cause"
     label (that is 056.019-T's own separate working-directory-mechanism
     hypothesis, tested independently), 056.019-T is expected to
     resolve as `done`/`not-needed` without needing its own bounded
     CLI adjudication -- to be confirmed when 056.019-T is implemented.

## Remaining work (not yet started)

- 056.002-T (out-of-process serve handshake test driver, main crate,
  independent of chain 1)
- 056.003-T (serve_preflight.rs hardening in main crate, depends on
  056.002-T)
- 056.019-T (H3-B terminal adjudication, depends on 056.003-T +
  056.001-T; expected to resolve `done`/`not-needed` per
  `h3_b_candidate=false` above, but implemented and confirmed rather
  than assumed; may legitimately end `blocked` if that expectation
  proves wrong, which would halt 049-S closure)
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
