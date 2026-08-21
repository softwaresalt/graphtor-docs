---
title: "Implementation Plan: Fix graphtor-docs MCP serve initialize-handshake regression (Copilot CLI OS error 232)"
description: "Grounded, evidence-first, test-first plan to restore graphtor-docs MCP STDIO serve compatibility with recent GitHub Copilot CLI builds by capturing the child-process exit cause and hardening the pre-serve startup path"
topic: "graphtor-docs MCP serve initialize handshake"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
backlog_refs:
  - "049-S"
  - "056-F"
status: "reviewed"
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
most plausibly one of `cmd_serve`'s six pre-`serve_server` early-exit /
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
* The failing child-process exit cause is captured with a concrete exit code
  and stderr, and an out-of-process harness reproduces it (red) and passes
  after the fix.
* The implicated pre-serve failure mode is hardened so a benign launch (e.g.
  a different cwd, or a stale lock from a killed prior child) no longer causes
  a silent exit-before-initialize.
* Server startup failures are diagnosable even when the CLI discards child
  stderr (opt-in file-log sink or a documented redirect recipe).
* All four quality gates pass: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`,
  `cargo test --all-targets`, `cargo audit`.
* Rollback and a short post-fix observation window are documented.

## Likely Surfaces (exact)

| Surface | Location | Change |
|---|---|---|
| Serve startup / early-exit paths (T2, single evidenced cause) | `src/main.rs::cmd_serve` (~2446-2655) | Containment-safe workspace-root resolution: resolve only from explicit `--db-path` / `--config` inputs or the launch cwd, **never** by walking to parent directories; refuse any candidate outside/above the launch-cwd boundary; convert the silent exit-2 discovery failure into a loud, actionable error; keep the existing fail-closed gates |
| Advisory lock handling (T2b, conditional on H0b) | `src/lock.rs` (`DatabaseLock::acquire`, `AdvisoryLock::acquire`, `handle_existing_lock`, ~120-200) | Only if H0b is evidenced: harden stale-lock liveness by recording process start-time alongside pid so a reused pid is not misread as a live holder |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if a documented stderr-redirect recipe proves insufficient: env-gated opt-in sink. It MUST capture the pre-serve `eprintln!` early-exit messages (~2504/2549/2635) — convert them to `tracing` or tee stderr — because a `tracing`-only sink would silently miss those direct `eprintln!` / AutoStream writes |
| Embedding-model resolution (conditional) | `src/embed/resolver.rs`, consumers in `src/mcp/server.rs` | Only if H1 evidenced: lazy `tokio::sync::OnceCell` + `spawn_blocking`, distinct "model loading" tool error |
| MCP dependency (conditional) | `Cargo.toml` (~43, `rmcp = "1.5"`) | Only if H3 evidenced: bump rmcp (1.8.0 available). **No `get_info` protocol-echo change — it is a no-op on rmcp 1.5.** |
| Tests | `tests/mcp_serve_handshake_test.rs` (new) | Out-of-process red-first harness that keeps stdin OPEN, writes a protocol-valid newline-delimited `initialize` JSON-RPC request, and awaits/validates the `initialize` response under a bounded timeout, capturing child exit code + stderr when the write or response fails — reproducing the client-visible pipe close (or the confirmed causal early exit) under a controlled cwd/env |

## Task Breakdown (evidence-first, test-first, ~2h each, single-width)

### T0 — Capture the failure evidence (investigate-first, ~30-60 min)

* Run the server binary with the **exact command line, cwd, and env** the
  newest CLI uses for the child (from `/mcp show graphtor-docs` and the CLI's
  MCP launch config), with `RUST_LOG=debug` and stderr redirected to a file.
* Record the **exit code** and stderr; check for a leftover `.graphtor/*.lock`;
  compare against a prior known-good CLI build if available.
* Deliverable: a recorded exit code + stderr that names the H0 sub-cause (or
  rules H0 out and points at H1). A nonzero early-exit code settles the
  diagnosis in one run and may make T3/T-rmcp unnecessary.
* Width: evidence capture only; no code change.

### T1 — Out-of-process regression harness (red)

* Add `tests/mcp_serve_handshake_test.rs` that **spawns the real binary** with a
  controllable cwd/env and a fixture workspace reproducing the T0 sub-cause,
  and drives a real STDIO client turn:
  * **Keep the child's stdin OPEN** for the duration of the attempt — do not
    pass an empty/closed stdin. A closed stdin only exercises a benign
    EOF-driven shutdown and cannot distinguish the regression from a normal
    end-of-input exit, so it would never actually write `initialize`.
  * Write a **protocol-valid, newline-delimited** MCP `initialize` JSON-RPC
    request to the child's stdin (a well-formed `jsonrpc: "2.0"` request with
    `method: "initialize"`, an `id`, and a `params` carrying `protocolVersion`,
    `capabilities`, and `clientInfo`), matching the framing rmcp's STDIO
    transport expects.
  * **Await and validate the `initialize` response under a bounded timeout**
    (a short, fixed deadline): a successful `initialize` result with a
    negotiated `protocolVersion` means the regression did **not** reproduce.
  * The assertion is the reproduced failure mode: the `initialize` **write
    fails** (broken/closed pipe — the client-visible OS-error-232 analogue) or
    the response **never arrives before the deadline** because the child
    **exited before answering `initialize`**. On either failure the harness
    **captures the child's exit code (or exit signal) and full stderr** and
    asserts they match the confirmed T0 early-exit cause (exit code +
    early-exit marker), so a red result is tied to the real cause rather than
    to an ambiguous EOF.
  * Pin model-cache state so the harness is deterministic (not dependent on a
    warm HF cache) and the bounded timeout cannot flake on a cold model load.
* Stop gate: if the harness cannot be made red **for the confirmed T0 cause**
  (initialize write/response failure attributable to the captured early exit),
  halt and return to T0 rather than refactoring startup on a green or
  ambiguous test.
* Deliverable: a **red** harness that performs a real `initialize` write over
  an open stdin, validates the response under a bounded timeout, and captures
  the child exit/stderr on failure — encoding the T0 evidence.
* Width: test infrastructure only.

### T2 — Harden the single evidenced pre-serve failure (green)

* Fix **only** the single H0 sub-cause identified by T0/T1. When H0a (launch-cwd
  discovery) is the evidenced cause:
  * resolve the workspace root in a **containment-safe** way — only from an
    explicit `--db-path` / `--config` input or the launch cwd itself, and
    **never** by walking to parent directories, so resolution can neither
    escape nor climb above the launch-cwd boundary (Principle III/IV);
  * **prefer an explicit `--db-path` / `--config` target over the launch cwd**
    so the served set decouples from a cwd the CLI may no longer supply — this
    is what actually restores connectivity for H0a (see the H0a scope note
    below);
  * establish only the trust-anchor root here and **delegate all containment
    refusal to the existing shared primitives** (`graphtor_core::path::validate_path`
    / `is_reparse_point`, the same guards `src/workspace/serve_discovery.rs`
    uses) — do **not** hand-roll a parallel string/prefix escape check in
    `main.rs`. **Canonicalize both operands** (the resolved candidate and the
    launch-cwd anchor) before comparison, and reject on canonicalization
    failure, so `..`, symlinks/junctions, and Windows short-name/case variants
    cannot fail open;
  * convert **every** silent exit-2 "no databases found to serve" site (there
    are two — after `discover_served_databases` and after the phantom-default
    `postures.retain`, plus the structurally-unreachable `primary` None guard)
    into a loud, actionable error, so no discovery path exits silently.
* **H0a scope note (diagnostic vs curative):** a loud early-exit still exits
  before the transport binds, so on its own it does not clear OS error 232 — it
  only makes the cause visible. H0a connectivity is restored only when
  resolution prefers an explicit target (above) that the CLI passes. If T0
  shows the CLI supplies neither an explicit target nor a cwd-local `.graphtor`,
  the remediation is an **operational launch-config / install recipe** (correct
  cwd or pass `--db-path` / `--config`), and the T2 code change is
  diagnosability only. T0 must record which case holds before T2 is scoped, and
  T4's connectivity gate depends on it.
* Green T1. Preserve all existing fail-closed semantics (malformed registry,
  missing explicit `--config`, pre-v4 gate, duplicate-intake preflight remain
  pre-serve gates) — add a regression assertion that each of these still exits
  pre-serve after the cwd-resolution change, so robustness never silently
  converts a fail-closed gate into a fail-open path. Do **not** add an
  unrelated variant or optional logging in this task.
* Width: serve startup, one failure mode. The stale-lock liveness variant
  (H0b) and the optional diagnosability sink are **isolated** into the
  conditional tasks below so this task implements exactly one evidenced cause.

#### T2b — (Conditional on H0b evidence) Harden stale-lock liveness — backlog `056.007-T`

* Only if T0/T1 evidences lock contention / stale-lock **pid reuse** rather
  than the cwd cause: in `src/lock.rs` (`DatabaseLock::acquire` /
  `AdvisoryLock::acquire` / `handle_existing_lock`), record process start-time
  alongside pid so a reused pid is not misread as a live lock holder. Prefer
  start-time+pid over a `--force` escape hatch.
* **Lock-file format compatibility (required):** a lock file written by a prior
  binary (no start-time field) must degrade to the current pid-only liveness
  check, **never** parse-error into `GraphtorError::Config` — a parse failure
  would itself become a new pre-serve fail-closed exit (a fresh 232). Preserve
  the existing atomic write-cleanup and concurrent-release NotFound-retry
  behavior. Add a compatibility/parse-fallback test.
* Contingency: close as *not-needed* with a one-line rationale if H0b is not
  evidenced (Constitution VI). Width: lock liveness only.

#### T2c — (Conditional/optional) Startup diagnosability sink — backlog `056.006-T`

* Default: rely on the **documented stderr-redirect recipe** (already exercised
  in T0) rather than new runtime logging — avoid speculative logging
  complexity. Only if that recipe proves insufficient (e.g. the CLI discards
  child stderr and redirection is impractical), add an env-gated opt-in
  file-log sink in `src/logging/init.rs`.
* If built, it MUST capture the pre-serve `eprintln!` early-exit messages
  (~`src/main.rs` 2504/2549/2635) — convert them to `tracing` or tee the real
  stderr stream — because a `tracing`-only sink would silently miss those
  direct `eprintln!` / AutoStream writes.
* Contingency: close as *not-needed* if the stderr recipe suffices. Width:
  logging/diagnosability only.

### T3 — (Conditional on H1 evidence) Defer model load off the handshake

* Only if T0/T1 shows handshake latency (not an early exit) is implicated:
  lazy-load **only** the embedding model via `tokio::sync::OnceCell` +
  `spawn_blocking`; make the affected tool handlers `async`; return a distinct
  retryable "model still loading" error (not the existing "semantic search is
  disabled" message) and stop `research_topic` from *silently* degrading to
  unranked text search during the load window.
* Own the lazy-model `OnceCell` as **`DocServer` instance state** (alongside the
  existing `model: Option<EmbeddingModel>` field), **not** a module-level
  global, to preserve DocServer's per-instance Clone-via-`Arc` test isolation.
  `search_semantic` **and** `research_topic` must surface the **same**
  machine-readable retryable signal during the load window (a stable error
  code / kind an agent can branch on, not prose only), so the two
  model-dependent tools present one coherent retry contract.
* **Keep DB open, lock acquisition, the pre-v4 gate, and the duplicate-intake
  preflight as pre-serve fail-closed gates** — do not convert loud pre-connect
  failures into silent per-tool errors.
* If the affected handler signatures change from `sync fn` to `async fn`, the
  existing synchronous server unit tests in `src/mcp/server.rs` **cannot** "pass
  unchanged": either update them to equivalent `async` tests (asserting the same
  behavior) or provide a sync-compatible wrapper so the old call sites still
  compile. State which approach is taken; do not claim the unchanged sync tests
  still pass against a changed signature.
* Contingency: close as *not-needed* with a one-line rationale if evidence does
  not implicate latency (Constitution VI).
* Width: embedding lazy-load + affected handlers.

### T4 — Runtime verification, rollback, and closure evidence

* Verify against the real newest Copilot CLI: `/mcp show graphtor-docs` shows a
  healthy connected server with no OS error 232; capture the serve-ready
  startup log.
* Record rollback (revert the shipment commits in reverse dependency order;
  re-pin prior rmcp if bumped) and a short observation window (next 3 serve
  starts or 24h) confirming no OS error 232 recurrence.
* Dependency note: T4 depends on the fix tasks, but the conditional fix tasks
  (T2b stale-lock = `056.007-T`, T2c diagnosability = `056.006-T`, T3 model
  lazy-load = `056.005-T`) may be **closed as *not-needed*** when their
  hypothesis is not evidenced. Closing a conditional task as *not-needed*
  **satisfies** T4's dependency on it — T4 does not wait for a conditional task
  that evidence ruled out.
* Width: runtime verification + closure evidence.

## Verification Commands

```text
# Evidence capture (T0), from the CLI's launch cwd:
$env:RUST_LOG='debug'; graphtor-docs serve 2> serve-stderr.log ; echo "exit=$LASTEXITCODE"
Get-ChildItem .graphtor -Filter *.lock

# Quality gates:
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings -D clippy::pedantic
cargo test --test mcp_serve_handshake_test
cargo test --all-targets
cargo audit
cargo build --release

# Manual runtime check against the newest Copilot CLI:
#   /mcp show graphtor-docs   (expect: connected, no OS error 232)
```

## Rollback / Compatibility

* T2 hardening is additive (containment-safe root resolution plus a loud
  discovery error; the conditional T2b richer lock metadata and T2c opt-in log
  sink are separate, evidence-gated tasks) and behavior-preserving for the
  happy path; revert commits in reverse dependency order if needed.
* If rmcp is bumped, keep the bump isolated so it can be pinned back
  independently; watch for transitive rmcp API changes in its own review.
* If the lazy model load (T3) is taken, verify semantic search returns correct
  results after the first lazy load and that the loading-window error is
  retryable rather than a silent degrade.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; `Result` propagation; clippy pedantic
  clean.
* **II Test-First (NON-NEGOTIABLE)** — T1 red harness precedes T2/T3.
* **III/IV Isolation & Containment** — serve stays localhost STDIO;
  workspace-root resolution is containment-safe (resolves only from explicit
  `--db-path` / `--config` inputs or the launch cwd, never by walking to parent
  directories) and refuses any candidate outside/above the launch-cwd boundary
  by **delegating to the shared `graphtor_core::path::validate_path` /
  `is_reparse_point` primitives** (canonicalizing both operands) rather than a
  hand-rolled check, so containment stays a single source of truth. The refusal
  test enumerates the escape vectors — absolute-above-boundary, `..`-traversal,
  escaping symlink, junction/reparse-point, and a Windows short-name/case
  variant — each asserted refused; no containment relaxation.
* **V Observability** — positive serve-ready startup log; a documented
  stderr-redirect recipe is the diagnosability default, with the conditional
  T2c opt-in file sink taken only if that recipe is insufficient.
* **VI Single Responsibility** — rmcp bump and model lazy-load taken only if
  evidence requires; no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — none.
* **VIII Safety Modes** — investigate-first (T0/T1 before fix).
* **XI Merge-commit history** — Ship enforces merge-commit-only at merge time.

## Plan Hardening Signals

* Public API, schema, or contract change: **absent** — the fix is internal to
  the `graphtor-docs` binary (`cmd_serve`, `src/lock.rs`, `src/logging`); no
  library public API, schema, or wire-contract change (no `get_info` change; an
  rmcp bump is conditional and reviewed separately if taken).
* Auth / security / permission / compliance-sensitive behavior: **present** —
  the change touches CLI workspace containment (Principle III/IV): the new
  workspace-root resolution must not climb above or escape the launch cwd.
* Migration / backfill / destructive or irreversible step: **absent**.
* External integration / operator checkpoint / partial-rollout: **present
  (bounded)** — behavior depends on how the external Copilot CLI launches the
  child (cwd/env/lifecycle); T0 evidence and a post-fix observation window
  bound it.
* High runtime or rollback risk: **present** — `serve` startup and advisory
  database locking are startup-critical; a wrong resolution or lock change can
  silently break connectivity for every client.

Requires plan hardening: yes

## Plan Hardening

Hardening was required (P-006) because the fix changes startup-critical
`serve` workspace-root resolution and advisory database locking, and because
Principle III/IV CLI containment is directly implicated: a resolution that
walked to parent directories would escape the launch-cwd boundary. The
protected invariants are (1) resolution never climbs above or escapes the
launch cwd, delegating refusal to the shared
`graphtor_core::path::validate_path` / `is_reparse_point` primitives with both
operands canonicalized (no hand-rolled prefix check that could fail open on
`..`, symlinks/junctions, or Windows short-name/case variants); (2) the
existing fail-closed gates (malformed registry, missing
explicit `--config`, pre-v4 schema, duplicate-intake preflight) stay pre-serve
gates; (3) stale-lock hardening must not weaken exclusion of a genuinely live
holder and must degrade a start-time-less legacy lock file to pid-only rather
than parse-error into a new fail-closed exit; (4) diagnosability changes must
not contaminate stdout or drop the
pre-serve `eprintln!` early-exit messages.

Instruction files / learnings consulted: `.github/instructions/constitution.instructions.md`
(III/IV, VIII), `.github/instructions/rust.instructions.md` (no `unwrap`/`expect`
in library code; `Result` propagation),
`docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`
(confirms the `serve_server` wiring is correct, so the failure is startup
early-exit, not malformed construction), and the sibling readonly-serve
hardening / serve auto-discovery decided plans for the cwd-relative discovery
and posture-classification context.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction: replace launch-cwd-relative workspace-root resolution with a
  containment-safe resolution (explicit `--db-path` / `--config` preferred, or
  launch cwd only, no parent walk; containment delegated to the shared
  `validate_path` / `is_reparse_point` primitives with both operands
  canonicalized) and convert every silent exit-2 discovery site into a loud,
  actionable error.
  * targets: `src/main.rs::cmd_serve` (~2446-2655); reuse of
    `graphtor_core::path` / `src/workspace/serve_discovery.rs` containment
    primitives.
  * change_kind: local edit to startup control flow.
  * ActionRisk: **moderate** — startup-critical but non-destructive and
    behavior-preserving on the happy path; guarded by the T1 red harness and an
    explicit outside/parent refusal test.
  * rollback: `git revert` the T2 commit(s) in reverse dependency order.
  * approval_required: no (non-destructive); ActionResult: **planned**.
* ProposedAction (conditional, T2b): record process start-time alongside pid in
  advisory lock metadata to survive pid reuse.
  * targets: `src/lock.rs` (`DatabaseLock::acquire` / `AdvisoryLock::acquire` /
    `handle_existing_lock`, ~120-200).
  * change_kind: lock-file metadata + liveness check.
  * ActionRisk: **moderate** — must not misclassify a live holder as stale;
    taken only if H0b is evidenced. rollback: revert the T2b commit.
  * approval_required: no; ActionResult: **planned** (or **abandoned** if
    closed not-needed).
* ProposedAction (conditional, T2c): env-gated opt-in diagnostic file-log sink.
  * targets: `src/logging/init.rs`, serve path in `src/main.rs`.
  * change_kind: additive, off-by-default logging sink that must capture the
    pre-serve `eprintln!` messages.
  * ActionRisk: **low** — off by default; taken only if the stderr-redirect
    recipe is insufficient. rollback: revert the T2c commit. ActionResult:
    **planned** (or **abandoned** if closed not-needed).

### Added verification / rollback / observation detail

* Verification: the T1 red harness (open stdin + real `initialize` write +
  bounded-timeout response validation + child exit/stderr capture) must go red
  before and green after T2. The primary red assertion is "no `initialize`
  response before the bounded deadline + captured nonzero child exit matching
  the T0 marker"; a write-side broken-pipe error is an opportunistic secondary
  signal (a ~500-byte write can buffer into a pipe whose reader already exited,
  so a write-only assertion could flake green). An explicit refusal test
  enumerates the escape vectors (absolute-above, `..`-traversal, escaping
  symlink, junction/reparse-point, Windows short-name/case variant), each
  asserted refused; a regression assertion confirms each fail-closed gate
  (malformed registry, missing explicit `--config`, pre-v4, duplicate-intake)
  still exits pre-serve after the cwd-resolution change. All four quality gates
  plus `cargo build --release`.
* Rollback: revert shipment commits in reverse dependency order; re-pin prior
  rmcp if bumped (T4).
* Post-deploy observation window (manual): owner is the merging developer;
  signal is a completed `initialize` handshake with no OS error 232 on the
  newest Copilot CLI; window is the next 3 serve starts or 24h; rollback
  trigger is any OS error 232 recurrence; outcome (healthy / degraded /
  rolled-back) is recorded in the shipment closure artifact (T4).

## Test-First Harness Expectations

* `tests/mcp_serve_handshake_test.rs` must exist and be **red** (reproducing
  the exit-before-initialize cause captured in T0) before T2/T2b/T3.
* The harness spawns the real binary out-of-process with a controlled cwd/env
  and pinned model-cache state so it is deterministic — not a happy-path
  in-process fixture (which would negotiate the handshake and pass trivially).
* The harness keeps the child's stdin **open**, writes a protocol-valid
  newline-delimited `initialize` JSON-RPC request, and awaits/validates the
  `initialize` response under a **bounded timeout**. Red = the `initialize`
  write fails on a closed pipe or the response never arrives before the
  deadline because the child exited first; the harness captures the child exit
  code + stderr and ties the failure to the confirmed T0 cause. An empty/closed
  stdin is explicitly disallowed — it would only exercise a benign EOF-driven
  shutdown and could not distinguish the regression.
* Existing MCP tests (`tests/mcp_manifest_test.rs`) must continue to pass
  unchanged. The server unit tests in `src/mcp/server.rs` also stay unchanged
  **unless** the conditional T3 changes handler signatures to `async`, in which
  case they must be updated to equivalent `async` tests (or shielded by a
  sync-compatible wrapper) rather than asserted as unchanged.

## Plan Review

**Gate decision: PASS** (after one in-pass remediation cycle). No unresolved
P0/P1 findings; the consensus P2 trust-boundary and correctness findings were
remediated directly in this plan and its backlog tasks; residual items are
recorded as Ship-phase P2/P3 advisories below.

### Reviewed artifact identity

* Plan: `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
  (this file), reviewed on branch `chore/stage-049-S` at its remediated state.
* Linked deliberation: `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`.
* Backlog scope: shipment `049-S` / feature `056-F`, tasks `056.001-T`..`056.007-T`
  (T0/T1/T2 + conditional T2b `056.007-T`, T2c `056.006-T`, T3 `056.005-T`, and
  verification T4 `056.004-T`).

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

* **P0: 0. P1: 0.** Every persona explicitly reported nothing blocking.
* **P2: 0 unresolved** — the consensus P2 clusters (below) were remediated in
  this pass.
* **P3 / carried advisories: several**, recorded for Ship execution.

### Consensus P2 findings (remediated in this pass)

* **Containment must reuse the shared primitives, canonicalize both operands,
  and enumerate escape vectors** (Constitution, Learnings, Architecture,
  Security consensus). Resolved: T2, the Constitution Check III/IV entry, and
  the Plan Hardening invariant now delegate refusal to
  `graphtor_core::path::validate_path` / `is_reparse_point` (the same guard
  `src/workspace/serve_discovery.rs` uses), canonicalize both operands, and the
  refusal test enumerates absolute-above / `..`-traversal / escaping symlink /
  junction-reparse-point / Windows short-name-case vectors.
* **H0a fix is diagnostic-plus-operational** (Rust, Architecture consensus): a
  loud early-exit alone does not clear OS error 232. Resolved: T2 now prefers an
  explicit `--db-path` / `--config` target over the launch cwd and adds an H0a
  scope note that connectivity restoration may require an operational
  launch-config/install recipe, which T0 must record and T4 depends on.
* **T2 must cover every silent exit-2 discovery site** (Rust): resolved — T2
  now names both discovery sites plus the `primary` None guard.
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
* Optionally extend the green-side T1 assertion to the `initialized`
  notification + one benign `tools/list` turn to prove transact-ability, not
  just handshake return.
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
  to this plan, and this plan and the deliberation carry an informational
  `backlog_refs` frontmatter field (`049-S`, `056-F`).
* **No implementation/test code on this staging PR is expected or a finding:**
  T0/T1/T2 and the conditionals are intentionally future Ship tasks; reviewers
  were instructed not to treat the absence of code as a finding.
* **Pre-existing MSRV note (out of scope):** the Rust 1.75 vs rmcp 1.5 /
  edition-2024 tooling context is a pre-existing residual and is not expanded by
  this shipment; any rmcp bump remains a conditional, separately-reviewed task.
