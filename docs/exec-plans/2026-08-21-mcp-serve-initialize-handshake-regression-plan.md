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
* `056.020-T` first self-tests a secure, owner-only, non-shipping transparent
  probe harness. T0 then records the exact newest failing CLI executable,
  version/build, and `/mcp show graphtor-docs` invocation through a minimal
  control/treatment pair. The wrapper performs concurrent full-duplex proxying,
  preserves inner launch identity and half-close semantics, restores config
  bytes or absence, and owns the isolated process tree. Direct replay confirms
  only evidence derived from that actual-client transcript.
* The evidenced branch restores connectivity without relaxing workspace
  containment, fail-closed validation, or verified-live lock ownership.
* Server startup failures are diagnosable even when the CLI discards child
  stderr (opt-in file-log sink or a documented redirect recipe).
* All quality gates pass: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit`, and `cargo build --release`.
  Any rmcp/dependency change also passes
  `cargo +1.75.0 check --all-targets`.
* Rollback and three successful `/mcp show graphtor-docs` starts on the exact
  T0 CLI identity and restored post-fix user-facing entry are documented.
  T0's wrapper or any executable substitution is invalid production evidence.

## Likely Surfaces (exact)

| Surface | Location | Change |
|---|---|---|
| Actual-client probe harness (T00, non-shipping) | feature-gated `tools/mcp-probe/main.rs` + owner-only builds under `logs/probe/<nonce>/` | Self-test duplex forwarding, handle-safe config restore, approval/redaction, and all-outcome cleanup. Default release targets exclude it → `056.020-T` |
| Serve startup diagnostics (T2, non-conditional, parity-safe) | `src/main.rs::cmd_serve`, duplicate-intake and database-open preflight | Route every pre-transport normal exit through an exhaustive typed seam, including pre-v4 and duplicate-intake exits. Preserve unconditional stderr and add structured events; emit `mcp_serve_ready` immediately before `serve_server` → `056.003-T` |
| Managed config outcome contract (conditional H0a/H3-B1) | `src/workspace/mcp_config.rs` | Distinguish typed create/update/no-change/collision outcomes from fail-closed `PathViolation`; forbid message sniffing → `056.017-T` |
| Managed MCP launch fields (T2d, conditional H0a/H3-B1) | `src/workspace/mcp_config.rs::managed_server_value` | Add only canonical project-root `cwd` and the evidenced stdio discriminator after T0/H3-B capability proof → `056.008-T` |
| Contained recovery primitive (conditional H0a/H3-B1) | bounded workspace recovery module + lazy accessor in `src/workspace/paths.rs` | Same verified no-follow handles for I/O, exclusive owner-protected artifacts, and exact restore; no managed-config/install/uninstall/doctor edits → `056.018-T` |
| Existing-install refresh (T2e, conditional H0a/H3-B1) | `src/main.rs::cmd_upgrade`, managed-config typed APIs | Refresh marked/exact-legacy entries and expose typed text/JSON action + recovery metadata; preserve collision/non-JSON bytes and Minimal footprint → `056.009-T` |
| Advisory lock characterization + implementation (conditional H0b) | `src/lock.rs` shared `AdvisoryLock` used by Database and Workspace locks | Passing characterization → `056.016-T`; one conservative high-resolution/boot-aware policy plus task-local red/green and legacy recovery → `056.007-T` |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if stderr is unavailable and CLI env inheritance works: unique exclusive absolute per-attempt sink consuming typed T2 events. No shared/relative sink or production-entry env field → `056.006-T` |
| H0c operational remediation (T2f, H0c-only — conditional) | evidenced fail-closed surface (registry / explicit `--config` / pre-v4 schema / duplicate-intake) + operational recipe | Repair one evidenced gate at a time with fresh approval and backup, retaining rollback through T4. Sequential H0c gates remain active; a newly exposed different branch is reclassified without discarding completed H0c recovery state. Pre-v4 rebuild uses `sync`, never `upgrade` → `056.010-T` |
| Embedding resolution outcomes (conditional H1) | `src/embed/resolver.rs` | Add typed `Loaded`/`Disabled`/`Failed` detailed result while preserving an adapter for unrelated callers → `056.014-T` |
| Shared lazy lifecycle (conditional H1) | `src/embed/lifecycle.rs` + MCP projection in `src/mcp/server.rs` | Supervised clone-shared state and versioned Loading/Failed/Disabled contract → `056.005-T` |
| Serve/background-sync lazy wiring (conditional H1) | `src/main.rs::cmd_serve`, `spawn_background_sync` | Inject one shared owner into MCP and Generation sync; neither eager load nor background sync may block initialize → `056.015-T` |
| Server transport compatibility (conditional H3-A) | rmcp pin + STDIO wiring | Task-local replay red/green and Rust 1.75 proof; T4 owns production acceptance → `056.011-T` |
| Client cwd compatibility (conditional H3-B) | actual Copilot CLI capability evidence | Separate B1/B2 adjudication from server transport; temporary proof only activates managed-config tasks and never satisfies T4 → `056.019-T` |
| Operator documentation (documentation-only) | two named sections each in `docs/troubleshooting.md` / `docs/cli-reference/graphtor-docs.md` or `docs/mcp-tools.md` / `docs/cli-reference/graphtor-docs.md` | Diagnostics plus selected H0b/H0c/H1 contracts → `056.012-T`; managed launch/recovery and H3 → `056.013-T` |
| Tests | `tests/common/mcp_driver.rs`, `tests/mcp_serve_handshake_test.rs`, and colocated focused tests | T1 owns the shared driver module; each production task owns at most three grouped scenarios. Actual-client acceptance remains the final H3/T4 gate |

## Task Breakdown (evidence-first, test-first, ~2h each, single-width)

### T00 — Secure transparent probe harness — backlog `056.020-T`

* Add a non-default feature-gated Cargo target at
  `tools/mcp-probe/main.rs`, named `graphtor-mcp-probe` and requiring
  `probe-harness`. Build it explicitly
  into `logs/probe/<nonce>/`; default release/all-target gates exclude it while
  dedicated Rust 1.75 gates cover it. The built executable/manifest are
  owner-only, ephemeral, never installed/committed, and invalid for T4.
* Self-test three groups before using the real CLI:
  1. independent concurrent pumps for client→child stdin and child→client
     stdout, continuous stderr drain, bounded buffers, client EOF→child stdin
     half-close, child stdout EOF→client close, and exit/deadline coordination;
  2. handle-level no-follow/reparse-safe open and same-handle I/O for every
     config/backup read/write/restore, identity revalidation before atomic
     replacement, exclusive owner-only backup, and exact restoration;
  3. argv/env/message redaction, owner-only capture, and complete all-outcome
     tree reaping, including two successful legs with no leaked process/lock.
* User-owned config substitution and raw frames require a caller-supplied,
  recorded operator approval receipt. The wrapper accepts
  the production entry's original args unchanged and never rewrites protocol
  bytes. No production source, managed config, or release artifact changes.

### T0 — Run the exact-CLI differential evidence probe — backlog `056.001-T`

* Record the **exact** newest failing Copilot executable path, version/build,
  and `/mcp show graphtor-docs` invocation. T4 must use the same identity.
* Use T00's validated harness for one same-build contrast pair from one
  controlled foreign directory. Both temporary entries are byte-equivalent to
  the user-facing production entry except `command = wrapper`; treatment alone
  adds canonical project-root `cwd`. No extra env, targets, `--db-path`, args,
  or alternate stdio discriminator.
* Record wrapper-entry cwd before mutation and inner-child spawn cwd
  separately; require the inner child to inherit the wrapper's CLI-assigned
  cwd. Preserve the unmodified bidirectional initialize transaction, stderr,
  exit/still-alive state, locks, and Generation posture.
* Causes are ordered, not mutually exclusive. Select H0a when treatment reaches
  healthy initialize **or** demonstrably advances past foreign-cwd discovery
  to a later gate/handshake stage; in the latter case retain H0a as a
  prerequisite and also select the newly exposed H0b/H0c/H1/H3-A cause.
  Iterate after each correction. If both legs remain foreign or the original
  field is rejected, select H3-B.
* Enforce the 30-second T00 deadline and fail if exact CLI identity, minimal
  entry parity, process ownership, or config restoration cannot be proved.
  Direct replay is confirmation only and cannot select the branch.
* Deliverable: correlated transcripts naming ordered proven prerequisites/
  causes or an explicit H3-B2 unsupported-client blocker. Preserve H3-A bytes.
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
* **Iterative verification:** each state mutation gets a fresh approval and
  backup. After it, rerun the same actual-client probe. If another H0c gate
  appears, keep `056.010-T` active and repeat. If a different branch appears,
  preserve the completed H0c remediation and rollback record, reclassify the
  remaining cause, and reactivate its owning task. If width is exceeded, create
  a bounded dependent follow-up and keep H0c blocked/active until that follow-up
  plus actual-client initialize succeeds. No invalid Cargo fixture remains.
* Contingency: when H0c is not evidenced, move the task to `done` with
  `not-needed: H0c not evidenced`. When H0c is selected, complete it only after
  the actual client negotiates initialize.

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
   behavior, and serialize retry transitions. A versioned availability
   projection defines stable Loading/Failed/Disabled code, retryability,
   remediation, and fallback metadata. Neither tool falls back while
   Loading/Failed; terminal Disabled permits `research_topic` keyword fallback
   only with explicit metadata. Exactly three groups cover concurrency,
   Disabled parity, and failure/panic/retry/Ready.
3. **Serve/background-sync wiring (`056.015-T`):** `cmd_serve` creates one owner
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
  negotiates a protocol version. Replay the exact unmodified bidirectional
  transaction red/green; a generic direct initialize is insufficient.
* Inspect candidate crate metadata before implementation. rmcp 1.8.x uses
  edition 2024 and is excluded by Rust 1.75. Prefer a minimal framing
  adaptation around pinned 1.5.x; use another release only after recording
  edition-2021/MSRV compatibility. If a fork/patch override or wider change is
  required, halt for a separate deliberation. T4 alone owns production-entry
  acceptance. No `get_info` echo change.
* If H3-A is not selected, complete done-plus-not-needed.

#### T-H3-B — Client cwd compatibility — backlog `056.019-T`

* Select only when the exact CLI ignores/rejects T0's original `cwd` field.
  **B1** requires that same executable to pass a second foreign-directory
  contrast using a different documented field placement or working-directory
  mechanism; it activates managed-config tasks but cannot satisfy T4.
  **B2** means the exact identity supports no safe mechanism: close managed
  tasks done-plus-not-needed and block shipment/T4 as `unsupported-client`.
  Switching CLI identity is not a fix for this acceptance contract.
* Record exact CLI executable path, version/build, invocation, and capability.
  Neither mode may add a server-side external-path fallback. T4 still requires
  three restored production-entry starts. If H3-B is not selected, complete
  done-plus-not-needed.

### T4 — Runtime verification, rollback, and closure evidence

* Verify with the exact T0 Copilot executable path/version/build:
  `/mcp show graphtor-docs` shows a healthy connected server with no OS error
  232; capture `mcp_serve_ready` separately as preflight evidence only.
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
  diagnostics — loud exit-2 errors + `mcp_serve_ready`, always lands) plus the
  **curative** fix tasks, each conditional and moved to **`done` with a
  `not-needed: <rationale>` log comment** when
  its hypothesis is not the evidenced cause: T2d launch-contract (H0a/H3-B1) =
  `056.008-T`, T2e existing-install migration (H0a/H3-B1) = `056.009-T`, T2b
  stale-lock harness/implementation (H0b) = `056.016-T`/`056.007-T`, T2c
  diagnosability = `056.006-T`, T2f H0c remediation = `056.010-T`, H1 resolver/
  lifecycle/orchestration = `056.014-T`/`056.005-T`/`056.015-T`, H3-A transport
  = `056.011-T`, H3-B compatibility = `056.019-T`, typed config/recovery =
  `056.017-T`/`056.018-T`, probe harness = `056.020-T`, and documentation-only
  tasks `056.012-T`/`056.013-T`. T0 may activate an **ordered sequence**:
  **H0a → 056.017 + 056.008 + 056.018 +
  056.009**; **H0b → 056.016 + 056.007**; **H0c → 056.010** with 056.006
  independently evidence-gated; **H1 → 056.014 + 056.005 + 056.015**;
  **H3-A → 056.011**; **H3-B1 → 056.019 + managed-config tasks**;
  **H3-B2 → 056.019 and BLOCKED**. A cwd correction that advances to a later
  blocker retains H0a and adds the new cause. The
  non-selected tasks complete with that explicit disposition, which
  **satisfies** T4's dependency on them — T4 does not wait for a conditional
  task that evidence ruled out. **The selected curative branch always includes a
  task sequence must produce a healthy deterministic/operational branch proof;
  T4 alone owns production-entry acceptance. H3-B2 is intentionally
  unsatisfiable and blocks rather than manufacturing a different CLI identity.
* Width: runtime verification + closure evidence.

## Verification Commands

```text
# Evidence capture (T0), through the actual target CLI:
$env:RUST_LOG = 'debug'
New-Item -ItemType Directory -Force logs
# Self-test 056.020-T's secure non-shipping wrapper, then run 056.001-T through
# exact `/mcp show graphtor-docs` target-CLI identity.
# Dedicated T00 target gate (default release/all-target commands exclude it):
cargo +1.75.0 check --features probe-harness --bin graphtor-mcp-probe
# Launch the same target CLI from one controlled foreign cwd. Run a control
# entry without cwd, then a treatment entry with canonical project-root cwd.
# Capture wrapper-entry + inner-server identity and bidirectional framing,
# propagate inner exit/pipe closure, wait <=30s, restore config on every
# outcome, and stop only the isolated owned process tree.
Get-ChildItem .graphtor -Filter *.lock
# Each T4 start uses the restored production entry and records exact CLI
# identity, production-config hash, timestamp, server PID, and capture path.
# T0's wrapper or any executable substitution is invalid T4 evidence.
# If 056.006-T is selected, set its env gate on the probe-owned CLI process.

