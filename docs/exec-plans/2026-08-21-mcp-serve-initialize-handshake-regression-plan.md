---
title: "Implementation Plan: Fix graphtor-docs MCP serve initialize-handshake regression (Copilot CLI OS error 232)"
description: "Grounded, evidence-first, test-first plan to restore graphtor-docs MCP STDIO serve compatibility with recent GitHub Copilot CLI builds by capturing the child-process exit cause and hardening the pre-serve startup path"
doc_type: "plan"
topic: "graphtor-docs MCP serve initialize handshake"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
backlog_refs:
  - "049-S"
  - "056-F"
status: "draft"
source: "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
tags:
  - mcp
  - serve
  - rmcp
  - regression
  - test-first
---

## Summary

Recent GitHub Copilot CLI builds can no longer connect to the `graphtor-docs`
MCP server: the STDIO transport pipe closes with Windows **OS error 232**
(`ERROR_NO_DATA`, "pipe is being closed") while the client sends the
`initialize` request. OS error 232 is a write to a **closed** pipe, so the
signature is the **server process exiting before the handshake completes** —
most plausibly one of `cmd_serve`'s pre-`serve_server` normal exits or
fail-closed errors (cwd-relative `.graphtor/*.db` discovery, lock contention /
stale-lock pid reuse, pre-v4 schema, duplicate intake, or another config/open
gate), triggered by a change in *how* the new CLI launches the child
(cwd / env / lifecycle) rather than by any graphtor-docs code change. The full
differential diagnosis — and
why an rmcp `get_info` protocol-echo change is a no-op on rmcp 1.5 — is in the
linked deliberation. This plan restores connectivity **evidence-first** and
**test-first**.

## Goal / Definition of Done

* The newest Copilot CLI connects to `graphtor-docs` via `/mcp show
  graphtor-docs` with no OS error 232 and a completed `initialize` handshake.
* The probe is a **standalone, non-published diagnostic crate** at
  `tools/mcp-probe/` (its own `Cargo.toml` with an empty `[workspace]` table plus
  `publish = false`, Rust 2021 / MSRV 1.75), split by width: `056.020-T`
  self-tests the **core synchronous transport** (raw `std::process`/`std::thread`
  duplex pumps, half-close, bounded stderr drain, deadlines); `056.023-T` owns
  the copy-only observer seam and in-memory evidence; `056.021-T` owns the
  isolated `logs/probe/<nonce>` workspace plus control/treatment and nested
  ancestor/child config fixtures; `056.022-T` owns process spawning and teardown
  by **direct `Child` handles only** (a `sysinfo` adapter observes identity for
  diagnostics/tests but never kills). The chain is
  `056.020 -> 056.022 -> 056.023 -> 056.021 -> 056.001`. T0 (`056.001-T`) owns
  `exact_cli.rs`, records the exact newest failing CLI executable/version/build
  and `/mcp show graphtor-docs` invocation, **first proves ancestor
  config-isolation** against the nested fixture, then runs ONE control/treatment
  pair through the diagnostic wrapper handoff (child `.mcp.json` uses the wrapper
  as `command`; wrapper args encode the exact absolute production inner
  executable plus original args; control/treatment differ only by treatment
  `cwd`), emits the ordered cause classification, and ends `done` or blocked/
  H3-B2 with no implementation loop. The user `.mcp.json` is never read, mutated,
  or restored, so no config approval receipt is required. Raw frames stay in
  memory (via the observer seam) for same-run replay; only redacted summaries and
  digests are persisted. The ordinary Cargo target is a build cache, not a trust
  boundary.
* The evidenced branch restores connectivity without relaxing workspace
  containment, fail-closed validation, or verified-live lock ownership.
* Server startup failures are diagnosable even when the CLI discards child
  stderr (opt-in file-log sink or a documented redirect recipe).
* All quality gates pass: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit`, and `cargo build --release`.
  Any rmcp/dependency change also passes
  `cargo +1.75.0 check --all-targets`. The standalone probe crate is verified
  separately via `cargo +1.75.0 check|test|build|clippy --manifest-path
  tools/mcp-probe/Cargo.toml` (clippy `-D warnings -D clippy::pedantic`).
* Rollback and three exact-Copilot sessions on the exact T0 CLI identity and
  restored post-fix user-facing entry are documented; each session records a
  completed `initialize`, a `tools/list` containing the expected tools, and one
  side-effect-free `get_status` fingerprint matched to a direct control, with the
  production config hash correlated to the Copilot-spawned server startup event.
  T0's wrapper or any executable substitution is invalid production evidence.

## Likely Surfaces (exact)

| Surface | Location | Change |
|---|---|---|
| Core synchronous transport (T00A, standalone crate) | standalone `tools/mcp-probe/` crate (own `Cargo.toml`, `[workspace]` + `publish=false`, Rust 2021/MSRV 1.75): `src/main.rs` + `src/transport.rs` | Self-test raw `std::process`/`std::thread` full-duplex pumps, half-close, bounded stderr drain and buffers, deadline signaling; expose a post-write bounded non-blocking copy-delivery seam. No observer/evidence, workspace/config, process teardown, Tokio, or `unsafe`. Ordinary Cargo target is a build cache, not a trust boundary → `056.020-T` |
| Probe process spawning + teardown (T00C, standalone crate) | `tools/mcp-probe/src/process.rs` + sequential `main.rs` wiring; observe-only `sysinfo` adapter | Direct `Child` handles are the sole kill/wait authority (runner owns the Copilot `Child`, wrapper owns the inner-server `Child`; wrapper PID observed-only). A `sysinfo` adapter observes PID/start-time/executable/parent/nonce for diagnostics/tests but never kills; same-second identity is ambiguous and fails closed; residual/unknown descendants fail the evidence and surface exact identities for operator action → `056.022-T` |
| Probe observer seam + evidence (T00D, standalone crate) | `tools/mcp-probe/src/evidence.rs` (+ `observer.rs` only if needed) | Copy-only read-only observer seam consumed after each direction's ordered write/flush; bounded non-blocking delivery; saturation/failure atomically invalidates capture while forwarding is unchanged; raw chunks in memory only, redacted structured summaries/digests persisted → `056.023-T` |
| Isolated probe workspace + config fixtures (T00B, standalone crate) | `tools/mcp-probe/src/workspace.rs` creating `logs/probe/<nonce>` + temporary in-workspace `.mcp.json` | Exclusive-create isolated workspace validated by `validate_path`/`is_reparse_point`; generate in-workspace control/treatment entries and an owned nested ancestor/child config fixture; owned-workspace-only cleanup. Owns no observer/evidence module; never touches the user `.mcp.json` → `056.021-T` |
| Serve startup diagnostics (T2, non-conditional, parity-safe) | `src/main.rs::cmd_serve`, duplicate-intake and database-open preflight | Route every pre-transport normal exit through an exhaustive typed seam, including pre-v4 and duplicate-intake exits. Preserve unconditional stderr and add structured events; emit `mcp_serve_ready` immediately before `serve_server` → `056.003-T` |
| Managed config outcome contract (conditional H0a/H3-B1) | `src/workspace/mcp_config.rs` | Distinguish typed create/update/no-change/collision outcomes from fail-closed `PathViolation`; forbid message sniffing → `056.017-T` |
| Managed MCP launch fields (T2d, conditional H0a/H3-B1) | `src/workspace/mcp_config.rs::managed_server_value` | Add only canonical project-root `cwd` and the evidenced stdio discriminator after T0/H3-B capability proof → `056.008-T` |
| Safe no-follow primitive decision (conditional H0a/H3-B1) | decision/spike recorded in `docs/decisions/` → `056.024-T` | Decide an MSRV-1.75, safe-call-site, no-follow/capability-based mutation primitive without relaxing `#![forbid(unsafe_code)]`; evaluate a narrowly justified safe dependency vs a std-only contract, prove with a minimal PoC, and record the decision or block 056.018/shipment → `056.024-T` |
| Contained recovery primitive (conditional H0a/H3-B1) | selected primitive in `graphtor_core::path` (e.g. `src/path/handle.rs`) + lazy accessor in `src/workspace/paths.rs` | Implement ONLY the `056.024-T`-selected no-follow/reparse-safe primitive for I/O, exclusive owner-protected artifacts, and exact restore; assert no std handle-level no-follow, take no `unsafe` exemption, make no managed-config/install/uninstall/doctor edits → `056.018-T` |
| Existing-install refresh (T2e, conditional H0a/H3-B1) | `src/main.rs::cmd_upgrade`, managed-config typed APIs | Refresh marked/exact-legacy entries and expose typed text/JSON action + recovery metadata; preserve collision/non-JSON bytes and Minimal footprint → `056.009-T` |
| Advisory lock characterization + implementation (conditional H0b) | `src/lock.rs` shared `AdvisoryLock` used by Database and Workspace locks | Passing characterization → `056.016-T`; one conservative high-resolution/boot-aware policy plus task-local red/green and legacy recovery → `056.007-T` |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if stderr is unavailable and CLI env inheritance works: unique exclusive absolute per-attempt sink consuming typed T2 events. No shared/relative sink or production-entry env field → `056.006-T` |
| H0c operational remediation (T2f, H0c-only — conditional) | evidenced fail-closed surface (registry / explicit `--config` / pre-v4 schema / duplicate-intake) + operational recipe | Repair exactly ONE evidenced gate with fresh approval and backup, then one confirming re-probe; retain rollback through T4. A second H0c gate, backward-pointing, or unowned evidence blocks T4 and becomes a NEW bounded Stage follow-up (no in-place loop or sibling reactivation). Pre-v4 rebuild uses `sync`, never `upgrade` → `056.010-T` |
| Embedding resolution outcomes (conditional H1) | `src/embed/resolver.rs` | Add typed `Loaded`/`Disabled`/`Failed` detailed result while preserving an adapter for unrelated callers → `056.014-T` |
| Shared lazy lifecycle (conditional H1) | `src/embed/lifecycle.rs` | Supervised clone-shared lifecycle state machine only; the versioned Loading/Failed/Disabled availability projection and `src/mcp/server.rs` wiring move to `056.015-T` → `056.005-T` |
| Serve/background-sync wiring + MCP availability projection (conditional H1) | `src/main.rs::cmd_serve`, `spawn_background_sync`, `src/mcp/server.rs` | Own the versioned Loading/Failed/Disabled MCP availability projection (moved from `056.005-T`); inject one shared owner into MCP and Generation sync; neither eager load nor background sync may block initialize → `056.015-T` |
| Server transport compatibility (conditional H3-A) | rmcp pin + STDIO wiring, after `056.015-T` | Own transport types/wiring only; reacquire/replay the exact-client transaction in one execution via the `056.001-T` runner + `056.023-T` observer (no persisted raw frames), red/green and Rust 1.75 proof; T4 owns production acceptance → `056.011-T` |
| Client isolated-config + cwd compatibility (conditional H3-B) | actual Copilot CLI capability evidence | Adjudicate BOTH a documented isolated-config discovery mechanism (owning the one bounded attempt when Gate 1 shows ancestor merge) and a distinct working-directory mechanism, each one-shot; B2 unsupported-client blocks; never reads/mutates the user root config; temporary proof only activates managed-config tasks and never satisfies T4 → `056.019-T` |
| Operator documentation (documentation-only) | two named sections each in `docs/troubleshooting.md` / `docs/cli-reference/graphtor-docs.md` or `docs/mcp-tools.md` / `docs/cli-reference/graphtor-docs.md` | Diagnostics plus selected H0b/H0c/H1 contracts → `056.012-T`; managed launch/recovery and H3 → `056.013-T` |
| Tests | `tests/common/mcp_driver.rs`, `tests/mcp_serve_handshake_test.rs`, and colocated focused tests | T1 owns the shared driver module; each production task owns at most three grouped scenarios. Actual-client acceptance remains the final H3/T4 gate |

## Task Breakdown (evidence-first, test-first, ~2h each, single-width)

### T00A — Core synchronous transport — backlog `056.020-T`

* Create a **standalone, non-published diagnostic crate** rooted at
  `tools/mcp-probe/` with its own `Cargo.toml` declaring an empty `[workspace]`
  table (so the crate stands outside the production package and workspace),
  `publish = false`, and `edition = "2021"` / `rust-version = "1.75"`. Add no
  root `probe-harness` feature, no root `[[bin]]`, no custom `--target-dir`, and
  no ACL/DACL/artifact hardening. The standalone crate's ordinary Cargo target
  directory is a build cache, not an evidence trust boundary — anyone able to
  modify source can already alter the probe, so build-artifact hardening is
  security theater and is removed.
* Own the **core synchronous transport only**: a thin `src/main.rs` composition
  root plus `src/transport.rs`. Self-test transport only (no actual CLI, no
  workspace, no config, no persisted evidence, no observer):
  1. raw `std::process`/`std::thread` independent full-duplex pumps for
     client→child stdin and child→client stdout, a bounded stderr drain, bounded
     buffers, client EOF→child stdin half-close, child stdout EOF→client close,
     and deadline signaling; protocol bytes never rewritten or reframed;
  2. after each direction's ordered write-and-flush completes, deliver forwarded
     byte copies through a **bounded, non-blocking** hook (the seam consumed by
     `056.023-T`); with an absent or slow delivery target the pump forwards bytes
     byte-for-byte unchanged and never blocks.
