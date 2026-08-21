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
| Serve startup / early-exit paths (T2, H0a-only — conditional) | `src/main.rs::cmd_serve` (~2446-2655) | Only if T0 evidences H0a: containment-safe workspace-root resolution (explicit `--db-path` / `--config` inputs or the launch cwd, **never** parent-walking; refuse any candidate outside/above the launch-cwd boundary) and convert the silent exit-2 discovery failures into loud, actionable errors; keep the existing fail-closed gates. Close as *not-needed* if H0a is not the evidenced cause (T4 dep satisfied by that closure) |
| Managed MCP launch contract (T2d, H0a-only — conditional) | `src/workspace/mcp_config.rs` (`managed_server_value` ~528-540, `generate_mcp_config`) | Only if T0 evidences H0a: emit a trusted, containment-validated launch identity in the generated managed `.mcp.json` entry (today only `command` + `args: ["serve"]` + `transport` + marker). **Primary lever: pin the child working directory (`cwd`) to the project root** — registry discovery (`load_source_config` → `cwd.join(".graphtor/config")`), posture/Generation-target validation (`validate_path(.., root=cwd)`), DB auto-discovery (`cwd.join(".graphtor")`), and background sync (`acquire_plan::plan(.., &cwd)`, `data_root = cwd.join(".graphtor/data")`) are ALL cwd-anchored, so pinning cwd restores registry-backed Generation/background-sync together **without** relaxing the runtime cwd boundary. Complement (only when the served DB legitimately lives outside that cwd): pin absolute `--db-path` and, when a registry exists, `--config`. Explicit targets ALONE do not restore Generation because posture/sync validate against `cwd`. Preserve the genuinely-absent-registry (zero-config) case. Validate every pinned path within project-root `.graphtor` via the shared `validate_path` / `is_reparse_point` primitives (canonicalized); no parent traversal. Covered by an unrelated-cwd launch regression test. Close as *not-needed* if H0a is not evidenced |
| Advisory lock handling (T2b, conditional on H0b) | `src/lock.rs` (`DatabaseLock::acquire`, `AdvisoryLock::acquire`, `handle_existing_lock`, ~120-200) | Only if H0b is evidenced: harden stale-lock liveness by recording process start-time alongside pid so a reused pid is not misread as a live holder |
| Diagnostic logging sink (T2c, conditional/optional) | `src/logging/init.rs`, serve path in `src/main.rs` | Only if a documented stderr-redirect recipe proves insufficient: env-gated opt-in sink. It MUST capture the pre-serve `eprintln!` early-exit messages (~2504/2549/2635) — convert them to `tracing` or tee stderr — because a `tracing`-only sink would silently miss those direct `eprintln!` / AutoStream writes |
| Embedding-model resolution (conditional) | `src/embed/resolver.rs`, consumers in `src/mcp/server.rs` | Only if H1 evidenced: lazy `tokio::sync::OnceCell` + `spawn_blocking`, distinct "model loading" tool error |
| MCP dependency (conditional) | `Cargo.toml` (~43, `rmcp = "1.5"`) | Only if H3 evidenced: bump rmcp (1.8.0 available). **No `get_info` protocol-echo change — it is a no-op on rmcp 1.5.** |
| Tests | `tests/mcp_serve_handshake_test.rs` (new) | Out-of-process red-first harness that keeps stdin OPEN, writes a protocol-valid newline-delimited `initialize` request, and awaits/validates the `initialize` response under a bounded timeout. **Red is branch-sensitive to T0:** for H0 the child exits before answering (nonzero exit / early-exit marker / pipe close captured); for H1 the child stays **alive** but the `initialize` response misses the bounded deadline (latency). Green (both) = a successful `initialize` response, under a controlled cwd/env |

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
  * The assertion is the reproduced failure mode, **branch-sensitive to the T0
    evidence**:
    * **H0 (early exit):** the `initialize` **write fails** (broken/closed
      pipe — the client-visible OS-error-232 analogue) or the response
      **never arrives before the deadline because the child exited**; the
      harness captures the child's exit code (or signal) and full stderr and
      asserts they match the confirmed T0 early-exit cause (exit code +
      early-exit marker), so red is tied to the real cause, not an ambiguous
      EOF.
    * **H1 (latency):** the child **stays alive** (no early exit) but the
      `initialize` response misses the bounded deadline; the harness asserts
      the child is **still running** (no exit code yet) and the deadline
      elapsed, distinguishing a slow handshake from an early-exit crash.
  * Green (both branches) = a successful `initialize` result with a negotiated
    `protocolVersion`.
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

