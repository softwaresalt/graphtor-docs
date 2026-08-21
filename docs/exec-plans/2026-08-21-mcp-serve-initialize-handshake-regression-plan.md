---
title: "Implementation Plan: Fix graphtor-docs MCP serve initialize-handshake regression (Copilot CLI OS error 232)"
description: "Grounded, evidence-first, test-first plan to restore graphtor-docs MCP STDIO serve compatibility with recent GitHub Copilot CLI builds by capturing the child-process exit cause and hardening the pre-serve startup path"
topic: "graphtor-docs MCP serve initialize handshake"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
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
| Serve startup / early-exit paths | `src/main.rs::cmd_serve` (~2446-2655) | Deterministic workspace-root resolution independent of launch cwd; convert silent exit-2 discovery failure into a loud, actionable error; keep fail-closed gates |
| Advisory lock handling | `src/lock.rs` (`acquire_database_lock`, `handle_existing_lock`, ~144-200) | Harden stale-lock liveness (record process start-time alongside pid to survive pid reuse) if H0b is implicated |
| Diagnostic logging sink | `src/logging/init.rs`, serve path in `src/main.rs` | Opt-in file-log sink (env-gated) so early-exit reasons survive a discarded child stderr |
| Embedding-model resolution (conditional) | `src/embed/resolver.rs`, consumers in `src/mcp/server.rs` | Only if H1 evidenced: lazy `tokio::sync::OnceCell` + `spawn_blocking`, distinct "model loading" tool error |
| MCP dependency (conditional) | `Cargo.toml` (~43, `rmcp = "1.5"`) | Only if H3 evidenced: bump rmcp (1.8.0 available). **No `get_info` protocol-echo change — it is a no-op on rmcp 1.5.** |
| Tests | `tests/mcp_serve_handshake_test.rs` (new) | Out-of-process red-first harness asserting exit-before-initialize under a controlled cwd/env |

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

* Add `tests/mcp_serve_handshake_test.rs` that **spawns the real binary** with
  a controllable cwd/env and an empty stdin, and asserts the reproduced failure
  mode: the process **exits before answering `initialize`** (nonzero exit /
  early-exit stderr), pinning model-cache state so the harness is deterministic.
* Stop gate: if the harness cannot be made red, halt and return to T0 rather
  than refactoring startup on a green test.
* Deliverable: a **red** harness encoding the T0 evidence.
* Width: test infrastructure only.

### T2 — Harden the implicated pre-serve failure + diagnosability (green)

* Fix the H0 sub-cause identified by T0/T1:
  * cwd sensitivity → resolve the workspace root deterministically (not purely
    from `cwd`) and/or emit a loud, actionable error instead of a silent
    exit-2 when discovery finds nothing;
  * lock contention / pid reuse → record process start-time alongside pid so a
    reused pid is not misread as a live lock holder;
* Add an **opt-in diagnostic file-log sink** (env-gated) so the exit reason is
  captured even when the CLI discards child stderr.
* Green T1. Preserve all existing fail-closed semantics (invalid registry,
  pre-v4 gate, duplicate-intake preflight remain pre-serve gates).
* Width: serve startup + lock + logging (one failure mode).

### T3 — (Conditional on H1 evidence) Defer model load off the handshake

* Only if T0/T1 shows handshake latency (not an early exit) is implicated:
  lazy-load **only** the embedding model via `tokio::sync::OnceCell` +
  `spawn_blocking`; make the affected tool handlers `async`; return a distinct
  retryable "model still loading" error (not the existing "semantic search is
  disabled" message) and stop `research_topic` from *silently* degrading to
  unranked text search during the load window.
* **Keep DB open, lock acquisition, the pre-v4 gate, and the duplicate-intake
  preflight as pre-serve fail-closed gates** — do not convert loud pre-connect
  failures into silent per-tool errors.
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

* T2 hardening is additive (deterministic root resolution, richer lock
  metadata, opt-in log sink) and behavior-preserving for the happy path;
  revert commits in reverse dependency order if needed.
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
  deterministic workspace-root resolution must still reject out-of-workspace
  paths; no containment relaxation.
* **V Observability** — serve-ready startup log plus opt-in diagnostic file
  sink so failures are never invisible.
* **VI Single Responsibility** — rmcp bump and model lazy-load taken only if
  evidence requires; no speculative `get_info` change (proven no-op).
* **VII Destructive Approval** — none.
* **VIII Safety Modes** — investigate-first (T0/T1 before fix).
* **XI Merge-commit history** — Ship enforces merge-commit-only at merge time.

## Test-First Harness Expectations

* `tests/mcp_serve_handshake_test.rs` must exist and be **red** (reproducing
  exit-before-initialize) before T2/T3.
* The harness spawns the real binary out-of-process with a controlled cwd/env
  and pinned model-cache state so it is deterministic — not a happy-path
  in-process fixture (which would negotiate the handshake and pass trivially).
* Existing MCP tests (`tests/mcp_manifest_test.rs`, server unit tests in
  `src/mcp/server.rs`) must continue to pass unchanged.