* This task owns no observer, evidence, redaction, workspace, config,
  process-identity, or exact-CLI concern. It holds only the direct
  `std::process::Child` handle needed to wire stdio for transport self-tests and
  adds no `sysinfo`/process-tree ownership, no kill/wait teardown authority
  (`056.022-T`), no Tokio, and no `unsafe`. The observer seam and evidence are
  `056.023-T`; process spawning/teardown are `056.022-T`; the isolated workspace
  and config fixtures are `056.021-T`; the exact-CLI run is `056.001-T`.
* Verify via `cargo +1.75.0 check|test|build|clippy --manifest-path
  tools/mcp-probe/Cargo.toml` (clippy `-D warnings -D clippy::pedantic`). The
  crate is never installed or committed as a binary and is invalid for T4.

### T00C — Probe process spawning and teardown — backlog `056.022-T`

* Depends on `056.020-T`. Own `tools/mcp-probe/src/process.rs` and focused tests;
  sequentially wire it into `tools/mcp-probe/src/main.rs` to compose process
  spawning onto the transport. Add no `unsafe` and no new process-control crate.
* **Direct `std::process::Child` handles are the ONLY kill/wait authority.** The
  exact-CLI runner (the `056.001-T` composition point) owns the Copilot `Child`
  guard; the wrapper owns the inner-server `Child` guard. Each guard kills and
  waits its own child on every outcome. The wrapper PID is **observed-only** and
  is never used to kill.
* Remove every `sysinfo`/PID/start-time **kill** fallback and every claim of
  exact arbitrary-descendant identity or atomic whole-tree ownership. A safe
  `sysinfo` adapter behind a trait MAY observe PID, process-start-time,
  executable, parent, and a launch nonce for diagnostics and deterministic
  tests, but enumeration can only verify and report — it MUST NOT kill.
* Fail closed on ambiguity: a same-second start-time (or otherwise
  indistinguishable identity) is never a confirmed match. If residual wrapper or
  unknown descendant processes remain after the owned guards run, the evidence
  fails and the exact diagnostic identities (PID/start-time/executable/parent)
  are surfaced for operator-approved action; the task never kills them itself.
* Self-test at most three grouped scenarios through an injectable observation
  adapter: (1) normal completion plus back-to-back runs cleaned up by direct
  handles with no leaked owned child; (2) deadline/error teardown by direct
  handles; (3) deterministic PID-reuse ambiguity (a reused PID with a
  same-second/indistinguishable start-time is reported ambiguous and fails
  closed) and an observed-only residual descendant surfaced, not reaped. Runs no
  real CLI, creates no workspace, writes no `.mcp.json`, persists no evidence.

### T00D — Probe observer seam and evidence capture — backlog `056.023-T`

* Depends on `056.022-T` (chain `056.020 → 056.022 → 056.023 → 056.021`). Own
  `tools/mcp-probe/src/evidence.rs` (add `observer.rs` only if genuinely needed)
  and the copy-only read-only observer seam. Never reimplement the `056.020-T`
  duplex pumps or the `056.022-T` teardown.
* Define the seam so the `056.020-T` pump performs its ordered write-and-flush
  FIRST, then delivers copies/immutable chunks. The pump NEVER awaits observer
  work and NEVER takes a cross-direction observer lock, so observation cannot
  reorder, delay, or deadlock forwarding.
* Deliver through a bounded, non-blocking channel. On saturation or observer
  failure, atomically invalidate the affected capture (mark evidence incomplete)
  while forwarding continues byte-for-byte unchanged. The observer receives
  copies or immutable chunks only and never mutates, reorders, drops, or reframes
  wire bytes.
* Keep raw transaction chunks in memory only for same-run replay by `056.001-T`;
  persist only redacted structured summaries and digests — never raw frame bytes
  on disk.
* Self-test exactly this evidence-observation domain (no process spawn, no
  workspace/config, no exact CLI): (1) a paused/slow observer and a failing
  observer each prove byte-identical forwarded order, half-close propagation, and
  deadline outcome versus a no-observer control; (2) redaction of secret-bearing
  argv/env/message fields; (3) no raw-frame persistence. Verify via
  `cargo +1.75.0 check|test|build|clippy --manifest-path tools/mcp-probe/Cargo.toml`.

### T00B — Isolated probe workspace and config fixtures — backlog `056.021-T`

* Depends on `056.023-T` (chain `056.020 → 056.022 → 056.023 → 056.021`). Own
  **only** `tools/mcp-probe/src/workspace.rs`; sequentially wire it into
  `tools/mcp-probe/src/main.rs` after `056.023-T`. Compose — never reimplement —
  the `056.020-T` transport, the `056.022-T` process guards, and the `056.023-T`
  observer/evidence seam. Own **no** observer/evidence module and add **no** new
  production path primitive.
* Create a fresh isolated workspace under the canonical repository path
  `logs/probe/<nonce>` through the Rust probe using **exclusive creation** (fail
  if the path already exists), validating every component with the existing
  `graphtor_core::path::validate_path` / `is_reparse_point` helpers before use.
* Threat model (explicit): this protects against accidental escape of the
  repository root and a pre-existing reparse point/junction on the workspace
  path. It does **not** defend against a malicious same-user process; the
  documented `validate_path`/`is_reparse_point` TOCTOU window is an **accepted
  residual risk** for this non-sensitive, same-user diagnostic workspace.
* Generate the temporary control and treatment `.mcp.json` **only inside** that
  isolated workspace. Never read-modify-write, back up, restore, or substitute
  the user `.mcp.json`; therefore no config backup, restore, or approval receipt
  is required. Control and treatment are byte-equivalent to the production
  server entry semantics except the diagnostic wrapper `command`; the treatment
  entry alone differs by adding the candidate `cwd` mechanism.
* Build an **owned nested ancestor-discovery fixture** inside the isolated
  workspace: an owned parent directory holding a deliberately invalid/sentinel
  ancestor `.mcp.json`, and a child run directory holding the intended temporary
  `.mcp.json`. This lets `056.001-T` prove, with the exact target CLI, that the
  nearest child config shadows (and does not merge) the ancestor before any
  causal contrast; the repository-root `.mcp.json` is never assumed unread.
* Own the exact workspace cleanup/retention policy: never delete anything outside
  the exact owned `logs/probe/<nonce>` workspace, and keep destructive-cleanup
  approval constraints explicit. The redacted evidence written into the workspace
  is owned by `056.023-T`.
* Performs no production acceptance. The exact-CLI run is `056.001-T` and
  restored-production acceptance remains solely T4.

### T0 — Run the one-shot exact-CLI classification — backlog `056.001-T`

* Own `tools/mcp-probe/src/exact_cli.rs` plus the ONE final subcommand-wiring
  edit in the thin `tools/mcp-probe/src/main.rs`. Record the **exact** newest
  failing Copilot executable path, version/build, and `/mcp show graphtor-docs`
  invocation. T4 must use the same identity.
* Run inside `056.021-T`'s isolated `logs/probe/<nonce>` workspace using its
  temporary in-workspace control/treatment `.mcp.json`, the `056.020-T`
  transport, the `056.022-T` process guards, and the `056.023-T` observer seam;
  never reimplement pumps, teardown, or evidence capture. The user `.mcp.json` is
  never read or written, so no approval receipt applies.
* **Diagnostic wrapper handoff (parity):** the child `.mcp.json` uses the
  diagnostic wrapper as `command`; diagnostic-only wrapper args encode the exact
  absolute production inner executable plus its original args. Control and
  treatment are identical in this handoff and differ ONLY by the treatment `cwd`.
  Record and hash the exact inner executable path and version.
* **Gate 1 (ancestor config-isolation, first):** using the exact target CLI
  against `056.021-T`'s nested fixture, prove the nearest child `.mcp.json`
  shadows and does not merge the sentinel ancestor. Only then may the single
  control/treatment contrast run. If the exact CLI reads or merges the ancestor
  config, stop the causal H0 comparison and route to H3-B via `056.019-T`; never
  assume the repository-root `.mcp.json` is unread.
* **One-shot classification:** run exactly one control (no `cwd`) / treatment
  (canonical project-root `cwd`) pair through the shared runner, then emit the
  current **ordered** cause classification (proven prerequisites first — H0a is
  retained when a cwd correction exposes a later blocker) from child
  exit/liveness/framing/lock/Generation evidence. No implementation loop runs
  inside T0; it never reruns per correction, waits for production health, or
  reopens a downstream task.
* Terminal outcome: after the single pair the task emits the ordered downstream
  classification and moves to `done`, or blocks as H3-B2 unsupported-client, or
  returns blocked with the captured evidence. Downstream causal tasks are visited
  once in the authoritative forward-chain order.
* Preserve the unmodified duplex transaction, concurrent stderr, exit/still-alive
  state, locks, and Generation posture. Keep raw frames in memory via the
  `056.023-T` observer seam for same-run replay; direct replay is confirmation
  only. Apply the deadline through `056.020-T` and the direct-`Child`-handle
  teardown through `056.022-T` (no whole-tree claim). Persist only redacted
  summaries and digests; fail closed if process ownership, exact CLI identity,
  ancestor config-isolation, or same-inner-executable parity is unproved.
* Runtime classification runs via `cargo +1.75.0 run --manifest-path
  tools/mcp-probe/Cargo.toml -- exact-cli ...`; there is no `self-test`
  subcommand. Deliverable: correlated transcripts naming ordered proven
  prerequisites/causes or an explicit H3-B2 blocker. T0 links tasks; it owns no
  production implementation or docs.

### T1 — Green out-of-process handshake driver

* Add `tests/common/mcp_driver.rs` as the reusable owner of real-process spawn,
  named stdin, initialize framing, bounded response, concurrent bounded stderr
  drain, and cleanup
  helpers. `tests/mcp_serve_handshake_test.rs` consumes that module to **spawn
  the real binary** with a controllable cwd/env and a fixture workspace
  reproducing the T0 sub-cause:
  * **Keep the child's stdin OPEN** in a named `ChildStdin` binding for the
    duration of the attempt — do not
    pass an empty/closed stdin. A closed stdin only exercises a benign
    EOF-driven shutdown and cannot distinguish the regression from a normal
    end-of-input exit, so it would never actually write `initialize`.
  * Write a **protocol-valid, newline-delimited** MCP `initialize` JSON-RPC
    request to the child's stdin (a well-formed `jsonrpc: "2.0"` request with
    `method: "initialize"`, an `id`, and a `params` carrying `protocolVersion`,
    `capabilities`, and `clientInfo`), matching the framing rmcp's STDIO
    transport expects.
  * The driver's only positive signal is a successful `initialize` response
    with negotiated `protocolVersion`. T1 ends green with neutral self-tests
    and commits no branch-specific failure. Each selected curative task uses
    the driver to add/observe its red before production changes, then greens it
    before that same task completes. The reproduced
    regression (broken pipe, early exit, or timeout) is **never** encoded as
    the expected assertion — a test that asserts the reproduced failure would
    pass on the current regression and could not be greened by the fix (Copilot
    review P1: red-test polarity).
  * On that red, the harness **captures the child exit code (or signal) and
    full stderr purely as diagnostic evidence** that explains and confirms the
    reproduced cause — it never accepts them as the passing result:
    * **H0 (early exit):** the successful-`initialize` assertion fails because
      the child exits before answering (the `initialize` write hits a
      broken/closed pipe — the client-visible OS-error-232 analogue — or no
      response arrives before the deadline because the child exited); the
      captured exit code + stderr MUST match the confirmed T0 early-exit
      marker, tying the red to the real cause rather than an ambiguous EOF.
    * **H1 (latency):** the successful-`initialize` assertion fails because the
      response misses the bounded deadline while the child is **still alive**
      (no exit code yet), isolating latency from an early-exit crash.
  * For non-H1 fixtures, prewarm/pin model state out of the path. For H1,
    preserve the bounded real-process latency transcript as runtime evidence,
    but use `056.005-T`'s injected blocking/failing loader seam for deterministic
    Cargo tests; do not make a committed regression depend on cold-cache
    wall-clock timing or network access.
* Stop gate: for a repository-code branch, if its successful-`initialize`
  assertion cannot be made **red for the confirmed T0 cause**, halt and return
  to T0 rather than refactoring startup on a green or ambiguous test. For
  external-only H0c/H3-B, require a reproducible bounded before transcript
  instead.
* Deliverable: the green shared `tests/common/mcp_driver.rs` module with a
  concurrent bounded stderr drain and neutral self-tests. T1 commits no
  branch-specific failing assertion.
* **Branch-appropriate proof ownership:** T0 alone selects the branch. T1
  supplies the reusable spawn + concurrent stderr drain + `initialize` +
  timeout driver. Each selected repository-code task adds and observes its own
  red test immediately before production changes, then greens it in the same
  task: H0a uses that driver in
  `056.008-T`'s generated-entry test (pre-change entry has no `cwd`, so launch
  from the unrelated parent is red); H0b uses a reachable Generation-lock
  fixture; H3 mode A uses a framing-pinned fixture; H1 uses `056.005-T`'s
  deterministic loader seam plus the runtime transcript. Operational-only H0c
  and H3 mode B use the same bounded actual-client probe before and after the
  approved state/client repair rather than leaving an unsatisfiable Cargo test.
  H3 mode A replays the unmodified bidirectional transaction captured by T0;
  a generic valid initialize request is not an adequate framing regression.
  Any H0c actionability code change receives its own red test. Every task ends
  with all tests green; no failing-suite handoff is permitted.
