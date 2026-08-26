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
  `publish = false`, Rust 2021 / MSRV 1.75, crate-root `#![forbid(unsafe_code)]`),
  split by width: `056.020-T` self-tests the std-only **core synchronous
  transport** (raw `std::process`/`std::thread` duplex pumps, half-close, bounded
  stderr drain, deadlines); `056.023-T` owns the copy-only observer seam,
  in-wrapper JSON-RPC correlation/redaction, and redacted evidence (standalone
  `serde_json`); `056.021-T` owns the isolated `logs/probe/<nonce>` workspace plus
  control/treatment and nested ancestor/child config fixtures (probe-local
  std-only containment, no `graphtor_core`); `056.022-T` owns process spawning,
  teardown by **direct `Child` handles only**, and the versioned `wrapper`
  subcommand (a required injectable observation trait with a standalone `sysinfo`
  impl that observes identity but never kills, plus a standalone `cargo audit`
  gate). The probe sub-chain is
  `056.020 -> 056.022 -> 056.023 -> 056.021 -> 056.001`. T0 (`056.001-T`) owns
  `exact_cli.rs` and the exact-cli subcommand, records the exact newest failing
  CLI executable/version/build and `/mcp show graphtor-docs` invocation, **first
  proves ancestor config-isolation** against the nested fixture, then runs ONE
  control/treatment pair against the affected build (plus one bounded
  additional pair against a last-known-stable build when available) through
  the `056.022-T` wrapper handoff (child
  `.mcp.json` uses the wrapper as `command`; byte-identical wrapper args encode
  the exact absolute production inner executable plus original args;
  control/treatment differ only by treatment `cwd`), emits the ordered cause
  classification, and ends `done` with no implementation loop. It NEVER declares
  H3-B2; a Gate-1 merge or both-legs-foreign result emits typed `H3-B-candidate`
  and continues forward to `056.019-T`. The user `.mcp.json` is never read,
  mutated, or restored by the probe/T0, so no config approval receipt is required.
  Raw frames stay in wrapper memory; the runner reads only the redacted summary
  and digests from the wrapper-owned `--evidence-output`. The ordinary Cargo
  target is a build cache, not a trust boundary.
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
* Rollback and three exact-Copilot `/mcp show graphtor-docs` sessions on the
  exact T0 CLI identity and restored post-fix user-facing entry are documented;
  each session proves the server connected and initialized with no OS error 232
  using ONLY the fields `/mcp show` surfaces (connection/initialization status and
  advertised tool list/count when shown), and correlates the production config
  hash/file identity with the Copilot-spawned server startup event. The correlated
  JSON-RPC wire fields (`jsonrpc` 2.0, correlated id, no error,
  `result.protocolVersion`) are proven SEPARATELY by the direct T1 (`056.002-T`)
  production driver against the same production binary/workspace — not by
  `/mcp show` — which also confirms the expected tools and one side-effect-free
  `get_status` as a server control (not proof of Copilot UI invocation). T0's
  wrapper or any executable substitution is invalid production evidence.

## Likely Surfaces (exact)

| Surface | Location | Change |
|---|---|---|
| Core synchronous transport (T00A, standalone crate) | standalone `tools/mcp-probe/` crate (own `Cargo.toml`, `[workspace]` + `publish=false`, Rust 2021/MSRV 1.75): `src/main.rs` + `src/transport.rs` | Self-test raw `std::process`/`std::thread` full-duplex pumps, half-close, bounded stderr drain and buffers, deadline signaling; expose a post-write bounded non-blocking copy-delivery seam. No observer/evidence, workspace/config, process teardown, or Tokio; crate-root `#![forbid(unsafe_code)]`; the std-only constraint scopes the core transport (later tasks may add narrowly justified non-Tokio deps). Ordinary Cargo target is a build cache, not a trust boundary → `056.020-T` |
| Probe process spawning + teardown + wrapper (T00C, standalone crate) | `tools/mcp-probe/src/process.rs` + versioned `wrapper` subcommand + sequential `main.rs` wiring; required `sysinfo` observation trait | Own the versioned `wrapper` subcommand (argv `--inner-exe`/`--inner-arg`/`--evidence-output`/`--run-nonce`, byte-identical across legs; wires the inner `Child` to the 056.020-T pumps and preserves exit/stderr/half-close/deadline). Direct `Child` handles are the sole kill/wait authority (runner owns the Copilot `Child`, wrapper owns the inner-server `Child`; wrapper PID observed-only). A REQUIRED injectable observation trait with a standalone `sysinfo` impl observes PID/start-time/executable/parent/nonce but never kills; same-second identity is ambiguous and fails closed; residual/unknown descendants surface exact identities for operator action; a standalone `cargo audit` gate applies → `056.022-T` |
| Probe observer seam + evidence (T00D, standalone crate) | `tools/mcp-probe/src/evidence.rs` (+ `observer.rs` only if needed) | Copy-only read-only observer seam and transcript correlator running INSIDE the wrapper (standalone `serde_json`); consumed after each direction's ordered write/flush; bounded non-blocking delivery; saturation/failure atomically marks the summary invalid while forwarding is unchanged; raw frames stay in wrapper memory; only redacted summaries/digests are written atomically to the wrapper-owned `--evidence-output`; incremental JSON-RPC reassembly correlates the exact initialize id to a non-error `result.protocolVersion` → `056.023-T` |
| Isolated probe workspace + config fixtures (T00B, standalone crate) | `tools/mcp-probe/src/workspace.rs` creating `logs/probe/<nonce>` + temporary in-workspace `.mcp.json` | Exclusive-create isolated workspace validated by a probe-local std-only `canonicalize`/`symlink_metadata` check (no `graphtor_core` import); generate in-workspace control/treatment entries with identical wrapper args on both legs (treatment alone adds `cwd`) and an owned nested ancestor/child config fixture; owned-workspace-only cleanup. Owns no observer/evidence module; never touches the user `.mcp.json` → `056.021-T` |
| Serve startup diagnostics (T2, non-conditional, parity-safe) | `src/main.rs::cmd_serve`, duplicate-intake and database-open preflight | Route every pre-transport normal exit through an exhaustive typed seam, including pre-v4 and duplicate-intake exits. Preserve unconditional stderr and add structured events; emit `mcp_serve_ready` immediately before `serve_server` → `056.003-T` |
| Managed config outcome contract (conditional H0a/H3-B1) | `src/workspace/mcp_config.rs` | Distinguish typed create/update/no-change/collision outcomes from fail-closed `PathViolation`; forbid message sniffing → `056.017-T` |
| Managed MCP launch cwd field (T2d, conditional H0a/H3-B1) | `src/workspace/mcp_config.rs::managed_server_value` | Generate ONLY the evidence-selected canonical project-root `cwd` after T0/H3-B capability proof and align the cwd legacy-shape recognition/tests. The generator `type`/`transport` discriminator reconciliation is split out to `056.026-T` (generation)/`056.027-T` (delivery), not this task → `056.008-T` |
| Managed MCP config discriminator (T2g, conditional on discriminator mismatch) | `src/workspace/mcp_config.rs::managed_server_value` | Reconcile the generator `type`/`transport` stdio discriminator to the exact CLI-honored field UNCONDITIONALLY when exact-CLI evidence shows a mismatch (else `not-needed`); own ONLY the discriminator field + legacy-shape recognition (`is_exact_legacy_shape`); depend only on `056.019-T` → `056.026-T` |
| Existing-install discriminator delivery (T2h, conditional on discriminator mismatch) | `src/main.rs::cmd_upgrade`, managed-config typed APIs | Compose `056.017-T` typed actions + `056.018-T` recovery handles to refresh ONLY the marked entry's stdio discriminator field; depend on `056.026-T`/`056.018-T`, not on `056.008-T`/`056.009-T` → `056.027-T` |
| Safe no-follow primitive decision (conditional H0a/H3-B1) | NEW `docs/decisions/YYYY-MM-DD-safe-no-follow-config-mutation-primitive.md` (parallel decision `056.017 → 056.024 → 056.018`, parallel to `056.017 → 056.008`) | Close `not-needed` when no managed-config mutation/recovery is selected; otherwise decide an MSRV-1.75, safe-call-site, no-follow/capability-based mutation primitive without relaxing `#![forbid(unsafe_code)]`; evaluate a narrowly justified safe dependency vs a std-only contract, prove with a minimal PoC, and record the decision in the NEW artifact (do not rewrite the OS-232 deliberation) or block 056.018/shipment → `056.024-T` |
| Contained recovery primitive (conditional H0a/H3-B1) | selected primitive in `graphtor_core::path` (e.g. `src/path/handle.rs`) + lazy accessor in `src/workspace/paths.rs` | Implement ONLY the `056.024-T`-selected no-follow/reparse-safe primitive for I/O, exclusive owner-protected artifacts, and exact restore; assert no std handle-level no-follow, take no `unsafe` exemption, make no managed-config/install/uninstall/doctor edits → `056.018-T` |
| Existing-install cwd/recovery delivery (T2e, conditional H0a/H3-B1) | `src/main.rs::cmd_upgrade`, managed-config typed APIs | Deliver the cwd/recovery refresh to existing installs: refresh marked/exact-legacy entries and expose typed text/JSON action + recovery metadata; preserve collision/non-JSON bytes and Minimal footprint. Discriminator existing-install delivery is `056.027-T`, not this task → `056.009-T` |
| Advisory lock characterization + implementation (conditional H0b) | `src/lock.rs` shared `AdvisoryLock` used by Database and Workspace locks | Passing characterization → `056.016-T`; one conservative high-resolution/boot-aware policy plus task-local red/green and legacy recovery → `056.007-T` |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if stderr is unavailable and CLI env inheritance works: unique exclusive absolute per-attempt sink consuming typed T2 events. No shared/relative sink or production-entry env field → `056.006-T` |
| H0c operational remediation (T2f, H0c-only — conditional) | evidenced fail-closed surface (registry / explicit `--config` / pre-v4 schema / duplicate-intake) + operational recipe | Repair exactly ONE evidenced gate with fresh approval and backup, then one confirming re-probe; retain rollback through T4. A second H0c gate, backward-pointing, or unowned evidence blocks T4 and becomes a NEW bounded Stage follow-up (no in-place loop or sibling reactivation). Pre-v4 rebuild uses `sync`, never `upgrade` → `056.010-T` |
| Embedding resolution outcomes (conditional H1) | `src/embed/resolver.rs` | Add typed `Loaded`/`Disabled`/`Failed` detailed result while preserving an adapter for unrelated callers → `056.014-T` |
| Shared lazy lifecycle (conditional H1) | `src/embed/lifecycle.rs` | Supervised clone-shared lifecycle state machine only; expose a stable typed lifecycle-state accessor; the versioned Loading/Failed/Disabled availability projection and `src/mcp/server.rs` consumers move to `056.025-T` → `056.005-T` |
| Serve/background-sync orchestration (conditional H1) | `src/main.rs::cmd_serve`, `spawn_background_sync` | Inject one shared lazy owner into `DocServer` and Generation sync; neither eager load nor background sync may block initialize; consume the 056.025-T availability projection → `056.015-T` |
| MCP availability projection (conditional H1) | `src/mcp/server.rs` | Own the versioned Loading/Failed/Disabled MCP availability projection and `search_semantic`/`research_topic` fallback metadata (moved from `056.005-T`/`056.015-T`); red-first projection/tool-contract tests; no `cmd_serve`/background sync → `056.025-T` |
| Server transport compatibility (conditional H3-A) | rmcp pin + STDIO wiring; standing dep `056.003-T`, after `056.015-T` ONLY when H1+H3-A co-selected | Own transport types/wiring only; reacquire the exact-client transaction SEPARATELY before and after the fix through the `056.022-T` wrapper (semantic initialize correlation plus redacted transcript digest, not raw replay or persistence), red/green and Rust 1.75 proof; T4 owns production acceptance → `056.011-T` |
| Client isolated-config + cwd compatibility (conditional H3-B) | actual Copilot CLI capability evidence | Sole H3-B terminal: consume T0's `H3-B-candidate` and adjudicate BOTH a documented explicit isolated-config discovery mechanism (owning the one bounded attempt, plus one deferred control/treatment contrast through the `056.001-T` runner when isolation becomes possible) and a distinct working-directory mechanism; H3-B1 is forward evidence, H3-B2 blocks, inconclusive blocks with evidence (not Unsupported); never reads/mutates the user root config; temporary proof only activates the managed-cwd/recovery tasks (the discriminator tasks `056.026-T`/`056.027-T` are gated separately on an evidenced `type`/`transport` mismatch) and never satisfies T4 → `056.019-T` |
| Operator documentation (documentation-only) | `056.012-T`: `docs/troubleshooting.md` + the `### search_semantic`/`### research_topic`/`### get_status` headings and a `Runtime diagnostics and recovery` subsection in `docs/mcp-tools.md`. `056.013-T`: the `### 2. Configure your MCP client` managed-launch content in `docs/mcp-tools.md` + `### serve`/`### install`/`### upgrade`/`### uninstall` in `docs/cli-reference/graphtor-docs.md` | Diagnostics plus selected H0b/H0c/H1 contracts (no CLI-reference) → `056.012-T`; managed launch/recovery and H3, reconciling the `type`/`transport` discriminator → `056.013-T` |
| Tests | `tests/common/mcp_driver.rs`, `tests/mcp_serve_handshake_test.rs`, and colocated focused tests | T1 owns the shared driver module; each production task owns at most three grouped scenarios. Actual-client acceptance remains the final H3/T4 gate |