### T2 — Harden the evidenced pre-serve failure (green, H0a-only — conditional)

* **Conditional on T0 evidencing H0a** (launch-cwd `.graphtor` discovery). If
  T0 evidences H0b, H1, or a fail-closed gate (H0c) instead, close this task as
  *not-needed* with a one-line rationale — its H0a-specific acceptance cannot be
  satisfied on a non-H0a branch, and the evidenced branch is handled by its own
  task (T2b/`056.007-T`, T3/`056.005-T`, or the diagnosability/operational
  path). Closing as *not-needed* satisfies T4's dependency (Constitution VI).
* Fix **only** the single H0a sub-cause identified by T0/T1:
  * resolve the workspace root in a **containment-safe** way — only from an
    explicit `--db-path` / `--config` input or the launch cwd itself, and
    **never** by walking to parent directories, so resolution can neither
    escape nor climb above the launch-cwd boundary (Principle III/IV);
  * **prefer an explicit `--db-path` / `--config` target over the launch cwd**
    for the *served DB set* when such a target is supplied, so read-only DB
    serving decouples from a cwd the CLI may no longer supply. This alone does
    **not** restore registry-backed Generation / background-sync (those
    validate against the launch cwd — see T2d's complete-contract note); the launch-contract
    task **T2d (`056.008-T`)** supplies the trusted identity that actually
    restores connectivity and Generation for H0a;
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
  only makes the cause visible. Curative H0a connectivity restoration is owned
  by the separate launch-contract task **T2d (`056.008-T`)**, which pins the
  child's trusted launch identity (working directory and/or explicit targets).
  T0 must record the cwd/env the CLI uses **and** whether its MCP launch config
  honors a `cwd`/working-directory (and `env`) field, because that determines
  which T2d lever applies. If T0 shows the CLI already supplies a usable
  cwd-local `.graphtor` or an explicit target, both T2 and T2d close as
  *not-needed* and no code change is required. T4's connectivity gate depends on
  this record.
* Green T1. Preserve all existing fail-closed semantics (malformed registry,
  missing explicit `--config`, pre-v4 gate, duplicate-intake preflight remain
  pre-serve gates) — add a regression assertion that each of these still exits
  pre-serve after the cwd-resolution change, so robustness never silently
  converts a fail-closed gate into a fail-open path. Do **not** add an
  unrelated variant or optional logging in this task.
* Width: serve startup runtime resolution, one failure mode. The managed
  launch-contract generation (T2d), the stale-lock liveness variant (H0b), and
  the optional diagnosability sink are **isolated** into the separate tasks
  below so this task implements exactly one evidenced runtime cause.

#### T2d — (Conditional on H0a evidence) Managed launch-contract generation — backlog `056.008-T`

* Only if T0 evidences H0a. **Distinct width from T2:** this changes the
  **install/config surface** (`src/workspace/mcp_config.rs`), not runtime
  `cmd_serve`. The generated managed `.mcp.json` entry today carries only
  `command` + `args: ["serve"]` + `transport` + the managed marker, so the CLI
  launches the child with no trusted workspace identity.
* **Complete minimal contract (verified against `run` / `cmd_serve` /
  `load_source_config` / `serve_discovery::classify_serve_postures` /
  `spawn_background_sync`):**
  * **Primary lever — pin the child working directory (`cwd`) to the project
    root.** Registry discovery (`load_source_config` → `cwd.join(".graphtor/config")`),
    posture/Generation-target validation (`validate_path(.., root=cwd)`), DB
    auto-discovery (`cwd.join(".graphtor")`), and background sync
    (`acquire_plan::plan(.., &cwd)`, `data_root = cwd.join(".graphtor/data")`)
    are **all** cwd-anchored. Pinning cwd restores registry-backed Generation
    and background-sync **together** and preserves the genuinely-absent-registry
    zero-config case — **without** relaxing the runtime cwd containment boundary
    (T2 keeps validating against the launch cwd, which is now the project root).
  * **Complement — pin explicit `--db-path` (the DB target) and, when a
    registry exists, `--config`** — needed only when the served DB / registry
    legitimately lives outside the pinned cwd. Explicit targets **alone** do
    NOT restore Generation/background-sync, because posture classification and
    the acquisition plan validate source paths and targets against the launch
    `cwd` (`root`); a pinned target outside that cwd is refused by
    `validate_path(&target, root)`. Do **not** fabricate a `--config` when the
    registry is genuinely absent (`load_source_config` errors on a missing
    explicit override; an absent default correctly falls through to zero-config).
* **Evidence gate on the launch mechanism:** T0 records whether the CLI's MCP
  launch config honors a `cwd`/working-directory (and `env`) field. If it does,
  the `cwd` pin (primary lever) is the fix. If it does **not**, restoring
  registry-backed Generation under a foreign cwd would require relaxing the
  containment root (explicitly refused); the remediation then reduces to pinning
  `--db-path` for read-only DB serving plus an operational recipe (launch from
  the project root), recorded in T0/`056.001-T`.
* **Containment (required):** every pinned path (cwd, `--db-path`, `--config`)
  is derived from the project root at generation time and validated within
  project-root `.graphtor` via the shared `graphtor_core::path::validate_path` /
  `is_reparse_point` primitives (both operands canonicalized). No
  parent-directory traversal.
* **Test:** an unrelated-cwd launch regression test — a managed entry generated
  for project `P` must serve `P`'s databases (and, when a registry exists,
  classify its real source targets `Generation`) when the child is spawned from
  an unrelated cwd.
