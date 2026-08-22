---
title: "MCP serve initialize-handshake regression: Copilot CLI OS error 232 (7BF1961D)"
description: "Investigate-first differential diagnosis of the graphtor-docs MCP serve pipe-close-on-initialize regression triggered by recent GitHub Copilot CLI builds, and the chosen evidence-first remediation ordering"
doc_type: "decision"
topic: "graphtor-docs MCP STDIO serve initialize-handshake compatibility with recent Copilot CLI builds"
depth: "lightweight"
decision_status: "decided"
promoted_to: "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
backlog_refs:
  - "049-S"
  - "056-F"
source: "stash:7BF1961D"
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

Windows OS error 232 is `ERROR_NO_DATA` — "The pipe is being closed." It
surfaces on the **client's** write to the server's **stdin** when that pipe's
read end is **gone** — i.e. the server process has exited or been killed. The
client sends `initialize` by writing to the child's stdin; once the server has
exited, its stdin read end is closed, so the client's write fails with 232.
The client's ~500-byte `initialize` request fits trivially in the OS pipe
buffer, so a merely slow server reader does not by itself produce a mid-write
232 on the client; the signature points at **the server process exiting (or
being killed) before the handshake completes**. In an MCP STDIO launch this
most often means the server hit an early-exit / fail-closed path before
`rmcp::serve_server` ever bound the transport.

This is an investigate-first situation: the trigger is an external component
(the CLI) that we do not control, the exact root cause is not yet proven, and
more than one plausible cause exists. This artifact records the differential
diagnosis and fixes the remediation ordering so the implementation plan can be
executed test-first without over-committing to a single unverified hypothesis.

## Evidence Gathered (read-only)

* `src/main.rs::cmd_serve` (lines ~2446-2655) has **multiple pre-`serve_server`
  early-exit paths plus fallible `?` operators**, all of which exit or return
  before the STDIO transport binds:
  * `no databases found to serve` (exit 2) — auto-discovery of `.graphtor/*.db`
    is **cwd-relative** (`cwd.join(".graphtor")`). If the launching client
    starts the child process with a different working directory than before,
    discovery finds nothing and the process exits 2.
  * `DatabaseLock::acquire` (`src/lock.rs`, delegating to
    `AdvisoryLock::acquire` / `handle_existing_lock`) returns `DatabaseLocked`
    when another live process holds `.graphtor/<db>.lock`, or when a stale
    lock's recorded pid has been reused. A client that spawns a probe
    instance, restarts servers, or hard-kills children makes this fire. This
    path is reachable only for a registry target classified as
    `ServeMode::Generation`; ReadOnly/auto-discovered targets do not acquire
    the database lock.
  * pre-v4 schema gate, a **malformed** `sources.yaml` (fail-closed `Err`),
    an explicit `--config` target that does not exist (exit 2), the
    duplicate-intake preflight, and `open_serve_databases` open failures. A
    genuinely **absent default** `sources.yaml` is NOT a gate: `serve` falls
    through to `.graphtor/*.db` auto-discovery (zero-config consumption), so
    only a malformed registry or a missing *explicit* `--config` fails closed.
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
  server 232, so it is at most a secondary contributor. A model **load
  failure** returns `Ok(None)` (graceful degrade), not an `Err`, so it is
  **not** a pre-serve fail-closed early exit and cannot itself produce 232 —
  only a slow/cold load (latency, H1) can contribute. Implementers must not add
  model resolution to the pre-serve fail-closed gate list during T2.
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
| H0 | **Server process exits before/at `initialize` via a pre-serve early-exit or fail-closed path**, so the client's next write hits a closed pipe → OS error 232. Sub-causes: (H0a) client launches the child with a different **cwd**, so cwd-relative `.graphtor/*.db` discovery finds nothing → "no databases found to serve" exit 2; (H0b) **lock contention / stale-lock pid reuse** on a Generation target when the CLI spawns probe/restart/hard-kill children → `DatabaseLocked`; (H0c) fail-closed gate: a **malformed** `sources.yaml` or a missing **explicit** `--config` target, pre-v4 schema, duplicate-intake, or DB open failure (an **absent default** registry is not a gate — it falls through to `.graphtor/*.db` auto-discovery). | 232 = write to a closed pipe = server gone; multiple pre-serve exit paths; discovery + Generation-lock handling are cwd-/lifecycle-sensitive; unchanged binary regressed on CLI change; troubleshooting "exits within seconds" class | Yes — robustness + diagnosability | **High** |
| H1 | **Initialize latency from eager model load.** Cold-cache candle model load before the transport binds delays the handshake enough to trip a client connect timeout, after which the client teardown surfaces as 232. Secondary/contributing, not the primary 232 mechanism. | Heavy synchronous pre-transport model load | Yes — lazy-load model only | Medium |
| H2 | Protocol-version negotiation mismatch via `get_info`. | — | — | **Ruled out** — rmcp 1.5 negotiates in-SDK; `get_info` change is a no-op; `LATEST` 2025-11-25 already newest |
| H3 | **Client/transport incompatibility (two modes).** **(A)** rmcp 1.5 vs newest CLI framing/transport incompatibility: the child is **alive** but the framed `initialize` never negotiates a `protocolVersion`. **(B)** the CLI **ignores/rejects configured `cwd`**, proven by a temporary backup-first diagnostic entry invoked through the real CLI, so the child starts in a **foreign cwd** and **early-exits** — distinct from H0a, where the same real-client probe honors the requested cwd. | Regression tracks CLI builds; rmcp pinned old (A); real-client cwd probe not honored (B) | Partly — **(A)** rmcp bump/minimal framing fix with Rust-1.75 verification; **(B1)** supported CLI version honors managed `cwd`, keeping `056.008-T`/`056.009-T`; or **(B2)** a different client-honored working-directory mechanism, closing those tasks; **no** server-side external-path fallback | Low (owned by `056.011-T`) |
| H4 | Startup panic/early-exit writing to stdout. | — | — | Ruled out (stderr logging, no pre-serve stdout) |