## Task Breakdown (evidence-first, test-first, ~2h each, single-width)

### T00A — Core synchronous transport — backlog `056.020-T`

* Create a **standalone, non-published diagnostic crate** rooted at
  `tools/mcp-probe/` with its own `Cargo.toml` declaring an empty `[workspace]`
  table (so the crate stands outside the production package and workspace),
  `publish = false`, and `edition = "2021"` / `rust-version = "1.75"`. This
  scaffold MUST add crate-root `#![forbid(unsafe_code)]`. Add no root
  `probe-harness` feature, no root `[[bin]]`, no custom `--target-dir`, and
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
  process-identity, or exact-CLI concern. It holds the direct
  `std::process::Child` handles needed to wire stdio for transport self-tests and
  OWNS bounded, test-only reaping of those self-test fixture `Child` handles on
  every outcome (success, assertion failure, panic/unwind, or deadline/timeout)
  through the owned direct handle (`kill()` then a bounded `wait()`) so no helper
  process leaks — scoped strictly to its own transport self-test fixtures via a
  platform-portable in-crate helper. It adds no `sysinfo`/process-tree ownership
  and no PID-scan or process-name-wide kills; PRODUCTION and WRAPPER process
  lifecycle/teardown authority remain `056.022-T`. No Tokio and no `unsafe`. The
  observer seam and evidence are `056.023-T`; the isolated workspace and config
  fixtures are `056.021-T`; the exact-CLI run is `056.001-T`.
* Verify via `cargo +1.75.0 check|test|build|clippy --manifest-path
  tools/mcp-probe/Cargo.toml` (clippy `-D warnings -D clippy::pedantic`). The
  crate is never installed or committed as a binary and is invalid for T4.

### T00C — Probe process spawning and teardown — backlog `056.022-T`

* Depends on `056.020-T`. Own `tools/mcp-probe/src/process.rs`, the versioned
  `wrapper` subcommand, and focused tests; sequentially wire them into
  `tools/mcp-probe/src/main.rs` to compose process spawning and the wrapper onto
  the transport. Preserve crate-root `#![forbid(unsafe_code)]` and add no new
  process-control crate beyond the standalone `sysinfo` observation dependency;
  because the probe now owns a separate `Cargo.lock`, run
  `cargo audit --file tools/mcp-probe/Cargo.lock` as a standalone-probe gate.
* Own the versioned `wrapper` subcommand and its argv contract: `--inner-exe`,
  repeated original `--inner-arg`, `--evidence-output`, and `--run-nonce`, all
  byte-identical between the control and treatment legs (the treatment leg alone
  adds the candidate `cwd`); the wrapper wires the inner `Child`'s stdio to the
  `056.020-T` pumps and preserves exit/stderr/half-close/deadline behavior.
* **Direct `std::process::Child` handles are the ONLY kill/wait authority.** The
  exact-CLI runner (the `056.001-T` composition point) owns the Copilot `Child`
  guard; the wrapper owns the inner-server `Child` guard. Each guard kills and
  waits its own child on every outcome. The wrapper PID is **observed-only** and
  is never used to kill.
* Remove every `sysinfo`/PID/start-time **kill** fallback and every claim of
  exact arbitrary-descendant identity or atomic whole-tree ownership. An
  injectable process-observation trait is **REQUIRED**; its standalone `sysinfo`
  production implementation observes PID, process-start-time, executable, parent,
  and a launch nonce for diagnostics and deterministic tests only — enumeration
  can verify and report but MUST NOT kill (a different crate is acceptable only if
  explicitly equivalent).
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
  `tools/mcp-probe/src/evidence.rs` (add `observer.rs` only if genuinely needed),
  the copy-only read-only observer seam, and in-wrapper JSON-RPC
  correlation/redaction using a narrowly justified standalone `serde_json`. The
  observer and transcript correlator run INSIDE the `056.022-T` wrapper process.
  Never reimplement the `056.020-T` duplex pumps or the `056.022-T` teardown.
* Define the seam so the `056.020-T` pump performs its ordered write-and-flush
  FIRST, then delivers copies/immutable chunks. The pump NEVER awaits observer
  work and NEVER takes a cross-direction observer lock, so observation cannot
  reorder, delay, or deadlock forwarding.
* Deliver through a bounded, non-blocking channel. On saturation or observer
  failure, atomically invalidate the affected capture (mark evidence incomplete)
  while forwarding continues byte-for-byte unchanged. The observer receives
  copies or immutable chunks only and never mutates, reorders, drops, or reframes
  wire bytes.
* Raw frames NEVER leave wrapper memory; the wrapper writes only a redacted
  structured summary plus digests, atomically, to its owned `--evidence-output`
  (which the `056.001-T` runner consumes) — never raw frame bytes on disk and
  never a cross-process raw replay. Incrementally reassemble the newline-delimited
  JSON-RPC stream for observation only: correlate the exact `initialize` request
  id to a `jsonrpc: "2.0"` non-error `result.protocolVersion` and record
  notifications/events separately.
* Self-test exactly this evidence-observation domain (no process spawn, no
  workspace/config, no exact CLI): (1) fragmented/partial frames and interleaved
  events plus a paused/slow observer and a failing observer each prove
  byte-identical forwarded order, half-close propagation, deadline outcome versus
  a no-observer control, and correct initialize correlation; (2) redaction of
  secret-bearing argv/env/message fields; (3) no raw-frame persistence (only the
  redacted summary/digests reach the `--evidence-output`). Verify via
  `cargo +1.75.0 check|test|build|clippy --manifest-path tools/mcp-probe/Cargo.toml`.

### T00B — Isolated probe workspace and config fixtures — backlog `056.021-T`

* Depends on `056.023-T` (chain `056.020 → 056.022 → 056.023 → 056.021`). Own
  **only** `tools/mcp-probe/src/workspace.rs`; sequentially wire it into
  `tools/mcp-probe/src/main.rs` after `056.023-T`. Compose — never reimplement —
  the `056.020-T` transport, the `056.022-T` process guards, and the `056.023-T`
  observer/evidence seam. Own **no** observer/evidence module, add **no** new
  production path primitive, and **MUST NOT import `graphtor_core`** or claim a
  reusable production security primitive.
* Create a fresh isolated workspace under the canonical repository path
  `logs/probe/<nonce>` through the Rust probe using **exclusive creation** (fail
  if the path already exists), validating containment with a probe-local, std-only
  `canonicalize` + `symlink_metadata` check before use — do **not** import
  `graphtor_core::path` helpers.
* Threat model (explicit): this protects against accidental escape of the
  repository root and a pre-existing reparse point/junction on the workspace
  path. It does **not** defend against a malicious same-user process; the
  probe-local std-only `canonicalize`/`symlink_metadata` TOCTOU window is an
  **accepted residual risk** for this non-sensitive, same-user diagnostic
  workspace.
* Generate the temporary control and treatment `.mcp.json` **only inside** that
  isolated workspace. Never read-modify-write, back up, restore, or substitute
  the user `.mcp.json`; therefore no config backup, restore, or approval receipt
  is required. Both legs use the `056.022-T` wrapper as `command` and pass the
  SAME wrapper args (`--inner-exe` / repeated `--inner-arg` / `--evidence-output`
  / `--run-nonce`); the treatment entry alone differs by adding the candidate
  `cwd` mechanism.
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

* Own `tools/mcp-probe/src/exact_cli.rs` and the exact-cli subcommand plus the
  ONE final subcommand-wiring edit in the thin `tools/mcp-probe/src/main.rs` (the
  diagnostic `wrapper` subcommand is `056.022-T`). Record the **exact** newest
  failing Copilot executable path, version/build, content hash, and `/mcp show
  graphtor-docs` invocation. T4 must use the same identity. When a
  last-known-stable Copilot executable is available for comparison, run one
  additional bounded classification pass against it (at most one affected-build
  pair plus at most one stable-build pair total) and record its
  path/version/build/hash alongside the affected build's for a genuine
  stable-vs-affected differential. On each leg of each pass, record the
  client-offered MCP `protocolVersion`/capabilities, and the server-negotiated
  `protocolVersion`/capabilities WHEN an `initialize` response exists; an
  absent negotiated value (the leg's child exits or the pipe closes before any
  response) is a first-class captured outcome, never an unmet requirement. No
  single protocol version is presumed correct unless the server's own
  negotiation contract requires it.
* Run inside `056.021-T`'s isolated `logs/probe/<nonce>` workspace using its
  temporary in-workspace control/treatment `.mcp.json`, the `056.020-T`
  transport, the `056.022-T` process guards and wrapper, and the `056.023-T`
  observer seam; never reimplement pumps, teardown, wrapper, or evidence capture.
  Read only the redacted summary/digests the wrapper writes to `--evidence-output`.
  The user `.mcp.json` is never read or written, so no approval receipt applies.
* **Diagnostic wrapper handoff (parity):** the child `.mcp.json` uses the
  `056.022-T` wrapper as `command`; the byte-identical wrapper args (`--inner-exe`
  / `--inner-arg` / `--evidence-output` / `--run-nonce`) encode the exact absolute
  production inner executable plus its original args. Control and
  treatment are identical in this handoff and differ ONLY by the treatment `cwd`.
  Record and hash the exact inner executable path and version.
* **Gate 1 (ancestor config-isolation, first):** using the exact target CLI
  against `056.021-T`'s nested fixture, prove the nearest child `.mcp.json`
  shadows and does not merge the sentinel ancestor. Only then may the bounded
  control/treatment contrast(s) below run. If the exact CLI reads or merges
  the ancestor config, stop the causal H0 comparison, emit typed
  `H3-B-candidate`, and continue
  forward to `056.019-T` (never declaring H3-B2 here); never assume the
  repository-root `.mcp.json` is unread.