* Contingency: close as *not-needed* with a one-line rationale if H0a is not
  evidenced (Constitution VI). Width: managed launch-config generation only.

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
* **Adjudication (retain, evidence-gated — not speculative):** the sink is kept
  in the plan only because it targets a **distinct evidenced condition** the
  default cannot cover — T0 showing the CLI **discards** the child's stderr so
  the `logs/serve-stderr.log` redirect is impossible/insufficient. It is **not**
  general speculative logging: if T0 shows child stderr is capturable via the
  documented redirect (the common case), this task closes as *not-needed* and
  no sink is built.
* Contingency: close as *not-needed* if the `logs/` stderr redirect fully
  solves the evidenced case. Width: logging/diagnosability only.

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
* **Branch-sensitive baseline:** the observation "before" state matches the T0
  evidence — for an **H0** cause it is the nonzero child exit + early-exit
  marker (+ client-visible OS error 232); for an **H1** cause it is a
  bounded-`initialize` timeout with the child **still alive** (latency, no
  early exit). The success signal is identical for both: a completed
  `initialize` handshake with no OS error 232.
* Dependency note: T4 depends on the fix tasks, but **every fix task is now
  conditional** and may be **closed as *not-needed*** when its hypothesis is
  not the evidenced cause: T2 cmd_serve (H0a) = `056.003-T`, T2d launch-contract
  (H0a) = `056.008-T`, T2b stale-lock (H0b) = `056.007-T`, T2c diagnosability =
  `056.006-T`, T3 model lazy-load (H1) = `056.005-T`. Exactly one causal branch
  activates from the T0 evidence (H0a → T2 + T2d; H0b → T2b; H1 → T3; H0c →
  diagnosability/operational); the non-selected tasks close as *not-needed*,
  which **satisfies** T4's dependency on them — T4 does not wait for a
  conditional task that evidence ruled out.
* Width: runtime verification + closure evidence.

## Verification Commands