* Width: test infrastructure only.

### T2 — cmd_serve pre-serve diagnostics (green, non-conditional, parity-safe)

* **Non-conditional.** This task delivers runtime-owned observability that is
  valuable on **every** causal branch and whose own test goes red before /
  green after its production change. It does **not** own curative H0a
  connectivity and **no longer claims** it can green a no-target wrong-cwd
  managed launch — H0a connectivity is owned by the pinned-cwd launch contract
  **T2d (`056.008-T`)** plus existing-install delivery **T2e (`056.009-T`)**.
* Runtime-owned diagnostics (Constitution V):
  * inventory **every** pre-transport normal exit across `cmd_serve`,
    duplicate-intake preflight, and `open_serve_databases`, including missing
    explicit config, empty discovery/classification/primary, pre-v4 schema, and
    duplicate-intake exits. Route them through one exhaustive
    `ServePreflightExit` enum (or equivalent typed seam), so the contract is
    not tied to an incorrect fixed count;
  * **mirror, never convert:** preserve each existing unconditional
    `eprintln!`/`errfmt` fatal message and additionally emit a stable
    `tracing::error!` event. `RUST_LOG=off` must not silence the user-facing
    failure. Before propagating registry/open/schema errors, emit one
    structured serve-preflight error event; the top-level renderer remains the
    sole fatal renderer for propagated errors;
  * emit a structured `mcp_serve_ready` info event with `transport=stdio`,
    `preflight_complete=true`, and the launch cwd immediately before calling
    `rmcp::serve_server`. It records intent to enter the server after preflight,
    not loop entry, transport readiness, or completed handshake.
* **No new containment surface and no discovery-signature change (F1/F2/F3/N1):**
  cmd_serve **continues** validating an explicit `--db-path`/`--config` against
  the authorized project-root cwd through the **shared**
  `discover_served_databases` / `graphtor_core::path::validate_path` /
  `is_reparse_point` primitives exactly as today (`candidate_root = cwd`,
  `scan_root = cwd/.graphtor`; `discover_served_databases` already validates an
  explicit `--db-path` against the broader project-root `candidate_root`). Do
  **not** derive a target-specific/split authorized root and **never**
  parent-walk from a foreign launch cwd — a foreign launch cwd is corrected by
  pinning cwd to the canonical project root in T2d, not by re-authorizing a
  target here.
* **Status parity preserved by construction (F4):** because no discovery
  signature changes, `discover_served_databases`, `classify_serve_postures`,
  `discover_status_db_paths`, and `cmd_status` stay in parity — no parity
  change and no new parity test are introduced. If a future runtime discovery
  signature change is ever taken, it MUST include those four surfaces plus
  parity tests; this remediation deliberately avoids that.
* Preserve **all** existing fail-closed semantics (malformed registry, missing
  explicit `--config`, pre-v4 gate, duplicate-intake preflight remain pre-serve
  gates). Add a representative propagated-error process row plus assertions
  that each class still exits pre-serve, so diagnostics never convert a
  fail-closed gate into a fail-open path.
* **Own exactly three observed-red groups:** exhaustive typed-exit
  formatter/event mapping; one propagated error plus `RUST_LOG=off`
  unconditional-stderr proof; and one seeded ready-event process row. Do not
  add an artificial control-flow injector for defensive variants.
  These tests assert the **diagnostic output** (message + serve-ready log),
  **not** loop entry or a successful `initialize` handshake — the raw no-target wrong-cwd
  `initialize` success is **not** this task's to green (it is owned by the H0a
  generated-contract test `056.008-T` and, for other branches, the selected
  curative task using T1's driver). Reuse `tests/common/mcp_driver.rs`; do not copy helpers between
  integration-test crates.
* Do **not** add the optional file-log sink (that is T2c/`056.006-T`).
* `056.012-T` owns troubleshooting and CLI-reference documentation.
* Width: serve startup runtime diagnostics only. Curative H0a launch-contract
  generation (T2d), existing-install delivery (T2e), stale-lock liveness (H0b),
  the diagnosability sink, H0c operational remediation, H1 model lazy-load, and
  the H3 transport fix are **separate** tasks below.

#### T2d — (Conditional on H0a/H3-B1) Managed launch-contract generation — backlog `056.008-T`

* Only if T0 evidences H0a and the target build honors `cwd`, or if H3-B1
  proves the same exact CLI honors a different documented working-directory
  mechanism. If the current build supports no safe mechanism, `056.019-T`
  selects B2: move this conditional task to `done` with
  `not-needed: H3-B2 selected` while shipment/T4 remain blocked.
  `056.017-T` first gives the generator typed create/update/no-change/collision
  outcomes and a distinct fail-closed `PathViolation`; no caller classifies
  config behavior by message text.
* **Distinct width from T2:** this changes the install/config surface
  (`src/workspace/mcp_config.rs`), not runtime `cmd_serve`. The generated
  managed entry today carries only `command` + `args: ["serve"]` + `transport`
  + the managed marker, so the CLI launches the child with no workspace
  identity.
* **Complete minimal contract:** pin the child working directory (`cwd`) to the
  canonical project root. Registry discovery, posture/Generation validation,
  DB auto-discovery, and background sync are all cwd-anchored, so this single
  lever restores them together without relaxing runtime containment. Do not
  generate `--db-path`, `--config`, or env target plumbing; those are
  unnecessary for H0a and cannot substitute when a client ignores `cwd`.
* **Containment:** validate the pinned `cwd` by equality to the canonicalized
  project root. Add no target-derived/split authorized root, target
  self-authorization, parent traversal, or external-path fallback.
* **Config-schema (F7):** emit the T0-evidenced supported transport field. The
  sibling `.mcp.json` entries (`backlogit`/`github`/`context7`/`tavily`) use
  `type: "stdio"` while `managed_server_value` emits `transport: "stdio"`; if T0
  confirms the Copilot CLI honors `type`, emit `type` (keeping or migrating the
  legacy `transport` recognition via `is_exact_legacy_shape`). Preserve
  marker-based recognition so already-installed managed entries still refresh
  after gaining `cwd` plus the evidenced stdio key. Do **not** claim the field name is the
  current root cause without T0 evidence.
* **Test (test-first proof for this width):** three grouped scenarios cover
  managed-value fields, the unrelated-parent launch below, and
  containment/legacy-marker preservation. The managed-launch integration test
  that (1) generates the managed entry via `generate_mcp_config` for project
  `P`, (2) reads it back, and (3) **executes the generated launch contract**
  (`command` + `args` + pinned `cwd`) directly from an **unrelated parent
  cwd**, asserting a successful `initialize` handshake and that `P`'s
  databases are served. The fixture uses a managed binary (or explicit PATH
  injection) and seeds a serveable database so command resolution or an empty
  workspace cannot mask the cwd signal. Red before the generator change, green after. This
  proves generator correctness after T0 establishes real-client field support;
  it does not prove Copilot honors the field.
* **Delivery to existing installs:** generating the entry only helps fresh
  installs / regeneration; the already-installed bug-reporter workspace is
  repaired by the separate migration task **`056.009-T`** (which refreshes the
  managed entry on `cmd_upgrade`/reinstall). Do not assume a binary upgrade
  rewrites `.mcp.json`.
* `056.013-T` owns the selected stdio-field and managed-cwd documentation.
* Contingency: move to `done` with a `not-needed: <rationale>` backlog comment
  when neither H0a nor H3-B1 selects managed `cwd`. `not-needed` is a
  disposition, not a backlog status. Width: managed launch-config generation
  only.

#### T2e — (Conditional on H0a/H3-B1) Deliver the launch contract to existing installs — backlog `056.009-T`

* Only when T2d/`056.008-T` is selected (H0a or H3-B1). If T2d closes,
  T2e closes too; under H3-B2 this task is not-needed but shipment remains
  blocked. **Distinct width from T2d:** T2d changes the
  generated value; this task **delivers** the refreshed managed entry to
  **already-installed** workspaces. Verified via Engram + exact reads:
  `generate_mcp_config` is invoked **only** from `cmd_install` (`src/main.rs`
  ~3258) and `cmd_install_full` (~3360); `cmd_upgrade` (~3480-3538) calls
  `workspace::upgrade::upgrade`, which never rewrites `.mcp.json`. So a binary
  upgrade leaves the bug reporter's existing managed entry **stale** and
  un-repaired (Copilot review P1: existing-install migration).
* **Prerequisites:** `056.017-T` solely supplies typed config actions and
  fail-closed path errors. `056.018-T` supplies only the lazy recovery path
  accessor and handle-level no-follow/reparse-safe primitives; each operation
  uses the verified handle and aborts if parent/destination identity changes.
* **Primary code acceptance (S1):** wire the idempotent, marker-safe typed
  refresh into `cmd_upgrade`. Preserve the binary-only responsibility of
  `workspace::upgrade::upgrade`; `cmd_upgrade` reports the separate config
  action in both text and JSON (`mcp_config.action`, recovery path, and warning).
  Marked/exact-legacy entries may update; user collision is a typed
  non-mutating outcome; containment remains a hard `PathViolation`.
* **Reinstall is a manual fallback/rollback only:** a required `graphtor-docs
  install`/reinstall recipe documented by `056.013-T` with a verification
  step, for when the automatic upgrade refresh is judged unsafe or must be
  reverted. It does **not** substitute for the automated red/green migration
  test.
* **Refresh outcomes:** preserve unowned/non-JSON bytes and never create a
  backup for no-change/collision. A Minimal footprint returns typed
  not-applicable unless a valid managed executable contract exists; it is not
  rewritten to a bare PATH command.
* **Backup-first mutation:** `056.018-T` owns only a lazy recovery path accessor
  plus handle-based no-follow create/restore primitives. It does not edit
  managed-config, install, uninstall, doctor, or upgrade surfaces.
  `056.009-T` composes those handles with `056.017-T`'s typed API in
  `cmd_upgrade`, keeps artifacts through T4, and records the target workspace's
  post-refresh production config hash.
* **Exactly three observed-red groups:** successful nested marked/legacy
  refresh with co-resident bytes and recovery metadata; no-change/collision/
  non-JSON preservation with no backup; and recovery/mutation failure with
  original bytes intact. `056.013-T` owns operator documentation.
* Contingency: move to `done` with a `not-needed: 056.008-T not selected`
  comment whenever `056.008-T` closes.
  Width: install/upgrade delivery of the managed entry only.

#### T2b — (Conditional H0b) Shared lock-policy harness and implementation — backlogs `056.016-T` / `056.007-T`

* Only if T0 selects H0b on a Generation target. The shared
  `AdvisoryLock` serves both Database and Workspace locks, so
  `056.016-T` first adds exactly three **passing** characterization groups:
  Database strong/reused/legacy identity; Workspace legacy/reused identity;
  and `--force-unlock`/replacement-guard interaction. It makes no production
  change. It records current legacy-live-old age eviction as baseline only;
  `056.007-T` adds and observes the desired refusal red before implementation.
* `056.007-T` then records an OS-native high-resolution process creation
  identity plus boot/session discriminator where available. A matching strong
  identity is live regardless of age; a mismatch proves reuse; a confirmed
  dead pid is stale. Epoch-second equality is never called strong. Any
  second-resolution or otherwise ambiguous identity fails closed.
* A live legacy pid-only record remains live regardless of age. Apply one
  conservative policy to both Database and Workspace locks; do not introduce
  per-`LockKind` divergence.
* **Lock-file format compatibility (required, both directions):** a lock file
  written by a prior binary (no start-time field) must degrade to a
  pid-liveness-only check: live pid = locked regardless of age; dead/absent pid
  = stale. It must **never** parse-error into `GraphtorError::Config` —
  a parse failure would itself become a new pre-serve fail-closed exit (a fresh
  232). Symmetrically, a lock file carrying an **unknown extra field** (as a
  future binary might add) must also parse **without error** (unknown fields
  ignored — forward-compatible), never a hard fail. Preserve the existing atomic
  write-cleanup and concurrent-release NotFound-retry behavior. Add
  the three groups established by `056.016-T`; do not add a second matrix.
* A verified live-but-hung holder is never age-evicted. Never terminate a
  process from pid-only legacy evidence. Operator recovery verifies executable,
  start identity where available, and target ownership. If the live pid is
  unrelated and no graphtor writer owns the target, an explicitly approved
  backup-and-remove of that exact legacy lock is recorded before retry. Do not
  add an unauthenticated force-eviction path.
* Contingency: if H0b is not evidenced, move both tasks to `done` with
  `not-needed: H0b not evidenced`. `056.012-T` owns recovery docs.

#### T2c — (Conditional/optional) Startup diagnosability sink — backlog `056.006-T`

* Depends on `056.003-T` so it consumes the normalized diagnostics rather than
  racing edits to the same early-exit sites.
* Default: rely on inherited target-CLI stderr. Select this task only if T0
  proves stderr unavailable **and** the exact CLI propagates the sink env gate;
  otherwise close it done-plus-not-needed. T4 never substitutes an executable.
* If built, it captures every typed T2 preflight exit, the propagated-error
  event, and `mcp_serve_ready`.