* **One-shot classification:** run exactly one control (no `cwd`) / treatment
  (canonical project-root `cwd`) pair against the affected build through the
  shared runner — plus, when a last-known-stable build is available per the
  identity bullet above, exactly one additional control/treatment pair against
  that stable build (at most two pairs total, never more) — then emit the
  current **ordered** cause classification (proven prerequisites first — H0a is
  retained when a cwd correction exposes a later blocker) from child
  exit/liveness/framing/lock/Generation evidence. No implementation loop runs
  inside T0; it never reruns per correction, waits for production health, or
  reopens a downstream task.
* Terminal outcome: this task NEVER declares H3-B2. After the bounded pair(s)
  above it
  emits the ordered downstream classification and moves to `done`, or (only when
  evidence capture itself is impossible for an explicit non-H3-B reason) returns
  blocked with the captured evidence. Downstream causal tasks (including
  `056.019-T` for any `H3-B-candidate`) are visited once in the authoritative
  release-unit order (evidence foundation, then selected remediation units, then
  acceptance) and are never reopened.
* Preserve the unmodified duplex transaction, concurrent stderr, exit/still-alive
  state, locks, and Generation posture. Raw frames stay in wrapper memory
  (`056.022-T`/`056.023-T`); this runner reads only the redacted summary/digests
  from the wrapper's `--evidence-output`. Apply the deadline through `056.020-T`
  and the direct-`Child`-handle teardown through `056.022-T` (no whole-tree
  claim). Read and persist only redacted summaries and digests; fail closed if
  process ownership, exact CLI identity, ancestor config-isolation, or
  same-inner-executable parity is unproved.
* Runtime classification runs via `cargo +1.75.0 run --manifest-path
  tools/mcp-probe/Cargo.toml -- exact-cli ...`; there is no `self-test`
  subcommand. Deliverable: correlated transcripts naming ordered proven
  prerequisites/causes or an explicit `H3-B-candidate` forwarded to `056.019-T`.
  T0 links tasks; it owns no production implementation or docs.

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
  assertion cannot be made **red for the confirmed T0 cause**, **block T4 and
  create a NEW bounded Stage follow-up** to derive the missing selected-branch
  red rather than refactoring startup on a green or ambiguous test. T0 is
  one-shot and is **never reopened**. For external-only H0c/H3-B, require a
  reproducible bounded before transcript instead.
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
  H3 mode A reacquires the exact-client transaction SEPARATELY before and after
  the fix through the `056.022-T` wrapper and `056.023-T` observer (semantic
  initialize correlation plus redacted transcript digest, not raw-frame replay or
  persistence); a generic valid initialize request is not an adequate framing
  regression.
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
* `056.012-T` owns troubleshooting and the `docs/mcp-tools.md` diagnostics
  headings; `056.013-T` owns the CLI-reference documentation.
* Width: serve startup runtime diagnostics only. Curative H0a launch-contract
  generation (T2d), existing-install delivery (T2e), stale-lock liveness (H0b),
  the diagnosability sink, H0c operational remediation, H1 model lazy-load, and
  the H3 transport fix are **separate** tasks below.

#### T2d — (Conditional on H0a/H3-B1) Managed launch-contract generation — backlog `056.008-T`

* **cwd generation only (discriminator split out).** This task now owns ONLY the
  evidence-selected canonical `cwd` generation. The generator `type`/`transport`
  discriminator reconciliation was split into the UNCONDITIONAL discriminator
  task **`056.026-T`** (T2g) and its existing-install delivery **`056.027-T`**
  (T2h); this task no longer reconciles or emits the discriminator.
* For the cwd-generation part: only if T0 evidences H0a and the target build
  honors `cwd`, or if H3-B1 proves the same exact CLI honors a different
  documented working-directory mechanism. If the current build supports no safe
  mechanism, `056.019-T` selects B2: close the cwd-generation part with
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
* **Config-schema (F7) — owned by `056.026-T` (T2g), not this task:** the
  generator `type`/`transport` discriminator reconciliation (emit the
  T0-evidenced supported field; the sibling `.mcp.json` entries
  `backlogit`/`github`/`context7`/`tavily` use `type: "stdio"` while today's
  `managed_server_value` emits `transport: "stdio"`; keep or migrate the legacy
  `transport` recognition via `is_exact_legacy_shape`) is the UNCONDITIONAL
  discriminator task `056.026-T`. This task preserves marker-based recognition so
  already-installed managed entries still refresh after gaining `cwd`, composing
  whatever discriminator `056.026-T` reconciled; it does **not** decide or emit
  the discriminator itself, and no one claims the field name is the current root
  cause without T0 evidence.
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
* Contingency: when neither H0a nor H3-B1 selects managed `cwd`, close this
  task with a `not-needed: managed cwd generation not selected` backlog comment.
  The `type`/`transport` discriminator reconciliation is `056.026-T` (T2g), not
  this task. `not-needed` is a disposition, not a backlog status. Width: managed
  cwd launch-config generation only.

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

#### T2g — (Conditional on discriminator mismatch) Generator type/transport discriminator reconciliation — backlog `056.026-T`

* **Unconditional-when-evidenced, split from T2d.** Whenever exact-CLI evidence
  (T0 / `056.019-T`) shows a docs/generator `type` vs `transport` mismatch,
  reconcile the generated stdio discriminator in `managed_server_value` to the
  exact CLI-honored field regardless of the primary H0/H1/H3-A cause; if no
  mismatch is evidenced, close with `not-needed: no type/transport mismatch
  evidenced`.
* **Depends only on `056.019-T`** (the completed evidence classification), NOT on
  `056.017-T`/`056.024-T`/`056.018-T`. Owns ONLY the `managed_server_value`
  discriminator field plus exact legacy-shape recognition (`is_exact_legacy_shape`,
  recognizing both the pre- and post-reconciliation discriminator) and focused
  generation tests. No cwd field (`056.008-T`), no mutation API (`056.017-T`), no
  recovery, no delivery.
* **Test-first:** red-then-green three groups — discriminator reconciled to the
  CLI-honored field; legacy-shape recognition across old and new discriminator;
  containment/marker preservation. This proves generation only.
* **Co-selection assembly rule:** when both the discriminator remedy and the
  H0a/H3-B1 cwd remedy (`056.008-T`) are selected into one managed-config
  shipment, Stage orders `056.026-T` before `056.008-T` at shipment-assembly time
  (discriminator reconciled first, then the cwd field composed) — a co-selection
  ordering, not a standing backlog edge. Width: generated discriminator field
  only.

#### T2h — (Conditional on discriminator existing-install delivery) Deliver reconciled discriminator to existing installs — backlog `056.027-T`

* **Bounded existing-install discriminator delivery, split so the discriminator
  remedy ships without the cwd chain.** `056.009-T` cannot own this: it closes
  `not-needed` when `056.008-T` (cwd) is unselected, so in a discriminator-only
  remedy it would deliver nothing. This task refreshes ONLY the marked managed
  entry's stdio discriminator on `cmd_upgrade`.
* **Depends on `056.026-T`** (reconciled generated value) **and `056.018-T`**
  (safe no-follow recovery; transitively `056.024-T`/`056.017-T`). It does NOT
  depend on `056.008-T` or `056.009-T`. Because a discriminator existing-install
  mutation IS a sensitive managed-`.mcp.json` mutation, the activation predicates
  of `056.017-T`/`056.024-T`/`056.018-T` INCLUDE this branch, so they are selected
  (not `not-needed`) and produce their typed API and recovery handles whenever
  this task is selected — no task depends on a prerequisite that closed
  `not-needed`.
* **Composition:** compose `056.017-T` typed config actions and `056.018-T`
  recovery handles in `cmd_upgrade` to refresh only the discriminator field,
  width-separated from `056.009-T`'s cwd/recovery refresh. Preserve
  unowned/non-JSON bytes; user collision is a typed non-mutating outcome;
  containment stays a hard `PathViolation`; backup-first through the verified
  handle.
* **Test-first + re-probe:** red-then-green three groups (nested marked/legacy
  discriminator refresh with recovery metadata; no-change/collision/non-JSON
  preservation without backup; recovery/mutation failure with original bytes
  intact). On a shipping discriminator remedy unit, own exactly one exact-client
  re-probe after delivery; record the post-refresh production config hash.
  `056.013-T` owns docs (incl. the README Quick Start example). Width:
  existing-install discriminator delivery only.

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

H1 is split into four red-first widths because the current eager resolver is
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
   `src/mcp/server.rs` consumers are `056.025-T`. Exactly three groups cover
   concurrency, terminal-Disabled/drop-cancel behavior, and
   failure/panic/retry/Ready.
3. **MCP availability projection (`056.025-T`):** own the versioned MCP
   availability projection and `src/mcp/server.rs` consumers (moved from
   `056.005-T`/`056.015-T`) — stable Loading/Failed/Disabled code, retryability,
   remediation, and fallback metadata; `search_semantic` returns typed disabled
   with no fallback and `research_topic` permits keyword fallback only with
   explicit Disabled `fallback=keyword` metadata; neither falls back while
   Loading/Failed. Red-first projection/tool-contract tests only; no
   `cmd_serve`/background sync. Depends on `056.005-T`.
4. **Serve/background-sync orchestration (`056.015-T`):** `cmd_serve` creates one
   owner and passes clones to `DocServer` and `spawn_background_sync`, consuming
   the `056.025-T` availability projection. The first
   model-dependent consumer may be sync or a request and calls the same
   serialized `ensure_loading`. Background sync remains subscribed across
   `Failed → Loading → Ready`, may request the one policy-controlled retry,
   embeds once, and exits only on Ready completion, Disabled, or shutdown.
   No second loader or detached task is permitted. `056.015-T` depends on
   `056.025-T` and (transitively) on T2 before editing `cmd_serve`.

DB open, lock acquisition, pre-v4, and duplicate-intake remain fail-closed
pre-serve gates. If H1 is not evidenced, all four tasks (`056.014-T`, `056.005-T`,
`056.025-T`, `056.015-T`) complete `done` with
`not-needed: H1 not evidenced`. `056.012-T` owns the operator-facing tool
retry contract.

#### T-H3-A — Server framing/version compatibility — backlog `056.011-T`

* Select only when the child remains alive but T0's framed transaction never
  negotiates a protocol version. Own **only** transport types and rmcp/stdio
  wiring. The transport wiring layers atop the `056.015-T` serve-orchestration
  restructure ONLY when H1 and H3-A are co-selected in the same shipment unit
  (a shipment-assembly-time co-selection edge per the Authoritative task
  ordering) — it is NOT a standing backlog dependency; the standing edge is
  `056.003 → 056.011` only. Do NOT require persisted or
  byte-identical raw frames: reacquire the exact-client transaction SEPARATELY
  before and after the fix through the `056.022-T` wrapper and the `056.023-T`
  in-wrapper observer, each validating semantic initialize correlation (exact
  request id → `jsonrpc: "2.0"` non-error `result.protocolVersion`) plus the
  redacted transcript digest; add/observe the H3-A red from the before-run, then
  green it with the minimal fix and re-validate from the after-run. No raw bytes
  on disk; a generic direct initialize is insufficient. `056.011-T` closes on its
  own once these before/after reacquisitions validate; it does not depend on T4 or
  production acceptance (no `056.011`↔T4 cycle), and T4 separately accepts production.
* Inspect candidate crate metadata before implementation. rmcp 1.8.x uses
  edition 2024 and is excluded by Rust 1.75. Prefer a minimal framing
  adaptation around pinned 1.5.x; use another release only after recording
  edition-2021/MSRV compatibility. If a fork/patch override or wider change is
  required, halt for a separate deliberation. T4 alone owns production-entry
  acceptance. No `get_info` echo change.
* If H3-A is not selected, complete done-plus-not-needed.

#### T-H3-B — Client cwd compatibility — backlog `056.019-T`

