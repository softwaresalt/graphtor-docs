---
title: "MCP serve initialize-handshake regression: Copilot CLI OS error 232 (7BF1961D)"
description: "Investigate-first differential diagnosis of the graphtor-docs MCP serve pipe-close-on-initialize regression triggered by recent GitHub Copilot CLI builds, and the chosen evidence-first remediation ordering"
topic: "graphtor-docs MCP STDIO serve initialize-handshake compatibility with recent Copilot CLI builds"
depth: "lightweight"
decision_status: "decided"
promoted_to: "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
tags:
  - mcp
  - serve
  - initialize-handshake
  - rmcp
  - regression
  - investigate-first
---

## Problem Frame

Stash bug `7BF1961D` (priority high) reports that the `graphtor-docs` MCP
server fails during client initialization on load: the GitHub Copilot CLI
(`Copilot.exe` / GHCP CLI) reports "Failed to connect to MCP server
`graphtor-docs`" because the transport pipe closes with **OS error 232**
while the client is sending the `initialize` request. The regression began
with the most recent Copilot CLI builds; the graphtor-docs binary and its
`serve` code path were unchanged across the regression boundary.

Windows OS error 232 is `ERROR_NO_DATA` — "The pipe is being closed." On the
server side this surfaces when the process writes to a STDIO pipe whose read
end is **gone** — i.e. the peer process has exited or been killed. The client's
~500-byte `initialize` request fits trivially in the OS pipe buffer, so a
merely slow server reader does not by itself produce a mid-write 232 on the
client; the signature points at **the server process exiting (or being killed)
before the handshake completes**. In an MCP STDIO launch this most often means
the server hit an early-exit / fail-closed path before `rmcp::serve_server`
ever bound the transport.

This is an investigate-first situation: the trigger is an external component
(the CLI) that we do not control, the exact root cause is not yet proven, and
more than one plausible cause exists. This artifact records the differential
diagnosis and fixes the remediation ordering so the implementation plan can be
executed test-first without over-committing to a single unverified hypothesis.

## Evidence Gathered (read-only)

* `src/main.rs::cmd_serve` (lines ~2446-2655) has **six pre-`serve_server`
  early-exit paths plus fallible `?` operators**, all of which exit or return
  before the STDIO transport binds:
  * `no databases found to serve` (exit 2) — auto-discovery of `.graphtor/*.db`
    is **cwd-relative** (`cwd.join(".graphtor")`). If the launching client
    starts the child process with a different working directory than before,
    discovery finds nothing and the process exits 2.
  * `acquire_database_lock` (`src/lock.rs`) returns `DatabaseLocked` when
    another live process holds `.graphtor/<db>.lock`, or when a stale lock's
    recorded pid has been reused. A client that spawns a probe instance,
    restarts servers, or hard-kills children makes this fire.
  * pre-v4 schema gate, invalid/missing `sources.yaml` (fail-closed `Err`),
    the duplicate-intake preflight, and `open_serve_databases` open failures.
* `docs/troubleshooting.md` already documents a "serve starts and exits within
  seconds" failure class for exactly these causes — an unchanged binary can
  regress purely from a change in *how* the client launches it.