* **Adjudication (retain, evidence-gated — not speculative):** the sink is kept
  in the plan only because it targets a **distinct evidenced condition** the
  default cannot cover — T0 showing the CLI **discards** child stderr while
  preserving inherited environment. It is **not**
  general speculative logging: if T0 shows child stderr is capturable via the
  documented redirect (the common case), this task closes as *not-needed* and
  no sink is built.
* **T4 verification coupling:** T0/T4 supply one unique **absolute** sink path
  per owned attempt through the CLI process environment. Open with exclusive
  create/no-follow under the authorized root; reject relative, existing,
  linked, stale, or escaping paths. Include nonce/run id, PID, config hash, and
  line-delimited events. A shared sink or cwd-relative `.graphtor` default is
  forbidden. Surface every initialization/write failure.
* Exactly three test groups cover unique gate-on capture,
  collision/link/write refusal, and gate-off/env-unavailable behavior.
* Contingency: move to `done` with
  `not-needed: actual-client stderr capture sufficient` when the sink is not
  required. Width: logging/diagnosability only.

#### T2f — (Conditional on H0c evidence) Operational remediation of the fail-closed cause — backlog `056.010-T`

* Only if T0 evidences a **legitimate fail-closed gate** as the H0c handshake
  blocker — a malformed source registry, a missing explicit `--config`, a
  pre-v4 schema DB, a duplicate-intake preflight failure, or another fail-closed
  cause that exits `cmd_serve` before the transport binds. The diagnosability
  sink (`056.006-T`) makes such a cause **visible** but cannot **remediate** it,
  so without this task the H0c branch can never reach the healthy handshake T4
  requires (Copilot review P1: H0c closure).
* **Do NOT weaken the fail-closed gate.** Provide a **tracked** operational
  remediation that repairs the workspace **state** so `serve` legitimately
  passes the gate and completes `initialize`:
  * malformed registry → fix/replace `sources.yaml` (or point `--config` at a
    valid registry) and re-validate;
  * missing explicit `--config` → supply the intended registry path (or fall
    through to zero-config `.graphtor/*.db` auto-discovery when that is the real
    intent);
  * pre-v4 schema → run `graphtor-docs sync` (the pre-v4→v4 index rebuild — the
    serve/status gates themselves instruct `sync`; `graphtor-docs upgrade` only
    replaces the `.graphtor/bin/` binary and does **not** rebuild the index) and
    verify;
  * duplicate intake → resolve the duplicate source targeting and re-run.
* The remediation is recorded in `056.010-T` and published by `056.012-T`; T0
  only links the selected recipe. If a code change is required to make the cause **actionable** (e.g. a clearer error
  that names the offending registry/schema), it is bounded to the evidenced
  cause and preceded by its own red test — the fail-closed behavior itself is
  never relaxed.
* **One-shot verification:** the single approved state mutation gets a fresh
  approval and backup, followed by exactly one confirming actual-client re-probe
  through the `056.001-T` runner and `056.023-T` observer. `056.010-T` owns
  exactly one H0c repair — it never loops, runs sequential repairs, or
  reactivates a sibling. A second H0c gate, backward-pointing evidence, or an
  unowned cause blocks T4 and becomes a NEW bounded Stage follow-up rather than an
  in-place repeat. No invalid Cargo fixture remains.
* Contingency: when H0c is not evidenced, move the task to `done` with
  `not-needed: H0c not evidenced`. When H0c is selected, complete it after the
  single approved repair plus its one confirming re-probe, or after handing a
  revealed second gate to a new bounded Stage follow-up.

### T3 — (Conditional H1) Typed outcomes, lazy lifecycle, and serve orchestration

H1 is split into three red-first widths because the current eager resolver is
consumed by both `DocServer` and Generation background sync:

1. **Resolver outcomes (`056.014-T`):** add a detailed typed
   `Loaded(model)` / `Disabled(reason)` / `Failed(error)` result. The existing
   resolver collapses load failures into `Ok(None)`, so a compatibility adapter
   preserves unrelated Sync/Prewarm/Query callers while H1 consumes the typed
   API. Exactly three tests cover disabled, loaded, and failed.
2. **Shared lifecycle (`056.005-T`):** one clone-shared supervised owner in
   `src/embed/lifecycle.rs`, with
   `Uninitialized` / `Loading` / `Ready` / `Disabled` / `Failed` owner, not a
   module global or bare OnceCell. Atomically start one blocking load, own and
   monitor its task, convert panic/JoinError to Failed, define drop/cancel
   behavior, and serialize retry transitions. It exposes a stable typed
   lifecycle-state accessor only; the versioned availability projection and
   `src/mcp/server.rs` wiring are `056.015-T`. Exactly three groups cover
   concurrency, terminal-Disabled/drop-cancel behavior, and
   failure/panic/retry/Ready.
3. **Serve/background-sync wiring + availability projection (`056.015-T`):** own
   the versioned MCP availability projection and `src/mcp/server.rs` wiring
   (moved from `056.005-T`) — stable Loading/Failed/Disabled code, retryability,
   remediation, and fallback metadata; neither tool falls back while
   Loading/Failed and terminal Disabled permits `research_topic` keyword fallback
   only with explicit metadata. `cmd_serve` creates one owner
   and passes clones to `DocServer` and `spawn_background_sync`. The first
   model-dependent consumer may be sync or a request and calls the same
   serialized `ensure_loading`. Background sync remains subscribed across
   `Failed → Loading → Ready`, may request the one policy-controlled retry,
   embeds once, and exits only on Ready completion, Disabled, or shutdown.
   No second loader or detached task is permitted. `056.015-T` depends on T2
   before editing `cmd_serve`.

DB open, lock acquisition, pre-v4, and duplicate-intake remain fail-closed
pre-serve gates. If H1 is not evidenced, all three tasks complete `done` with
`not-needed: H1 not evidenced`. `056.012-T` owns the operator-facing tool
retry contract.

#### T-H3-A — Server framing/version compatibility — backlog `056.011-T`

* Select only when the child remains alive but T0's framed transaction never
  negotiates a protocol version. Own **only** transport types and rmcp/stdio
  wiring, landing **after** `056.015-T`. Do NOT require persisted T0 raw frames:
  in ONE same execution, reacquire the exact-client transaction through the
  `056.001-T` runner and the `056.023-T` in-memory observer, add/observe the
  H3-A capture/replay red, then green it with the minimal fix. No raw bytes on
  disk; a generic direct initialize is insufficient.
* Inspect candidate crate metadata before implementation. rmcp 1.8.x uses
  edition 2024 and is excluded by Rust 1.75. Prefer a minimal framing
  adaptation around pinned 1.5.x; use another release only after recording
  edition-2021/MSRV compatibility. If a fork/patch override or wider change is
  required, halt for a separate deliberation. T4 alone owns production-entry
  acceptance. No `get_info` echo change.
* If H3-A is not selected, complete done-plus-not-needed.

#### T-H3-B — Client cwd compatibility — backlog `056.019-T`

* Adjudicate BOTH client mechanisms one-shot on the exact CLI. If T0's Gate 1
  shows the exact CLI **merges** ancestor config, own the ONE bounded attempt to
  find and use a documented isolated-config discovery mechanism; if it exists it
  activates managed-config tasks, else classify B2 and block. Separately, when
  the exact CLI ignores/rejects T0's original `cwd` field, **B1** requires that
  same executable to pass a second foreign-directory contrast using a different
  documented field placement or working-directory mechanism (activates
  managed-config tasks, cannot satisfy T4). Never deliberately read or mutate the
  user root config.
* **B2** means the exact identity supports no safe isolated-config or
  working-directory mechanism: close managed tasks done-plus-not-needed and block
  shipment/T4 as `unsupported-client`. Switching CLI identity is not a fix for
  this acceptance contract.
* Record exact CLI executable path, version/build, invocation, and capability.
  Neither mode may add a server-side external-path fallback. T4 still requires
  three restored production-entry sessions. If H3-B is not selected, complete
  done-plus-not-needed.

### T4 — Runtime verification, rollback, and closure evidence

* Verify with the exact T0 Copilot executable path/version/build across three
  exact-Copilot sessions:
  `/mcp show graphtor-docs` shows a healthy connected server with no OS error
  232, and each session records a completed initialize, a `tools/list`
  containing the expected tools, and one side-effect-free `get_status`
  fingerprint matched to a direct control, with the production config hash
  correlated to the Copilot-spawned server startup event (PID, executable/build,
  canonical cwd, timestamp). If the exact client lacks a deterministic
  tool-invocation surface, route/block through H3-B2 — handshake-only is
  incomplete. Capture `mcp_serve_ready` separately as preflight evidence only.
* On a managed-config branch, first require `056.009-T` evidence that the
  delivered upgrade refreshed the target workspace and record the post-refresh
  production config hash.
* Record branch-aware rollback: revert commits in reverse dependency order,
  restore prior client config for H3-B, re-pin rmcp if bumped, and restore H0c
  workspace state from `056.010-T`'s recorded backups. Observe the next 3 serve starts, with a
  24-hour review checkpoint that records an incomplete/conditional outcome if
  fewer than 3 starts occurred.
* **Branch-sensitive baseline:** the observation "before" state matches the T0
  evidence — for an **H0** cause it is the nonzero child exit + early-exit
  marker (+ client-visible OS error 232); for an **H1** cause it is a
  bounded-`initialize` timeout with the child **still alive** (latency, no
  early exit); for an **H3** cause it is one of two modes — **mode A**
  (framing/version): the child **still alive** with the framed `initialize`
  never negotiating a `protocolVersion` (transport/framing, no early exit);
  **mode B** (client ignores/rejects configured `cwd`): T0's real-client
  diagnostic-entry probe records a foreign actual cwd despite the requested
  project-root cwd. The success signal is identical for all: a completed
  `initialize` handshake with no OS error 232.
* **Correlated actual-client evidence:** each start uses the restored delivered
  user-facing command/args/cwd/env entry. T0's wrapper, temporary entry,
  wrapper PID/log, or any executable substitution is invalid. Capture may use
  inherited stderr, the unique 056.006-T sink, or OS-level tracing that does
  not alter the entry. The sink adds only its declared `GRAPHTOR_*` gate/path;
  at least one start runs gate-off through inherited stderr or OS tracing.
  Record exact CLI identity/invocation, production config
  hash/fields, timestamp, Copilot-spawned server pid, capture path, and
  `/mcp show` result. A separately launched server cannot satisfy evidence.
  `mcp_serve_ready` proves preflight only; the correlated `/mcp show` result
  proves initialize completion.
* Dependency note: T4 depends on `056.003-T` (**non-conditional** cmd_serve
  diagnostics — loud exit-2 errors + `mcp_serve_ready`, always lands), on the
  always-land probe evidence chain `056.020-T → 056.022-T → 056.023-T →
  056.021-T → 056.001-T` (with **explicit direct evidence-chain edges** to
  `056.001-T`, `056.021-T`, `056.022-T`, `056.023-T`, and `056.024-T` for
  robustness), plus the
  **curative** fix tasks, each conditional and moved to **`done` with a
  `not-needed: <rationale>` log comment** when
  its hypothesis is not the evidenced cause: T2d launch-contract (H0a/H3-B1) =
  `056.008-T`, T2e existing-install migration (H0a/H3-B1) = `056.009-T`, T2b
  stale-lock harness/implementation (H0b) = `056.016-T`/`056.007-T`, T2c
  diagnosability = `056.006-T`, T2f H0c remediation = `056.010-T`, H1 resolver/
  lifecycle/orchestration = `056.014-T`/`056.005-T`/`056.015-T`, H3-A transport
  = `056.011-T`, H3-B compatibility = `056.019-T`, typed config/recovery =
  `056.017-T`/`056.018-T`,   core transport = `056.020-T`,
  process spawning/teardown = `056.022-T`, observer seam/evidence = `056.023-T`,
  isolated workspace/fixtures = `056.021-T`, safe no-follow decision =
  `056.024-T`, and documentation-only
  tasks `056.012-T`/`056.013-T`. T0 may activate an **ordered sequence**:
  **H0a → 056.017 + 056.008 + 056.024 + 056.018 +
  056.009**; **H0b → 056.016 + 056.007**; **H0c → 056.010** with 056.006
  independently evidence-gated; **H1 → 056.014 + 056.005 + 056.015**;
  **H3-A → 056.011**; **H3-B1 → 056.019 + managed-config tasks**;
  **H3-B2 → 056.019 and BLOCKED**. A cwd correction that advances to a later
  blocker retains H0a and adds the new cause. The
  non-selected tasks complete with that explicit disposition, which
  **satisfies** T4's dependency on them — T4 does not wait for a conditional
  task that evidence ruled out. **The selected curative branch's task sequence
  must always produce a healthy deterministic/operational branch proof**;
  T4 alone owns production-entry acceptance. H3-B2 is intentionally
  unsatisfiable and blocks rather than manufacturing a different CLI identity.
* Width: runtime verification + closure evidence.

## Issue and Dependency Graph

This section is the authoritative task-ordering and issue graph for the current
artifact. It supersedes every DAG in the historical Plan Review sections below.
The live review decision remains the PR #106 `## Local Review Readiness` block;
this plan claims no current READY.

### Authoritative invariants (single source of truth)

These invariants hold for the whole plan; task sections reference them rather
than restate every clause:

* **Standalone crate, not a trust boundary** — the probe is a standalone,
  non-published crate at `tools/mcp-probe/` (own `Cargo.toml` with `[workspace]`
  + `publish = false`, Rust 2021 / MSRV 1.75); no feature-gated root `[[bin]]`,
  custom `--target-dir`, DACL/ACL, `Assert-*` helper, owner-only artifact,
  user-config backup, or approval receipt exists. Its ordinary Cargo target is a
  build cache; the built binary is ephemeral and invalid as T4 evidence.
* **Width-split probe** — `056.020-T` owns core synchronous transport,
  `056.023-T` owns the copy-only observer seam and in-memory evidence,
  `056.021-T` owns the isolated workspace and config fixtures, `056.022-T` owns
  process spawning/teardown, and `056.001-T` owns the exact-CLI run.
* **Copy-only observer seam** — `056.023-T`'s observer receives copies/immutable
  chunks after the `056.020-T` pump's ordered write/flush and never mutates,
  reorders, drops, or reframes forwarded wire bytes; the pump never awaits the
  observer or takes a cross-direction lock; saturation/failure invalidates
  capture but never changes the wire.
* **Direct-handle teardown, no whole-tree guarantee** — `056.022-T` reaps by
  direct `Child` handle only (runner owns the Copilot child, wrapper owns the
  inner-server child; wrapper PID observed-only). A `sysinfo` adapter observes
  identity for diagnostics/tests but never kills; same-second identity is
  ambiguous and fails closed; residual/unknown descendants fail evidence and
  surface exact identities for operator action.
* **Ancestor config-isolation gate first** — `056.001-T` proves with the exact
  CLI that the nearest child `.mcp.json` shadows and does not merge a sentinel
  ancestor before any causal contrast; if the CLI merges ancestor config, route
  to H3-B via `056.019-T`. The repository-root `.mcp.json` is never assumed
  unread.
* **One-shot classification, forward-only chain** — `056.001-T` runs exactly one
  control/treatment pair and emits an ordered classification; every causal task
  is visited once in the authoritative forward order and never reopens a
  completed/not-needed sibling. Backward-pointing or unowned evidence blocks T4
  and creates a new bounded Stage follow-up.
* **No user-config mutation** — control/treatment and the nested fixture live
  only inside the owned `logs/probe/<nonce>` workspace; the user `.mcp.json` is
  never read, mutated, backed up, restored, or substituted.
* **PR readiness is the sole dynamic authority** — the PR #106
  `## Local Review Readiness` block is authoritative; this plan claims no
  current READY.

### Root cause of the prior P1s

The earlier `056.020-T` coupled the actual-client proxy to a custom
`logs/probe/<nonce>` Cargo `--target-dir` plus PowerShell ACL/reparse hardening
of build artifacts. That machinery was disconnected from proving OS error 232
and was the common cause of the current P1s: undefined verification helpers
(`Assert-ProtectedDir`/`Assert-OwnerOnlyArtifact`), an ancestor-junction TOCTOU,
a wrong Cargo artifact path, and fail-open native gates. Build artifacts are not
the trust boundary — anyone able to modify source or `target/` can already alter
the probe — so hardening them is security theater. The sensitive state is the
runtime config, the evidence, and process ownership.

Correction round 2 further splits the probe into a standalone `tools/mcp-probe/`
crate with width-separated ownership (transport `056.020-T`, observer/evidence
`056.023-T`, workspace/fixtures `056.021-T`, process teardown `056.022-T`,
exact-CLI `056.001-T`) and replaces the earlier re-entrant causal loop with a
forward-only, single-total-order state machine.

### Removed nodes (do not reintroduce)

* The feature-gated `probe-harness` required feature, the root `[[bin]]`
  `graphtor-mcp-probe`, and any probe target inside the production package/
  workspace (the probe is a standalone `tools/mcp-probe/` crate).
* Custom Cargo `--target-dir` and the `logs/probe/<nonce>` build directory.
* Owner-only build artifacts, Windows ACL/DACL creation, and the post-build
  artifact-manifest/ACL verification, plus the undefined `Assert-ProtectedDir`
  and `Assert-OwnerOnlyArtifact` helpers.
* Every `sysinfo`/PID/start-time **kill** fallback and any atomic whole-tree
  ownership claim (teardown is by direct `Child` handle only).
* `056.021-T` owning `evidence.rs` or raw-frame capture (the observer seam and
  evidence move to `056.023-T`); on-disk raw-frame persistence.
* Unbounded/sequential H0c repairs, sibling reactivation, and any in-place loop
  (`056.010-T` owns one repair; a second gate is a new bounded follow-up).
* The unsupported assertion that std already provides handle-level no-follow, and
  any `unsafe` exemption for the recovery primitive.
* User `.mcp.json` mutation, backup, restore, and the config approval receipt.
* The `self-test` probe subcommand and the `--features probe-harness --bin
  graphtor-mcp-probe` verification commands.

### Retained and new nodes

* `056.020-T` (retained, narrowed) — core synchronous transport of the
  standalone `tools/mcp-probe/` crate (`src/main.rs` + `src/transport.rs`): raw
  std-process/std-thread duplex pumps, half-close, bounded stderr drain,
  deadlines, and a post-write bounded non-blocking copy-delivery seam. No
  observer/evidence, workspace/config, teardown, Tokio, or `unsafe`.
* `056.022-T` (retained, corrected) — probe process spawning and teardown by
  direct `Child` handles only; a `sysinfo` adapter observes identity but never
  kills; same-second identity fails closed. Depends on `056.020-T`.
* `056.023-T` (new) — copy-only observer seam and in-memory evidence
  (`evidence.rs`): bounded non-blocking delivery, saturation invalidates
  capture, redacted persistence only. Depends on `056.022-T`.
* `056.021-T` (retained, narrowed) — isolated `logs/probe/<nonce>` workspace and
  the owned nested ancestor/child config fixture (`workspace.rs` only);
  exclusive-create plus `validate_path`/`is_reparse_point`. Owns no
  observer/evidence module. Depends on `056.023-T`.
* `056.001-T` (retained, one-shot) — owns `exact_cli.rs`; runs one control/
  treatment pair inside `056.021-T`'s workspace, proves ancestor
  config-isolation first, and emits the ordered classification. Depends on
  `056.021-T`.
* `056.024-T` (new) — bounded decision/spike selecting an MSRV-1.75 safe
  no-follow config-mutation primitive (or blocking `056.018`/shipment). In the
  chain after `056.008`, before `056.018`.
* Every causal node is re-wired into one authoritative forward chain (see
  ordering below). `056.005-T` narrows to the embedding lifecycle state machine;
  `056.015-T` gains the MCP availability projection and `src/mcp/server.rs`
  wiring; `056.011-T` owns transport types/wiring after `056.015-T`;
  `056.006-T` composes last after `056.011-T`; `056.010-T` owns one H0c repair;
  `056.018-T` implements only the `056.024-T` decision; `056.019-T` adjudicates
  both isolated-config and cwd mechanisms. T4 gains explicit evidence-chain edges
  to `056.001-T`, `056.021-T`, `056.022-T`, `056.023-T`, and `056.024-T`.

### Authoritative task ordering

The single authoritative forward sequence — each task depends only on its
immediate predecessor, and T4 additionally gates on all tasks directly. This
intentionally serializes every shared runtime/docs surface under P-001/P-016 and
prevents reopen/merge races:

`056.020 → 056.022 → 056.023 → 056.021 → 056.001 → 056.002 → 056.003 →
056.019 → 056.017 → 056.008 → 056.024 → 056.018 → 056.009 → 056.016 →
056.007 → 056.010 → 056.014 → 056.005 → 056.015 → 056.011 → 056.006 →
056.012 → 056.013 → 056.004`

* Each conditional task may close as `not-needed` at its turn and pass evidence
  unchanged; a selected task implements exactly its one owned correction and runs
  one exact-client re-probe before passing evidence forward. No task reopens a
  completed/not-needed sibling; backward-pointing or unowned evidence blocks T4
  and creates a new bounded Stage follow-up.
* T4 `056.004` keeps direct dependencies on every task `056.001`, `056.003`,
  `056.005`..`056.024` (the chain is also transitive), with explicit
  evidence-chain edges to `056.001`, `056.021`, `056.022`, `056.023`, and
  `056.024`.

### Explicit residual risks

* The `validate_path`/`is_reparse_point` TOCTOU window is accepted for this
  non-sensitive, same-user diagnostic workspace; it defends against accidental
  escape and pre-existing reparse points, not a malicious same-user source
  modifier. No new production path primitive is introduced by the probe.
* The standalone probe crate's ordinary Cargo target is a build cache, not an
  evidence trust boundary; the probe binary is ephemeral and invalid as T4
  production evidence.
* Teardown is by direct `Child` handle only (`056.022-T`); the `sysinfo` adapter
  observes but never kills, same-second identity fails closed, and
  residual/unknown descendants surface exact identities for operator action.
* The `056.023-T` observer seam is copy-only: it captures copies for evidence
  after the pump's ordered write/flush and never mutates or reframes forwarded
  wire bytes; the pump never awaits it.
* Ancestor config-isolation is proven with the exact CLI before any causal
  contrast (`056.001-T` Gate 1); the repository-root `.mcp.json` is never
  assumed unread, and a merging CLI routes to H3-B via `056.019-T`.
* If `056.024-T` finds no MSRV-safe no-follow primitive, `056.018-T` and the
  shipment stay blocked rather than accepting an unsafe check-then-open recovery.
* T4 remains the sole restored-production actual-client acceptance node.

## Verification Commands

```text
# Fail closed on any error throughout this block:
$ErrorActionPreference = 'Stop'
$env:RUST_LOG = 'debug'

# 1. Resolve and verify the canonical repository root BEFORE any mutation, then
#    anchor the shell to it so every relative path is repo-rooted.
$repoRoot = (& git rev-parse --show-toplevel)
if ($LASTEXITCODE -ne 0) { throw "cannot resolve canonical repository root" }
$repoRoot = [System.IO.Path]::GetFullPath($repoRoot)
Set-Location -LiteralPath $repoRoot

# 2. Verify the STANDALONE, non-published probe crate at tools/mcp-probe/ (its
#    own Cargo.toml with an empty [workspace] table + publish=false stands
#    outside the production workspace). No custom --target-dir or ACL/DACL: the
#    crate's ordinary Cargo target is a build cache, not an evidence trust
#    boundary. Check $LASTEXITCODE immediately after EVERY native command (one
#    command at a time):
cargo +1.75.0 fmt --manifest-path tools/mcp-probe/Cargo.toml --all -- --check
if ($LASTEXITCODE -ne 0) { throw "probe fmt failed (exit $LASTEXITCODE)" }
cargo +1.75.0 check --manifest-path tools/mcp-probe/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "probe check failed (exit $LASTEXITCODE)" }
cargo +1.75.0 clippy --manifest-path tools/mcp-probe/Cargo.toml -- -D warnings -D clippy::pedantic
if ($LASTEXITCODE -ne 0) { throw "probe clippy pedantic failed (exit $LASTEXITCODE)" }
cargo +1.75.0 test --manifest-path tools/mcp-probe/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "probe self-tests failed (exit $LASTEXITCODE)" }
cargo +1.75.0 build --manifest-path tools/mcp-probe/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "probe build failed (exit $LASTEXITCODE)" }
# audit is as-applicable: meaningful only once the probe has a Cargo.lock with
# third-party dependencies (the core transport is std-only).
cargo audit --file tools/mcp-probe/Cargo.lock
if ($LASTEXITCODE -ne 0) { throw "probe audit failed (exit $LASTEXITCODE)" }

# 3. The one-shot actual-client classification (T0 = 056.001-T) is performed BY
#    the probe via its exact-cli subcommand, not by this block. 056.021-T creates
#    a fresh isolated logs/probe/<nonce> workspace (exclusive creation,
#    validate_path/is_reparse_point), writes temporary in-workspace
#    control/treatment .mcp.json plus an owned nested ancestor/child config
#    fixture, and captures redacted evidence through the 056.023-T copy-only
#    observer seam. The user .mcp.json is never read, mutated, backed up, or
#    restored, so no approval receipt is required. The probe first proves ancestor
#    config-isolation with the exact CLI (nearest child config shadows and does
#    not merge the sentinel ancestor); only then does it launch the exact target
#    CLI through the diagnostic wrapper handoff (control without cwd, then
#    treatment with canonical project-root cwd; the wrapper args encode the exact
#    absolute production inner executable plus original args), forward both
#    directions, keep raw frames in memory for same-run replay, enforce a <=30s
#    deadline, and reap by direct Child handle (056.022-T) on every outcome — not
#    a broad process-tree sweep or sysinfo kill. If 056.006-T is selected, set its
#    env gate on the probe-owned CLI process. Provide the exact recorded Copilot
#    path/version/build as $CopilotExe (no substitution) and run the one-shot
#    classifier:
$CopilotExe = $env:GRAPHTOR_EXACT_COPILOT
if (-not (Test-Path -LiteralPath $CopilotExe)) { throw "exact Copilot identity not resolved" }
cargo +1.75.0 run --manifest-path tools/mcp-probe/Cargo.toml -- exact-cli --copilot $CopilotExe
if ($LASTEXITCODE -ne 0) { throw "exact-cli classification failed (exit $LASTEXITCODE)" }

# 4. Root production quality gates — check $LASTEXITCODE immediately after EVERY
#    native command:
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { throw "fmt check failed (exit $LASTEXITCODE)" }
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
if ($LASTEXITCODE -ne 0) { throw "clippy failed (exit $LASTEXITCODE)" }
# `mcp_serve_handshake_test` hosts the reusable open-stdin driver and any
# selected repository-code branch fixture. H0a uses it from 056.008-T's
# generated-entry test; H0b/H3-A use branch fixtures; H1 deterministic behavior
# is proven by 056.014-T/056.005-T/056.015-T. Operational-only H0c/H3-B use
# bounded before/after actual-client transcripts; no failing Cargo test remains.
cargo test --test mcp_serve_handshake_test
if ($LASTEXITCODE -ne 0) { throw "handshake test failed (exit $LASTEXITCODE)" }
cargo test --all-targets
if ($LASTEXITCODE -ne 0) { throw "test suite failed (exit $LASTEXITCODE)" }
cargo audit
if ($LASTEXITCODE -ne 0) { throw "audit failed (exit $LASTEXITCODE)" }
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "release build failed (exit $LASTEXITCODE)" }
# Conditional when rmcp/dependencies change:
cargo +1.75.0 check --all-targets
if ($LASTEXITCODE -ne 0) { throw "MSRV check failed (exit $LASTEXITCODE)" }

# 5. Restored-production acceptance runner (056.001-T owns the exact-cli
#    classifier; 056.004-T/T4 owns restored-production acceptance). It launches
#    the exact Copilot CLI NORMALLY against the restored user-facing entry and
#    MUST NOT wrap/substitute the server or use temp config. Repeat for THREE
#    exact-Copilot sessions; each records completed initialize, tools/list with
#    the expected tools, and one side-effect-free get_status fingerprint matched
#    to a direct control, with the production config hash correlated to the
#    Copilot-spawned server startup event. Use the exact recorded T0 invocation:
$McpShowArgs = $env:GRAPHTOR_MCP_SHOW_INVOCATION -split ' '
for ($i = 1; $i -le 3; $i++) {
  & $CopilotExe @McpShowArgs
  if ($LASTEXITCODE -ne 0) { throw "T4 session $i restored-production /mcp show failed (exit $LASTEXITCODE)" }
}
Get-ChildItem .graphtor -Filter *.lock

# Manual runtime check against the newest Copilot CLI:
#   /mcp show graphtor-docs   (expect: connected, no OS error 232)
```