* Sole H3-B terminal: consume T0's `H3-B-candidate` and adjudicate BOTH client
  mechanisms one-shot on the exact CLI. If T0's Gate 1 shows the exact CLI
  **merges** ancestor config, own the ONE bounded attempt to find and use a
  documented explicit isolated-config discovery mechanism; if it exists and
  isolates config, perform exactly ONE deferred control/treatment contrast through
  the `056.001-T` runner and emit the same ordered cause classification
  (activating the managed-cwd/recovery tasks). Separately, when the exact CLI
  ignores/rejects T0's original `cwd` field, **B1** requires that same executable
  to pass a second foreign-directory contrast using a different documented field
  placement or working-directory mechanism (activates the managed-cwd/recovery
  tasks 056.017/056.024/056.008/056.018/056.009, cannot satisfy T4; the
  discriminator tasks 056.026/056.027 are gated separately on an evidenced
  `type`/`transport` mismatch). Never deliberately read or mutate the user root config.
* **B2** means the exact identity supports no safe isolated-config or
  working-directory mechanism: mark `056.019-T` `done` with the recorded H3-B2
  classification and block shipment/T4 as `unsupported-client`. The managed-config
  family is thereby unselected; `056.019-T` does NOT itself mutate any sibling
  task's status — Stage's Final Assembly Protocol applies the `done` +
  `not-needed: H3-B2` disposition to those unselected tasks (selection gate). Only
  a conclusive **B1** (supported) or a proven **B2** is a terminal verdict that
  closes `056.019-T` and lets `049-S`
  close. An **INCONCLUSIVE** result (neither proven supported nor proven
  unsupported) is NOT terminal and NOT trusted cause selection: it moves
  `056.019-T` to **`blocked`** status with the captured evidence, **blocks
  `049-S` closure**, and **requires a NAMED new bounded Stage follow-up** (or
  operator adjudication); it is never `done`, never classified Unsupported, and
  never switches CLI identity. Switching CLI identity is not a fix for this
  acceptance contract.
* Record exact CLI executable path, version/build, invocation, and capability.
  Neither mode may add a server-side external-path fallback. T4 still requires
  three restored production-entry sessions. If H3-B is not selected, complete
  done-plus-not-needed.

### T4 — Runtime verification, rollback, and closure evidence

* Verify with the exact T0 Copilot executable path/version/build across three
  exact-Copilot `/mcp show graphtor-docs` sessions (including one
  diagnostic-gate-off start). Acceptance for THIS bug is that each session shows a
  healthy connected, initialized server with no OS error 232 — using ONLY the
  connection/initialization status the CLI surfaces (and advertised tool
  list/count when shown), not raw JSON-RPC wire fields — correlating the exact
  production config hash/file identity with the Copilot-spawned server startup
  event (PID, executable/build, canonical cwd, timestamp). The correlated wire
  fields (`jsonrpc: "2.0"`, correlated id, no error, `result.protocolVersion`)
  are proven SEPARATELY by the direct T1 (`056.002-T`) production driver against
  the same production binary/workspace, not by `/mcp show`. If `/mcp show`
  reports advertised tools, record their list/count as
  supporting evidence, but do not require a tool invocation unrelated to the
  reported load bug and do not route a missing deterministic tool-call UI to
  H3-B2. A direct T1 production driver separately confirms the expected MCP tools
  and one side-effect-free `get_status` against the same production
  binary/workspace as a server control (not proof of Copilot UI invocation); this
  does not expand into a get_status workspace-fingerprint product feature. Capture
  `mcp_serve_ready` separately as preflight evidence only.
* On a managed-cwd branch, first require `056.009-T` cwd/recovery delivery
  evidence that the delivered upgrade refreshed the target workspace; on a
  discriminator branch, require `056.027-T` discriminator delivery evidence (the
  analog of `056.009-T`). Record the post-refresh production config hash; when
  both branches are selected, require both.
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
  proves actual-client connection/initialization completion (CLI-surfaced
  status), while the direct T1 driver proves the JSON-RPC wire fields.
* Dependency note: T4 depends on `056.003-T` (**non-conditional** cmd_serve
  diagnostics — loud exit-2 errors + `mcp_serve_ready`, always lands), on the
  always-land probe evidence chain `056.020-T → 056.022-T → 056.023-T →
  056.021-T → 056.001-T` (with **explicit direct evidence-chain edges** to
  `056.001-T`, `056.021-T`, `056.022-T`, `056.023-T`, and `056.024-T` for
  robustness), plus the
  **curative** fix tasks, each conditional and moved to **`done` with a
  `not-needed: <rationale>` log comment** when
  its hypothesis is not the evidenced cause: T2d cwd launch-contract (H0a/H3-B1) =
  `056.008-T`, T2g discriminator generation = `056.026-T`, T2h discriminator
  existing-install delivery = `056.027-T`, T2e existing-install cwd migration
  (H0a/H3-B1) = `056.009-T`, T2b
  stale-lock harness/implementation (H0b) = `056.016-T`/`056.007-T`, T2c
  diagnosability = `056.006-T`, T2f H0c remediation = `056.010-T`, H1 resolver/
  lifecycle/projection/orchestration =
  `056.014-T`/`056.005-T`/`056.025-T`/`056.015-T`, H3-A transport
  = `056.011-T`, H3-B compatibility = `056.019-T`, typed config/recovery =
  `056.017-T`/`056.018-T`,   core transport = `056.020-T`,
  process spawning/teardown = `056.022-T`, observer seam/evidence = `056.023-T`,
  isolated workspace/fixtures = `056.021-T`, safe no-follow decision =
  `056.024-T`, standalone-probe CI = `056.028-T`, and documentation-only
  tasks `056.012-T`/`056.013-T`. T0 may activate an **ordered sequence**:
  **H0a → 056.017 + 056.008 (cwd) + 056.024 + 056.018 +
  056.009**; **discriminator → 056.026 + 056.027 (+ 056.017 + 056.024 + 056.018)**;
  **H0b → 056.016 + 056.007**; **H0c → 056.010** with 056.006
  independently evidence-gated; **H1 → 056.014 + 056.005 + 056.025 + 056.015**;
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
For the current staging-review state, the `## Stage Readiness Evidence (durable —
2026-08-25)` section below is a manifest/disposition cross-check only — it is **NOT
a review-currency authority or merge gate**. The **authoritative dynamic gate** is
the live PR #107 `## Local Review Readiness` current-HEAD record plus Ship's
mandatory fresh current-HEAD local review. PR #106 is merged and historical; this
plan claims no current READY and does not authorize Ship or merge.

### Authoritative invariants (single source of truth)

These invariants hold for the whole plan; task sections reference them rather
than restate every clause:

* **Standalone crate, not a trust boundary** — the probe is a standalone,
  non-published crate at `tools/mcp-probe/` (own `Cargo.toml` with `[workspace]`
  + `publish = false`, Rust 2021 / MSRV 1.75); no feature-gated root `[[bin]]`,
  custom `--target-dir`, DACL/ACL, `Assert-*` helper, owner-only artifact,
  user-config backup, or approval receipt exists. Its ordinary Cargo target is a
  build cache; the built binary is ephemeral and invalid as T4 evidence.
* **Width-split probe** — `056.020-T` owns the std-only core synchronous
  transport, `056.023-T` owns the copy-only observer seam plus in-wrapper
  JSON-RPC correlation/redaction and redacted evidence (standalone `serde_json`),
  `056.021-T` owns the isolated workspace and config fixtures (probe-local
  std-only containment, no `graphtor_core`), `056.022-T` owns process
  spawning/teardown and the versioned `wrapper` subcommand (required injectable
  observation trait with a standalone `sysinfo` impl plus a standalone
  `cargo audit` gate), and `056.001-T` owns the exact-CLI run reading only the
  redacted evidence file.
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
  ancestor before any causal contrast; if the CLI merges ancestor config, it
  emits typed `H3-B-candidate` and continues forward to `056.019-T` (never
  declaring H3-B2 itself). The repository-root `.mcp.json` is never assumed
  unread.
* **Release-unit phases, not one total-order chain** — the work is delivered as
  phased release units: PHASE 1 evidence foundation (shipment `049-S`), PHASE 1.5
  the unconditional evidence-infrastructure unit (`056.028` standalone-probe CI,
  assembled by Stage after `049-S` closes and before any remedy shipment), PHASE 2
  selected remediation units (unshipped until T0 selects them, grouped by cause
  family), PHASE 3 final acceptance/docs (unshipped, dependent on the selected
  remedies). Remedy family entries depend on the evidence foundation
  (`056.003-T`, or `056.019-T` for the H3-B-selected managed-config/discriminator
  groups); intra-family order is a true dependency edge, but unrelated cause
  families are NOT chained into a total order. Sequential single-shipment
  execution is enforced by the **explicit selection gate below plus** one
  in-flight shipment at a time (P-001) and single-worktree topology (P-016), not
  by fake cross-cause edges. Total task count is 28 (`056.001-T`..`056.028-T`).
* **Explicit selection gate (dependency-ready ≠ selected)** — every PHASE 2
  (`phase:remedy`) task carries a machine-queryable `selection:pending` label plus
  its `cause:<family>` label. After `049-S` closes, Stage consumes the
  T0/`056.019-T` classification, flips ONLY the selected families' tasks to
  `selection:selected`, and only then creates a queued remedy shipment of those
  tasks. Unselected tasks keep `selection:pending` and no shipment membership (so
  no Ship/Orchestrator routing claims them) until dispositioned `done` +
  `not-needed:<evidence id>`. Required pre-shipment gate (Stage/Orchestrator MUST
  run it before creating or claiming any remedy shipment, and MUST NOT add a
  `selection:pending` task to any shipment): the query `SELECT id, labels FROM
  items WHERE id IN (<candidate member ids>) AND (labels NOT LIKE
  '%selection:selected%' OR labels LIKE '%selection:pending%')` — any returned row
  FAILS the gate and the shipment MUST NOT be created. P-001/P-016 do NOT
  perform this selection — the label + shipment-membership gate is the claim
  authority. Unselected remedy tasks are NOT set to backlog status `blocked`;
  they stay `queued` + `selection:pending`.
* **One-shot classification** — `056.001-T` runs exactly one control/treatment
  pair against the affected build (plus one bounded additional pair against a
  last-known-stable build when available) and emits an ordered classification;
  it never loops, reruns per
  correction, or reopens a downstream task. Backward-pointing or unowned evidence
  blocks T4 and creates a new bounded Stage follow-up rather than looping in
  place.
* **Shipment-interface re-probe** — every SHIPPING production-remediation release
  unit owns exactly one exact-client re-probe after applying its correction;
  characterization (`056.016-T`) and decision/PoC (`056.024-T`) tasks are not
  corrections and perform no re-probe. This replaces any earlier "every selected
  causal task implements and re-probes" rule.
* **Probe-scoped user-config invariant** — control/treatment and the nested
  fixture live only inside the owned `logs/probe/<nonce>` workspace; the user
  `.mcp.json` is never read, mutated, backed up, restored, or substituted by the
  probe/T0. This invariant is scoped to the probe/T0 only; production
  managed-config tasks (`056.008-T`/`056.009-T`/`056.018-T`/`056.027-T`) may read,
  mutate, back up, and recover the configured workspace `.mcp.json` with typed
  ownership and approval where destructive.