H0 is the leading hypothesis and is settled by a **single evidence run** that
captures the server child's exit code and stderr under the exact cwd/env the
CLI uses; a nonzero exit code with an early-exit message confirms it and may
make the latency work (H1) unnecessary. H0 sub-causes and H1 are discriminated
by evidence, not static reading.

## Decision

Proceed **evidence-first**, because the leading hypothesis (H0) is settled by a
single capture run and the remaining fixes are individually small and bounded:

1. **Capture the failure evidence first (T0).** Invoke the actual target CLI
   through a backup-first temporary transparent wrapper that proxies stdio
   byte-for-byte and records allowlisted/redacted launch context, raw initialize
   framing, exit, and stderr. Own the CLI process, bound it to 30 seconds, clean
   up only its isolated process tree, and restore config on every outcome.
   Direct replay is derived secondary evidence. A correlated nonzero early
   exit identifies H0; H0b additionally requires a Generation target.
2. **Build an out-of-process driver and branch proof (T1).** Spawn the real binary
   with a controllable cwd/env, hold a named stdin handle open, send a protocol-valid
   `initialize`, and make the sole pass assertion a successful initialize
   response. Capture exit, stderr, or a still-alive timeout only as diagnostic
   evidence explaining the red. For non-H1 fixtures, pin/prewarm model state
   out of the path; for H1 use an injected blocking/failing loader seam rather
   than cold-cache wall-clock behavior. Stop gate: if the
   successful-response assertion cannot be made red for the evidenced
   repository-code cause, return to T0 rather than speculatively refactoring
   startup.    H3-A replays T0's raw target-client transaction. Operational-only H0c/H3-B use bounded actual-client before/after
   transcripts instead of a permanently failing Cargo test.