## Rollback / Compatibility

* The probe evidence chain (`056.020-T` core transport, `056.022-T`
  direct-handle teardown, `056.023-T` observer seam + evidence, `056.021-T`
  isolated workspace/ancestor fixture, `056.001-T` exact-CLI run) is a
  **standalone, non-published** `tools/mcp-probe/` crate that is never installed
  or committed as a binary. Reverting those commits removes the standalone crate
  with no production impact; `056.022-T` direct-handle teardown and `056.021-T`
  owned-workspace cleanup leave no persistent state, and the user `.mcp.json` is
  never mutated, so there is nothing to restore.
* T2 is additive diagnostics (loud discovery errors + `mcp_serve_ready`; no
  containment or discovery-signature change — the conditional T2d
  launch-contract, T2b richer lock metadata, and T2c opt-in log sink are
  separate, evidence-gated tasks) and behavior-preserving for the happy path;
  revert commits in reverse dependency order if needed. The T2d
  managed-`.mcp.json` change is reversible by restoring the timestamped
  byte-for-byte recovery file created before a changing upgrade refresh (or
  regenerating/reverting); its sole launch-identity change pins `cwd` to the
  canonical project root and authorizes no generated target paths.
* **H3 rollback covers both separately owned modes:** for **H3-A**
  (`056.011-T` framing/version), keep an rmcp change isolated so
  it can be pinned back independently, watching for transitive rmcp API changes
  in its own review; for **H3-B** (`056.019-T`, client ignores/rejects `cwd`),
  B1 requires the same exact CLI to prove a different documented
  working-directory mechanism and may include the
  `056.008-T`/`056.009-T` managed-contract commits. B2 means no safe mechanism
  exists and blocks shipment. Rollback reverts any B1 commits and restores the previous
  documented client configuration. No server-side external-path fallback exists.
* The T2c diagnosability sink (`056.006-T`) is off by default; disable its env
  gate to restore inherited stderr and revert its commit to remove the sink.
* If H1 is taken, verify semantic search and Generation background sync share
  one supervised model lifecycle, produce embeddings after Ready, and expose
  retryable Loading/Failed rather than silent degradation.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; `Result` propagation; clippy pedantic
  clean.
* **II Test-First (NON-NEGOTIABLE)** — each production code task is preceded by
  at most three grouped observed-failing scenarios. The T1 driver's sole
  positive signal is a negotiated `initialize`, but T1 itself commits only
  green neutral driver tests. H0a/H0b/H1/H3-A curative tasks each own their
  observed red and return the suite to green before task completion.
  Operational-only H0c and H3-B use bounded actual-client before/after
  evidence, while any bounded code change gets its own red test. H0a/H3-B1
  managed connectivity is greened by the generated-entry integration test →
  T2d/`056.008-T` (delivered by the migration test → T2e/`056.009-T`); the
  non-conditional cmd_serve diagnostics (T2/`056.003-T`) carries its own
  ready-event + loud-error test. The reproduced failure is never the
  expected assertion, and no single test proves an unrelated surface.
* **III/IV Isolation & Containment** — serve stays localhost STDIO. The runtime
  keeps its **existing** containment unchanged: cmd_serve validates an explicit
  `--db-path` / `--config` against the authorized launch cwd through the
  shared `discover_served_databases` / `graphtor_core::path::validate_path` /
  `is_reparse_point` primitives (both operands canonicalized) — **no
  target-derived/split authorized root**, no parent-directory walk, and no
  target self-authorization (F1/F2/F3/N1). A foreign launch cwd is corrected by
  the T2d **cwd pin**, not by re-authorizing a target in the runtime. The
  conditional T2d launch contract pins a `cwd` validated by **equality to the
  canonicalized project root** (NOT constrained inside `.graphtor`) and adds no
  generated target arguments; pinning `cwd` does not relax the runtime boundary
  because the launch cwd becomes the project root. Containment refusal is already
  enforced by the shared primitives and their existing tests in
  `src/workspace/serve_discovery.rs` (absolute-above-boundary, `..`-traversal,
  escaping symlink, junction/reparse-point, Windows short-name/case); T2 adds no
  new containment surface and T2d adds only a canonical-project-root `cwd`, so
  no containment relaxation is introduced.
* **V Observability** — unconditional fatal stderr is preserved and mirrored
  into exhaustive typed preflight events. `mcp_serve_ready` immediately before
  `serve_server` means preflight-complete/about-to-call only. Production
  capture uses inherited stderr, unique T2c sink, or non-substituting OS
  tracing; T0's wrapper is diagnostic-only.
* **VI Single Responsibility** — the core synchronous transport
  (`056.020-T`), process spawning and direct-handle teardown (`056.022-T`), the
  copy-only observer seam and in-memory evidence (`056.023-T`), the isolated
  workspace and config fixtures (`056.021-T`), the exact-CLI run (`056.001-T`),
  diagnostics, typed
  config
  outcomes, generated fields, the safe no-follow primitive decision
  (`056.024-T`) plus the narrow handle-safe recovery primitive,
  existing-install delivery, shared-lock
  characterization/implementation, diagnosability, H0c remediation, H1
  resolver/lifecycle/orchestration, H3-A transport, and H3-B capability are
  separate tasks and split from documentation-only tasks
  `056.012-T`/`056.013-T`; every
  **curative** task is evidence-gated (taken only if its hypothesis is
  evidenced); no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — T00B creates the isolated `logs/probe/<nonce>`
  workspace via exclusive creation validated by the existing
  `validate_path`/`is_reparse_point` helpers, generates the temporary
  control/treatment `.mcp.json` and the nested ancestor/child fixture only inside
  it, and T0 runs entirely inside it. `056.022-T` teardown reaps only via direct `Child` handles (the
  `sysinfo` adapter observes identity but never kills; same-second identity fails
  closed). The user `.mcp.json` is never read, mutated, backed up, or
  restored, so no config approval receipt is required, and isolated config
  creation plus owned-workspace cleanup need no approval; destructive cleanup
  beyond the owned workspace stays approval-gated. T2e consumes 056.018-T's typed, no-follow
  contained recovery
  policy before any changing managed-entry refresh. The
  **conditional H0c operational remediation (T2f/`056.010-T`)** can require a
  **pre-v4→v4 schema rebuild via `graphtor-docs sync`** or a **source-registry
  replacement** — high-risk, potentially data-affecting steps that are
  **approval-gated** (operator approval required, with a backup taken before
  mutating) and **never** a fail-closed-gate weakening. See the Risky actions
  T2f entry (ActionRisk: high, approval_required: yes).
  An evidenced live legacy pid-only lock may be backed up and removed only
  through `056.007-T`'s exact-lock approval-gated recovery; it never terminates
  a process from pid-only evidence.
* **VIII Safety Modes** — investigate-first: T0 orders the causal chain; each
  selected curative task owns its red/green proof before implementation closes.
* **XI Merge-commit history** — Ship enforces merge-commit-only at merge time.

## Plan Hardening Signals

* Public API, schema, or contract change: **present (bounded, conditional on
  H0a/H3-B1)** — the conditional T2d task changes the **managed `.mcp.json` launch
  contract** generated by `src/workspace/mcp_config.rs` (`managed_server_value`
  / `generate_mcp_config`): the managed entry gains a pinned child working
  directory (`cwd`) and (per F7) may emit the CLI-honored `type` field in place
  of `transport`. It does not gain generated target arguments.
  This is an install/config-surface contract, not a library public API, wire, or
  DB-schema change (no `get_info` change; no runtime MCP tool-contract change;
  an rmcp bump remains conditional and separately reviewed). The runtime fix
  (`cmd_serve`, `src/lock.rs`, `src/logging`) stays internal.
* Auth / security / permission / compliance-sensitive behavior: **present** —
  the change touches CLI workspace containment (Principle III/IV): the T2d
  launch contract pins the child `cwd` to the canonical project root and adds
  no generated target arguments, while the runtime keeps its existing
  containment (no target self-authorization, no parent walk).
* Migration / backfill / destructive or irreversible step: **present (bounded,
  conditional on H0a/H3-B1 and H0c)** — the existing-install managed-entry refresh
  (T2e/`056.009-T`) is an idempotent, marker-safe, backup-first config rewrite
  that preserves user-authored entries and is reversible by restoring the
  reported recovery file. However, the **conditional H0c
  operational remediation (T2f/`056.010-T`)** can require a **pre-v4→v4 schema
  rebuild via `graphtor-docs sync`** or a **source-registry replacement** — a
  potentially data-affecting, hard-to-reverse step. It is **approval-gated**
  (operator approval plus a backup of the affected `sources.yaml`/DB before
  mutating) and never weakens a fail-closed gate; rollback restores the
  pre-remediation backup. See the Risky actions T2f entry (ActionRisk: high,
  approval_required: yes).
* External integration / operator checkpoint / partial-rollout: **present
  (bounded)** — behavior depends on how the external Copilot CLI launches the
  child. Documentation/schema only establishes that `cwd` may be expressible;
  T0's actual-client probe must prove whether the target build honors it. T0
  evidence, a post-fix observation window, and the required
  upgrade/reinstall refresh for already-installed workspaces (T2e/`056.009-T`)
  bound it.
* High runtime or rollback risk: **present** — `serve` startup and advisory
  database locking are startup-critical; a wrong resolution or lock change can
  silently break connectivity for every client.

Requires plan hardening: yes

## Plan Hardening

Hardening was required (P-006) because the fix changes startup-critical
`serve` diagnostics and the managed launch-contract (the T2d `cwd` pin), plus
advisory database locking, and because Principle III/IV CLI containment is
directly implicated: a resolution that walked to parent directories would
escape the project-root boundary. The protected invariants are (1) the runtime
keeps its existing containment — cmd_serve validates an explicit
`--db-path`/`--config` against the authorized project-root cwd
(`candidate_root = cwd`) through the shared
`graphtor_core::path::validate_path` / `is_reparse_point` primitives with both
operands canonicalized, with **no target-derived/split authorized root**, no
target self-authorization, and no parent walk (no hand-rolled prefix check that
could fail open on `..`, symlinks/junctions, or Windows short-name/case
variants); (2) the
existing fail-closed gates (malformed registry, missing
explicit `--config`, pre-v4 schema, duplicate-intake preflight) stay pre-serve
gates — the H0c remediation (T2f/`056.010-T`) repairs workspace state rather
than weakening any gate; (3) shared Database/Workspace lock hardening uses
high-resolution process-start identity plus a boot/session discriminator.
A matching strong identity stays live regardless of age; ambiguous, legacy,
or second-resolution identity stays locked. Legacy/unknown fields remain
parse-compatible; (4) typed diagnostics preserve unconditional fatal stderr
and add structured tracing without contaminating stdout; (5) the conditional T2d launch
contract validates the generated `cwd` by **equality to the canonicalized
project root** (NOT constrained inside `.graphtor`), adds no generated target
arguments, and must not relax runtime cwd containment (the launch cwd becomes
the project root); and (6) delivering the refreshed managed entry to
existing installs (T2e/`056.009-T`) must preserve any user-authored
`graphtor-docs` entry byte-for-byte (marker / exact-legacy-shape gating only)
and use `056.018-T`'s component-by-component no-follow/reparse, canonical
containment, exclusive owner-protected recovery primitive before mutation.