* **The authoritative dynamic gate is the live PR + Ship's fresh review** — the
  `## Stage Readiness Evidence (durable — 2026-08-25)` section is a manifest/
  disposition cross-check only, **NOT a review-currency authority or merge gate**.
  The gate is the live PR #107 `## Local Review Readiness` current-HEAD record plus
  Ship's mandatory fresh current-HEAD local review. PR #106 is merged and historical;
  this plan claims no current READY and does not authorize Ship or merge.

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
forward-only, single-total-order state machine. Correction round 3 (final) moves
the diagnostic `wrapper` subcommand and required `sysinfo`/`serde_json` evidence
into `056.022-T`/`056.023-T`, replaces the probe's `graphtor_core` path helpers
with a probe-local std-only containment check, adds the projection task
`056.025-T` (25 tasks total), moves `056.024-T` before `056.008-T`, scopes the
`.mcp.json` invariant to the probe/T0, and narrows T4 to handshake acceptance (no
mandatory `tools/list`/`get_status` fingerprint, no missing-tool-UI H3-B2
routing); `056.001-T` never declares H3-B2 and emits `H3-B-candidate` forward to
`056.019-T`. A subsequent Stage re-decomposition (2026-08-23) then replaced the
single forward-only total order with release-unit phases — an evidence-foundation
shipment (`049-S`) plus unshipped cause-family remediation units and an unshipped
final acceptance/docs unit — and replaced the global re-probe rule with the
shipment-interface re-probe rule; see the Authoritative task ordering above.

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
* Probe/T0 user `.mcp.json` mutation, backup, restore, and the config approval
  receipt (production managed-config tasks remain allowed to mutate/recover the
  configured workspace `.mcp.json`).
* The `self-test` probe subcommand and the `--features probe-harness --bin
  graphtor-mcp-probe` verification commands.

### Retained and new nodes

* `056.020-T` (retained, narrowed) — core synchronous transport of the
  standalone `tools/mcp-probe/` crate (`src/main.rs` + `src/transport.rs`): raw
  std-process/std-thread duplex pumps, half-close, bounded stderr drain,
  deadlines, a post-write bounded non-blocking copy-delivery seam, and a
  platform-portable in-crate helper-child self-test mechanism whose fixture
  `Child` handles it reaps (kill + bounded wait on every outcome incl.
  panic/timeout) — test-fixture cleanup ONLY. No observer/evidence,
  workspace/config, production/wrapper teardown (that is `056.022-T`), Tokio, or
  `unsafe`.
* `056.022-T` (retained, corrected) — probe process spawning, teardown by
  direct `Child` handles only, and the versioned `wrapper` subcommand (argv
  `--inner-exe`/`--inner-arg`/`--evidence-output`/`--run-nonce`, byte-identical
  across legs); a REQUIRED injectable observation trait with a standalone
  `sysinfo` impl observes identity but never kills; same-second identity fails
  closed; a standalone `cargo audit` gate applies. Depends on `056.020-T`.
* `056.023-T` (new) — copy-only observer seam plus in-wrapper JSON-RPC
  correlation/redaction and redacted evidence (`evidence.rs`, standalone
  `serde_json`): bounded non-blocking delivery, saturation invalidates capture,
  raw frames stay in wrapper memory, and only redacted summaries/digests are
  written atomically to the wrapper-owned `--evidence-output`. Depends on
  `056.022-T`.
* `056.021-T` (retained, narrowed) — isolated `logs/probe/<nonce>` workspace and
  the owned nested ancestor/child config fixture (`workspace.rs` only);
  exclusive-create plus a probe-local std-only `canonicalize`/`symlink_metadata`
  containment check (no `graphtor_core` import), with identical wrapper args on
  both legs and the treatment leg alone adding `cwd`. Owns no observer/evidence
  module. Depends on `056.023-T`.
* `056.001-T` (retained, one-shot) — owns `exact_cli.rs`; runs one control/
  treatment pair against the affected build (plus one bounded additional pair
  against a last-known-stable build when available) inside `056.021-T`'s workspace, proves ancestor
  config-isolation first, and emits the ordered classification. Depends on
  `056.021-T`.
* `056.024-T` (retained, moved) — bounded decision/spike that closes
  `not-needed` when no managed-config mutation/recovery is selected, otherwise
  selects an MSRV-1.75 safe no-follow config-mutation primitive and records it in
  a NEW `docs/decisions/` artifact (or blocks `056.018`/shipment). Corrected DAG:
  after `056.017`, before `056.018` (parallel to `056.017 → 056.008`); it gates
  only `056.018`/shipment, never `056.008` cwd generation or `056.026`
  discriminator generation. Its activation predicate includes discriminator
  existing-install delivery (`056.027-T`), not only H0a/H3-B1.
* `056.025-T` (new) — the versioned Loading/Failed/Disabled MCP availability
  projection and `search_semantic`/`research_topic` fallback metadata in
  `src/mcp/server.rs` (moved out of `056.005-T`/`056.015-T`); red-first
  projection/tool-contract tests only, no `cmd_serve`/background sync. In the
  chain after `056.005`, before `056.015`.
* `056.026-T` (new) — UNCONDITIONAL generator `type`/`transport` discriminator
  reconciliation split out of `056.008-T`: owns only the `managed_server_value`
  discriminator field + exact legacy-shape recognition + focused generation tests.
  Depends on `056.019-T` only (not `056.017`/`056.024`/`056.018`); closes
  `not-needed` when no mismatch is evidenced.
* `056.027-T` (new) — bounded existing-install discriminator delivery: composes
  `056.017-T` typed API + `056.018-T` recovery in `cmd_upgrade` to refresh only
  the marked entry's discriminator, width-separated from `056.009-T`'s
  cwd/recovery delivery. Depends on `056.026-T` and `056.018-T`; does not require
  `056.008`/`056.009`.
