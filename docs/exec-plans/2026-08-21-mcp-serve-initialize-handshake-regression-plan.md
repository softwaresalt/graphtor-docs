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
most plausibly one of `cmd_serve`'s four pre-`serve_server` exit-2 /
fail-closed paths (cwd-relative `.graphtor/*.db` discovery, lock contention /
stale-lock pid reuse, or a fail-closed config/schema gate), triggered by a
change in *how* the new CLI launches the child (cwd / env / lifecycle) rather
than by any graphtor-docs code change. The full differential diagnosis — and
why an rmcp `get_info` protocol-echo change is a no-op on rmcp 1.5 — is in the
linked deliberation. This plan restores connectivity **evidence-first** and
**test-first**.

## Goal / Definition of Done

* The newest Copilot CLI connects to `graphtor-docs` via `/mcp show
  graphtor-docs` with no OS error 232 and a completed `initialize` handshake.
* The failing cause is captured first through the actual target CLI with a
  bounded, cleanup-safe transparent wrapper. Direct replay is derived from
  that transcript. Repository-code branches have an observed-red proof that
  passes after the fix; external-only H0c/H3-B branches have bounded
  before/after actual-client evidence.
* The evidenced branch restores connectivity without relaxing workspace
  containment, fail-closed validation, or verified-live lock ownership.
* Server startup failures are diagnosable even when the CLI discards child
  stderr (opt-in file-log sink or a documented redirect recipe).