Instruction files / learnings consulted: `.github/instructions/constitution.instructions.md`
(III/IV, VIII), `.github/instructions/rust.instructions.md` (no `unwrap`/`expect`
in library code; `Result` propagation),
`docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`
(confirms the `serve_server` wiring is correct, so the failure is startup
early-exit, not malformed construction), and the sibling readonly-serve
hardening / serve auto-discovery decided plans for the cwd-relative discovery
and posture-classification context.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction (non-conditional, T00A/T00C/T00D/T00B/T0 evidence transaction):
  after `056.020-T` proves the core transport, `056.022-T` proves direct-handle
  teardown, and `056.023-T` proves the copy-only observer seam and redaction,
  `056.021-T` creates a fresh isolated `logs/probe/<nonce>` workspace, generates
  temporary in-workspace control/treatment `.mcp.json` plus an owned nested
  ancestor/child fixture, and `056.001-T` proves ancestor config-isolation
  before running the one-shot exact-CLI control/treatment pair inside it through
  the transport.
  * targets: the isolated `logs/probe/<nonce>` workspace and its temporary
    `.mcp.json`, plus the probe-owned Copilot/wrapper/inner-server processes held
    by direct `Child` handles. The user `.mcp.json` is never touched.
  * change_kind: isolated-workspace creation plus temporary in-workspace config
    generation plus external process launch (no user-config mutation).
  * ActionRisk: **moderate** — a crash can leak processes or leave an owned
    workspace behind, so the probe enforces a deadline, reaps by direct `Child`
    handle (`056.022-T`; a `sysinfo` adapter observes but never kills; no
    whole-tree guarantee), redacts sensitive values, and confines every write to
    the owned workspace. The standalone crate's ordinary Cargo target is a build
    cache, not a trust boundary.
  * rollback: on every outcome, reap by the owned direct `Child` handles and
    clean up only the owned `logs/probe/<nonce>` workspace; never delete outside
    it, surface any residual/unknown descendant identity for operator action, and
    keep destructive cleanup beyond the workspace approval-gated.
  * approval_required: no for config, because the real `.mcp.json` is never read
    or written; ActionResult: **planned**.
* ProposedAction (non-conditional, T2 diagnostics): route every typed normal
  preflight exit through one exhaustive seam, preserve loud unconditional
  stderr, add structured events, and emit `mcp_serve_ready`
  immediately before calling `serve_server` as preflight-complete evidence —
  with **no**
  containment or discovery-signature change (cmd_serve keeps validating explicit
  targets against `candidate_root = cwd` via the shared primitives; F1/F2/F3/N1).
  * targets: `src/main.rs::cmd_serve` (~2446-2655); reuse of
    `graphtor_core::path` / `src/workspace/serve_discovery.rs` primitives (no new
    containment surface).
  * change_kind: local edit to startup diagnostics / control flow.
  * ActionRisk: **low-moderate** — startup-critical but non-destructive,
    parity-safe, and behavior-preserving on the happy path; guarded by its own
    ready-event + loud-error test and the fail-closed-gate regression
    assertions.
  * rollback: `git revert` the T2 commit(s) in reverse dependency order.
  * approval_required: no (non-destructive); ActionResult: **planned** (always
    lands; not branch-conditional).
* ProposedAction (conditional, T2d managed-cwd launch-contract): when T0
  confirms H0a or H3-B1 selects a supported CLI that honors managed `cwd`,
  emit a trusted, containment-validated launch identity in the generated managed
  `.mcp.json` entry: pin the child working directory (`cwd`) to the project root.
  This restores cwd-anchored registry discovery, DB auto-discovery,
  posture/Generation validation, and background sync together without relaxing
  the runtime cwd boundary. Add no generated target arguments.
  * targets: `src/workspace/mcp_config.rs` (`managed_server_value` ~528-540,
    `generate_mcp_config`).
  * change_kind: install-time managed-`.mcp.json` launch-contract generation.
  * ActionRisk: **moderate** — changes the launch contract the CLI consumes;
    the pinned `cwd` equals the canonical project root, no target-derived
    authorization or parent traversal is added, and a generated-contract test
    guards the value. Taken only if T0's real-CLI probe proves the selected
    target build honors the field. rollback: revert the T2d commit /
    regenerate the entry.
  * approval_required: no (non-destructive); ActionResult: **planned** (or
    **abandoned** if T0 shows the CLI already supplies a usable target/cwd, or
    H0a is not evidenced).
* ProposedAction (conditional, T2b): record process start-time and boot/session
  discriminator alongside pid in advisory lock metadata and apply one
  conservative policy to Database and Workspace locks.
  * targets: `src/lock.rs` (`DatabaseLock::acquire` / `AdvisoryLock::acquire` /
    `handle_existing_lock`, ~120-200).
  * change_kind: lock-file metadata + liveness check.
  * ActionRisk: **moderate** — must not misclassify a live holder as stale;
    taken only if H0b is evidenced. rollback: revert the T2b commit.
  * approval_required: no; ActionResult: **planned** (or **abandoned** with a
    `not-needed` rationale if the task completes `done` without activation).
* ProposedAction (conditional, T2b legacy recovery): after H0b evidence
  identifies one live legacy pid-only reused-pid lock, back up and remove only
  that exact lock without terminating any process.
  * targets: the evidenced lock path and an owner-protected recovery artifact.
  * change_kind: destructive targeted lock-file recovery.
  * ActionRisk: **destructive** — legacy pid-only metadata cannot distinguish
    the original holder from pid reuse, so automatic eviction is forbidden.
  * rollback: restore the exact lock bytes from the contained backup if
    recovery does not produce a healthy follow-up acquisition.
  * approval_required: **yes**; record the approval receipt and exact lock
    identity. ActionResult: **planned** (or **abandoned** when no such lock is
    evidenced).
* ProposedAction (conditional, T2c): env-gated opt-in diagnostic file-log sink.
  * targets: `src/logging/init.rs`, serve path in `src/main.rs`.
  * change_kind: additive, off-by-default logging sink for structured events.
  * ActionRisk: **low** — off by default; taken only if actual-client capture
    recipe is insufficient. rollback: revert the T2c commit. ActionResult:
    **planned** (or **abandoned** with a logged not-needed disposition).
* ProposedAction (conditional, T2e H0a/H3-B1 existing-install delivery): wire the
  idempotent, marker-safe `generate_mcp_config` refresh into `cmd_upgrade` so an
  already-installed managed entry is refreshed to the new launch contract,
  passing the **canonical project root** (`find_workspace_dir(cwd).parent()` /
  `workspace::paths::project_root`) — never the nested invocation `cwd` nor the
  located `.graphtor` dir itself (P1-3).
  * targets: `src/main.rs::cmd_upgrade` (~3480-3538);
    `src/workspace/mcp_config.rs::generate_mcp_config`;
    `src/workspace/paths.rs::find_workspace_dir` / `project_root`.
  * change_kind: install/upgrade-time rewrite of the managed `.mcp.json` entry.
  * ActionRisk: **moderate** — rewrites an existing config file, but only after
    a contained timestamped byte-for-byte backup succeeds; marker /
    exact-legacy gating preserves user-authored entries. Proven by an
    observed-red migration test. rollback: restore the reported backup or
    revert/regenerate; reinstall is the manual fallback.
  * approval_required: no (non-destructive, idempotent); ActionResult:
    **planned** (or **abandoned** if neither H0a nor H3-B1 is evidenced).
* ProposedAction (conditional, T2f H0c operational remediation): repair the
  evidenced fail-closed workspace **state** (fix/replace `sources.yaml`, supply
  the intended `--config`, run the pre-v4→v4 index rebuild via
  `graphtor-docs sync` (not `upgrade`, which only replaces the binary), or
  resolve the duplicate intake) so `serve` legitimately passes the gate —
  **never** weakening any
  fail-closed gate; plus, only if needed, a bounded actionability code change
  preceded by its own red test.
  * targets: the evidenced fail-closed surface in `cmd_serve`'s pre-serve path +
    operational workspace state (`sources.yaml` / DB / registry).
  * change_kind: operational workspace-state repair (+ optional bounded code
    edit).
  * ActionRisk: **high** — touches workspace state or a pre-v4→v4 schema rebuild
    (`graphtor-docs sync`) that
    could damage data if mishandled; take a **backup** of the affected
    `sources.yaml` / DB before mutating and require **operator approval** for a
    schema rebuild or registry replacement.
  * rollback: restore the pre-remediation backup; revert any actionability
    commit. approval_required: **yes** for the schema rebuild / registry
    replacement; ActionResult: **planned** (or **abandoned** if H0c is not
    evidenced).
* ProposedAction (conditional, T2g safe no-follow primitive decision): decide an
  MSRV-1.75, safe-call-site, no-follow/capability-based file-mutation primitive
  for the sensitive recovery path without relaxing `#![forbid(unsafe_code)]`;
  compare a narrowly justified safe dependency against a std-only contract and
  prove the choice with a minimal PoC.
  * targets: `docs/decisions/` (the linked deliberation) and a bounded PoC; no
    production `graphtor_core` edit (that is `056.018-T`).
  * change_kind: bounded technical decision/spike (plus an optional throwaway PoC).
  * ActionRisk: **low** — investigation and a minimal PoC; no production mutation.
    If no compliant MSRV-safe primitive exists, it blocks `056.018-T` and the
    shipment rather than accepting an unsafe check-then-open.
  * rollback: none required (decision/PoC only). approval_required: no.
    ActionResult: **planned** (or **abandoned** if managed-config recovery is not
    selected).
* ProposedAction (conditional, H3-A/H3-B compatibility): obtain task-local
  branch proof for the evidenced mode; T4 alone accepts production.
  **H3-A** (child alive,
  framing/version) uses the minimal edition-2021/Rust-1.75-compatible framing
  fix in `056.011-T`; rmcp 1.8.x is excluded. **H3-B** (client ignores/rejects
  pinned `cwd`) uses `056.019-T` to choose B1 (the same exact CLI passes a
  second contrast through a different documented working-directory
  mechanism, activating managed-config work) or B2 (that exact CLI supports no
  safe mechanism, blocking shipment) — **no** server-side external-path fallback.
  * targets: **mode A** — `Cargo.toml` `[dependencies]` `rmcp` pin + rmcp
    `serve_server` / transport wiring in `src/main.rs` / `src/mcp/server.rs`;
    **mode B** — the documented client-launch capability in `056.019-T`, plus
    managed-config changes only for B1.
  * change_kind: **mode A** compatible dependency/transport edit; **mode B**
    operator/client configuration selection (documentation only).
  * ActionRisk: **moderate** — a mode-A rmcp bump can pull transitive API
    changes (`serve_server` signature, `schemars` re-export), re-verified in its
    own review with deterministic captured-transaction replay and
    `cargo +1.75.0 check --all-targets`; no `get_info` change. Mode B changes no repo code and adds no
    containment surface. rollback: **mode A** revert the bump and re-pin rmcp
    1.5; **mode B** revert to the previously documented client configuration.
  * approval_required: no (non-destructive); ActionResult: **planned** (or
    **abandoned** if H3 is not evidenced).

### Added verification / rollback / observation detail

* Verification: the green T1 driver supplies open stdin + real `initialize`
  write + bounded response validation + concurrent stderr capture. The
  selected curative task owns the red-before/green-after lifecycle. Its primary
  red assertion is
  **branch-sensitive**: for **H0**, "no `initialize` response before the bounded
  deadline + captured nonzero child exit matching the T0 marker" (a write-side
  broken-pipe error is an opportunistic secondary signal — a ~500-byte write can
  buffer into a pipe whose reader already exited, so a write-only assertion
  could flake green); for **H1**, "no `initialize` response before the bounded
  deadline while the child is **still alive** (no exit code)", isolating latency
  from an early-exit crash. Green (both) = a successful `initialize` response.
  When H0a is the cause, an **unrelated-cwd launch regression test** (T2d) also
  asserts a managed entry generated for project `P` serves `P`'s databases from
  an unrelated cwd. The shared-primitive refusal coverage in
  `src/workspace/serve_discovery.rs` already enumerates the escape vectors
  (absolute-above, `..`-traversal, escaping symlink, junction/reparse-point,
  Windows short-name/case variant), each refused, and T2d reuses those
  primitives for its pinned `cwd`/targets; a regression assertion confirms each
  fail-closed gate (malformed registry, missing explicit `--config`, pre-v4,
  duplicate-intake) still exits pre-serve after the T2 diagnostics change. All
  four quality gates plus `cargo build --release`.
* Rollback: revert shipment commits in reverse dependency order; re-pin prior
  rmcp if bumped (T4).