3. **Restore connectivity for the evidenced H0 sub-cause, split by width:** the
   runtime `cmd_serve` change (`056.003-T`) is **diagnostics-only and
   parity-safe** — convert the four exit-2 messages to structured
   `tracing::error!` events and add `mcp_serve_ready` immediately before the
   `serve_server` call as preflight-complete/about-to-call evidence, **without** changing containment
   or discovery: `cmd_serve` keeps validating an explicit `--db-path` /
   `--config` against the authorized launch cwd through the shared
   `discover_served_databases` / `validate_path` / `is_reparse_point` primitives
   (`candidate_root = cwd`), with **no target-derived/split authorized root** and
   **no** parent-directory walk (Principle III/IV). **Curative-connectivity
   note:** a loud early-exit alone does not clear
   OS error 232, and registry discovery, posture/Generation-target validation,
   DB auto-discovery, and background sync are **all** anchored to the launch
   `cwd` (`load_source_config` → `cwd/.graphtor/config`; `validate_path(..,
   root=cwd)`; `cwd/.graphtor`; `acquire_plan::plan(.., &cwd)`). The
   curative H0a lever therefore lives in a separate launch-contract task
   (`056.008-T`) that pins the child's **working directory** to the canonical
   project root. This single sufficient lever restores registry-backed
   Generation/background-sync together and makes existing explicit-target
   validation resolve correctly without relaxing runtime containment. It does
   not generate `--db-path`, `--config`, or env target plumbing. T0 first proves
   that the real target CLI honors its `cwd` field with a temporary,
   backup-first diagnostic entry; a direct child spawn is not client evidence.
   Because
   `generate_mcp_config` runs only from `install`/`install_full` and
   `cmd_upgrade` never rewrites `.mcp.json`, delivering the refreshed managed
   entry to **already-installed** workspaces is a separate H0a/H3-B1 task
   (`056.009-T`: backup-first marker-safe refresh-on-upgrade as the primary
   path, with a manual reinstall recipe as fallback), so the
   reporter's existing install is actually repaired. Isolate the unrelated H0b
   stale-lock liveness variant (record process start-time alongside pid so a
   matching pid+start-time identity stays live **regardless of lock age**; a
   live legacy pid-only lock also stays locked because ownership is ambiguous)
   and the
   optional startup diagnosability sink as **separate conditional tasks**, taken
   only if evidenced, rather than bundling them into the single-cause fix.
   * **H0c fail-closed branch (`056.010-T`):** if T0 instead evidences a
     legitimate fail-closed gate (malformed registry, missing explicit
     `--config`, pre-v4 schema, duplicate intake, or another fail-closed cause),
     the remediation is a **tracked operational correction that repairs the
     workspace state** — never a weakening of the gate — plus, only if needed, a
     bounded code change that makes the cause more actionable (preceded by its
     own red test). Diagnostics (`056.006-T`) surface the cause but do not
     remediate it, so this branch owns the curative path that reaches the
     healthy `initialize` handshake before runtime verification (T4). After
     each approved repair, rerun the same probe; if another sequential H0c gate
     appears, keep the task active and repeat or rescope before further
     mutation. One causal branch (H0a / H0b / H0c / H1 / H3) activates from
     the evidence; non-selected conditional tasks move to `done` with
     `not-needed: <rationale>` comments. (H3 is a client/transport
     incompatibility owned by `056.011-T`, kept live but low-confidence, with
     **two modes**: **(A)** an rmcp/client-transport framing incompatibility
     (child alive, `initialize` never negotiates) and **(B)** the CLI
     ignoring/rejecting configured `cwd`, proven by T0's real-client diagnostic
     probe. B1 selects a supported CLI that honors managed `cwd` and keeps
     `056.008-T`/`056.009-T` active; B2 uses a supported independent mechanism
     and completes them `done` with rationale. Neither adds a server-side
     external-path fallback.)
4. **Only if H1 is evidenced, defer the model load off the handshake (T3):**
   lazy-load *only* the embedding model via clone-shared per-server state such
   as `Arc<tokio::sync::OnceCell<_>>` + `spawn_blocking`, with a distinct
   retryable "model still loading" tool error
   — while **keeping DB open, lock acquisition, the pre-v4 gate, and the
   duplicate-intake preflight as pre-serve fail-closed gates** (do not convert
   loud pre-connect failures into silent per-tool errors). If not selected,
   complete `done` with a not-needed rationale (Constitution Principle VI).