```text
# Evidence capture (T0), from the CLI's launch cwd — one command per line,
# evidence written under logs/ (never the repo root):
$env:RUST_LOG = 'debug'
graphtor-docs serve 2> logs/serve-stderr.log
echo "exit=$LASTEXITCODE"
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
  discovery error; the conditional T2d launch-contract, T2b richer lock
  metadata, and T2c opt-in log sink are separate, evidence-gated tasks) and
  behavior-preserving for the happy path; revert commits in reverse dependency
  order if needed. The T2d managed-`.mcp.json` change is reversible by
  regenerating the entry (or `git revert`) and re-pins nothing outside
  project-root `.graphtor`.
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
  hand-rolled check, so containment stays a single source of truth. The
  conditional T2d launch contract only pins project-root-derived `cwd` /
  `--db-path` / `--config` values, each validated within project-root
  `.graphtor` by the same primitives, and pinning `cwd` does not relax the
  runtime boundary (the launch cwd becomes the project root). The refusal
  test enumerates the escape vectors — absolute-above-boundary, `..`-traversal,
  escaping symlink, junction/reparse-point, and a Windows short-name/case
  variant — each asserted refused; no containment relaxation.
* **V Observability** — positive serve-ready startup log; a documented
  stderr-redirect recipe is the diagnosability default, with the conditional
  T2c opt-in file sink taken only if that recipe is insufficient.
* **VI Single Responsibility** — every fix task is evidence-gated and
  single-width; the managed launch-contract (T2d) is split from runtime
  `cmd_serve` (T2); rmcp bump and model lazy-load taken only if evidence
  requires; no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — none.
* **VIII Safety Modes** — investigate-first (T0/T1 before fix).
* **XI Merge-commit history** — Ship enforces merge-commit-only at merge time.

## Plan Hardening Signals

* Public API, schema, or contract change: **present (bounded, conditional on
  H0a)** — the conditional T2d task changes the **managed `.mcp.json` launch
  contract** generated by `src/workspace/mcp_config.rs` (`managed_server_value`
  / `generate_mcp_config`): the managed entry gains a pinned child working
  directory (`cwd`) and/or explicit `--db-path` / `--config` launch arguments.
  This is an install/config-surface contract, not a library public API, wire, or
  DB-schema change (no `get_info` change; no runtime MCP tool-contract change;
  an rmcp bump remains conditional and separately reviewed). The runtime fix
  (`cmd_serve`, `src/lock.rs`, `src/logging`) stays internal.
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
pre-serve `eprintln!` early-exit messages; (5) the conditional T2d launch
contract must pin only project-root-derived paths — the generated `cwd`,
`--db-path`, and `--config` values must each resolve **within** project-root
`.graphtor` after the shared `validate_path` / `is_reparse_point` containment
checks (both operands canonicalized), never a parent-traversed or out-of-root
target, and pinning `cwd` must not relax the runtime cwd containment boundary
(T2 continues validating against the launch cwd, which becomes the project root).

Instruction files / learnings consulted: `.github/instructions/constitution.instructions.md`
(III/IV, VIII), `.github/instructions/rust.instructions.md` (no `unwrap`/`expect`
in library code; `Result` propagation),
`docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`
(confirms the `serve_server` wiring is correct, so the failure is startup
early-exit, not malformed construction), and the sibling readonly-serve
hardening / serve auto-discovery decided plans for the cwd-relative discovery
and posture-classification context.

### Risky actions (ProposedAction / ActionRisk / ActionResult)

* ProposedAction (conditional, T2 H0a runtime): replace launch-cwd-relative
  workspace-root resolution with a containment-safe resolution (explicit
  `--db-path` / `--config` preferred, or launch cwd only, no parent walk;
  containment delegated to the shared `validate_path` / `is_reparse_point`
  primitives with both operands canonicalized) and convert every silent exit-2
  discovery site into a loud, actionable error.
  * targets: `src/main.rs::cmd_serve` (~2446-2655); reuse of
    `graphtor_core::path` / `src/workspace/serve_discovery.rs` containment
    primitives.
  * change_kind: local edit to startup control flow.
  * ActionRisk: **moderate** — startup-critical but non-destructive and
    behavior-preserving on the happy path; guarded by the T1 red harness and an
    explicit outside/parent refusal test.
  * rollback: `git revert` the T2 commit(s) in reverse dependency order.
  * approval_required: no (non-destructive); ActionResult: **planned** (or
    **abandoned** if H0a is not the evidenced cause).
* ProposedAction (conditional, T2d H0a launch-contract): when T0 confirms H0a,
  emit a trusted, containment-validated launch identity in the generated managed
  `.mcp.json` entry — **primary lever: pin the child working directory (`cwd`)
  to the project root** (restores cwd-anchored registry discovery, DB
  auto-discovery, posture/Generation validation, and background sync together
  without relaxing the runtime cwd boundary); complement: pin absolute
  `--db-path` (and `--config` when a registry exists) for a served DB/registry
  outside that cwd. Explicit targets alone do not restore Generation. Preserve
  the genuinely-absent-registry zero-config case.
  * targets: `src/workspace/mcp_config.rs` (`managed_server_value` ~528-540,
    `generate_mcp_config`); reuse of the shared
    `graphtor_core::path::validate_path` / `is_reparse_point` containment
    primitives (both operands canonicalized).
  * change_kind: install-time managed-`.mcp.json` launch-contract generation.
  * ActionRisk: **moderate** — changes the launch contract the CLI consumes;
    every pinned `cwd` / `--db-path` / `--config` must resolve within
    project-root `.graphtor` (no parent traversal) and is guarded by an
    unrelated-cwd launch regression test. Taken only if H0a is evidenced and the
    CLI honors the chosen launch field (T0). rollback: revert the T2d commit /
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
  an unrelated cwd. An explicit refusal test
  enumerates the escape vectors (absolute-above, `..`-traversal, escaping
  symlink, junction/reparse-point, Windows short-name/case variant), each
  asserted refused; a regression assertion confirms each fail-closed gate
  (malformed registry, missing explicit `--config`, pre-v4, duplicate-intake)
  still exits pre-serve after the cwd-resolution change. All four quality gates
  plus `cargo build --release`.
* Rollback: revert shipment commits in reverse dependency order; re-pin prior
  rmcp if bumped (T4).
* Post-deploy observation window (manual — no hosted observability is
  available; STDIO serve is a local child process):
  * **owner:** the merging developer (Ship / operator); no on-call rotation.
  * **pre-fix baseline (branch-sensitive):** the T0 capture, recorded in
    `056.001-T` and referenced here as the "before" state — for an **H0** cause
    a concrete nonzero child exit code + early-exit stderr marker + the
    client-visible OS error 232; for an **H1** cause a bounded-`initialize`
    timeout with the child **still alive** (latency, no early exit) on the
    newest Copilot CLI.
  * **exact method / invocation:** for each of the next 3 serve starts (or 24h,
    whichever comes first) run `/mcp show graphtor-docs` on the newest Copilot
    CLI and capture the child's stderr with `RUST_LOG=debug` redirected to
    `logs/serve-stderr.log` (per the Verification Commands recipe).
  * **files / log signals:** `logs/serve-stderr.log` shows the serve-ready
    startup log (transport bound) and no OS error 232; `/mcp show
    graphtor-docs` reports connected with a completed `initialize` handshake.
  * **success trigger:** all 3 starts (or the 24h window) complete the
    handshake with no OS error 232 → outcome `healthy`.
  * **rollback trigger:** any OS error 232 recurrence, an exit-before-initialize
    (H0), or a failed/timed-out `initialize` handshake (H1) in the window →
    revert the shipment commits in reverse dependency order (T4) → outcome
    `rolled-back`.
  * outcome (healthy / degraded / rolled-back) is recorded in the shipment
    closure artifact (T4).

## Test-First Harness Expectations

* `tests/mcp_serve_handshake_test.rs` must exist and be **red** (reproducing the
  T0-captured cause — an exit-before-initialize for **H0** or a
  bounded-`initialize` timeout with the child still alive for **H1**) before the
  evidenced fix task (T2/T2d/T2b/T3).
* The harness spawns the real binary out-of-process with a controlled cwd/env
  and pinned model-cache state so it is deterministic — not a happy-path
  in-process fixture (which would negotiate the handshake and pass trivially).
* The harness keeps the child's stdin **open**, writes a protocol-valid
  newline-delimited `initialize` JSON-RPC request, and awaits/validates the
  `initialize` response under a **bounded timeout**. Red is branch-sensitive:
  for **H0** the `initialize` write fails on a closed pipe or the response never
  arrives because the child exited first (harness captures child exit code +
  stderr and ties it to the T0 marker); for **H1** the response misses the
  deadline while the child is **still alive** (latency, no exit code). Green
  (both) = a successful `initialize` response. An empty/closed stdin is
  explicitly disallowed — it would only exercise a benign EOF-driven shutdown
  and could not distinguish the regression.
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
* Backlog scope: shipment `049-S` / feature `056-F`, tasks `056.001-T`..`056.008-T`
  (T0 `056.001-T`, T1 `056.002-T`, and the mutually-exclusive evidence-gated fix
  tasks T2 `056.003-T` + T2d `056.008-T` (H0a), T2b `056.007-T` (H0b), T2c
  `056.006-T` (diagnosability), T3 `056.005-T` (H1), plus verification T4
  `056.004-T`).

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

### Cycle 3 remediation (final targeted review-fix — not a fresh full persona run)

A third and final review-fix cycle (hard cap) at HEAD `59e883a` addressed merged
high-confidence findings. These are targeted documentation/backlog remediations
by the Stage agent, verified against the actual source via ENGRAM_DIRECT graph /
search plus exact reads; **not** a new multi-persona run, and the Cycle 1 gate
decision above still stands (no fresh PASS is claimed).

* **Dependency coherence (P1, blocking)** — `056.003-T` was unconditional but
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