* All four quality gates pass: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit`.
* Rollback and a short post-fix observation window are documented.

## Likely Surfaces (exact)

| Surface | Location | Change |
|---|---|---|
| Serve startup diagnostics (T2, non-conditional, parity-safe) | `src/main.rs::cmd_serve` (~2446-2655) | Convert all four exit-2 messages to structured `tracing::error!` events naming the launch cwd/candidate root and remediation. Process-level red tests cover the three reachable guards; the structurally unreachable `primary` None guard reuses the formatter without an artificial control-flow injector. Emit `mcp_serve_ready` immediately before calling `serve_server`; it means preflight complete/about to call only, not loop entry or handshake success. Preserve containment/discovery signatures and fail-closed gates. Update troubleshooting and CLI-reference docs → `056.003-T` |
| Managed MCP launch contract (T2d, H0a/H3-B1 — conditional) | `src/workspace/mcp_config.rs` (`managed_server_value` ~526-544, `generate_mcp_config` ~138-227) | Emit canonical project-root `cwd` plus the evidenced stdio field only when H0a or H3-B1 is selected. Preserve marker/exact-legacy recognition and containment. A seeded managed-fixture test proves generation; actual-client support belongs to T0. Update `docs/mcp-tools.md` and, when activated, `docs/configuration.md`. Other branches complete the task as `done` with a `not-needed: <rationale>` comment → `056.008-T` |
| Existing-install migration / refresh (T2e, H0a/H3-B1 — conditional) | `src/main.rs::cmd_upgrade` (~3480-3538), `src/workspace/mcp_config.rs::generate_mcp_config`, `src/workspace/paths.rs::find_workspace_dir`/`project_root` | Whenever `056.008-T` is selected, marker-safely refresh existing installs from the canonical project root. Before a byte-changing owned-entry mutation, write and report a contained timestamped byte-for-byte backup; backup failure aborts config mutation. Preserve unowned/non-JSON bytes, make security/partial-upgrade failures explicit, test the migration matrix, and update CLI-reference docs. Reinstall is fallback only. Otherwise complete `done` with rationale → `056.009-T` |
| Advisory lock handling (T2b, conditional on H0b) | `src/lock.rs` (`DatabaseLock::acquire`, `AdvisoryLock::acquire`, `handle_existing_lock` ~166-201, `is_stale_with_system` ~472-481, `LockDetails{pid,timestamp}` ~21-24) | Record `start_time=<u64 epoch seconds>` with pid. Matching strong identity stays live regardless of age; a different start time proves pid reuse. A legacy pid-only lock with a currently live pid also stays locked because ownership is ambiguous. Automatic age fallback is limited to records without a usable live pid/strong identity; confirmed-dead pid is stale. Preserve parser compatibility and atomic cleanup → `056.007-T` |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if actual-client inherited stderr/wrapper capture is insufficient: env-gated contained sink consuming T2's normalized tracing events. It stays off by default, surfaces sink errors, never touches stdout, and supplies per-attempt correlated T4 evidence. Otherwise complete `done` with a not-needed rationale → `056.006-T` |
| H0c operational remediation (T2f, H0c-only — conditional) | evidenced fail-closed surface (registry / explicit `--config` / pre-v4 schema / duplicate-intake) + operational recipe | Repair one evidenced gate at a time with approval and backup, then rerun the same actual-client probe. If a second sequential H0c gate appears, keep the task active and repeat; do not close remaining remedies until initialize succeeds. Rescope before further mutation if width limits are exceeded. Pre-v4 rebuild uses `sync`, never binary-only `upgrade` → `056.010-T` |
| Embedding-model resolution (conditional) | `src/embed/resolver.rs`, consumers in `src/mcp/server.rs` | Only if H1 evidenced: clone-shared per-server lazy state such as `Arc<tokio::sync::OnceCell<_>>` + `spawn_blocking`, deterministic injected loader, one retry signal, and split-before-code width guard → `056.005-T` |
| MCP client/transport compatibility (T-H3, conditional on H3) | `Cargo.toml` `[dependencies]` `rmcp` pin + rmcp `serve_server`/transport-io wiring; plus the client launch-compatibility recipe hosted in `056.001-T` | **H3-A:** replay the raw target-CLI transaction captured by T0; a generic initialize is not a substitute. Any rmcp bump passes `cargo +1.75.0 check --all-targets`. **H3-B1:** supported CLI honors managed `cwd`; keep `056.008/009`. **H3-B2:** supported independent mechanism; complete those tasks `done` with rationale. Repeat the actual-client probe and `/mcp show`; no external-path fallback or `get_info` change → `056.011-T` |
| Tests | `tests/mcp_serve_handshake_test.rs` (new) + colocated integration tests | Test-first proofs, **one per production width**. T1 keeps stdin open, sends a valid newline-delimited `initialize`, and accepts only a successful response. Repository-code branches use the driver for observed-red/green proof: H0b, H1, and H3-A. H0a/H3-B1 use it through the managed generated contract; H0c/H3-B use bounded before/after actual-client evidence. Exit/stderr or a still-alive timeout are diagnostics only. `056.009-T` owns existing-install delivery, and `056.003-T` owns its diagnostic matrix. Existing MCP tests continue to pass |

## Task Breakdown (evidence-first, test-first, ~2h each, single-width)

### T0 — Capture the failure evidence (investigate-first, ~30-60 min)

* Invoke the **actual target Copilot CLI** against a backup-first temporary
  diagnostic entry whose transparent wrapper proxies stdio byte-for-byte while
  recording argv, actual cwd, allowlisted/redacted env, CLI/server pid when
  available, raw initialize framing, exit, and stderr under `logs/`. This
  actual-client transcript is the source of truth; a direct server replay may
  only be derived from it.
* The probe runner owns the target CLI process, enforces a 30-second deadline,
  cleans up only its isolated process tree, and restores the original config
  byte-for-byte on success, failure, and timeout. Check for a leftover
  `.graphtor/*.lock`; compare a prior known-good CLI build if available.
* **Record the exact Copilot CLI MCP config schema (F7):** whether the stdio
  server entry uses `type` vs `transport`, whether it supports `cwd`, and how it
  handles `env`. The minimal `056.008-T` contract emits only the evidenced stdio
  discriminator and `cwd`; env behavior is recorded for diagnosis but no target
  env plumbing is added. (The local `.mcp.json` sibling entries already use
  `type: "stdio"` + `env`/`${workspaceFolder}`; T0 confirms what the specific
  CLI build honors; this schema is not asserted as the root cause without
  evidence.)
* **Prove real-client `cwd` capability (H0a vs H3 mode B):** the same bounded
  probe uses a known canonical `cwd` and records the actual current directory.
  A direct `Command::current_dir` spawn is not client evidence.
  If the real CLI honors the field, H0a may select `056.008-T`; if it
  ignores/rejects the field, route to H3 mode B. H3-B1 (a supported CLI version
  that honors managed `cwd`) still activates `056.008-T` and `056.009-T`;
  H3-B2 (a different supported working-directory mechanism) completes them as
  `done` with `not-needed: H3-B2 selected` comments.
* **Prove H0b is reachable before selecting it:** database locks are acquired
  only for a registry target classified as `ServeMode::Generation`.
  ReadOnly/auto-discovered targets do not reach `acquire_database_lock`, so a
  leftover lock file by itself is not H0b evidence.
* T0/`056.001-T` also hosts the **operational recipes** referenced by later
  tasks: the H0c workspace-state remediation recipe (`056.010-T`) and the manual
  reinstall fallback recipe (`056.009-T`).
* Deliverable: a correlated actual-client transcript that names the H0
  sub-cause or rules H0 out and points at H1/H3. Preserve a failing H3-A raw
  transaction for replay. A nonzero early exit settles H0 only when tied to the
  target CLI launch.
* Width: evidence capture only; no code change.

### T1 — Out-of-process regression harness (red)

* Add `tests/mcp_serve_handshake_test.rs` that **spawns the real binary** with a
  controllable cwd/env and a fixture workspace reproducing the T0 sub-cause,
  and drives a real STDIO client turn:
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
  * **The harness's ONLY pass assertion is a successful `initialize` response**
    with a negotiated `protocolVersion`, awaited under a short, fixed bounded
    deadline. It is **red before** the evidenced fix because that response
    never arrives, and **green after** because it does. The reproduced
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
* Deliverable: the reusable driver plus either a branch-owned red test whose
  sole pass assertion is a real `initialize` response, or a bounded
  actual-client before transcript for an external-only repair.
* **Branch-appropriate proof ownership:** T1 supplies the reusable spawn +
  `initialize` + timeout + diagnostic driver. Repository-code branches commit a
  red test that the selected curative task greens: H0a uses that driver in
  `056.008-T`'s generated-entry test (pre-change entry has no `cwd`, so launch
  from the unrelated parent is red); H0b uses a reachable Generation-lock
  fixture; H3 mode A uses a framing-pinned fixture; H1 uses `056.005-T`'s
  deterministic loader seam plus the runtime transcript. Operational-only H0c
  and H3 mode B use the same bounded actual-client probe before and after the
  approved state/client repair rather than leaving an unsatisfiable Cargo test.
  H3 mode A replays the raw client transaction captured by T0; a generic valid
  initialize request is not an adequate framing regression.
  Any H0c actionability code change receives its own red test. No intentionally
  failing test remains in `cargo test`.
* Width: test infrastructure only.

### T2 — cmd_serve pre-serve diagnostics (green, non-conditional, parity-safe)

* **Non-conditional.** This task delivers runtime-owned observability that is
  valuable on **every** causal branch and whose own test goes red before /
  green after its production change. It does **not** own curative H0a
  connectivity and **no longer claims** it can green a no-target wrong-cwd
  managed launch — H0a connectivity is owned by the pinned-cwd launch contract
  **T2d (`056.008-T`)** plus existing-install delivery **T2e (`056.009-T`)**.
* Runtime-owned diagnostics (Constitution V):
  * convert **every** silent pre-transport exit-2 site into a structured
    `tracing::error!` event with a loud, actionable message
    error that names the actual launch cwd / authorized candidate root and the
    remediation (launch from or pin the project root, drop a `.db` into
    `.graphtor`, or pass an already-supported explicit target), so no discovery
    path exits silently. There are
    **four** distinct exit-2 sites, named by their semantic guard (not brittle
    line numbers):
    1. **missing explicit `--config`** — `config_override` points at a file
       that does not exist (`eprintln!("error: config file '...' not found")`);
    2. **`served_paths` empty** — the post-`discover_served_databases` union is
       empty ("no databases found to serve; drop a `.db` ...");
    3. **`classified.postures` empty** — after `classify_serve_postures` and the
       phantom-default `retain` filter drops non-existent `ReadOnly` candidates,
       the classified set is empty (a **second, distinct** "no databases found
       to serve" site the earlier text conflated with site 2);
    4. **`primary` None** — after `open_serve_databases`, `stores.next()` yields
       no primary read-only store (the structurally-unreachable defence-in-depth
       guard, "no databases found to serve").
    The malformed-registry `Err`, duplicate-intake preflight,
    `open_serve_databases` open failure, and pre-v4 gate stay **separate**
    fail-closed paths asserted below;
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
  gates) — add a regression assertion that each still exits pre-serve after this
  change, so diagnostics never silently convert a fail-closed gate into a
  fail-open path.
* **Own observed-red tests (parity-safe, from this task's production change):**
  one process-level row for each of the three reachable exit-2 sites above.
  Keep the structurally unreachable primary-None guard on the same formatter,
  but do not add an artificial control-flow injector solely to reach it; use a
  focused formatter/unit assertion if its text changes. Each row asserts the
  structured event and actionable message. Add a ready-event row that starts a seeded
  fixture with stdin open, sends no initialize, observes `mcp_serve_ready`, and
  cleans up only its owned child; all green after.
  These tests assert the **diagnostic output** (message + serve-ready log),
  **not** loop entry or a successful `initialize` handshake — the raw no-target wrong-cwd
  `initialize` success is **not** this task's to green (it is owned by the H0a
  generated-contract test `056.008-T` and, for the other branches, the T1
  harness). Reuses the T1 (`056.002-T`) transport-harness spawn/capture
  scaffolding for out-of-process launch + stderr capture.
* Do **not** add the optional file-log sink (that is T2c/`056.006-T`).
* Update `docs/troubleshooting.md` and
  `docs/cli-reference/graphtor-docs.md` with the new diagnostics and
  preflight-event semantics.
* Width: serve startup runtime diagnostics only. Curative H0a launch-contract
  generation (T2d), existing-install delivery (T2e), stale-lock liveness (H0b),
  the diagnosability sink, H0c operational remediation, H1 model lazy-load, and
  the H3 transport fix are **separate** tasks below.

#### T2d — (Conditional on H0a/H3-B1) Managed launch-contract generation — backlog `056.008-T`

* Only if T0 evidences H0a and the target build honors `cwd`, or if H3-B1
  selects a supported CLI version that honors managed `cwd`. If the current
  build ignores/rejects the field, `056.011-T` first chooses B1 (activate this
  task) or B2 (move it to `done` with `not-needed: H3-B2 selected`).
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
* **Test (test-first proof for this width):** a managed-launch integration test
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
* Update `docs/mcp-tools.md` to match the selected stdio field and, when this
  branch activates, document managed project-root cwd behavior in
  `docs/configuration.md`.
* Contingency: move to `done` with a `not-needed: <rationale>` backlog comment
  when neither H0a nor H3-B1 selects managed `cwd`. `not-needed` is a
  disposition, not a backlog status. Width: managed launch-config generation
  only.

#### T2e — (Conditional on H0a/H3-B1) Deliver the launch contract to existing installs — backlog `056.009-T`

* Only when T2d/`056.008-T` is selected (H0a or H3-B1). If T2d closes,
  T2e closes too, including H3-B2. **Distinct width from T2d:** T2d changes the
  generated value; this task **delivers** the refreshed managed entry to
  **already-installed** workspaces. Verified via Engram + exact reads:
  `generate_mcp_config` is invoked **only** from `cmd_install` (`src/main.rs`
  ~3258) and `cmd_install_full` (~3360); `cmd_upgrade` (~3480-3538) calls
  `workspace::upgrade::upgrade`, which never rewrites `.mcp.json`. So a binary
  upgrade leaves the bug reporter's existing managed entry **stale** and
  un-repaired (Copilot review P1: existing-install migration).
* **Primary code acceptance (S1):** wire the idempotent, marker-safe
  `generate_mcp_config` refresh into `cmd_upgrade` so `graphtor-docs upgrade`
  refreshes the managed entry in place. Safe because the generator's four-way
  decision only touches the **marked** managed entry (or the exact pre-marker
  legacy shape) and never clobbers a user-authored `graphtor-docs` entry. The
  upgrade orchestration preserves unowned/non-JSON bytes and turns the current
  collision result into an actionable warning/manual path rather than an
  opaque binary-upgrade failure. This automatic refresh, proven by an
  observed-red migration matrix, is the acceptance for this task.
* **Reinstall is a manual fallback/rollback only:** a required `graphtor-docs
  install`/reinstall recipe documented in T0/`056.001-T` with a verification
  step, for when the automatic upgrade refresh is judged unsafe or must be
  reverted. It does **not** substitute for the automated red/green migration
  test.
* **Refresh outcomes:** marked/exact-legacy entries refresh. Unowned or
  non-JSON config is preserved byte-for-byte with an actionable warning/manual
  reinstall path and does not turn an otherwise successful binary upgrade into
  an opaque failure. Reparse/containment failures remain explicit security
  errors and fail before binary replacement where possible; otherwise report
  partial-upgrade state rather than silently succeeding.
* **Backup-first mutation:** when a refresh will change owned entry bytes,
  atomically persist the original `.mcp.json` bytes to a contained timestamped
  recovery file and report its path before mutation. Backup failure aborts the
  config mutation. An idempotent no-change run creates no backup.
* **Test (observed-red migration matrix, this width's code acceptance):** an
  existing marked (or exact legacy-shape) managed entry is **updated to the new
  launch contract** after `cmd_upgrade` runs, and a co-resident user-authored
  server entry is preserved byte-for-byte. Include nested-subdirectory
  invocation (only project-root `.mcp.json` changes), unowned/non-JSON
  preservation with warning, backup failure, and explicit fail-closed
  reparse/containment behavior. Red before the `cmd_upgrade` refresh change,
  green after. Update `docs/cli-reference/graphtor-docs.md` with refresh,
  recovery path, and reinstall fallback semantics.
* Contingency: move to `done` with a `not-needed: 056.008-T not selected`
  comment whenever `056.008-T` closes.
  Width: install/upgrade delivery of the managed entry only.

#### T2b — (Conditional on H0b evidence) Harden stale-lock liveness — backlog `056.007-T`

* Only if T0/T1 evidences lock contention / stale-lock **pid reuse**, and proves
  the target is classified as `ServeMode::Generation` (ReadOnly/auto-discovered
  targets never acquire this database lock): in `src/lock.rs` (`DatabaseLock::acquire` /
  `AdvisoryLock::acquire` / `handle_existing_lock` / `is_stale_with_system`),
  record process start-time alongside pid so a reused pid is not misread as a
  live lock holder. **Identity over age (Copilot review P1):** a matching
  pid + process-start-time identity is treated as a **live** holder
  **regardless of lock age**. Encode the field portably as
  `start_time=<u64 epoch seconds>`. A legacy pid-only lock whose pid is
  currently live also remains locked regardless of age because ownership is
  ambiguous. The `STALE_SECS` age fallback is limited to records without a
  usable live pid/strong identity; a confirmed-dead pid is stale. Do **not**
  let age alone evict a possibly live holder: today `is_stale_with_system` falls through to the age
  check even when the recorded pid is alive, so a long-running live server can
  be evicted purely by age. Prefer start-time+pid over a `--force` escape hatch.
* **Lock-file format compatibility (required, both directions):** a lock file
  written by a prior binary (no start-time field) must degrade to the current
  pid-only liveness check, **never** parse-error into `GraphtorError::Config` —
  a parse failure would itself become a new pre-serve fail-closed exit (a fresh
  232). Symmetrically, a lock file carrying an **unknown extra field** (as a
  future binary might add) must also parse **without error** (unknown fields
  ignored — forward-compatible), never a hard fail. Preserve the existing atomic
  write-cleanup and concurrent-release NotFound-retry behavior. Add
  observed-red tests: (a) the reused-pid staleness case; and (b) a genuinely
  live long-running holder older than `STALE_SECS` staying live (matching pid +
  start-time identity is live regardless of age). Add pre-change-green
  compatibility characterization guards for (c) a live legacy
  start-time-less lock older than `STALE_SECS` remaining locked and
  degrading to pid-only liveness without a parse-error and (d) an
  unknown-extra-field lock file parsing without error; the existing hand-written
  line parser already tolerates both, so they are not red anchors.
* A verified live-but-hung holder is never age-evicted. Recovery is an explicit
  operator action: identify and terminate the matching process, then retry.
  Do not add an unauthenticated force-eviction path.
* Contingency: if H0b is not evidenced, move the task to `done` and append
  `not-needed: H0b not evidenced`. Width: lock liveness only.

#### T2c — (Conditional/optional) Startup diagnosability sink — backlog `056.006-T`

* Depends on `056.003-T` so it consumes the normalized diagnostics rather than
  racing edits to the same early-exit sites.
* Default: rely on inherited target-CLI stderr or T0's transparent wrapper
  capture. Only if actual-client capture remains unavailable because the CLI
  discards child stderr, add an env-gated opt-in sink.
* If built, it MUST capture all four normalized pre-transport diagnostics,
  including missing explicit `--config`, plus `mcp_serve_ready`.
* **Adjudication (retain, evidence-gated — not speculative):** the sink is kept
  in the plan only because it targets a **distinct evidenced condition** the
  default cannot cover — T0 showing the CLI **discards** the child's stderr and
  the transparent wrapper cannot provide it. It is **not**
  general speculative logging: if T0 shows child stderr is capturable via the
  documented redirect (the common case), this task closes as *not-needed* and
  no sink is built.
* **T4 verification coupling (P1-2):** when this sink is the selected
  diagnosability path (T0 shows the CLI discards child stderr), it becomes the
  authoritative capture source — write it to a **known configured location**
  (e.g. under `.graphtor/logs/`) so **T4 (`056.004-T`) reads and validates the
  configured sink file** with per-attempt timestamp/PID/config correlation.
  On normal branches T4 uses the actual-client stderr/wrapper capture.
* The configured sink path remains under the authorized workspace. Create its
  parent directory when absent and surface initialization/write failures rather
  than silently losing the only evidence source.
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
* The remediation is recorded as an operational recipe in T0/`056.001-T`. If a
  code change is required to make the cause **actionable** (e.g. a clearer error
  that names the offending registry/schema), it is bounded to the evidenced
  cause and preceded by its own red test — the fail-closed behavior itself is
  never relaxed.
* **Iterative verification:** after each backup-first approved remediation,
  rerun the same actual-client probe. If a second sequential H0c gate becomes
  visible, keep `056.010-T` active and repeat evidence/approval/backup. Do not
  disposition other H0c remedies until initialize succeeds. If the sequence
  would exceed task-width limits, amend the plan and create a bounded follow-up
  before further mutation. No invalid Cargo fixture remains.
* Contingency: when H0c is not evidenced, move the task to `done` with
  `not-needed: H0c not evidenced`. When H0c is selected, complete it only after
  the actual client negotiates initialize.

### T3 — (Conditional on H1 evidence) Defer model load off the handshake

* Only if T0/T1 shows handshake latency (not an early exit) is implicated:
  lazy-load **only** the embedding model via clone-shared per-server state such
  as `Arc<tokio::sync::OnceCell<EmbeddingModel>>` + `spawn_blocking`; make the affected tool handlers `async`; return a distinct
  retryable "model still loading" error (not the existing "semantic search is
  disabled" message) and stop `research_topic` from *silently* degrading to
  unranked text search during the load window.
* Preserve `DocServer: Clone`: the lazy cell is shared by clones of one logical
  server instance, not a bare non-Clone field or module global. Enable Tokio's
  `sync` feature when used; add `time` only if timers are introduced.
  `search_semantic` **and** `research_topic` must surface the **same**
  machine-readable retryable signal during the load window (a stable error
  code / kind an agent can branch on, not prose only), so the two
  model-dependent tools present one coherent retry contract.
* If `tokio::sync::OnceCell` is used, enable Tokio's `sync` feature explicitly
  and verify the resulting dependency set under Rust 1.75. A different
  per-instance primitive is acceptable if it preserves the same semantics.
* **Deterministic test proof:** inject a blocking/failing model-loader seam.
  Server construction and initialize must not invoke it; the first
  model-dependent request starts loading; retryable behavior and eventual
  success are tested without a cold cache, network access, or wall-clock race.
* **Keep DB open, lock acquisition, the pre-v4 gate, and the duplicate-intake
  preflight as pre-serve fail-closed gates** — do not convert loud pre-connect
  failures into silent per-tool errors.
* If the affected handler signatures change from `sync fn` to `async fn`, the
  existing synchronous server unit tests in `src/mcp/server.rs` **cannot** "pass
  unchanged": either update them to equivalent `async` tests (asserting the same
  behavior) or provide a sync-compatible wrapper so the old call sites still
  compile. State which approach is taken; do not claim the unchanged sync tests
  still pass against a changed signature.
* Contingency: if evidence does not implicate latency, move to `done` and
  append `not-needed: H1 not evidenced`.
* Width: embedding lazy-load + affected handlers. Before coding, split if the
  evidenced design exceeds the 2-hour/3-file/5-function ceiling.

#### T-H3 — (Conditional on H3 evidence) client/transport compatibility — backlog `056.011-T`

* Only if T0/T1 evidences **H3**. Mode A is a live-child framing/version
  incompatibility; mode B is the actual CLI ignoring/rejecting a known `cwd`.
  H3 is **low confidence but live**, given a queued owner for
  traceability rather than an implicit Ship-created task. H3 has **two modes**,
  and this task owns a
  curative path to a **healthy `initialize` handshake** for each — every mode
  has a red/green or manual compatibility verification capable of reaching a
  healthy handshake:
  * **Mode A — framing/version incompatibility (child alive):** the child stays
    **alive** (no early-exit code, ruling out H0) yet the framed `initialize`
    never negotiates a `protocolVersion`. **Fix:** bump rmcp (1.8.0 available)
    and/or apply the minimal client-transport framing fix the newest CLI
    requires. Keep the rmcp bump **isolated** (its own commit) so re-pinning
    rmcp 1.5 is a clean revert. **No `get_info` protocol-echo change** (proven
    no-op on rmcp 1.5; H2 ruled out). **Verification (observed-red handshake
    test):** replay the raw target-CLI stdin/stdout transaction captured by T0.
    The replay is red on the incompatibility (child alive, no early exit,
    `initialize` never negotiates)
    and green after the fix; the sole pass assertion stays a successful
    `initialize` response. A generic valid direct request is not a substitute.
  * **Mode B — client ignores/rejects configured `cwd`:** T0's temporary,
    backup-first diagnostic entry, invoked through the actual target CLI,
    records that the child remains in a **foreign cwd** despite a known `cwd`
    field. **B1:** select/document a supported CLI version that honors managed
    `cwd`; keep `056.008-T` and `056.009-T` active to generate and deliver that
    field. **B2:** select a supported working-directory mechanism independent
    of managed `cwd`; move those tasks to `done` with
    `not-needed: H3-B2 selected` comments. Never add a server-side
    external-path fallback. **Verification:** repeat the applicable real-client
    probe and `/mcp show graphtor-docs`; both must show the project-root launch
    context and a healthy initialize handshake.
* **Distinguish H3 from H0a with T0 real-client evidence:** H0a = the current
  CLI honors known `cwd`; H3 mode B = it ignores/rejects that field and must
  choose B1 or B2. A direct child spawn is not client-capability evidence.
* If an rmcp bump (mode A) pulls transitive API changes (`serve_server`
  signature, `schemars` re-export), handle them in this task's own review per
  `docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`,
  and require build/clippy/tests plus
  `cargo +1.75.0 check --all-targets`.
* Contingency: if T0/T1 does not implicate H3, move to `done` and append
  `not-needed: H3 not evidenced`. Width: MCP dependency / transport +
  client-launch compatibility only.

### T4 — Runtime verification, rollback, and closure evidence

* Verify against the real newest Copilot CLI: `/mcp show graphtor-docs` shows a
  healthy connected server with no OS error 232; capture `mcp_serve_ready`
  separately as preflight-complete/about-to-call evidence only.
* Record rollback (revert the shipment commits in reverse dependency order;
  re-pin prior rmcp if bumped) and observe the next 3 serve starts, with a
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
* **Correlated actual-client evidence:** each of the three observed starts uses
  the T0-selected actual-client capture path: inherited CLI stderr, transparent
  wrapper, or the 056.006-T sink. Record CLI version, server-entry/config
  identity, timestamp, child pid when available, capture path, and `/mcp show`
  result. A log from a separately launched server cannot satisfy this evidence.
  `mcp_serve_ready` proves preflight only; the correlated `/mcp show` result
  proves initialize completion.
* Dependency note: T4 depends on `056.003-T` (**non-conditional** cmd_serve
  diagnostics — loud exit-2 errors + `mcp_serve_ready`, always lands) plus the
  **curative** fix tasks, each conditional and moved to **`done` with a
  `not-needed: <rationale>` log comment** when
  its hypothesis is not the evidenced cause: T2d launch-contract (H0a/H3-B1) =
  `056.008-T`, T2e existing-install migration (H0a/H3-B1) = `056.009-T`, T2b
  stale-lock (H0b) = `056.007-T`, T2c diagnosability = `056.006-T`, T2f H0c
  operational remediation (H0c) = `056.010-T`, T3 model lazy-load (H1) =
  `056.005-T`, T-H3 client/transport compatibility (H3) = `056.011-T`. One
  causal branch activates from the T0 evidence (**H0a → T2d + T2e**; **H0b → T2b**; **H0c →
  T2f** operational remediation, with T2c diagnosability optional; **H1 → T3**;
  **H3-A → T-H3**; **H3-B1 → T-H3 + T2d + T2e**; **H3-B2 → T-H3**); the
  non-selected tasks complete with that explicit disposition, which
  **satisfies** T4's dependency on them — T4 does not wait for a conditional
  task that evidence ruled out. **The selected curative branch always includes a
  task that produces a healthy `initialize` handshake** (H0c is no longer
  diagnosability-only; H3 must reach a healthy handshake), so T4's connectivity
  gate is always satisfiable.
* Width: runtime verification + closure evidence.

## Verification Commands

```text
# Evidence capture (T0), through the actual target CLI:
$env:RUST_LOG = 'debug'
New-Item -ItemType Directory -Force logs
# Run 056.001-T's bounded transparent wrapper via `/mcp show graphtor-docs`.
# Own the target CLI process, proxy stdio byte-for-byte, wait <=30s, capture
# allowlisted launch context/framing/exit/stderr, restore config in cleanup, and
# stop only the isolated probe process tree on timeout.
Get-ChildItem .graphtor -Filter *.lock
# Each T4 start records CLI version/config identity/timestamp/PID/capture path.
# If inherited stderr/wrapper capture is unavailable, use the 056.006-T sink.

# Quality gates:
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
# `mcp_serve_handshake_test` hosts the reusable open-stdin driver and any
# selected repository-code branch fixture. H0a uses it from 056.008-T's
# generated-entry test; H0b/H3-A use branch fixtures; H1 deterministic behavior
# is proven with 056.005-T's injected loader seam. Operational-only H0c/H3-B use
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
* **H3 (client/transport compatibility) rollback covers both modes:** for
  **mode A** (framing/version), keep the rmcp bump isolated (its own commit) so
  it can be pinned back independently, watching for transitive rmcp API changes
  in its own review; for **mode B** (client ignores/rejects the pinned `cwd`),
  B1 may include the
  `056.008-T`/`056.009-T` managed-contract commits plus supported-client
  selection, while B2 is a client-supported alternate mechanism with no managed
  cwd commits. Rollback reverts any B1 commits and restores the previous
  documented client configuration. No server-side external-path fallback exists.
* The T2c diagnosability sink (`056.006-T`) is off by default; disable its env
  gate to restore inherited stderr/transparent-wrapper capture, and `git revert` its
  commit to remove the sink entirely.
* If the lazy model load (T3) is taken, verify semantic search returns correct
  results after the first lazy load and that the loading-window error is
  retryable rather than a silent degrade.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; `Result` propagation; clippy pedantic
  clean.
* **II Test-First (NON-NEGOTIABLE)** — each production code task is preceded by
  its own observed-failing test. The T1 driver's sole success signal is a
  negotiated `initialize`; H0a/H0b/H1/H3-A have branch-owned deterministic
  tests. Operational-only H0c and H3-B use bounded actual-client before/after
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
* **V Observability** — structured `mcp_serve_ready` immediately before
  `serve_server` means preflight-complete/about-to-call only and stays separate
  from completed-handshake evidence. Actual-client inherited stderr or the T0
  wrapper is the default capture; T2c is used only when both are insufficient.
* **VI Single Responsibility** — the runtime cmd_serve diagnostics (T2,
  non-conditional), managed launch-contract generation (T2d), existing-install
  delivery (T2e), stale-lock liveness (T2b), diagnosability sink (T2c), H0c
  operational remediation (T2f), model lazy-load (T3), and client/transport
  compatibility (T-H3) are each single-width and split from one another; every
  **curative** task is evidence-gated (taken only if its hypothesis is
  evidenced); no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — none in the non-conditional path. T2e writes a
  contained byte-for-byte recovery file before any changing owned-entry
  refresh and aborts mutation when backup fails. The
  **conditional H0c operational remediation (T2f/`056.010-T`)** can require a
  **pre-v4→v4 schema rebuild via `graphtor-docs sync`** or a **source-registry
  replacement** — high-risk, potentially data-affecting steps that are
  **approval-gated** (operator approval required, with a backup taken before
  mutating) and **never** a fail-closed-gate weakening. See the Risky actions
  T2f entry (ActionRisk: high, approval_required: yes).
* **VIII Safety Modes** — investigate-first (T0/T1 before fix).
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
than weakening any gate; (3) stale-lock hardening keeps a matching
pid + process-start-time identity **live regardless of lock age** and must not
weaken exclusion of a genuinely live holder. A live legacy pid-only lock also
stays locked because its identity is ambiguous; age fallback applies only when
no usable live pid/strong identity exists. Legacy/unknown fields remain
parse-compatible; (4) diagnosability changes use structured tracing and must
not contaminate stdout; (5) the conditional T2d launch
contract validates the generated `cwd` by **equality to the canonicalized
project root** (NOT constrained inside `.graphtor`), adds no generated target
arguments, and must not relax runtime cwd containment (the launch cwd becomes
the project root); and (6) delivering the refreshed managed entry to
existing installs (T2e/`056.009-T`) must preserve any user-authored
`graphtor-docs` entry byte-for-byte (marker / exact-legacy-shape gating only)
and write a reported recovery file before any changing owned-entry mutation.

Instruction files / learnings consulted: `.github/instructions/constitution.instructions.md`
(III/IV, VIII), `.github/instructions/rust.instructions.md` (no `unwrap`/`expect`
in library code; `Result` propagation),
`docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`
(confirms the `serve_server` wiring is correct, so the failure is startup
early-exit, not malformed construction), and the sibling readonly-serve
hardening / serve auto-discovery decided plans for the cwd-relative discovery
and posture-classification context.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction (non-conditional, T2 diagnostics): convert every silent exit-2
  discovery site into a loud, actionable error naming the launch cwd /
  authorized candidate root + remediation, and emit `mcp_serve_ready`
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
* ProposedAction (conditional, T2b): record process start-time alongside pid in
  advisory lock metadata to survive pid reuse.
  * targets: `src/lock.rs` (`DatabaseLock::acquire` / `AdvisoryLock::acquire` /
    `handle_existing_lock`, ~120-200).
  * change_kind: lock-file metadata + liveness check.
  * ActionRisk: **moderate** — must not misclassify a live holder as stale;
    taken only if H0b is evidenced. rollback: revert the T2b commit.
  * approval_required: no; ActionResult: **planned** (or **abandoned** with a
    `not-needed` rationale if the task completes `done` without activation).
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
* ProposedAction (conditional, T-H3 H3 client/transport compatibility): reach a
  healthy `initialize` handshake for the evidenced H3 mode. **Mode A** (child
  alive, framing/version): bump rmcp (1.8.0 available) and/or apply the minimal
  client-transport framing fix, keeping the bump isolated. **Mode B** (client
  ignores/rejects the pinned `cwd` → managed-launch early exit): choose B1
  (supported CLI honors managed `cwd`, retaining T2d/T2e) or B2 (a different
  client-honored working-directory mechanism, closing T2d/T2e) — **no**
  server-side external-path fallback.
  * targets: **mode A** — `Cargo.toml` `[dependencies]` `rmcp` pin + rmcp
    `serve_server` / transport wiring in `src/main.rs` / `src/mcp/server.rs`;
    **mode B** — the documented client-launch configuration / recipe in
    `056.001-T`, plus T2d/T2e repo changes only for B1.
  * change_kind: **mode A** dependency bump + transport/framing edit; **mode B**
    operator/client configuration selection (documentation only).
  * ActionRisk: **moderate** — a mode-A rmcp bump can pull transitive API
    changes (`serve_server` signature, `schemars` re-export), re-verified in its
    own review with raw target-client replay and
    `cargo +1.75.0 check --all-targets`; no `get_info` change. Mode B changes no repo code and adds no
    containment surface. rollback: **mode A** revert the bump and re-pin rmcp
    1.5; **mode B** revert to the previously documented client configuration.
  * approval_required: no (non-destructive); ActionResult: **planned** (or
    **abandoned** if H3 is not evidenced).

### Added verification / rollback / observation detail

* Verification: the T1 red harness (open stdin + real `initialize` write +
  bounded-timeout response validation + child exit/stderr capture) must go red
  before and green after the evidenced fix. The primary red assertion is
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
    24-hour review checkpoint. For each start run `/mcp show graphtor-docs` on
    the newest CLI through the T0-selected capture path: inherited CLI stderr,
    transparent wrapper, or the 056.006-T sink. Record CLI version,
    server-entry/config identity, timestamp, child pid when available, capture
    path, and result. A separately launched server log is invalid evidence.
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
  A replays T0's raw target-client transaction; H1 uses `056.005-T`'s deterministic injected
  loader seam plus runtime confirmation.
* Operational-only H0c and H3 mode B preserve a bounded actual-client red
  transcript and rerun the same probe after the approved state/client repair.
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
* Per-width proofs remain separate: `056.008-T` owns generated-entry execution,
  `056.009-T` owns the upgrade migration matrix, `056.003-T` owns three
  reachable process diagnostics plus the defensive formatter and
  `mcp_serve_ready`, `056.006-T` owns sink behavior, and
  `056.007-T` owns pid/start-time liveness. No single test proves an unrelated
  surface, and no intentionally failing test remains after the selected fix.
* Existing MCP tests (`tests/mcp_manifest_test.rs`) must continue to pass
  unchanged. The server unit tests in `src/mcp/server.rs` also stay unchanged
  **unless** the conditional T3 changes handler signatures to `async`, in which
  case they must be updated to equivalent `async` tests (or shielded by a
  sync-compatible wrapper) rather than asserted as unchanged.

## Plan Review

**Current status: correction cycle 3, report-only gate PENDING — NOT a
PASS.** A fresh standard report-only review of committed HEAD **`60f32c0`**
returned `BLOCKED` with actual-client proof, lock safety, H0c sequencing,
backlog-disposition, and branch-verification defects. This final bounded
correction updates the current authoritative sections and linked
backlog/deliberation artifacts. The next committed HEAD must pass a fresh
current-HEAD standard review and mandatory adversarial re-review before Ship.
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
  committed HEAD **`60f32c0`**, outcome **BLOCKED**. **Review status of the
  corrected artifacts:** report-only gate **PENDING** against the next
  committed HEAD — explicitly **not** a PASS.
* Linked deliberation: `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`.
* Backlog scope: shipment `049-S` / feature `056-F`, tasks `056.001-T`..`056.011-T`
  (T0 `056.001-T`, T1 `056.002-T`; the **non-conditional** cmd_serve diagnostics
  T2 `056.003-T`; the evidence-gated **curative** fix tasks T2d `056.008-T` + T2e
  `056.009-T` (H0a), T2b `056.007-T` (H0b), T2c `056.006-T` (diagnosability), T2f
  `056.010-T` (H0c operational remediation), T3 `056.005-T` (H1), T-H3
  `056.011-T` (H3); plus verification T4 `056.004-T`).

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
* **Current status:** the standard report-only review of HEAD `60f32c0` found
  actual-client proof, lock safety, H0c sequencing, and backlog-disposition
  defects. Deduplicated counts were `P0=0, P1=6, P2=8`; out-of-scope and
  wrong-diff persona findings were excluded. Fresh correction cycle 3
  addresses the valid findings; a fresh current-HEAD review is required to establish
  `P0=0, P1=0`, followed by the mandatory adversarial re-review.
* **Consensus review (2026-08-21, HEAD `22d18f1`):** a 3-model adversarial
  consensus review produced a deduplicated remediation queue (F1/F2/F3/N1
  containment reversal; F4 status parity; F5 stale wording; F6 H3 owner; F7
  config schema; S1 migration primacy; S7 hardening; per-surface test-first),
  applied in "Consensus review remediation cycle 1 (2026-08-21)" below. A fresh
  current-HEAD report-only review is required to re-establish `P0=0, P1=0`.
* **Fresh-cycle P2 status:** the six consensus P2s were corrected or explicitly
  adjudicated in fresh correction cycle 1; validation is pending.
* **P3 / carried advisories: several**, recorded for Ship execution.

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
* **T3 OnceCell as instance state + single coherent retry signal** (Architecture,
  Agent-Native): resolved — the lazy-model `OnceCell` is DocServer instance
  state, and `search_semantic` + `research_topic` share one machine-readable
  retryable signal (`056.005-T`).
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
* If T2c is taken, place the sink file inside `.graphtor/logs/` with restrictive
  permissions and rotation, and route pre-serve early-exit reporting through a
  single seam rather than mirroring N scattered `eprintln!` sites.
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
* **Branch-sensitive evidence/baseline (P2)** — T1 red evidence and the T4
  observation baseline are now branch-sensitive: H0 = nonzero exit / marker /
  pipe close; H1 = bounded `initialize` timeout with the child still alive.
  Green stays a successful `initialize` response for both.
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
* **P1-2 — branch-sensitive T2c sink verification:** when the T2c sink
  (`056.006-T`) is selected because the CLI **discards** child stderr, T4 cannot
  require `logs/serve-stderr.log` from a shell redirect (impossible on that
  branch). Resolved: T4 verification is **branch-sensitive** — stderr capture
  (`logs/serve-stderr.log`) for the normal branches, and **read/validate the
  configured env-gated sink file** (e.g. under `.graphtor/logs/`) for the T2c
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

### Fresh correction cycle 1 (2026-08-21) — report-only gate pending

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

### Fresh correction cycle 2 (2026-08-22) — report-only gate PENDING

Standard review of `13485d0` found that the prior common-harness contract could
not be greened mechanically on H0a or operational-only H0c, H3 mode B closed
the managed-cwd tasks too early, and several live sections still asserted
superseded behavior. This cycle:

* makes T1 a reusable driver with branch-owned red/green tests and bounded
  before/after actual-client evidence for external-only H0c/H3-B;
* splits H3 mode B into B1 (supported CLI honors managed `cwd`, so T2d/T2e
  remain active) and B2 (alternate supported mechanism, so they close);
* makes `mcp_serve_ready` an initialize-ready pre-`serve_server` event and
  requires three observed healthy starts before closure can be `healthy`;
* makes H1 tests deterministic through an injected loader seam and records the
  Tokio `sync`/Rust-1.75 dependency gate;
* sequences T2c after T2, covers all four diagnostics, and hardens sink,
  upgrade-refresh, lock-recovery, and fixture prerequisites.

The corrected artifacts are not yet reviewed. A fresh current-HEAD standard
report-only review is required; if P0/P1 clears, the mandatory three-family
adversarial re-review follows.