5. **Verify against the real newest Copilot CLI** via three correlated
   `/mcp show graphtor-docs` starts. Record CLI/config/timestamp/PID/capture
   identity, separate preflight from handshake evidence, and document rollback.

A `get_info` protocol-echo change is explicitly **excluded** as a no-op on
rmcp 1.5; H3's levers are **(A)** an rmcp bump / client-transport framing fix
and **(B)** — when the CLI ignores/rejects the managed `cwd` — an
evidence-backed client-compatibility adjustment (a supported CLI version or a
client-honored working-directory mechanism), **never** a server-side
external-path fallback. Both are **owned by the queued conditional task
`056.011-T`** (H3 kept live but low-confidence) and taken only if evidence
requires.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; all new paths return `Result`; changes
  must pass `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`.
* **II Test-First (NON-NEGOTIABLE)** — repository-code branches are preceded
  by observed-red tests; operational-only H0c/H3-B use bounded actual-client
  before/after evidence, and any code change on those branches gets its own red
  test.
* **III/IV Workspace isolation & CLI containment** — serve remains localhost
  STDIO; deterministic workspace-root resolution must still reject paths
  outside the workspace; no relaxation of containment.
* **V Structured Observability** — normalize exit diagnostics through tracing;
  `mcp_serve_ready` means preflight complete/about to call `serve_server`;
  completed handshake evidence stays separate. Add the opt-in sink only if
  actual-client stderr/wrapper capture is insufficient.
* **VI Single Responsibility** — an rmcp upgrade and the model-lazy-load are
  taken only if evidence requires, not speculatively.
* **VII Destructive Approval** — changing upgrade refresh writes a contained
  byte-for-byte recovery file before mutating owned config. If H0c
  requires a pre-v4 rebuild via `graphtor-docs sync` or source-registry
  replacement, 056.010-T requires explicit operator approval and a backup
  before the state-changing remediation.
* **VIII Safety Modes** — investigate-first: evidence (T0/T1) precedes the fix.

## Open Questions / Residual Risk

* The exact cwd/env/lifecycle the newest CLI uses to launch the child is not
  yet captured; T0 must record it along with the child exit code and stderr.
* The exact Copilot CLI MCP config schema (does the stdio entry use `type` vs
  `transport`; does it honor `cwd`/`env`?) is not yet confirmed for the failing
  build; T0 (`056.001-T`) must record it and `056.008-T` emits the evidenced
  field while preserving legacy recognition. Local `.mcp.json` siblings use
  `type: "stdio"` + `env`, but this is not asserted as the root cause without
  evidence (F7).
* Diagnosability gap: if the CLI discards child stderr, early-exit reasons are
  invisible — a documented stderr-redirect recipe is the low-complexity
  default (already exercised in T0). An opt-in runtime file-log sink is
  deferred/conditional and, if ever built, MUST actually capture the pre-serve
  `eprintln!` early-exit messages (convert them to `tracing`, or tee the real
  stderr stream) rather than adding a `tracing`-only sink that would silently
  miss those direct `eprintln!` / AutoStream writes.
* Stale-lock liveness is decided by pid today; pid reuse after an ungraceful
  kill yields a false "locked" — prefer recording process start-time over
  `--force` if H0b is implicated.
* If H3 ever dominates: **mode A** (an rmcp bump, 1.8.0 available) may pull
  transitive API changes, contained to its own conditional task (`056.011-T`)
  and review; **mode B** (the CLI ignores/rejects the managed `cwd`) is repaired
  by an evidence-backed client-compatibility adjustment (a supported CLI version
  or a client-honored working-directory mechanism), never a server-side
  external-path fallback — distinguished from H0a by generated-contract /
  client-capability evidence.
* If H1/T3 is taken, a lazy model load shifts first-`search_semantic` latency
  to first use and must preserve semantic-search correctness; `research_topic`
  currently *silently* falls back to unranked text search when the model is
  `None` (`src/mcp/server.rs`), so the lazy window needs a distinct
  "loading" signal rather than a silent-degrade or "disabled" message.