* Post-deploy observation window (manual — no hosted observability is
  available; STDIO serve is a local child process):
  * **owner:** the merging developer (Ship / operator); no on-call rotation.
  * **pre-fix baseline (branch-sensitive):** the T0 capture, recorded in
    `056.001-T` and referenced here as the "before" state — for an **H0** cause
    a concrete nonzero child exit code + early-exit stderr marker + the
    client-visible OS error 232; for an **H1** cause a bounded-`initialize`
    timeout with the child **still alive** (latency, no early exit); for an
    **H3** cause one of two modes — **mode A** (framing/version): the child
    **still alive** with the framed `initialize` never negotiating a
    `protocolVersion` (no early exit); **mode B** (client ignores/rejects
    configured `cwd`): T0's real-CLI probe records the foreign actual cwd — on
    the newest Copilot CLI.
  * **exact method / invocation:** observe the next 3 serve starts, with a
    24-hour review checkpoint. For each start run `/mcp show graphtor-docs`
    through the exact T0 CLI executable/version/build and restored production
    entry. Capture only through inherited stderr, a unique 056.006-T sink, or
    non-substituting OS tracing. Record CLI identity, production config hash,
    timestamp, server pid, capture path, and result. T0's wrapper/temporary
    entry and separately launched logs are invalid evidence.
  * **files / log signals:** the recorded per-attempt capture path shows
    `mcp_serve_ready` (preflight complete; about to call `serve_server`) and no
    OS error 232;
    `/mcp show graphtor-docs` reports
    connected with a completed `initialize` handshake.
  * **success trigger:** all 3 observed starts complete the handshake with no
    OS error 232 → outcome `healthy`. If 24 hours elapse before 3 starts, record
    `incomplete` / `READY_WITH_CONDITIONS` with the observed count and a
    follow-up; never infer healthy from fewer than 3 starts.
  * **rollback trigger:** any OS error 232 recurrence, an exit-before-initialize
    (H0 or H3 mode B), or a failed/timed-out / never-negotiated `initialize`
    handshake (H1 or H3 mode A) in the window → revert the shipment commits in
    reverse dependency order (T4; for H3 mode B, revert to the previously
    documented client configuration) → outcome `rolled-back`.
  * outcome (healthy / degraded / incomplete / rolled-back) is recorded in the shipment
    closure artifact (T4).

## Test-First Harness Expectations

* T1 supplies a reusable real-process transport driver. Its only success signal
  is a negotiated `initialize` response; broken pipe, exit, stderr, or timeout
  remain diagnostic evidence.
* Repository-code branches commit a branch-specific observed-red test that the
  selected curative task greens: H0a uses the driver in `056.008-T`'s
  generated-entry test; H0b uses a reachable Generation-lock fixture; H3 mode
  A replays T0's unmodified bidirectional target-client transaction but closes
  only after actual-client production-entry acceptance; H1 uses the split
  `056.014-T`/`056.005-T`/`056.015-T` deterministic seams.
* Operational-only H0c and H3-B preserve a bounded actual-client red
  transcript and rerun the exact `/mcp show graphtor-docs` production-entry
  probe after the approved state/client repair.
  They do not leave a fixture permanently invalid in `cargo test`. Any bounded
  H0c actionability code change gets its own red test.
* Non-H1 process fixtures prewarm/pin model state away from the path. H1 tests
  do not depend on cold-cache timing or network access.
* The harness holds a named `ChildStdin` handle **open** and writes a protocol-valid
  newline-delimited `initialize` JSON-RPC request. When the pass assertion fails
  red, the harness captures the child exit code + stderr **purely as diagnostic
  evidence**: for **H0** the response never arrives because the child exited
  first (captured exit code + stderr must match the T0 marker); for **H1** the
  response misses the bounded deadline while the child is **still alive**
  (latency, no exit code); for **H3 mode A** the child is **still alive** but the
  framed `initialize` never negotiates a `protocolVersion` (transport/framing),
  while **H3 mode B** (client ignores configured `cwd`) is evidenced by T0's
  temporary real-CLI diagnostic-entry probe, not by this harness or a direct
  child spawn. These
  diagnostics explain the red — they are never
  accepted as the passing result. An empty/closed stdin is explicitly
  disallowed — it would only exercise a benign EOF-driven shutdown and could not
  distinguish the regression.
* Per-width proofs remain separate: `056.020-T` owns the core synchronous
  transport; `056.023-T` owns the copy-only observer seam and in-memory evidence;
  `056.022-T` owns process spawning and direct-handle teardown; `056.021-T` owns
  the isolated workspace and config fixtures;
  `056.003-T` owns the exhaustive typed preflight seam and `mcp_serve_ready`;
  `056.017-T` owns config outcomes; `056.008-T` owns generated-entry execution;
  `056.018-T` owns recovery containment; `056.009-T` owns upgrade integration;
  `056.016-T`/`056.007-T` own shared-lock characterization/implementation;
  and `056.014-T`/`056.005-T`/`056.015-T` own H1 outcomes/lifecycle/wiring.
  No test proves an unrelated surface and no intentionally failing test remains
  after the selected fix.
* Existing MCP tests (`tests/mcp_manifest_test.rs`) must continue to pass.
  If H1 changes handler signatures, update server unit tests to equivalent
  async tests or preserve a typed sync-compatible adapter.

## Plan Review

**Current status: the authoritative review state is tracked solely by the PR
#106 `## Local Review Readiness` block** — reviewed HEAD, outcome, and P0/P1
counts live there. This plan document does not assert its own current review
outcome and does not independently authorize Ship or merge; consult the PR
block for the live decision. The current artifact additionally reflects the
**correction round 2** holistic graph rewrite (2026-08-23): the probe becomes a
**standalone, non-published** `tools/mcp-probe/` crate split by width —
`056.020-T` core synchronous transport, the new `056.023-T` copy-only observer
seam plus in-memory evidence, `056.021-T` isolated workspace and config fixtures
(evidence.rs ownership moved out), `056.022-T` process spawning and
**direct-`Child`-handle** teardown (all `sysinfo`/PID kill fallbacks removed; the
adapter observes but never kills), and `056.001-T` a **one-shot** exact-CLI
classifier owning `exact_cli.rs`. The re-entrant causal loop is replaced by one
authoritative forward-only chain
`056.020 → 056.022 → 056.023 → 056.021 → 056.001 → 056.002 → 056.003 → 056.019 →
056.017 → 056.008 → 056.024 → 056.018 → 056.009 → 056.016 → 056.007 → 056.010 →
056.014 → 056.005 → 056.015 → 056.011 → 056.006 → 056.012 → 056.013 → 056.004`;
the new `056.024-T` decides a safe MSRV-1.75 no-follow config-mutation primitive
(or blocks `056.018`/shipment); `056.010-T` owns one H0c repair; `056.005-T`
narrows to the embedding lifecycle while `056.015-T` gains the MCP availability
projection; `056.019-T` adjudicates both isolated-config and cwd mechanisms; T4
correlates the production config hash with the server startup event across three
`tools/list`+`get_status` sessions and gains direct evidence-chain edges to
`056.001-T`/`056.021-T`/`056.022-T`/`056.023-T`/`056.024-T`; and all
build-artifact ACL/target-dir/user-config-substitution machinery remains removed.
The authoritative task ordering now lives in the `## Issue and Dependency Graph`
section above; the DAGs in the historical sections below are superseded. The
round-3 correction below — applied against the
earlier exact HEAD **`41adf77f1767aaec1b7b588b03fb6ea41d2a67fc`**, which had
returned `BLOCKED` after deduplication (`P0=1, P1=5, P2=15, P3=7`) — corrected
the convergent failing-suite handoff, layered-cause selection, H1 retry,
recovery-width/ownership, overlapping `cmd_serve`, and legacy-lock blockers,
plus tightly coupled P2 safety/actionability gaps. Findings based only on
excluded old memory/archive state remain discarded. Earlier
review/remediation sections below are historical records of prior reviewed
HEADs and do not describe the current artifact state.

> [!IMPORTANT]
> Historical review sections below use the shorthand "close as not-needed."
> That wording is superseded. The executable contract is always: move the task
> to the valid `done` status and append a `not-needed: <rationale>` backlog
> comment.

**Gate decision (Cycle 1, historical — superseded): PASS** (after one in-pass
remediation cycle). At the Cycle-1 HEAD there were no unresolved P0/P1 findings;
the consensus P2 trust-boundary and correctness findings were remediated
directly in this plan and its backlog tasks; residual items are recorded as
Ship-phase P2/P3 advisories below. This decision does **not** describe the
current artifact state — see the current-status note above.

### Reviewed artifact identity

* Plan: `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (this file), on branch `chore/stage-049-S`. **Latest reviewed input:**
  committed HEAD **`41adf77f1767aaec1b7b588b03fb6ea41d2a67fc`**, outcome
  **BLOCKED**. **Review status of the correction-round-2 corrected artifacts:**
  report-only gate **PENDING** against the next committed HEAD — explicitly
  **not** a PASS.
* Linked deliberation: `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`.
* Backlog scope: shipment `049-S` / feature `056-F`, tasks
  `056.001-T`..`056.024-T`: T0/T1 `056.001-T`/`056.002-T`; T2 diagnostics
  `056.003-T`; T4 `056.004-T`; H1 lifecycle/resolver/wiring
  `056.005-T`/`056.014-T`/`056.015-T`; diagnostic sink `056.006-T`; H0b
  characterization/implementation `056.016-T`/`056.007-T`; managed
  generation/delivery `056.008-T`/`056.009-T`; H0c `056.010-T`; H3-A
  `056.011-T`; docs `056.012-T`/`056.013-T`; typed config + recovery
  `056.017-T`/`056.018-T`; H3-B `056.019-T`; standalone probe crate
  transport/teardown/observer-evidence/workspace/exact-CLI
  `056.020-T`/`056.022-T`/`056.023-T`/`056.021-T`/`056.001-T`; and safe
  no-follow primitive decision `056.024-T`.

### Personas and criteria

Seven reviewer personas ran against the corrected plan (multi-model where
available): Constitution Reviewer (principles I/II/III/IV/V/VI/VIII, P-006),
Rust Reviewer (code-grounded technical soundness against `cmd_serve`,
`src/lock.rs`, `src/logging/init.rs`, `src/mcp/server.rs`, `src/embed/resolver.rs`),
Scope Boundary Auditor (single-cause isolation, YAGNI, 2h granularity),
Learnings Researcher (`docs/compound/` prior-solution consistency), Architecture
Strategist (cohesion/coupling/boundaries), Agent-Native Parity Reviewer (MCP
handshake + tool-contract fidelity), and Security Lens Reviewer (Principle
III/IV containment trust boundary).

### Finding counts

* **Cycle-1 snapshot (historical): P0: 0, P1: 0.** Every persona reported
  nothing blocking **at the Cycle-1 HEAD**.
* **Current status:** the standard report-only review of exact HEAD
  `41adf77f1767aaec1b7b588b03fb6ea41d2a67fc` was `BLOCKED` with the
  deduplicated counts above. Correction round 2 (the current holistic rewrite)
  supersedes those blockers with the standalone probe crate and forward-only
  chain named in the current-status paragraph.
  A fresh current-HEAD review must establish `P0=0, P1=0`, followed by the
  mandatory adversarial re-review.
* **Consensus review (2026-08-21, HEAD `22d18f1`):** a 3-model adversarial
  consensus review produced a deduplicated remediation queue (F1/F2/F3/N1
  containment reversal; F4 status parity; F5 stale wording; F6 H3 owner; F7
  config schema; S1 migration primacy; S7 hardening; per-surface test-first),
  applied in prior remediation cycles (see the historical audit trail below). A
  fresh current-HEAD report-only review is required to re-establish `P0=0, P1=0`.
* **Fresh-cycle P2 status:** the six consensus P2s were corrected or explicitly
  adjudicated in prior correction cycles; validation is pending.
* **P3 / carried advisories: several**, recorded for Ship execution.

### Historical review audit trail (superseded)

All prior Plan Review DAGs, dependency orderings, and per-cycle remediation
bodies are **superseded** by the current `## Issue and Dependency Graph` section
and its authoritative forward chain. They are retained only as audit metadata;
none of their task-ordering or dependency prose is normative. The live decision
is the PR #106 `## Local Review Readiness` block.

| Reviewed HEAD | Date | Outcome | Notes |
|---|---|---|---|
| `22d18f1` | 2026-08-21 | remediation queue | 3-model adversarial consensus (F1/F2/F3/N1 containment, F4 parity, F5 wording, F6 H3 owner, F7 config schema, S1 migration, S7 hardening) |
| consensus cycles 1-3 | 2026-08-21 | report-only PENDING | containment reversal, status parity, H3 owner split, config schema, migration primacy, per-surface test-first |
| `1bcadaa` | 2026-08-22 | BLOCKED (2x P1) | fresh three-round budget, correction round 1 |
| `dddcac3` | 2026-08-22 | BLOCKED | review-fix round 2 |
| `41adf77` | 2026-08-22 | BLOCKED (P0=1, P1=5, P2=15, P3=7) | review-fix round 3 (final round of the prior budget) |
| current HEAD | 2026-08-23 | report-only PENDING | correction round 2 holistic rewrite: standalone probe crate, forward-only chain, `056.023-T`/`056.024-T` added. A fresh current-HEAD review is required; no PASS is claimed |

Carried P3 advisories from prior cycles remain non-blocking Ship-phase notes:
prefer targeted diagnostics over broad logging, keep the standalone probe crate
out of the production dependency graph, and preserve MSRV (`cargo +1.75.0`) on
any rmcp or dependency change. Any retained "close as not-needed" shorthand
means: move the task to `done` and append a `not-needed: <rationale>` comment.