* `056.028-T` (new) — dedicated standalone-probe CI job for the `tools/mcp-probe/`
  crate's separate manifest/lockfile (fmt/clippy/test/build + `cargo audit --file
  tools/mcp-probe/Cargo.lock`). Depends on `056.020-T`; evidence-infrastructure
  follow-up outside `049-S`, in T4 fan-in. Created by Stage now; implemented by
  Ship per its test-first contract (seeded-violation red proof).
* Every causal node is wired into cause-family branches off the evidence
  foundation (see phased ordering below), not one total-order chain. `056.005-T`
  narrows to the embedding lifecycle state machine; `056.025-T` owns the MCP
  availability projection; `056.015-T` owns `cmd_serve`/background-sync
  orchestration; `056.011-T` owns transport types/wiring and reacquires the
  exact-client transaction separately before/after through the wrapper (not raw
  replay) — when H1 and H3-A are co-selected in one unit, transport wiring lands
  after the `056.015-T` serve-orchestration restructure, applied at
  shipment-assembly time rather than as a standing cross-family backlog edge;
  `056.006-T` is the optional diagnostic sink, selected only when stderr is
  unavailable and env inheritance is proven (via `056.001-T` sentinel +
  `056.022-T` observation); `056.010-T` owns one H0c repair; `056.008-T`
  generates the evidence-selected `cwd` ONLY (the unconditional `type`/`transport`
  discriminator reconciliation is `056.026-T` and its existing-install delivery is
  `056.027-T`); `056.018-T` implements only the `056.024-T` decision (recovery
  consumed by `056.009-T` cwd delivery and `056.027-T` discriminator delivery);
  `056.019-T` is the sole H3-B terminal, adjudicating both isolated-config and cwd
  mechanisms with at most one deferred contrast, fail-closed on inconclusive
  (`blocked` + named Stage follow-up). T4's direct fan-in is all tasks
  `056.001`..`056.028` except `056.004`, with explicit evidence-chain edges to
  `056.001-T`, `056.002-T`, `056.021-T`, `056.022-T`, `056.023-T`, `056.024-T`,
  `056.025-T`, `056.026-T`, `056.027-T`, and `056.028-T`.

### Authoritative task ordering (release-unit phases)

The authoritative structure is a set of release-unit phases with cause-family
branches off a shared evidence spine, not a single 28-task total order. Each edge
`X → Y` is a backlog dependency (Y depends on X; X runs first). Sequential
single-shipment execution is enforced by the explicit selection gate plus one
in-flight shipment at a time (P-001) and single-worktree topology (P-016), so
unrelated cause families are deliberately NOT chained into a total order, and
dependency-ready never means selected.

**PHASE 1 — Evidence foundation (shipment `049-S`, the only queued shipment):**

Two evidence spines converge at the sole H3-B terminal `056.019`:

* probe + T0: `056.020 → 056.022 → 056.023 → 056.021 → 056.001`
* independent green driver + diagnostics: `056.002 → 056.003`
* convergence: `056.019` depends on both `056.001` and `056.003`

`056.002` is a T0-agnostic reusable green driver and carries NO dependency on
`056.001`; `056.003` (always-on production diagnostics) reuses the `056.002`
driver. This unit is the "evidence foundation + parity-safe always-on
diagnostics"; its acceptance is the trusted exact-Copilot classification/
cause-selection record plus the always-on diagnostics seam, and does NOT require
the restored-production T4. Fail-closed H3-B: only a conclusive H3-B1 or proven
H3-B2 closes `056.019` and lets `049-S` close; an inconclusive verdict moves
`056.019` to `blocked`, blocks `049-S` closure, and requires a named new bounded
Stage follow-up. The eight-task manifest is `056.020`/`056.022`/`056.023`/
`056.021`/`056.001`/`056.002`/`056.003`/`056.019`; the umbrella feature `056-F`
is excluded (protected covering feature per P-015).

**PHASE 1.5 — Evidence-infrastructure release unit (unconditional; assembled by
Stage after `049-S` closes and BEFORE any selected remedy shipment):**

* `056.028` (dedicated standalone-probe CI job) is the sole member. It is
  deliberately NOT in `049-S` because its `.github/workflows` job depends on the
  standalone `tools/mcp-probe/` crate delivered by the `049-S` tasks
  (`056.020`/`056.022`/`056.023`/`056.021`), so it cannot be authored until that
  crate lands. It is NOT cause-selected — it carries no remedy `selection:` gate
  (`cause:probe-ci` is a categorization label, not a remedy selection) and is
  always assembled.
* **Assembly owner:** Stage. After `049-S` closes, Stage assembles this PHASE 1.5
  unit — a task-only manifest containing `056.028`, excluding the protected
  covering feature `056-F` per P-015 — as the immediate next release unit, before
  flipping any remedy family to `selection:selected` or creating any remedy
  shipment. No shipment manifest is created at plan time.
* **Ordering + readiness gate:** PHASE 1.5 executes after PHASE 1 (`049-S`) and
  before any PHASE 2 remedy shipment; its readiness gate is the standalone-probe
  CI job green against the probe manifest/lockfile. `056.028` remains in the T4
  (`056.004`) fan-in, so restored-production acceptance cannot pass until the
  probe CI job is green.

**PHASE 2 — Selected remediation units (unshipped follow-ups; NOT queued for
Ship until the selection gate flips them to `selection:selected`; each family
entry depends on the evidence foundation):**

* managed-config cwd/recovery (corrected DAG, delivery split):
  `056.019 → 056.017`; then `056.017 → 056.008` (cwd gen) IN PARALLEL WITH
  `056.017 → 056.024 → 056.018` (recovery); then `{056.008, 056.018} → 056.009`
  (cwd/recovery existing-install delivery).
* type/transport discriminator: `056.019 → 056.026` (unconditional generation
  when a mismatch is evidenced) and `{056.026, 056.018} → 056.027` (existing-
  install discriminator delivery). A discriminator-only remedy also selects the
  safe-mutation prerequisites `056.017`/`056.024`/`056.018` (their activation
  predicates include discriminator delivery), while `056.008`/`056.009` close
  `not-needed`. Co-selection with the cwd remedy orders `056.026` before
  `056.008` at assembly time.
* lock recovery: `056.003 → 056.016 → 056.007`
* H0c state repair: `056.003 → 056.010`
* H1 model lifecycle: `056.003 → 056.014 → 056.005 → 056.025 → 056.015`
* H3-A transport compatibility: `056.003 → 056.011` (standing edge; the
  `after 056.015` ordering is a co-selection-only assembly edge)
* diagnostic sink (optional): `056.003 → 056.006` (selected only when stderr is
  unavailable AND env inheritance is proven; the `after 056.011` ordering is a
  co-selection-only assembly edge)

Each SHIPPING remediation unit owns exactly one exact-client re-probe after its
correction (shipment-interface rule); characterization (`056.016`) and
decision/PoC (`056.024`) are not corrections and do not re-probe. Conditional
families not selected close `done` + `not-needed:<evidence id>` and pass evidence
unchanged; `056.010-T` owns exactly one H0c repair and a second gate is a new
bounded follow-up.

**PHASE 3 — Final acceptance and documentation (unshipped; dependent on all
selected remedies; not prematurely Ship-ready):**

* docs: `056.012 → 056.013`, where `056.012` depends on every remedy-family leaf
  (`056.006`, `056.007`, `056.009`, `056.010`, `056.011`, `056.015`) and
  `056.013` additionally depends on the discriminator delivery `056.027`, so docs
  are not ready until every family resolves (done or not-needed).
* `056.004` (T4) keeps direct dependencies on every other task
  `056.001`..`056.028` except `056.004` itself — with explicit evidence-chain
  edges to `056.001`, `056.002`, `056.021`, `056.022`, `056.023`, `056.024`,
  `056.025`, `056.026`, `056.027`, and `056.028` — and is the sole
  restored-production actual-client acceptance node and the single registry-backed
  acceptance gate.
* **Final Assembly Protocol (Stage-owned):** after the selected remedy shipments
  complete, Stage (1) confirms every PHASE 2 task is terminal — `done` (selected +
  shipped) or `done` + `not-needed:<evidence id>` (unselected) — the owned
  disposition that satisfies the `056.012`/`056.004` fan-in without an unowned
  mass sweep; (2) for any selected NEW follow-up created during remediation (e.g.
  a second H0c gate or an H3-B-inconclusive follow-up), adds it to the fan-in
  owners (`056.004`, and `056.012` for docs) by a single Stage dependency update
  BEFORE creating the PHASE 3 shipment — never by reopening a completed code task;
  (3) only then creates the queued PHASE 3 shipment. Where backlogit supports
  shipment-to-shipment `blocks` edges, Stage may also sequence the PHASE 3
  shipment after the selected remedy shipments.

### Explicit residual risks

* The probe-local std-only `canonicalize`/`symlink_metadata` TOCTOU window is
  accepted for this non-sensitive, same-user diagnostic workspace; it defends
  against accidental escape and pre-existing reparse points, not a malicious
  same-user source modifier. The probe introduces no new production path
  primitive and does not import `graphtor_core`.
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
* Selection gate: dependency-ready ≠ selected. Every PHASE 2 task carries
  `selection:pending` + `cause:<family>`; Stage flips only selected families to
  `selection:selected` after `049-S` closes, and unselected tasks stay `queued` /
  `selection:pending` with no shipment membership (never backlog status
  `blocked`). No Ship/Orchestrator routing can claim an unselected task because it
  has no shipment membership.
* Discriminator split (delivery-safe): the unconditional `type`/`transport`
  reconciliation (`056.026-T`) depends only on the evidence classification, and
  its existing-install delivery (`056.027-T`) composes the selected
  `056.017`/`056.024`/`056.018` safe-mutation contracts — so no delivery task
  depends on a prerequisite that closed `not-needed`.
* Direct-`Child` guards are RAII (`Drop`) so kill+wait runs on panic/unwind
  (`056.020-T`/`056.022-T`); the `056.006-T` env-inheritance criterion is proven
  by a bounded probe-owned sentinel (`056.001-T` set + `056.022-T` observe), not
  assumed. The `056.020-T` transport self-test helper-child mechanism is
  platform-portable and within-width.
* `056.028-T` owns a dedicated standalone-probe CI job because the probe crate
  has a separate manifest/lockfile the root pipeline does not cover; it is an
  evidence-infrastructure follow-up outside `049-S`, gated into T4 fan-in. Stage
  assembles it as the unconditional PHASE 1.5 evidence-infrastructure release unit
  immediately after `049-S` closes and before any selected remedy shipment (owner:
  Stage; readiness gate: standalone-probe CI green); it is not cause-selected.
* **Scope boundary (round 3, extended 2026-08-23):** unrelated learnings about
  direct-`main`/closure lifetimes, post-merge closure, or cargo-process TOCTOU are
  NOT changes to this feature plan; this plan does not expand into install-binary
  overwrite, database-open, or unrelated production TOCTOU findings. `056.024-T`
  and `056.018-T` are scoped only to the managed-config recovery selected by this
  feature; `056.026-T`/`056.027-T` are scoped only to the `type`/`transport`
  discriminator; `056.028-T` only to the standalone-probe CI job.
* **Hook-stream duplicate (benign; no repair attempted):** the append-only
  backlogit hook queue (`.backlogit/hooks_queue.jsonl`) holds two consecutive
  `create_artifact` events for `056.021-T` (seq 994 and 995) with no intervening
  delete/recreate. This is benign duplicate create noise: the artifact
  `056.021-T` is singular (one queue file, one identity), no supported
  removal/supersede operation exists for the append-only stream, so the stream is
  left intact and NO destructive repair was attempted. Hook consumers MUST dedupe
  by item/event identity (`item_id` + `event_type`), not by raw sequence count.

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
cargo +1.75.0 clippy --manifest-path tools/mcp-probe/Cargo.toml --all-targets -- -D warnings -D clippy::pedantic
if ($LASTEXITCODE -ne 0) { throw "probe clippy pedantic failed (exit $LASTEXITCODE)" }
cargo +1.75.0 test --manifest-path tools/mcp-probe/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "probe self-tests failed (exit $LASTEXITCODE)" }
cargo +1.75.0 build --manifest-path tools/mcp-probe/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "probe build failed (exit $LASTEXITCODE)" }
# audit applies: the probe now has a Cargo.lock with third-party dependencies
# (sysinfo in 056.022-T, serde_json in 056.023-T), so run cargo audit against the
# probe's own lockfile; the core transport itself remains std-only.
cargo audit --file tools/mcp-probe/Cargo.lock
if ($LASTEXITCODE -ne 0) { throw "probe audit failed (exit $LASTEXITCODE)" }

# 3. The one-shot actual-client classification (T0 = 056.001-T) is performed BY
#    the probe via its exact-cli subcommand, not by this block. 056.021-T creates
#    a fresh isolated logs/probe/<nonce> workspace (exclusive creation,
#    probe-local std-only canonicalize/symlink_metadata containment, no
#    graphtor_core), writes temporary in-workspace
#    control/treatment .mcp.json plus an owned nested ancestor/child config
#    fixture, and captures redacted evidence through the 056.023-T copy-only
#    observer seam. The user .mcp.json is never read, mutated, backed up, or
#    restored, so no approval receipt is required. The probe first proves ancestor
#    config-isolation with the exact CLI (nearest child config shadows and does
#    not merge the sentinel ancestor); only then does it launch the exact target
#    CLI through the 056.022-T wrapper handoff (control without cwd, then
#    treatment with canonical project-root cwd; the byte-identical wrapper args
#    (--inner-exe/--inner-arg/--evidence-output/--run-nonce) encode the exact
#    absolute production inner executable plus original args), forward both
#    directions, keep raw frames in wrapper memory while writing only redacted
#    summaries/digests to the wrapper-owned --evidence-output, enforce a <=30s
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
#    exact-Copilot /mcp show sessions (one gate-off); each records ONLY the
#    CLI-visible connected/initialized status (and advertised tool list/count when
#    shown) with no OS error 232, correlating the production config hash/file
#    identity with the Copilot-spawned server startup event. The JSON-RPC wire
#    fields (jsonrpc 2.0, correlated id, no error, result.protocolVersion) are NOT
#    read from /mcp show; the direct 056.002-T read-only production 'server
#    control' proves them SEPARATELY (plus expected tools and one side-effect-free
#    get_status) against the same production binary/workspace. Provide the exact
#    recorded T0 invocation as a STRUCTURED argv array (never split a command
#    string on spaces, which loses quoting/argument boundaries):
# 056.004-T (T4) OWNS the concrete runtime-verification runner
# scripts/verify_copilot_mcp_show.ps1 (test-first; Ship implements it when
# 056.004-T lands — NOT implemented at plan time). The script invokes the exact
# STRUCTURED Copilot argv normally (no wrapper/temp config/substitution), captures
# CLI-visible output for each of the three sessions WITHOUT treating exit 0 alone
# as pass, FAILS on OS error 232 / failed-connection or a missing
# CONNECTED/INITIALIZED status for the recorded build, records advertised tools
# when shown, correlates each run with the production config hash/file identity and
# a NAMED non-substituting server-start evidence source (inherited stderr, the
# selected 056.006-T sink, or OS tracing; the one gate-off run may NOT rely on the
# sink), consumes 056.002-T's read-only production-control mode for the JSON-RPC
# id/protocolVersion/expected-tools/get_status wire evidence, and emits a redacted
# structured result. Immediate native exit checking is preserved.
$McpShowArgs = $env:GRAPHTOR_MCP_SHOW_INVOCATION | ConvertFrom-Json  # persisted argv array, e.g. ["mcp","show","graphtor-docs"]
& scripts/verify_copilot_mcp_show.ps1 -CopilotExe $CopilotExe -McpShowArgs $McpShowArgs -Sessions 3
if ($LASTEXITCODE -ne 0) { throw "T4 restored-production /mcp show verification failed (exit $LASTEXITCODE)" }
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
  workspace via exclusive creation validated by a probe-local, std-only
  `canonicalize`/`symlink_metadata` containment check (no `graphtor_core`
  import), generates the temporary
  control/treatment `.mcp.json` and the nested ancestor/child fixture only inside
  it, and T0 runs entirely inside it. `056.022-T` teardown reaps only via direct `Child` handles (the
  `sysinfo` adapter observes identity but never kills; same-second identity fails
  closed). The user `.mcp.json` is never read, mutated, backed up, or
  restored, so no config approval receipt is required, and isolated config
  creation plus owned-workspace cleanup need no approval; destructive cleanup
  beyond the owned workspace stays approval-gated. T2e (`056.009-T`, cwd/recovery)
  and T2h (`056.027-T`, discriminator) each consume 056.018-T's typed, no-follow
  contained recovery policy and, as destructive managed-`.mcp.json` mutations,
  run their existing-install refresh backup-first and **operator-approval-gated**
  before T4 (Principle VII). The
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
* **Phased release-unit structure (P-001/P-015/P-016, IX, X)** — the 28 tasks
  ship as phased release units, not one chain. **P-015**: the umbrella feature
  `056-F` is the protected covering feature, excluded from every task-only
  manifest (`049-S` and later remedy/acceptance units). **P-001**: exactly one
  release unit is in flight at a time; it does NOT perform intra-feature cause
  selection. **P-016**: a single active implementation branch/worktree; it does
  NOT perform selection either. Cause selection is the explicit
  `selection:pending → selection:selected` label + shipment-membership gate owned
  by Stage after `049-S` closes. **IX (Git-friendly persistence)**: backlog
  artifacts stay Markdown + YAML frontmatter and dependency edges are mutated only
  through backlogit operations. **X (context efficiency)**: the authoritative
  graph lives in this single `## Issue and Dependency Graph` section; superseded
  historical Plan Review DAGs are retained only as audit metadata.

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
  conditional on H0a/H3-B1, the discriminator remedy, and H0c)** — the
  existing-install managed-entry refresh (T2e/`056.009-T`, cwd/recovery, and
  T2h/`056.027-T`, `type`/`transport` discriminator) is an idempotent,
  marker-safe, backup-first config rewrite that preserves user-authored entries,
  is reversible by restoring the reported recovery file, and is
  **operator-approval-gated** before T4 (Principle VII). However, the
  **conditional H0c
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
  before running the bounded exact-CLI control/treatment pair(s) (one against
  the affected build, plus one against a last-known-stable build when
  available) inside it through
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
* ProposedAction (conditional, T2i safe no-follow primitive decision — `056.024-T`): decide an
  MSRV-1.75, safe-call-site, no-follow/capability-based file-mutation primitive
  for the sensitive recovery path without relaxing `#![forbid(unsafe_code)]`;
  compare a narrowly justified safe dependency against a std-only contract and
  prove the choice with a minimal PoC.
  * targets: a NEW `docs/decisions/YYYY-MM-DD-safe-no-follow-config-mutation-primitive.md`
    artifact (not the OS-232 deliberation) and a bounded PoC; no production
    `graphtor_core` edit (that is `056.018-T`).
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
  pinned `cwd` or merges ancestor config) uses `056.019-T` to choose **B1** (the
  same exact CLI passes a second contrast through a different documented
  isolated-config or working-directory mechanism, activating managed-config code/
  config mutation) or **B2** (that exact CLI supports no safe mechanism, blocking
  shipment with no repo code) — **no** server-side external-path fallback.
  * targets: **mode A** — `Cargo.toml` `[dependencies]` `rmcp` pin + rmcp
    `serve_server` / transport wiring in `src/main.rs` / `src/mcp/server.rs`;
    **mode B1** — the documented client-launch capability in `056.019-T` plus the
    managed-config mutation tasks (`056.008-T`/`056.018-T`/`056.009-T`); **mode
    B2** — the client-capability classification in `056.019-T` only.
  * change_kind: **mode A** compatible dependency/transport edit; **mode B1**
    managed-config code/config mutation with rollback; **mode B2**
    operator/client-capability classification only (no repo code).
  * ActionRisk: **moderate** — a mode-A rmcp bump can pull transitive API
    changes (`serve_server` signature, `schemars` re-export), re-verified in its
    own review with a separate before/after exact-client reacquisition through
    the `056.022-T` wrapper and
    `cargo +1.75.0 check --all-targets`; no `get_info` change. Mode B1 mutates
    managed config (rolled back via the recovery primitive); mode B2 changes no
    repo code and adds no containment surface. rollback: **mode A** revert the
    bump and re-pin rmcp 1.5; **mode B1** revert managed-config changes via the
    recovery backups; **mode B2** revert to the previously documented client
    configuration.
  * approval_required: mode B1 managed-config mutation follows the T2e/T2f
    approval path; otherwise no (non-destructive). ActionResult: **planned** (or
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
  A reacquires the exact-client transaction SEPARATELY before and after the fix
    through the `056.022-T` wrapper and `056.023-T` observer (semantic initialize
    correlation plus redacted transcript digest, not raw-frame replay) and closes
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
  transport; `056.023-T` owns the copy-only observer seam, in-wrapper JSON-RPC
  correlation, and redacted evidence;
  `056.022-T` owns process spawning, direct-handle teardown, and the versioned
  wrapper subcommand; `056.021-T` owns
  the isolated workspace and config fixtures;
  `056.003-T` owns the exhaustive typed preflight seam and `mcp_serve_ready`;
  `056.017-T` owns config outcomes; `056.008-T` owns generated-entry execution;
  `056.018-T` owns recovery containment; `056.009-T` owns upgrade integration;
  `056.016-T`/`056.007-T` own shared-lock characterization/implementation;
  and `056.014-T`/`056.005-T`/`056.025-T`/`056.015-T` own H1
  outcomes/lifecycle/projection/orchestration.
  No test proves an unrelated surface and no intentionally failing test remains
  after the selected fix.
* Existing MCP tests (`tests/mcp_manifest_test.rs`) must continue to pass.
  If H1 changes handler signatures, update server unit tests to equivalent
  async tests or preserve a typed sync-compatible adapter.

## Stage Readiness Evidence (durable — 2026-08-25)

This section is a **durable, in-repo Stage staging-review record** for shipment
`049-S`. It exists only so Ship can cross-check the eight-task manifest membership
and the plan-review disposition; it is **explicitly NOT a review-currency authority
and NOT a merge-readiness gate**. The **authoritative dynamic gate** is the live
PR #107 `## Local Review Readiness` current-HEAD record **plus Ship's mandatory
fresh current-HEAD local review** (GitHub PR automation §1.9) at the implementation
HEAD. **No commit SHA in this document is authoritative** — any SHA here is
historical evidence only and goes stale the moment new commits land (including this
edit). The earlier "PR #106 `## Local Review Readiness` block is the sole dynamic
authority" framing is superseded: PR #106 is merged and historical.

**Scope of this record:** Stage staging/planning readiness only. It does **not**
authorize Ship or merge and is **not** a Ship merge-readiness gate — Ship still
runs its own fresh current-HEAD local review before merging the eventual
implementation PR (per the GitHub PR automation §1.9 gate).

* **Staging branch:** `chore/stage-dark-security-pipeline`.
* **Stage-review snapshot (HISTORICAL — non-authoritative):** the manifest and
  review disposition below were last cross-checked at a Stage plan/backlog snapshot
  on 2026-08-25. A specific commit SHA is deliberately **not pinned here**: a pinned
  staging SHA goes stale on the next commit and must not be mistaken for a gate. The
  authoritative current HEAD is whatever the live PR #107 `## Local Review Readiness`
  record and Ship's fresh current-HEAD local review report at execution time.
* **Shipment `commit` field:** intentionally **empty/cleared**. The premature
  `commit: 642c3e4` (a Stage docs-reconciliation commit, not a 049-S
  implementation) was removed on 2026-08-25 with the rationale preserved in the
  `049-S` backlog log. Ship populates the `commit` field with the real
  implementation/merge commit at execution time.

**Eight-task manifest (verified queued 2026-08-25; covering feature `056-F`
excluded per P-015):**

| Task | Status | Role |
|---|---|---|
| `056.020-T` | queued | Build transparent MCP byte proxy |
| `056.022-T` | queued | Own probe process identities and teardown |
| `056.023-T` | queued | Non-interfering probe observation + evidence capture |
| `056.021-T` | queued | Isolated probe workspace and config fixtures |
| `056.001-T` | queued | Exact-CLI differential serve probe + cause ordering (T0) |
| `056.002-T` | queued | Out-of-process serve handshake test driver |
| `056.003-T` | queued | Harden `cmd_serve` diagnostics + preflight-complete event |
| `056.019-T` | queued | Conditional H3-B isolated-config/cwd adjudication (sole terminal) |

* **Covering feature `056-F`** (queued) is the P-015 protected covering feature
  and is **excluded** from the shipment manifest (verified present, not a member).
* Manifest is **unchanged** by this staging-review pass.

**Plan-review state (durable):** No unresolved P0/P1 in this plan for the `049-S`
release unit. The most recent Stage-run report-only reviews (2026-08-23 narrow
remediation and 2026-08-23 re-decomposition, recorded in `## Plan Review` below)
closed all consensus P1/P2 findings in-artifact; residual items are P3 advisories
carried for Ship execution. **Counts (durable snapshot): P0 = 0, P1 = 0, P2 = 0
(all prior P2 remediated), P3 = several advisories** (prefer targeted diagnostics
over broad logging; keep the standalone probe crate out of the production
dependency graph; preserve MSRV `cargo +1.75.0` on any dependency change). The
fail-closed H3-B rule stands: an inconclusive `056.019-T` verdict moves it to
`blocked` and blocks `049-S` closure pending a named bounded Stage follow-up.

**Ship verification checklist (from this durable record):**

1. Confirm the eight manifest members above are all present and `queued` (SQL:
   `SELECT id,status FROM items WHERE id IN (…)`); `056-F` must be excluded.
2. Run a fresh current-HEAD local review at the implementation HEAD and record it
   in the live PR #107 `## Local Review Readiness` block (do not reuse this
   historical staging snapshot or the frozen PR #106 body).
3. Populate the shipment `commit` field only from the real implementation/merge
   commit.

## Plan Review

**Current status: PR #106 is merged and historical; it is no longer a live
readiness authority.** The **authoritative dynamic gate** is the live PR #107
`## Local Review Readiness` current-HEAD record plus Ship's mandatory fresh
current-HEAD local review. The `## Stage Readiness Evidence (durable — 2026-08-25)`
section above is a manifest/disposition cross-check only — its eight-task manifest
and durable P0/P1/P2 counts are advisory staging evidence, **not** a
review-currency authority and **not** pinned to any commit SHA. This plan document
does not assert its own current review outcome and does not independently
authorize Ship or merge; Ship still runs a fresh current-HEAD local review before
merging the eventual implementation PR. The frozen PR #106 body is retained only
as historical evidence.

**Latest structure (2026-08-23 Stage re-decomposition):** the earlier single
forward-only chain is superseded by release-unit phases — PHASE 1 evidence
foundation (shipment `049-S`, task-only manifest of
`056.020`/`056.022`/`056.023`/`056.021`/`056.001`/`056.002`/`056.003`/`056.019`;
`056-F` excluded as protected covering feature per P-015), PHASE 2 unshipped
cause-family remediation units grouped by `phase:`/`cause:` labels, and PHASE 3
unshipped final acceptance/docs (`056.012`/`056.013`/`056.004`) — as defined in
the `## Issue and Dependency Graph` section above. The global "every selected
causal task re-probes" rule is replaced by the shipment-interface re-probe rule.
PR #106 is merged and staging/planning-only; its frozen
`## Local Review Readiness` block is historical and no longer a live authority.
The authoritative dynamic gate is the live PR #107 `## Local Review Readiness`
current-HEAD record plus Ship's mandatory fresh current-HEAD local review; the
`## Stage Readiness Evidence (durable — 2026-08-25)` section above is a
manifest/disposition cross-check only.

### Report-only review — 2026-08-23 narrow remediation (fresh cycle)

**Gate: ADVISORY** (report-only; the backlog already exists). Four reviewer
personas — Constitution Reviewer, Architecture Strategist, Correctness Reviewer,
and Agent-Native Parity Reviewer (multi-model) — reviewed the narrow remediation
(discriminator split `056.026`/`056.027`, corrected managed-config DAG, explicit
selection gate, fail-closed H3-B inconclusive, PHASE 3 assembly protocol,
evidence-cohesion + `056.001→056.002` edge removal, stale-string removal,
`056.002` read-only production control, `056.028` probe CI, `056.022` RAII
teardown, `056.020` portable fixture, task count 28).

**Consensus P1 findings — all remediated in this pass:**

* Verification Commands step 5 still demanded per-session JSON-RPC wire fields
  from `/mcp show` and split the invocation on spaces — reworded to CLI-visible
  status only (wire fields via the direct `056.002-T` control) and a structured
  argv array.
* The re-decomposition Plan Review "Remediated P2" bullet still called an
  inconclusive H3-B verdict a terminal classification that closes
  `056.019-T`/`049-S` — superseded with the fail-closed rule (inconclusive →
  `blocked` + named follow-up).
* `056.019-T`'s H3-B2 outcome mutated sibling managed-config task status,
  conflicting with the Stage-owned selection gate — reworded so `056.019-T`
  records its own classification/blocks only, and Stage's Final Assembly Protocol
  applies the `done`+`not-needed:H3-B2` disposition.
* The selection gate lacked a concrete enforceable check — added the required
  pre-shipment SQL gate (reject any candidate member lacking `selection:selected`
  or retaining `selection:pending`).
* `056.002-T`'s read-only production 'server control' was an unbounded assertion
  — specified an enforceable read-only boundary (ReadOnly/auto-discovered posture
  only, background sync/persistence disabled, DB opened read-only, fail on any
  write/lock); and the stdout-parity assertion (which needs `056.003`) was moved
  out of `056.002`'s completion gate into `056.003` + an `049-S` shipment-level
  control.

**Consensus P2 findings — remediated:**

* `056.027-T` (discriminator existing-install delivery) lacked the
  operator-approval gate its sibling `056.009-T` carries for the destructive
  managed-`.mcp.json` mutation — added, and enumerated in the Constitution Check
  VII + Plan Hardening destructive-step lists.
* `056.028-T` contradicted itself (implement the workflow vs "plan-only now") —
  reconciled: Stage creates the task now, Ship implements the CI job per the
  test-first contract; the stale duplicate description was removed.

**P3 advisories (accepted, non-blocking):** `056.008`/`056.026` share
`managed_server_value` at field granularity (co-selection ordering handles it;
per-field helper extraction is an implementation nicety); `056.012` fan-in
includes `056.009` whose docs surface `056.013` owns (transitively harmless —
docs ship as one PHASE 3 unit; `056.013` already depends on the discriminator
delivery `056.027`); `056.024` bundles a throwaway PoC with a decision doc
(explicit spike); `056.003` always-on diagnostics in the evidence unit is a
defensible non-conditional inclusion (Principle V, YAGNI-justified, carries its
own red/green). The selection gate now has a concrete required query but still
relies on Stage/Orchestrator running it (runtime enforcement is a
Ship/Orchestrator concern, out of Stage's planning scope).

**Verdict: no unresolved P0/P1** after in-pass remediation; residual items are P3
advisories. PR #106 is merged and historical; its `## Local Review Readiness`
block is no longer a live authority. The **authoritative dynamic gate** is the
live PR #107 `## Local Review Readiness` current-HEAD record plus Ship's mandatory
fresh current-HEAD local review; the `## Stage Readiness Evidence (durable —
2026-08-25)` section above is a manifest/disposition cross-check only, pinned to no
commit SHA. This plan does not restate a dynamic readiness value.

### Report-only review — 2026-08-23 Stage re-decomposition

**Gate: ADVISORY** (report-only; the backlog already exists, so this does not
gate harvest). Four reviewer personas — Constitution, Scope Boundary Auditor,
Architecture Strategist, and Agent-Native Parity — reviewed the re-decomposition.
**No P0 or P1 findings.** The phased graph is acyclic, evidence-first ordering is
intact, the P-015 covering-feature protection (`056-F` excluded from the `049-S`
manifest) is correct and required, and the no-kill test-only teardown split in
`056.020-T` is safe.

**Remediated P2 findings (fixed in this pass):**

* T4 (`056.004-T`) description and implementation-notes still conflated
  `/mcp show` with the JSON-RPC wire fields — reworded so the wire fields
  (`jsonrpc`/id/`result.protocolVersion`) are proven by the direct T1 driver
  (`056.002-T`), not by `/mcp show`.
* The T-H3-A plan section (`056.011-T`) stated "landing after `056.015-T`"
  unconditionally, contradicting the removed cross-family edge — qualified as a
  co-selection-only shipment-assembly edge; the standing edge is
  `056.003 → 056.011` only.
* H3-B closability (superseded 2026-08-23 by the narrow remediation —
  fail-closed): only a conclusive H3-B1 or a proven H3-B2 is a terminal evidence
  classification that closes `056.019-T` and lets `049-S` close (H3-B2's "blocks
  shipment" semantics apply to the downstream production-remediation units and T4,
  not the evidence unit). An INCONCLUSIVE verdict is NOT terminal: it moves
  `056.019-T` to `blocked`, blocks `049-S` closure, and requires a named new
  bounded Stage follow-up. (The earlier round wording that listed inconclusive as
  terminal-closing is superseded.)
* managed-config block-scope — `056.024-T` blocks only `056.018`/shipment, never
  `056.008-T`'s cwd generation or `056.026-T`'s unconditional `type`/`transport`
  discriminator reconciliation.
* Added an explicit `056.019-T ← 056.001-T` evidence-chain edge (mirrors T4's
  explicit-edge robustness; `056.019-T` reuses the `056.001-T` runner).

**Deferred P2 advisories (Ship-phase; recorded, not gate-blocking):**

* The T4 driver "server control" and the `/mcp show` sessions can contend on the
  Generation/Workspace advisory lock against the shared production workspace —
  sequence them with verified lock quiescence (no overlapping `serve`).
* Require the `/mcp show` advertised tool list to MATCH the direct-driver
  expected tool set as an explicit agent-visible ↔ server-registered parity check.
* Ground the "INITIALIZED" claim in T0's recorded exact `/mcp show` output; where
  the build has no distinct init label, treat the advertised tool list/count
  (which requires an initialized session) as mandatory init-completion evidence.
* `056.002-T` does not yet own an explicit read-only production-workspace
  "server control" mode that T4 depends on — enumerate it in its contract.
* The managed-config chain is modeled as a total order `017 → 024 → 008 → 018`;
  the true DAG is `017 → 008` in parallel with `017 → 024 → 018`. The settled
  chain is retained for sequential-execution simplicity, and the added 024
  block-scope clause resolves the block-scope conflict.

**P3 advisories (awareness):**

* The plan's `## Constitution Check` section still frames the work as a causal
  chain and lacks explicit P-001/P-015/P-016/IX/X mapping of the phased structure.
* `dependencies` frontmatter lists on `056.004-T` and `056.012-T` are not
  stable-sorted (backlogit appends), adding minor merge-diff noise (Principle IX).
* The plan retains superseded historical Plan Review DAGs and round narratives
  (Principle X context cost); consider pruning to a history artifact.
* Operational-only tasks (`056.010-T` H0c, `056.019-T` H3-B) green via runtime
  before/after transcripts, not a failing `cargo test`; the P-002/P-004
  red-phase waiver-with-rationale is documented in the decision's Constitution
  Check.
* `056.012-T`'s fan-in includes `056.009`/`056.011`, whose docs surfaces it does
  not own (transitively harmless; docs ship as one PHASE 3 unit).
* Require `056.012-T` operator-facing tool retry/fallback docs to mirror
  `056.025-T`'s exact Loading/Failed (no fallback) vs terminal-Disabled (keyword
  fallback) semantics.
* Cross-family sequential execution relies on runtime Orchestrator/Ship
  P-001/P-016 enforcement (by design); the acyclic graph alone permits two remedy
  families to become Ready simultaneously.

PR #106 is now MERGED and its `## Local Review Readiness` block is historical
(frozen), no longer a live authority. Consult the `## Stage Readiness Evidence
(durable — 2026-08-25)` section above for the current staging-review state; Ship
runs a fresh current-HEAD local review before merge.

The historical **correction round 2** holistic graph rewrite (2026-08-23): the probe becomes a
**standalone, non-published** `tools/mcp-probe/` crate split by width —
`056.020-T` core synchronous transport, the new `056.023-T` copy-only observer
seam plus in-memory evidence, `056.021-T` isolated workspace and config fixtures
(evidence.rs ownership moved out), `056.022-T` process spawning and
**direct-`Child`-handle** teardown (all `sysinfo`/PID kill fallbacks removed; the
adapter observes but never kills), and `056.001-T` a **one-shot** exact-CLI
classifier owning `exact_cli.rs`. The re-entrant causal loop is replaced by one
authoritative forward-only chain
`056.020 → 056.022 → 056.023 → 056.021 → 056.001 → 056.002 → 056.003 → 056.019 →
056.017 → 056.024 → 056.008 → 056.018 → 056.009 → 056.016 → 056.007 → 056.010 →
056.014 → 056.005 → 056.025 → 056.015 → 056.011 → 056.006 → 056.012 → 056.013 →
056.004`;
the new `056.024-T` decides a safe MSRV-1.75 no-follow config-mutation primitive
(or blocks `056.018`/shipment); `056.010-T` owns one H0c repair; `056.005-T`
narrows to the embedding lifecycle, the new `056.025-T` owns the MCP availability
projection, and `056.015-T` owns `cmd_serve`/background-sync orchestration;
`056.019-T` is the sole H3-B terminal adjudicating both isolated-config and cwd
mechanisms; T4
correlates the production config hash with the server startup event across three
`/mcp show` initialize sessions (advertised tools recorded as supporting
evidence; a direct driver confirms tools/`get_status` as a server control) and
gains direct evidence-chain edges to
`056.001-T`/`056.002-T`/`056.021-T`/`056.022-T`/`056.023-T`/`056.024-T`/`056.025-T`; and all
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
  (this file). **Historical branch caveat:** this reviewed-artifact-identity
  record was written on branch `chore/stage-049-S`, which is a HISTORICAL staging
  branch — do NOT read it as the current branch state. Current Stage work is on
  `chore/stage-dark-security-pipeline` (see the `## Stage Readiness Evidence
  (durable — 2026-08-25)` section). PR #106 is merged and its
  `## Local Review Readiness` block is historical/frozen, no longer a live
  authority; the authoritative dynamic gate is the live PR #107 `## Local Review
  Readiness` current-HEAD record plus Ship's mandatory fresh current-HEAD local
  review (the durable section is a manifest/disposition cross-check only). This
  plan asserts no static "latest reviewed HEAD/outcome" (such a claim would stale
  on every push). Prior review outcomes, with their historical SHAs, are recorded
  in the historical audit trail below.
* Linked deliberation: `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`.
* Backlog scope: feature `056-F`, tasks `056.001-T`..`056.028-T` (28 tasks).
  **PHASE 1 evidence shipment `049-S`** (eight-task task-only manifest; `056-F`
  excluded): standalone probe crate transport/teardown/observer-evidence/workspace
  `056.020-T`/`056.022-T`/`056.023-T`/`056.021-T`, exact-CLI T0 `056.001-T`,
  out-of-process driver `056.002-T`, T2 diagnostics `056.003-T`, and the sole
  H3-B terminal `056.019-T`. **PHASE 2 unshipped remediation families:**
  managed-config cwd/recovery `056.017-T`/`056.024-T`/`056.008-T`/`056.018-T`/`056.009-T`;
  type/transport discriminator `056.026-T` (generation)/`056.027-T` (existing-install
  delivery); H0b lock characterization/implementation `056.016-T`/`056.007-T`; H0c
  `056.010-T`; H1 lifecycle/resolver/projection/wiring
  `056.014-T`/`056.005-T`/`056.025-T`/`056.015-T`; H3-A `056.011-T`; optional
  diagnostic sink `056.006-T`; and the safe no-follow primitive decision
  `056.024-T`. **PHASE 3 unshipped final:** docs `056.012-T`/`056.013-T` and T4
  restored-production acceptance `056.004-T`. **PHASE 1.5 unshipped
  evidence-infrastructure unit:** the standalone-probe CI job `056.028-T`, which
  Stage assembles after `049-S` closes and before any remedy shipment (in T4
  fan-in, not cause-selected). The projection task `056.025-T` (versioned
  Loading/Failed/Disabled MCP availability projection) and the discriminator split
  (`056.026-T` generation / `056.027-T` delivery) are also explicitly in scope.
  This review identity covers all 28 tasks `056.001-T`..`056.028-T`.

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
  supersedes those blockers with the standalone probe crate and release-unit
  phases named in the current-status paragraph.
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
none of their task-ordering or dependency prose is normative. PR #106 is merged
and its `## Local Review Readiness` block is historical/frozen; the authoritative
dynamic gate is the live PR #107 `## Local Review Readiness` current-HEAD record
plus Ship's mandatory fresh current-HEAD local review (the `## Stage Readiness
Evidence (durable — 2026-08-25)` section above is a manifest/disposition
cross-check only).

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