* rmcp 1.5 **negotiates `protocolVersion` inside the SDK**: the service layer
  overwrites the value returned by `get_info` with either the client's offer
  (when the client's version is lower) or the server's `LATEST`
  (`2025-11-25`, already the newest published MCP revision). `partial_cmp`
  over `ProtocolVersion` is total, so `UnsupportedProtocolVersion` is
  unreachable and a `get_info` change **cannot** alter the wire response. A
  protocol-version mismatch is therefore not a credible cause on rmcp 1.5.
* `resolve_embedding_model` (`src/embed/resolver.rs`) loads the candle
  all-MiniLM-L6-v2 model synchronously before the transport binds (seconds on
  a cold cache / Hub fetch). This can delay the handshake, but on its own
  produces a *client-side* connect timeout rather than the observed mid-write
  server 232, so it is at most a secondary contributor.
* Logging is routed to **stderr** (`src/logging/init.rs`,
  `.with_writer(std::io::stderr)`), and the `serve` path emits no `println!`
  to stdout before `serve_server`. STDOUT contamination is ruled out. A key
  diagnosability gap: if the launching CLI discards the child's stderr, the
  early-exit reason is invisible and only the downstream 232 is observed.
* Compound learning `docs/compound/best-practices/rmcp-1-5-serve-server-pattern-2026-04-30.md`
  confirms the `serve_server` + `#[tool_router]`/`#[tool_handler]` wiring is
  the correct rmcp 1.5 shape, so the failure is not malformed construction.

## Candidate Root Causes

| # | Hypothesis | Supporting signal | In our control | Confidence |
|---|---|---|---|---|
| H0 | **Server process exits before/at `initialize` via a pre-serve early-exit or fail-closed path**, so the client's next write hits a closed pipe → OS error 232. Sub-causes: (H0a) client launches the child with a different **cwd**, so cwd-relative `.graphtor/*.db` discovery finds nothing → "no databases found to serve" exit 2; (H0b) **lock contention / stale-lock pid reuse** when the CLI spawns probe/restart/hard-kill children → `DatabaseLocked`; (H0c) fail-closed gate: invalid/missing `sources.yaml`, pre-v4 schema, duplicate-intake, or DB open failure. | 232 = write to a closed pipe = server gone; six pre-serve exit paths; discovery + locks are cwd-/lifecycle-sensitive; unchanged binary regressed on CLI change; troubleshooting "exits within seconds" class | Yes — robustness + diagnosability | **High** |
| H1 | **Initialize latency from eager model load.** Cold-cache candle model load before the transport binds delays the handshake enough to trip a client connect timeout, after which the client teardown surfaces as 232. Secondary/contributing, not the primary 232 mechanism. | Heavy synchronous pre-transport model load | Yes — lazy-load model only | Medium |
| H2 | Protocol-version negotiation mismatch via `get_info`. | — | — | **Ruled out** — rmcp 1.5 negotiates in-SDK; `get_info` change is a no-op; `LATEST` 2025-11-25 already newest |
| H3 | rmcp 1.5 vs newest CLI framing/transport incompatibility. | Regression tracks CLI builds; rmcp pinned old | Partly — rmcp bump (1.8.0 available) | Low |
| H4 | Startup panic/early-exit writing to stdout. | — | — | Ruled out (stderr logging, no pre-serve stdout) |

H0 is the leading hypothesis and is settled by a **single evidence run** that
captures the server child's exit code and stderr under the exact cwd/env the
CLI uses; a nonzero exit code with an early-exit message confirms it and may
make the latency work (H1) unnecessary. H0 sub-causes and H1 are discriminated
by evidence, not static reading.

## Decision

Proceed **evidence-first**, because the leading hypothesis (H0) is settled by a
single capture run and the remaining fixes are individually small and bounded:

1. **Capture the failure evidence first (T0).** Run the server with the exact
   command line, cwd, and env the CLI uses for the child, capturing exit code
   and `RUST_LOG=debug` stderr to a file, and check for a leftover
   `.graphtor/*.lock`. A nonzero exit code with an early-exit message
   identifies the H0 sub-cause directly.
2. **Reproduce with an out-of-process red harness (T1).** Spawn the real binary
   with a controllable cwd/env and assert the *exit-before-initialize* failure
   mode, with model-cache state pinned so the harness is deterministic. Stop
   gate: if the harness cannot be made red, return to T0 rather than
   speculatively refactoring startup.
3. **Fix the implicated H0 sub-cause (T2), scoped by evidence:** make `serve`
   robust to the launching cwd (deterministic workspace-root resolution and a
   loud, diagnosable error instead of a silent exit), and/or harden stale-lock
   handling (record process start-time alongside pid to survive pid reuse),
   and/or add an **opt-in diagnostic file-log sink** so the exit reason is
   never invisible again.
4. **Only if H1 is evidenced, defer the model load off the handshake (T3):**
   lazy-load *only* the embedding model via `tokio::sync::OnceCell` +
   `spawn_blocking`, with a distinct retryable "model still loading" tool error
   — while **keeping DB open, lock acquisition, the pre-v4 gate, and the
   duplicate-intake preflight as pre-serve fail-closed gates** (do not convert
   loud pre-connect failures into silent per-tool errors). Close as not-needed
   if evidence does not implicate latency (Constitution Principle VI).
5. **Verify against the real newest Copilot CLI** via `/mcp show
   graphtor-docs`, record startup-log evidence, and document rollback.

A `get_info` protocol-echo change is explicitly **excluded** as a no-op on
rmcp 1.5; H3's only real lever is an rmcp bump, taken only if evidence requires.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; all new paths return `Result`; changes
  must pass `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`.
* **II Test-First (NON-NEGOTIABLE)** — the reproduction harness is written and
  observed red before any fix lands.
* **III/IV Workspace isolation & CLI containment** — serve remains localhost
  STDIO; deterministic workspace-root resolution must still reject paths
  outside the workspace; no relaxation of containment.
* **V Structured Observability** — add startup/serve-ready and (if taken)
  negotiated-protocol info logs on stderr, plus the opt-in file-log sink.
* **VI Single Responsibility** — an rmcp upgrade and the model-lazy-load are
  taken only if evidence requires, not speculatively.
* **VII Destructive Approval** — none; no destructive commands.
* **VIII Safety Modes** — investigate-first: evidence (T0/T1) precedes the fix.

## Open Questions / Residual Risk

* The exact cwd/env/lifecycle the newest CLI uses to launch the child is not
  yet captured; T0 must record it along with the child exit code and stderr.
* Diagnosability gap: if the CLI discards child stderr, early-exit reasons are
  invisible — an opt-in file-log sink (or a documented stderr-redirect recipe)
  is high-value regardless of the specific sub-cause.
* Stale-lock liveness is decided by pid today; pid reuse after an ungraceful
  kill yields a false "locked" — prefer recording process start-time over
  `--force` if H0b is implicated.
* If H3 ever dominates, an rmcp bump (1.8.0 is available) may pull transitive
  API changes; that risk is contained to its own conditional task and review.
* If H1/T3 is taken, a lazy model load shifts first-`search_semantic` latency
  to first use and must preserve semantic-search correctness; `research_topic`
  currently *silently* falls back to unranked text search when the model is
  `None` (`src/mcp/server.rs`), so the lazy window needs a distinct
  "loading" signal rather than a silent-degrade or "disabled" message.