# Quality gates:
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
# `mcp_serve_handshake_test` hosts the reusable open-stdin driver and any
# selected repository-code branch fixture. H0a uses it from 056.008-T's
# generated-entry test; H0b/H3-A use branch fixtures; H1 deterministic behavior
# is proven by 056.014-T/056.005-T/056.015-T. Operational-only H0c/H3-B use
# bounded before/after actual-client transcripts; no failing Cargo test remains.
cargo test --test mcp_serve_handshake_test
cargo test --all-targets
cargo audit
cargo build --release
# Conditional when rmcp/dependencies change:
cargo +1.75.0 check --all-targets

# Manual runtime check against the newest Copilot CLI:
#   /mcp show graphtor-docs   (expect: connected, no OS error 232)
```

## Rollback / Compatibility

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
* **VI Single Responsibility** — probe harness/run, diagnostics, typed config
  outcomes, generated fields, the narrow handle-safe recovery primitive,
  existing-install delivery, shared-lock
  characterization/implementation, diagnosability, H0c remediation, H1
  resolver/lifecycle/orchestration, H3-A transport, and H3-B capability are
  separate tasks and split from documentation-only tasks
  `056.012-T`/`056.013-T`; every
  **curative** task is evidence-gated (taken only if its hypothesis is
  evidenced); no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — T00 records explicit approval before
  substitution, validates every path component, creates an
  exclusive owner-only config backup, and restores exact bytes/absence before
  T0 can run. T2e consumes 056.018-T's typed, no-follow contained recovery
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

* ProposedAction (non-conditional, T00/T0 evidence transaction): after
  `056.020-T` proves duplex forwarding, secure artifact handling, and exact
  restoration, temporarily substitute its diagnostic wrapper entry, run the
  exact-CLI control/treatment pair, and restore exact original bytes/absence.
  * targets: project-root `.mcp.json`, probe-owned CLI/wrapper/server process
    tree, and `logs/` capture artifacts.
  * change_kind: temporary local config mutation plus external process launch.
  * ActionRisk: **moderate** — a crash can leave the diagnostic entry installed
    or leak processes, so the harness exclusively creates an owner-only backup,
    redacts sensitive values, and owns deadline/process-tree cleanup.
  * rollback: restore exact bytes or prior absence on every outcome from the
    validated contained backup; record manual recovery before mutation.
  * approval_required: yes before changing the user-owned config; record the
    approval receipt/decision in the T0 evidence record;
    ActionResult: **planned**.
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
* Per-width proofs remain separate: `056.020-T` owns the secure proxy;
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

**Current status: final authorized correction round 3, report-only gate
PENDING — NOT a PASS.** The exact-HEAD standard review of
**`41adf77f1767aaec1b7b588b03fb6ea41d2a67fc`** returned `BLOCKED`
after deduplication (`P0=1, P1=5, P2=15, P3=7`). Round 3 corrects the
convergent failing-suite handoff, layered-cause selection, H1 retry,
recovery-width/ownership, overlapping `cmd_serve`, and legacy-lock blockers,
plus tightly coupled P2 safety/actionability gaps. Findings based only on
excluded old memory/archive state remain discarded. This is the final
user-authorized correction round; its next committed HEAD requires a fresh
standard review.
Earlier review/remediation sections below are historical. PR
[#106](https://github.com/softwaresalt/graphtor-docs/pull/106) remains blocked;
no fresh PASS is claimed here.

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
  **BLOCKED**. **Review status of the round-3 corrected artifacts:** report-only
  gate **PENDING** against the next committed HEAD — explicitly **not** a PASS.
* Linked deliberation: `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`.
* Backlog scope: shipment `049-S` / feature `056-F`, tasks
  `056.001-T`..`056.020-T`: T0/T1 `056.001-T`/`056.002-T`; T2 diagnostics
  `056.003-T`; T4 `056.004-T`; H1 lifecycle `056.005-T`; diagnostic sink
  `056.006-T`; H0b implementation `056.007-T`; managed generation/delivery
  `056.008-T`/`056.009-T`; H0c `056.010-T`; H3-A `056.011-T`; docs
  `056.012-T`/`056.013-T`; typed resolver + serve wiring
  `056.014-T`/`056.015-T`; shared-lock harness `056.016-T`; typed config +
  recovery `056.017-T`/`056.018-T`; H3-B `056.019-T`; and secure T00
  `056.020-T`.

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
  deduplicated counts above. Round 3 remediates the blockers and coupled
  safety/actionability findings named in the current-status paragraph.
  A fresh current-HEAD review must establish `P0=0, P1=0`, followed by the
  mandatory adversarial re-review.
* **Consensus review (2026-08-21, HEAD `22d18f1`):** a 3-model adversarial
  consensus review produced a deduplicated remediation queue (F1/F2/F3/N1
  containment reversal; F4 status parity; F5 stale wording; F6 H3 owner; F7
  config schema; S1 migration primacy; S7 hardening; per-surface test-first),
  applied in "Consensus review remediation cycle 1 (2026-08-21)" below. A fresh
  current-HEAD report-only review is required to re-establish `P0=0, P1=0`.
* **Fresh-cycle P2 status:** the six consensus P2s were corrected or explicitly
  adjudicated in fresh correction cycle 1; validation is pending.
* **P3 / carried advisories: several**, recorded for Ship execution.

### Additional review-fix round 2 remediation (2026-08-22) — report-only gate PENDING

The exact-HEAD standard review of `dddcac33a1e0adae27ef34f0870e7d279676ba7f`
remained `BLOCKED`. This second user-authorized correction round merges only
findings grounded in the reviewed commit and current source:

* T00 is now a separately owned, non-shipping secure probe harness
  (`056.020-T`) with true full-duplex forwarding, bounded buffers, half-close
  propagation, continuous stderr drain, deadline/process-tree teardown,
  owner-protected exclusive artifacts, sensitive-data redaction, and
  exact-byte-or-absence `.mcp.json` restoration.
* T2 uses an exhaustive typed normal-exit seam, including pre-v4 and duplicate
  intake, and mirrors every typed event to unconditional fatal stderr. Tracing
  is additive because `RUST_LOG=off` can suppress tracing.
* H1 is split into typed resolver outcomes (`056.014-T`), one shared supervised
  lifecycle (`056.005-T`), and `cmd_serve`/Generation sync wiring
  (`056.015-T`); one owner now serves both MCP handlers and background sync.
* H0b is split into a shared Database/Workspace red harness (`056.016-T`) and
  implementation (`056.007-T`); only high-resolution process identity plus a
  boot/session discriminator is strong, and ambiguity remains locked.
* Managed launch/recovery is split into typed mutation outcomes
  (`056.017-T`), generated fields (`056.008-T`), contained recovery primitives
  (`056.018-T`), and upgrade orchestration (`056.009-T`).
* H3-A transport (`056.011-T`) and H3-B client capability (`056.019-T`) are
  independent. rmcp 1.8.x is excluded under Rust 1.75. Temporary H3-B
  capability evidence cannot satisfy T4.
* T4 accepts only three exact-CLI `/mcp show graphtor-docs` starts against the
  restored production command/args/cwd/env. The diagnostic wrapper,
  temporary config, executable substitution, wrapper PID, or wrapper-only logs
  are invalid production evidence.

The unusable learnings-review result and findings based only on explicitly
excluded old memory/stash files were discarded; they do not expand this
remediation queue.

**Historical round-2 DAG (superseded by round 3 below):** shipment `049-S` contains `056-F` and
`056.001-T`..`056.020-T`.

* `056.020 → 056.001 → 056.002`
* `056.002 → {056.003, 056.006, 056.007, 056.008, 056.010, 056.011,
  056.014, 056.016, 056.017, 056.019}`
* `056.014 → 056.005`; `056.005 + 056.014 → 056.015`
* `056.003 → 056.006`; `056.016 → 056.007`
* `056.001 + 056.002 → {056.011, 056.019}`
* `056.019 → 056.017`; `056.002 + 056.017 + 056.019 → 056.008`;
  `056.017 → 056.018`; `056.008 + 056.017 + 056.018 → 056.009`
* `056.003 + 056.007 + 056.010 + 056.015 → 056.012`
* `056.008 + 056.009 + 056.011 + 056.018 + 056.019 → 056.013`
* T4 `056.004` depends on `056.003` and every task `056.005`..`056.020`;
  evidence selection and all branch dispositions therefore complete first.

### Additional review-fix round 3 remediation (2026-08-22) — final authorized round

The exact-HEAD review of `41adf77f1767aaec1b7b588b03fb6ea41d2a67fc`
remained `BLOCKED`. This final user-authorized correction round:

* makes T1/T2b-characterization infrastructure finish green and assigns every
  red/green lifecycle atomically to its curative task;
* treats H0a as an ordered prerequisite when cwd correction exposes a later
  blocker rather than discarding proven evidence;
* gives H1 a retryable `Failed` state shared by MCP and Generation sync;
* narrows recovery to one handle-safe primitive and removes unrelated
  install/uninstall/doctor ownership;
* serializes `cmd_serve` ownership by placing T2 preflight before H1 wiring;
* applies one conservative lock policy to Database/Workspace locks and adds
  approval-gated recovery for evidenced live legacy pid-only locks;
* makes H3-B1 require a same-executable distinct documented mechanism and
  makes H3-B2 an explicit unsupported-client shipment blocker; and
* leaves actual-client production acceptance solely with T4, including target
  upgrade refresh and at least one diagnostic-gate-off start.

Out-of-scope findings against archived `054-F`/`055.*` artifacts and the
unusable workspace-inaccessible learnings pass were discarded.

**Authoritative round-3 DAG:** shipment `049-S` contains `056-F` and
`056.001-T`..`056.020-T`.

* `056.020 → 056.001 → 056.002`
* `056.002 → {056.003, 056.006, 056.007, 056.008, 056.010, 056.011,
  056.014, 056.016, 056.017, 056.019}`
* `056.002 + 056.014 → 056.005`;
  `056.003 + 056.005 + 056.014 → 056.015`
* `056.003 → 056.006`; `056.016 → 056.007`
* `056.001 + 056.002 → {056.011, 056.019}`
* `056.002 + 056.017 + 056.019 → 056.008`; `056.017 → 056.018`;
  `056.008 + 056.017 + 056.018 → 056.009`
* `056.003 + 056.007 + 056.010 + 056.015 → 056.012`
* `056.008 + 056.009 + 056.011 + 056.018 + 056.019 → 056.013`
* T4 `056.004` depends on `056.003` and every task `056.005`..`056.020`.

### Consensus P2 findings (historical; superseded by current T0-T4 contracts)

* **Containment must reuse the shared primitives, canonicalize both operands,
  and enumerate escape vectors** (Constitution, Learnings, Architecture,
  Security consensus). Resolved: T2, the Constitution Check III/IV entry, and
  the Plan Hardening invariant now delegate refusal to
  `graphtor_core::path::validate_path` / `is_reparse_point` (the same guard
  `src/workspace/serve_discovery.rs` uses), canonicalize both operands, and the
  refusal test enumerates absolute-above / `..`-traversal / escaping symlink /
  junction-reparse-point / Windows short-name-case vectors.
* **SUPERSEDED — do not implement:** **H0a fix is diagnostic-plus-operational**
  (Rust, Architecture consensus): replaced by the authoritative current T2
  diagnostics-only contract plus T2d canonical-cwd generation and T2e
  backup-first existing-install delivery. Do not generate target arguments.
* **T2 must cover every silent exit-2 discovery site** (Rust): resolved — T2
  now names all four exit-2 sites — missing `--config`, `served_paths` empty, `classified.postures` empty, and the structurally-unreachable `primary` None guard.
* **T2b lock-file format compatibility** (Rust, Learnings): resolved — a
  legacy start-time-less lock file degrades to pid-only rather than
  parse-erroring into a new fail-closed exit; atomic write-cleanup preserved;
  compatibility test required (`056.007-T`).
* **Gates-still-fail-closed regression assertion** (Security): resolved — T2
  now requires a regression assertion that each fail-closed gate still exits
  pre-serve after the cwd change.
* **Historical T3 OnceCell guidance (superseded):** the earlier DocServer-only
  instance state is replaced by the shared `src/embed/lifecycle.rs` owner,
  projected consistently to MCP handlers and Generation background sync.
* **Model resolution is `Ok(None)` graceful degrade, not a fail-closed gate**
  (Rust): resolved — noted in the deliberation so implementers do not add it to
  the pre-serve gate list.

### Carried P3 advisories (for Ship, non-blocking)

* If the conditional T2b/T2c are activated, each should carry its own colocated
  red test rather than relying solely on the out-of-process T1 harness.
* Assign the positive serve-ready startup log as an explicit T2 deliverable if
  it does not already exist.
* T1's primary red assertion is the bounded-timeout response-absence + captured
  child exit; the write-side broken-pipe error is a secondary signal (already
  reflected in the verification detail) — keep the primary/secondary ordering
  explicit at implementation time.
* **Historical sink-location guidance (superseded):** do not use a fixed
  `.graphtor/logs/` path. The current contract uses a unique absolute
  owner-protected path supplied by the explicit per-attempt diagnostic gate.
* If a conditional rmcp bump (H3) is ever taken, re-verify the `serve_server`
  signature and `schemars` re-export per
  `docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`.

### Notes on limitations acknowledged

* **Structured feature references:** backlogit's typed link/dependency
  operations act on backlogit items, not on `docs/exec-plans` /
  `docs/decisions` markdown, so the plan/deliberation cannot be a backlogit
  link endpoint. As the safe alternative, the backlog tasks carry `references:`
  to this plan, the feature `056-F` carries structured `references:` to **both**
  this plan and the deliberation (Cycle 3), and this plan and the deliberation
  carry an informational `backlog_refs` frontmatter field (`049-S`, `056-F`).
* **No implementation/test code on this staging PR is expected or a finding:**
  T0/T1/T2 and the conditionals are intentionally future Ship tasks; reviewers
  were instructed not to treat the absence of code as a finding.
* **Pre-existing MSRV note (out of scope):** the Rust 1.75 vs rmcp 1.5 /
  edition-2024 tooling context is a pre-existing residual and is not expanded by
  this shipment; any rmcp bump remains a conditional, separately-reviewed task.

### Cycle 2 remediation (targeted review-fix — not a fresh full persona run)

A second review-fix cycle addressed bot findings raised against an earlier HEAD
(`1cb6554`) that remained valid at the reviewed HEAD. These are targeted
documentation/backlog remediations by the Stage agent, **not** a new
seven-persona run; the Cycle 1 gate decision above stands.

* **Managed-launcher omission (H0a curative surface)** — the H0a "prefer an
  explicit target" remediation relied on the CLI passing `--db-path` / `--config`,
  but the managed `.mcp.json` generator (`src/workspace/mcp_config.rs::managed_server_value`)
  emits only `args: ["serve"]`, so no trusted workspace identity reaches the
  child under a changed cwd. Resolved: added `src/workspace/mcp_config.rs` as an
  evidence-gated (H0a-only) curative surface in the Likely Surfaces table, the
  T2 H0a scope note, the T2 Plan Hardening risky-action, and backlog `056.003-T`,
  with an unrelated-cwd launch regression test and containment delegated to the
  shared `validate_path` / `is_reparse_point` primitives (no parent traversal).
* **Observation-window specificity** — the window now names owner (merging
  developer), the pre-fix T0 baseline, the exact per-start invocation, the
  `logs/serve-stderr.log` signals, and explicit success/rollback triggers, in
  both this plan and `056.004-T`; no hosted observability is promised.
* **Pipe-direction wording** — the linked deliberation's Problem Frame now
  states 232 surfaces on the **client's** write to the server's stdin (read end
  gone because the server exited), consistent with H0.
* **Terminal-command hygiene** — the Verification Commands evidence recipe is
  split to one command per line and writes evidence under `logs/` rather than
  the repo root.
* **Task-title / single-width** — `056.003-T` no longer references the
  diagnosability sink in its title (the sink is isolated in `056.006-T`);
  T2 single-width and the stdin-open harness polarity remain as remediated in
  Cycle 1.

### Historical cycle 3 remediation (superseded)

A third and final review-fix cycle (hard cap) at HEAD `59e883a` addressed merged
high-confidence findings. These are targeted documentation/backlog remediations
by the Stage agent, verified against the actual source via ENGRAM_DIRECT graph /
search plus exact reads; **not** a new multi-persona run, and the Cycle 1 gate
decision above still stands (no fresh PASS is claimed).

* **Dependency coherence (P1, blocking)** — **[SUPERSEDED by Consensus review remediation cycle 1 (below): `056.003-T` is now NON-conditional cmd_serve diagnostics that always lands; the "conditional H0a-only" disposition in this bullet is retained for history only.]** `056.003-T` was unconditional but
  its acceptance is H0a-specific, so an H0b/H1 evidence branch made it
  impossible. Resolved: `056.003-T` (T2 cmd_serve) is now **explicitly
  conditional H0a-only** with a close-as-not-needed disposition; every causal
  branch has a satisfiable path (H0a → `056.003` + `056.008`; H0b → `056.007`;
  H1 → `056.005`; H0c → diagnosability/operational), and non-selected tasks
  close as *not-needed* to satisfy T4. No speculative H0a implementation forced.
* **Task width (P2)** — the managed launch-contract generation
  (`src/workspace/mcp_config.rs`) is split out of `056.003-T` into its own
  H0a-gated task **`056.008-T` (T2d)**; shipment membership, dependencies, plan,
  and Likely Surfaces updated. Each task stays single-width / ~2h.
* **Launch-identity contract (P2, correctness)** — verified via Engram
  (`run` / `cmd_serve` / `load_source_config` / `classify_serve_postures` /
  `spawn_background_sync`) that registry discovery, posture/Generation
  validation, DB auto-discovery, and background sync are **all** launch-cwd
  anchored. The contract is now non-ambiguous: **primary lever = pin the child
  `cwd` to the project root** (restores registry-backed Generation together);
  explicit `--db-path`/`--config` are a complement and do **not** alone restore
  Generation; the genuinely-absent-registry zero-config case is preserved.
* **Historical branch-sensitive evidence/baseline (P2; T1 ownership
  superseded)** — the branch signals remain: H0 = nonzero exit / marker / pipe
  close; H1 = bounded `initialize` timeout with the child alive. The selected
  curative task, not T1, owns the red/green lifecycle.
* **Plan Hardening Signals (P2)** — the public/contract-change signal is now
  **present (bounded, H0a)**, naming `src/workspace/mcp_config.rs` / the managed
  `.mcp.json` contract, with a new protected invariant (5): pinned `cwd` /
  `--db-path` / `--config` must resolve within project-root `.graphtor` after
  the shared containment checks, and pinning `cwd` must not relax the runtime
  boundary.
* **Structured `references:` on `056-F` (P3)** — the feature now carries
  `references:` to both this plan and the deliberation (direct frontmatter edit;
  index re-synced).
* **Speculative-logging adjudication (`056.006-T`, P2)** — retained, but
  tightened to be strictly evidence-gated on the T0 condition that the CLI
  discards child stderr (the `logs/` redirect being impossible/insufficient);
  closes as *not-needed* when the default `logs/serve-stderr.log` capture
  solves the evidenced case. Rationale recorded in the T2c plan bullet and the
  task.

### Stage correction session (2026-08-21) — report-only gate PENDING

A new **bounded Stage correction session** (operator-directed continuation
after the prior session hit its three-cycle review-fix cap; a fresh session with
a reset budget, **not** a hidden counter reset) corrected the six P1
plan-contract defects and the P3 advisory that a later report-only review
recorded against HEAD `4525cd0` (PR #106, readiness `BLOCKED`). These are
targeted Stage-owned plan/backlog corrections **grounded in the actual source
via `ENGRAM_DIRECT=1` code-graph/symbol lookups plus exact reads**; **no fresh
multi-persona review was run and no fresh PASS is claimed.** A fresh
current-HEAD report-only review must gate the corrected artifacts before Ship.

* **P1-1 red-test polarity** — the T1 harness's **sole pass assertion** is now a
  successful `initialize` response; the reproduced broken-pipe/exit/timeout is
  captured only as **diagnostic evidence** explaining the red, never the
  expected result. Updated: Likely-Surfaces Tests row, T1 section, Test-First
  Harness Expectations, `056.002-T`, and `056-F` DoD.
* **P1-2 H0a proof coupling** — the raw T1 transport harness greens the runtime
  `cmd_serve` fix (`056.003-T`) only; the generator (`056.008-T`) and the new
  existing-install delivery (`056.009-T`) each carry their **own** test-first
  proof (a managed-launch integration test that **executes the generated launch
  contract** from an unrelated cwd; a migration test that refreshes an existing
  managed entry). No single test proves an unrelated surface.
* **P1-3 existing-install migration** — Engram + exact reads confirm
  `generate_mcp_config` runs **only** from `cmd_install`/`cmd_install_full`
  (`src/main.rs` ~3258/~3360) and `cmd_upgrade` (~3480-3538) →
  `workspace::upgrade::upgrade` never rewrites `.mcp.json`. Added the tracked
  H0a delivery task **`056.009-T`** (idempotent, marker-safe refresh on
  `cmd_upgrade` and/or a required reinstall recipe) so the bug reporter's
  already-installed workspace is repairable, with a migration test.
* **P1-4 H0c closure** — added the tracked H0c operational-remediation task
  **`056.010-T`** (repairs the evidenced fail-closed workspace state — malformed
  registry / missing `--config` / pre-v4 schema / duplicate intake — and reaches
  a healthy handshake **without weakening any fail-closed gate**), wired into T4,
  with a matching H0c decision branch added to the deliberation. Non-selected
  H0c sub-causes close *not-needed*; the selected branch must reach T4.
* **P1-5 authorized-root / launch-cwd containment** (the split-root portion was
  **later reversed by Consensus cycle 1 — F1/F2/F3/N1**; see below) — removed the
  contradiction:
  the launch `cwd` is authorized by **equality to the canonicalized project
  root** (NOT constrained inside `.graphtor`), while `--db-path`/`--config` are
  validated as **project-root-derived** paths (typically under `.graphtor`), and
  an explicit target establishes its authorized root **from the target itself**,
  not a foreign launch cwd. Verified against the GitHub Copilot CLI/SDK MCP docs
  that the stdio contract **supports `cwd` and `env`** (locally the `backlogit`
  entry already uses `env`), so the primary `cwd`-pin lever is viable with an
  `env`/explicit-arg fallback; no parent traversal, nothing outside the project
  root. Updated T2, T2d, Constitution III/IV, and Plan Hardening invariant (5).
* **P1-6 live-lock age semantics** — a matching pid + process-start-time
  identity stays **live regardless of lock age**; `STALE_SECS` age evicts only
  as a fallback when strong identity is unavailable (legacy pid-only), with
  legacy compatibility and concurrent-release NotFound preserved. Confirmed via
  exact read that today's `is_stale_with_system` (`src/lock.rs` ~472-481) evicts
  by age even when the pid is alive. Updated the T2b surface/section and
  `056.007-T`, and Plan Hardening invariant (3).
* **P3 `056.003-T` title integrity** — re-scoped the title to
  workspace-root resolution only (no diagnostic-sink promise; that sink is owned
  by `056.006-T`).

**Superseded note:** the Cycle-3 log described an invariant (5) requiring the
pinned `cwd` to "resolve within project-root `.graphtor`", and the Stage
correction session replaced it with a **split-trust-root** invariant (5). Both
are now **superseded** by Consensus review remediation cycle 1 (below): the
runtime introduces **no target-derived/split authorized root** at all —
cmd_serve keeps validating explicit `--db-path`/`--config` against the
authorized project-root cwd via the shared primitives, and the T2d generated
`cwd` equals the canonical project root by equality (targets
project-root-derived). The Plan Hardening invariant (5) now reflects that final
state.

**Shipment / DAG (snapshot at the Stage correction session; superseded by
Consensus cycle 1 below):** `049-S` membership was `056-F` + `056.001-T`..
`056.010-T`; `056.008-T → 056.009-T`; T4 depended on `056.003`..`056.010`.

**Engram evidence (ENGRAM_DIRECT=1):** `engram symbols --prefix cmd_`
(cmd_install/cmd_install_full/cmd_upgrade), `engram map-code generate_mcp_config`
/ `managed_server_value`, `engram symbols --file src/lock.rs` +
`engram symbols --file src/workspace/upgrade.rs`, and `engram search` for the
managed launch-contract; each corroborated by exact reads of `src/main.rs`,
`src/lock.rs`, and `src/workspace/mcp_config.rs`.

### Consensus review remediation cycle 1 (2026-08-21) — report-only gate PENDING

A **3-model adversarial consensus review** at HEAD `22d18f1` produced a
deduplicated remediation queue that this cycle applies to the Stage-owned
plan/backlog artifacts. Corrections are **grounded in the actual source via
`ENGRAM_DIRECT=1` code-graph/symbol lookups plus exact reads**; **no fresh
multi-persona review was run and no fresh PASS is claimed.** A fresh
current-HEAD report-only review must gate the corrected artifacts before Ship.

* **F1/F2/F3/N1 — containment reversal (BLOCKING):** the earlier
  split-root / target-derived authorization (`workspace::paths::project_root` /
  `find_workspace_dir` on an explicit target) is **removed** — no target
  self-authorizes. Engram + exact reads confirm the runtime already validates an
  explicit `--db-path` against the **project-root** `candidate_root`
  (`discover_served_databases(scan_root = cwd/.graphtor, candidate_root = cwd,
  …)` in `src/workspace/serve_discovery.rs`; `cmd_serve` at `src/main.rs`
  ~2489-2499). H0a connectivity is owned by the managed launch contract
  `056.008-T` (generated config pins the child `cwd` to the canonical project
  root); the runtime `cmd_serve` continues validating explicit `--db-path` /
  `--config` against that authorized project-root cwd with the shared
  `validate_path` / `is_reparse_point` primitives, never parent-walking from a
  foreign launch cwd. `056.003-T` is retitled and re-scoped to **non-conditional
  diagnostics** (loud exit-2 errors + serve-ready log) that no longer claim to
  green a no-target wrong-cwd managed launch; its own serve-ready-log/loud-error
  test goes red→green from its production change. Updated: Likely Surfaces, T2,
  T2d, Constitution III/IV, Plan Hardening intro + invariants (1)(5), Risky
  actions (T2/T2d), `056.003-T`, `056.008-T`, `056.002-T` coupling, and the
  deliberation Decision step 3.
* **F4 — status/discovery parity (chosen scope):** because F1 removes any
  split-root signature change, the runtime discovery signature is **unchanged**,
  so `discover_served_databases`, `classify_serve_postures`,
  `discover_status_db_paths`, and `cmd_status` remain in parity with **no new
  test and no divergence**. Rationale recorded in T2 and `056.003-T`. (If a
  future runtime discovery signature change is ever taken, it must include those
  four surfaces + parity tests; this remediation deliberately avoids that.)
* **F5 — stale wording:** every remaining requirement that `cwd` must live
  inside project-root `.graphtor` or "cannot escape the original foreign launch
  cwd" is removed. Authoritative rule: the generated `cwd` equals the canonical
  project root; file targets are project-root-derived and validated against the
  project root; no external-path capability. Updated Rollback, Constitution
  III/IV, Plan Hardening, Risky actions, and `056.008-T`.
* **F6 — H3 branch (low confidence, kept live):** added the queued conditional
  owner **`056.011-T` (T-H3)** for an rmcp/client transport-framing
  compatibility fix, taken only if T0/T1 implicates H3 (child alive, no early
  exit, `initialize` never negotiates). Wired: depends on `056.002-T`, added to
  `049-S`, and T4 `056.004-T` depends on it; branch taxonomy, the T4
  baseline, and the deliberation H3 row updated so the branch reaches a healthy
  handshake.
* **F7 — config schema:** T0 `056.001-T` now records the exact target Copilot
  CLI MCP config schema (`type` vs `transport`; `cwd`/`env` support).
  `056.008-T` emits the evidenced supported field and preserves legacy
  recognition safely. Evidence (read-only): the local `.mcp.json` sibling
  entries (`backlogit`/`github`/`context7`/`tavily`) use `type: "stdio"` +
  `env`/`${workspaceFolder}` while `managed_server_value` emits
  `transport: "stdio"` — not claimed as the current root cause without T0
  evidence.
* **S1 — migration primacy:** `056.009-T` makes the marker-safe `cmd_upgrade`
  managed-entry refresh the **primary code acceptance** with an observed-red
  migration test; reinstall is an explicit **manual fallback/rollback** only,
  not an alternative that satisfies automated red/green. Updated T2e and
  `056.009-T`.
* **S7 — hardening gaps:** added ProposedAction / ActionRisk / rollback entries
  for T2e (`056.009-T`, existing-install managed-config refresh) and T2f
  (`056.010-T`, operational H0c repair — backups, no fail-closed weakening,
  operator approval for schema upgrade / registry replacement) in the Risky
  actions section and each task's Safety bullet.
* **Per-surface test-first:** explicit observed-red tests added/confirmed for
  `056.003-T` (serve-ready-log/loud-error), `056.006-T` (diagnostic sink if
  activated), `056.007-T` (pid-reuse / live-long-running / legacy-lock),
  `056.008-T` (generated-contract launch), `056.009-T` (upgrade migration), and
  `056.011-T` (H3 handshake). The T1 sole pass assertion stays a **successful
  `initialize`**; diagnostics explain the red only.
* **Preserved false-positive dispositions:** the stash journal is left as-is
  (the missing-journal claim was refuted by consensus); `056.008-T` keeps its
  dependency on `056.002-T` (the unnecessary-dependency claim was refuted); and
  the pre-existing `013.008-T` orphan, unrelated stale lock files, and
  pre-existing symlink-write backlog items are **out of 049-S scope** and left
  untouched.

**Shipment / DAG (historical; superseded by correction-cycle-2 DAG):** `049-S` membership is
`056-F` + `056.001-T`..`056.011-T`. Dependencies: `056.001-T → 056.002-T`;
`056.002-T →` {`056.003-T`, `056.005-T`, `056.006-T`, `056.007-T`, `056.008-T`,
`056.010-T`, `056.011-T`}; `056.008-T → 056.009-T`; and T4 `056.004-T` depends
on {`056.003`, `056.005`, `056.006`, `056.007`, `056.008`, `056.009`, `056.010`,
`056.011`}. `056.003-T` is **non-conditional** (cmd_serve diagnostics; always
lands); the curative branch tasks are evidence-gated (**H0a → 056.008 +
056.009**; **H0b → 056.007**; **H0c → 056.010**; **H1 → 056.005**; **H3 →
056.011**) and non-selected branches close *not-needed*. Each task stays
single-width / ~2h.

**Engram evidence (ENGRAM_DIRECT=1, this cycle):** `engram symbols --prefix
discover_` / `--prefix classify_` (located `discover_served_databases`
serve_discovery.rs:92-165, `classify_serve_postures` 263-325,
`discover_status_db_paths` main.rs:2664-2758); `engram map-code
discover_served_databases` / `classify_serve_postures` (confirmed
`candidate_root`=project-root validation of explicit `--db-path` and the shared
`validate_path`/`is_reparse_point` guards, grounding F1/F4); corroborated by
exact reads of `src/main.rs` (`cmd_serve` ~2446-2520, `discover_status_db_paths`
~2664-2720), `src/workspace/mcp_config.rs` (`managed_server_value` ~526-544,
`generate_mcp_config` / `is_exact_legacy_shape`), `Cargo.toml` (`rmcp = "1.5"`),
and read-only `.mcp.json` (F7 `type`/`env` sibling evidence).

### Consensus review remediation cycle 2 (2026-08-21) — report-only gate PENDING

A second fresh bounded Stage correction session applied a further report-only
remediation queue against the Consensus-cycle-1 artifacts. Corrections are
**grounded in the actual source via `ENGRAM_DIRECT=1` code-graph/symbol lookups
plus exact reads**; **no fresh multi-persona review was run and no fresh PASS is
claimed.** A fresh current-HEAD report-only review must gate the corrected
artifacts before Ship.

* **F8 (P1) — H0c pre-v4 remediation corrected to `sync` (not `upgrade`):**
  exact reads confirm `workspace::upgrade::upgrade` (`src/workspace/upgrade.rs`)
  only **replaces the `.graphtor/bin/` binary** ("Preserves config and data
  directories") and never rebuilds an index or touches schema, while both the
  serve gate (`open_serve_databases`) and the status gate
  (`load_status_databases`) themselves emit *"has pre-v4 schema; run
  `graphtor-docs sync` to rebuild the index"*. The pre-v4→v4 rebuild lives in the
  **sync** path (`src/sync/mod.rs::validate_and_apply_v4_migration` →
  `apply_v4_prune` → `prune_v4_data_for_rebuild` → `migrate_to_v4`). Fixed the
  H0c recipe to `graphtor-docs sync` in T2f, the Likely-Surfaces T2f row, the
  Risky-actions T2f entry (and its "schema upgrade" → "pre-v4→v4 schema rebuild"
  wording), and `056.010-T`; the binary/config `cmd_upgrade` refresh
  (T2e/`056.009-T`) is kept **distinct** and the fail-closed gate stays intact
  (state repair only, never a fail-open).
* **F9 (P1) — exit-site completeness (four sites, not "two + primary"):** exact
  reads of `cmd_serve` enumerate **four** distinct pre-transport exit-2 sites,
  named by semantic guard: (1) missing explicit `--config` (`config_override` →
  non-existent file); (2) `served_paths.is_empty()` after
  `discover_served_databases`; (3) `classified.postures.is_empty()` after
  `classify_serve_postures` + the phantom-default `retain` filter (a **second,
  distinct** "no databases found to serve" the prior text conflated with site
  2); (4) the structurally-unreachable `primary` None guard (`stores.next() ==
  None`). T2 and `056.003-T` now own all four with a **red diagnostic test per
  site** (four total) plus the serve-ready-log test, and preserve the
  no-discovery-signature / status-parity guarantee (F4).
* **F10 (P2) — false H0a env/arg fallback removed:** the claim that
  `GRAPHTOR_DB_PATH`/`GRAPHTOR_SOURCES` or explicit `--db-path`/`--config` can
  substitute when the target CLI ignores the managed `cwd` is removed from T2d
  and `056.008-T`. The runtime is **cwd-anchored by containment** (validates
  targets against `candidate_root = cwd`), so a foreign launch cwd cannot be
  re-authorized by a target; a CLI that lacks/ignores `cwd` is routed to H3
  (`056.011-T`) or an explicit operational **unsupported-client** path, never a
  fake curative fallback. The pinned-`cwd` primary lever and the
  within-project-root `--db-path`/`--config` complement (valid **only** because
  `cwd` is pinned) are unchanged.
* **F11 (P2) — verification consistency:** the Verification Commands now annotate
  that `cargo test --test mcp_serve_handshake_test` is the reusable T1 protocol
  harness (`056.002-T`) whose **success** scenario is greened by the selected
  curative branch; the H0a red/green proof is `056.008-T`'s generated-contract
  integration test, and `056.003-T` owns its **diagnostic** exit-site tests (it
  does **not** green the raw no-target wrong-cwd `initialize`). Updated the
  Verification Commands, `056.003-T` DoD/description, and T2.
* **F12 (P2, FALSE POSITIVE — DAG preserved) — conditional dependency gate:** the
  missing-conditional-dependency claim is refuted. The curative tasks
  (`056.008-T` et al.) depend on `056.002-T`, which depends on `056.001-T` (T0),
  so the **T0 evidence gate is transitively enforced**; no direct edge to T0 is
  needed. DAG left unchanged; the transitive gate is noted here for clarity only.
* **F13 (P2) — H3 pre-fix baseline added to the Plan Hardening observation
  window** to match T4/`056.004-T`: for an **H3** cause, the child **still
  alive** with the framed `initialize` never negotiating a `protocolVersion`
  (transport/framing, no early exit).
* **F14 (P2) — Cycle-3 "056.003 conditional H0a-only" marked superseded in
  place:** an inline SUPERSEDED marker now points readers to Consensus cycle 1,
  where `056.003-T` became NON-conditional diagnostics; the history is retained.
* **F15 (P2) — Plan Review current status de-HEAD-anchored:** the status line no
  longer hard-codes a HEAD; it names the **next committed HEAD** as the review
  target (this remediation is uncommitted at authoring time).
* **F16 (verified — stash journal) — PRESERVED, false-positive recorded:** a
  direct literal read confirms stash entry `7BF1961D` **is present exactly once**
  at line 51 of `.backlogit/archive/stash.jsonl`. The adversarial reviewer's
  presence claim is verified; the other reviewer's "absent" claim is the false
  positive. The journal is left as-is (no duplication, no backlogit repair).
* **F17 (P2 Rust guidance, non-blocking) — applied to task notes:** `056.008-T`
  compares **parsed `serde_json::Value`** (never raw serialized bytes/key-order)
  and preserves `is_exact_legacy_shape`; `056.007-T` emits a debug diagnostic on
  the legacy start-time-less lock parse fallback; `056.003-T` keeps the
  loud-message formatting DRY (one shared remediation-text helper) without scope
  creep.
* **Grounded accuracy fix (`acquire_database_lock`):** the prior claim "there is
  no `acquire_database_lock` symbol" is **false** — it is a real `src/main.rs`
  helper (~2803-2824) that validates the path and delegates to
  `workspace::lock::DatabaseLock::acquire`. Corrected in `056.001-T`,
  `056.003-T`, and `056.007-T`; the H0b liveness change still lands in the
  `src/lock.rs` primitives, not this wrapper.
* **Preserved false positives / refuted items (unchanged):** `056.008-T`'s
  dependency on `056.002-T`; no target self-authorization; no split-root
  helper/signature change; and the pre-existing `013.008-T` orphan, unrelated
  stale `.lock` files in the queue, and pre-existing symlink-write backlog items
  (all out of 049-S scope) are left untouched.

**Shipment / DAG (unchanged this cycle):** identical to the Consensus-cycle-1
authoritative DAG above — `049-S` = `056-F` + `056.001-T`..`056.011-T`;
`056.001-T → 056.002-T → {056.003, 056.005, 056.006, 056.007, 056.008, 056.010,
056.011}`; `056.008-T → 056.009-T`; T4 `056.004-T →` the eight fix/diagnostic
tasks. No membership or edge change.

**Engram evidence (ENGRAM_DIRECT=1, this cycle):** `engram symbols --prefix
cmd_` (located `cmd_serve` main.rs:2446-2654, `cmd_upgrade` 3480-3538, `cmd_sync`
441-601); `engram map-code cmd_upgrade` (→ `workspace::upgrade::upgrade`);
`engram map-code needs_v4_migration` / `validate_and_apply_v4_migration` (v4
rebuild owned by `src/sync/mod.rs` + `src/db/schema.rs::apply_v4_prune`); `engram
symbols --prefix acquire_` + `map-code acquire_database_lock` (`src/main.rs`
~2803-2824 wrapping `DatabaseLock::acquire`); `engram search` for the pre-v4 gate
message; each corroborated by exact reads of `src/main.rs` (`cmd_serve`
2446-2660, `open_serve_databases` 2370-2443, `load_status_databases`
2760-2801), `src/workspace/upgrade.rs`, and a literal read of
`.backlogit/archive/stash.jsonl:51`.

### Consensus review remediation cycle 3 (final) (2026-08-21) — report-only gate PENDING

A third and **final** fresh bounded Stage correction session (hard review-fix
cap) applied a further report-only remediation queue against the
Consensus-cycle-2 artifacts on branch `chore/stage-049-S` (reviewed input HEAD
`b6133ed`, unpushed). Corrections are **grounded in the actual source via
`ENGRAM_DIRECT=1` code-graph/symbol lookups plus exact reads**; **no fresh
multi-persona review was run and no fresh PASS is claimed.** A fresh
current-HEAD report-only review must gate the corrected artifacts before Ship.

* **P1-1 (BLOCKING) — H3 expanded to two-mode client/transport compatibility:**
  the prior text routed a **client that ignores/rejects the managed `cwd`** to
  H3 (`056.011-T`), but H3's discriminator assumed the child stays **alive**,
  whereas an ignored `cwd` makes the managed-launch child start in a **foreign
  cwd** and **early-exit** (child dead) — a contradiction. Resolved: `056.011-T`
  (T-H3) now owns **client/transport compatibility** with **two modes**, each
  reaching a healthy `initialize`: **mode A** (framing/version — child alive,
  `initialize` never negotiates → rmcp bump / minimal framing fix, observed-red
  handshake test) and **mode B** (client ignores/rejects the pinned `cwd` →
  managed-launch early exit → an **evidence-backed client-compatibility
  adjustment**: a supported CLI version or a client-honored working-directory
  mechanism, verified by a manual compatibility check — **no server-side
  external-path fallback**, containment unchanged F1/F2/F3/N1). **H3 is
  distinguished from H0a by generated-contract / client-capability evidence:**
  H0a = the CLI honors the pinned `cwd` (`056.008-T`'s pin cures it); H3 mode B
  = it ignores/rejects the pin. Updated: Likely Surfaces (T-H3 + Tests rows),
  the T-H3 section, the T2d "cwd ignored" routing note, T4 branch-sensitive
  baseline, the Plan Hardening observation-window baseline + rollback trigger,
  Risky actions (T-H3), Rollback / Compatibility, Constitution Check II/VI,
  Test-First Expectations, the deliberation H3 row / Decision / residual, and
  `056.011-T` / `056.008-T` / `056.001-T` / `056.004-T`.
* **Historical P1-2 — branch-sensitive T2c sink verification (location
  superseded by round 3):** when the T2c sink
  (`056.006-T`) is selected because the CLI **discards** child stderr, T4 cannot
  require `logs/serve-stderr.log` from a shell redirect (impossible on that
  branch). Resolved: T4 verification is **branch-sensitive** — stderr capture
  (`logs/serve-stderr.log`) for the normal branches, and **read/validate the
  configured unique absolute env-gated sink file for the T2c
  branch. Updated: T4 section, T2c section, the Plan Hardening observation window
  (method + signals), Verification Commands, and `056.004-T` / `056.006-T`.
* **P1-3 — `cmd_upgrade` canonical-project-root derivation:** grounded via
  Engram + exact reads — `find_workspace_dir` (`src/workspace/paths.rs:37-63`)
  returns the **`.graphtor` directory itself** ("Returns the path to the
  `.graphtor/` directory (not the project root)"), `project_root` is its
  `.parent()`, and `generate_mcp_config(project_root)` does
  `project_root.join(".mcp.json")` + validates the binary within `project_root`;
  `cmd_install`/`cmd_install_full` pass their `cwd` because install runs from the
  project root. Resolved: `056.009-T`'s `cmd_upgrade` refresh MUST pass the
  **canonical project root = the located `.graphtor` parent**
  (`find_workspace_dir(cwd).parent()` / `workspace::paths::project_root`, already
  canonicalized and reparse-guarded — no parent-walk beyond locating the
  workspace) — **not** the nested invocation `cwd` and **not** `.graphtor`
  itself — with a **nested-subdirectory invocation red test** and marker-safe
  user-entry preservation. Updated: Likely Surfaces (T2e row), the T2e section,
  Risky actions (T2e), and `056.009-T`.
* **SUPERSEDED — do not implement:** **P2 — raw-harness H0a success-list
  correction:** the Verification Commands
  comment claimed the raw `mcp_serve_handshake_test` SUCCESS scenario is greened
  by `H0a 056.008-T`; removed. The raw harness greens **H0b / H0c / H1 / H3 mode
  A**; the current contract instead uses the common T1 helper in
  `056.008-T`'s generated-entry integration test, bounded before/after evidence
  for operational-only H0c, and the T0 actual-client probe with explicit B1/B2
  disposition for H3 mode B.
* **P2 — H0c destructive / approval-gated acknowledged:** Constitution Check VII
  and the Plan Hardening Signals migration/destructive signal no longer read
  flat "none/absent"; they acknowledge the **conditional H0c** operational
  remediation (`056.010-T`) can require a **pre-v4→v4 schema rebuild via
  `graphtor-docs sync`** or a **source-registry replacement** — high-risk,
  approval-gated (operator approval + backup-first), never a fail-closed-gate
  weakening.
* **P2 — `056.003-T` table-driven diagnostic matrix (supersedes F9 per-site
  granularity):** the four exit-2 diagnostic tests + serve-ready-log scenario are
  consolidated into **one table-driven red diagnostic matrix** (four exit-2 cases
  + one serve-ready-log row), preserving every semantic site. The existing
  `tests/explicit_db_target_no_registry_test.rs` negative **"config file"**
  assertion (verified present: `serve` with an explicit `--db-path` and no
  registry must NOT emit "config file") is explicitly preserved while wording the
  new loud messages. Updated T2, Test-First Expectations, and `056.003-T`.
* **SUPERSEDED polarity note:** **P2 — `056.007-T` forward-compat lock test:**
  a lock file carrying an **unknown extra field** parses
  **without error** (forward-compatible), alongside the existing
  alongside the start-time / legacy / pid-reuse / live-long-running tests. The
  unknown-field and legacy cases are pre-change-green guards; pid reuse and the
  live-old-holder case are red anchors. A matching
  pid + start-time identity stays live **regardless of age**. Updated T2b and
  `056.007-T`.
* **P2 — `Cargo.toml` rmcp anchored by dependency name:** the T-H3 surface and
  Risky action reference `Cargo.toml` `[dependencies]` `rmcp` pin, not the
  brittle line ~44.
* **Preserved dispositions / false positives (unchanged):** the Cycle-3
  "056.003 conditional H0a-only" superseded marker (F14) is retained; the stash
  journal `7BF1961D` is present exactly once (`.backlogit/archive/stash.jsonl:51`)
  and is **not** duplicated (F16); `049-S` stays **frontmatter-only** (backlogit
  standard shipment format — no hand-woven body/items section); `056.008-T`
  keeps its parsed-`serde_json::Value` equality + `is_exact_legacy_shape`
  preservation and its `056.002-T` dependency; and the absence of implementation
  / evidence-pending task work is **not** treated as a review defect (staging is
  planning-only). The pre-existing `013.008-T` orphan, unrelated stale `.lock`
  files, and pre-existing symlink-write backlog items remain out of 049-S scope.

**Shipment / DAG (unchanged this cycle):** identical to the Consensus-cycle-1
authoritative DAG — `049-S` = `056-F` + `056.001-T`..`056.011-T`;
`056.001-T → 056.002-T → {056.003, 056.005, 056.006, 056.007, 056.008, 056.010,
056.011}`; `056.008-T → 056.009-T`; T4 `056.004-T →` the eight fix/diagnostic
tasks. No membership or edge change.

**Engram evidence (ENGRAM_DIRECT=1, this cycle):** `engram workspace-status`
(bound; 1307 files scanned; not stale); `engram symbols --prefix find_workspace`
(→ `find_workspace_dir` `src/workspace/paths.rs:37-63`); `engram map-code
find_workspace_dir` (→ `project_root`) and `engram map-code generate_mcp_config`
(callers `cmd_install`/`cmd_install_full`; project-root argument). Corroborated
by exact reads of `src/workspace/paths.rs` (`find_workspace_dir` returns the
`.graphtor` dir; `project_root` = its parent), `src/workspace/mcp_config.rs`
(`generate_mcp_config(project_root)` → `project_root.join(".mcp.json")`),
`src/main.rs` (`cmd_install` ~3258 / `cmd_install_full` ~3360 pass `cwd`;
`cmd_upgrade` ~3480-3538 resolves `workspace_dir = find_workspace_dir(cwd)`),
`Cargo.toml` (`rmcp` in `[dependencies]`), and
`tests/explicit_db_target_no_registry_test.rs` (the negative "config file"
assertion).

### Historical fresh correction cycle 1 (2026-08-21) — superseded

Standard and mandatory three-family adversarial report-only reviews of HEAD
`7b56a42` confirmed three HIGH-confidence P1 residuals and related P2
inconsistencies. This fresh bounded correction session applies one
reconciliation pass; it does not claim a review PASS.

* **Real-client cwd discriminator:** T0 now uses a temporary, backup-first
  diagnostic MCP entry invoked through the actual target Copilot CLI to record
  whether the configured `cwd` is honored. The current managed entry has no
  `cwd`, and a direct child spawn cannot prove client behavior.
* **H3 mode-B ownership:** `056.011-T` depends explicitly on `056.001-T`.
  Mode B consumes T0's real-client evidence and chooses B1 (keep
  `056.008-T`/`056.009-T`) or B2 (close them *not-needed*); it never treats a
  direct child spawn as client evidence.
* **Minimal H0a contract:** `056.008-T` adds only the canonical-project-root
  `cwd` and the evidenced stdio field. Generated `--db-path`, `--config`, and
  env target plumbing were removed as unnecessary coupling.
* **Test polarity and parser accuracy:** T1 supplies the common helper; the
  selected code branch owns its red/green proof, while external-only H0c/H3-B
  use bounded before/after evidence. The H0b
  reused-pid and live-old-holder tests remain the observed-red anchors; legacy
  and unknown-field lock cases are pre-change-green compatibility guards for
  the existing hand-written parser.
* **Satellite reconciliation:** the feature description marker and
  branch-neutral existing-install DoD, deliberation initialize polarity, and
  H0c approval text now match the authoritative plan.
* **Review identity:** frontmatter is `draft`; committed HEAD `13485d0` is the
  latest blocked reviewed input. The next committed HEAD requires a fresh
  current-HEAD report-only review.

**Authoritative correction-cycle-2 DAG:** `056.001-T → 056.002-T`;
`056.001-T + 056.002-T → 056.011-T`; `056.002-T → {056.003, 056.005,
056.007, 056.010}`; `056.003-T → 056.006-T`;
`056.002-T + 056.011-T → 056.008-T`; `056.008-T → 056.009-T`; T4
`056.004-T` depends on the eight fix/diagnostic tasks. Shipment membership is
unchanged.

**Engram evidence:** `ENGRAM_DIRECT=1 engram --workspace
C:\Source\GitHub\graphtor workspace-status` reported the branch bound and not
stale. Exact reads grounded the changed task, plan, decision, and feature
contracts.

### Historical fresh correction cycle 2 (2026-08-22) — superseded

Standard review of `13485d0` found that the prior common-harness contract could
not be greened mechanically on H0a or operational-only H0c, H3 mode B closed
the managed-cwd tasks too early, and several live sections still asserted
superseded behavior. This cycle:

* makes T1 a reusable driver with branch-owned red/green tests and bounded
  before/after actual-client evidence for external-only H0c/H3-B;
* historically split H3 mode B into B1/B2; this wording is superseded by
  round 3, where B1 requires the same exact CLI through a distinct documented
  mechanism and B2 is an unsupported-client shipment blocker;
* makes `mcp_serve_ready` a preflight-complete pre-`serve_server` event and
  requires three observed healthy starts before closure can be `healthy`;
* makes H1 tests deterministic through an injected loader seam and records the
  Tokio `sync`/Rust-1.75 dependency gate;
* sequences T2c after T2, covers all four diagnostics, and hardens sink,
  upgrade-refresh, lock-recovery, and fixture prerequisites.

The corrected artifacts are not yet reviewed. A fresh current-HEAD standard
report-only review is required; if P0/P1 clears, the mandatory three-family
adversarial re-review follows.

### Fresh three-round budget: correction round 1 (2026-08-22)

The operator authorized up to three more review-fix rounds. Exact-HEAD review
of `1bcadaa4213b9cc37c26c2bdd8f336af64e2c175` was `BLOCKED` with two
deduplicated P1 findings. Round 1 applies the following current contracts:

* T0 uses an actual-CLI foreign-directory control/treatment pair and records
  wrapper-entry versus inner-server identity before any mutation
* The wrapper preserves cwd/env/args, bidirectional framing, inner exit/pipe
  closure, and full isolated process-tree ownership
* Historical T0/T1 ownership (superseded): round 3 orders layered causes in T0,
  keeps T1 green, and assigns red/green proof to each selected curative task
* H0b legacy-live-old behavior is an observed-red safety anchor, not a
  pre-change-green parser guard
* H1 uses explicit clone-shared typed load states rather than a bare lazy cell
* H3-B1 capability proof uses the temporary contrast entry; production-entry
  verification remains in T4 after generation and delivery
* T4 verifies three starts against the restored user-facing entry and restores
  H0c state from recorded backups when rollback triggers
* Documentation is isolated from code in `056.012-T` and `056.013-T`

Round 1 does not claim a PASS. The next committed HEAD requires a fresh
exact-HEAD standard report-only review.
