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
The causes may be layered: correcting foreign cwd can expose a later lock,
schema, model, or framing blocker, so every proven prerequisite remains in the
causal chain until the exact client reaches a healthy initialize or is
classified as unsupported.

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
| H3 | **Client/transport incompatibility (two independently owned modes).** **(A)** rmcp 1.5 vs newest CLI framing/transport incompatibility: the child is **alive** but the framed `initialize` never negotiates a `protocolVersion`. **(B)** the CLI **ignores/rejects configured `cwd`**, proven by a temporary secure diagnostic entry invoked through the exact real CLI, so the child starts in a **foreign cwd** and **early-exits** — distinct from H0a, where the same executable honors the requested cwd. | Regression tracks CLI builds; rmcp pinned old (A); real-client cwd probe not honored (B) | Partly — **(A)** minimal Rust-1.75-compatible framing fix (`056.011-T`; rmcp 1.8.x excluded because it requires edition 2024); **(B1)** the same exact CLI passes a second contrast through a different documented working-directory mechanism, activating managed-config work, or **(B2)** that exact CLI has no safe mechanism and blocks shipment as unsupported-client (`056.019-T`); **no** server-side external-path fallback | Low |
| H4 | Startup panic/early-exit writing to stdout. | — | — | Ruled out (stderr logging, no pre-serve stdout) |

H0 is the leading hypothesis. T0 runs one controlled control/treatment contrast
that captures the server child's exit code and stderr under the exact cwd/env the
CLI uses, then emits an ordered classification: if a cwd correction advances to a
later blocker, H0a remains a proven prerequisite while the new H0b/H0c/H1/H3-A
cause is ordered after it. Downstream causal tasks are then visited once in the
authoritative forward chain; evidence, not static reading, orders them, and no
task reopens a completed sibling.

## Decision

Proceed **evidence-first**, because the leading hypothesis (H0) can be ordered
by bounded contrasts and the remaining fixes are individually small and
bounded:

1. **Validate the standalone probe crate, then capture T0.**
   The probe is a standalone, non-published crate at `tools/mcp-probe/` (own
   `Cargo.toml` with `[workspace]` + `publish = false`, Rust 2021 / MSRV 1.75).
   `056.020-T` supplies the core synchronous transport (`src/main.rs` +
   `src/transport.rs`): raw `std::process`/`std::thread` independent full-duplex
   pumps, half-close propagation, bounded buffers, a continuous bounded stderr
   drain, deadline signaling, and a post-write bounded non-blocking copy-delivery
   seam. `056.023-T` owns that copy-only observer seam plus in-wrapper JSON-RPC
   correlation/redaction and redacted evidence (standalone `serde_json`): the
   observer and transcript correlator run inside the wrapper process, delivery is
   bounded non-blocking, saturation/failure atomically marks the summary invalid
   while forwarding is unchanged, raw frames never leave wrapper memory, and the
   wrapper writes only a redacted structured summary plus digests atomically to
   its owned `--evidence-output` (which the T0 runner consumes). `056.022-T` owns
   process spawning, teardown by **direct `Child` handles only**, and the
   versioned `wrapper` subcommand (argv `--inner-exe` / repeated `--inner-arg` /
   `--evidence-output` / `--run-nonce`, byte-identical across control/treatment):
   the exact-CLI runner owns the Copilot `Child` guard and the wrapper owns the
   inner-server `Child` guard (wrapper PID observed-only). A REQUIRED injectable
   observation trait with a standalone `sysinfo` implementation observes
   PID/start-time/executable/parent/nonce for diagnostics and deterministic tests
   but never kills; same-second identity is ambiguous and fails closed; residual
   or unknown descendants fail the evidence and surface exact identities for
   operator-approved action. Because the probe now owns a separate lockfile, a
   standalone `cargo audit` gate applies. `056.021-T` owns the isolated `logs/probe/<nonce>`
   workspace and config fixtures (`workspace.rs` only): exclusive creation
   validated by a probe-local, std-only `canonicalize`/`symlink_metadata`
   containment check (no `graphtor_core` import and no reusable production
   security primitive), temporary in-workspace control/treatment `.mcp.json`
   passing identical wrapper args on both legs (treatment alone adds `cwd`), and
   an owned nested ancestor/child config fixture. Scoped to the probe/T0 only, the
   user `.mcp.json` is never read, modified, backed up, restored, or substituted,
   so no config approval receipt is required.
   T0 (`056.001-T`) owns `exact_cli.rs` and the final `main.rs` subcommand wiring.
   It first proves ancestor config-isolation with the exact CLI against the owned
   nested fixture (the nearest child `.mcp.json` shadows and does not merge the
   sentinel ancestor); if the exact CLI reads or merges the ancestor config it
   stops the causal H0 comparison, emits typed `H3-B-candidate` evidence, and
   continues forward to `056.019-T`, never assuming the repository-root
   `.mcp.json` is unread and never declaring H3-B2 itself. Only after isolation is
   proven does it run ONE control/treatment pair through the `056.022-T` wrapper
   handoff — the child `.mcp.json` uses the wrapper as `command`, the byte-identical
   wrapper args encode the exact absolute production inner executable plus its
   original args, and control/treatment differ only by the treatment
   canonical-project-root `cwd`. It then emits the ordered cause classification and
   ends `done` (or blocks only when evidence capture itself is impossible for an
   explicit non-H3-B reason) — one-shot, with no implementation loop, no
   per-correction rerun, and no reopening of a downstream task. A correlated nonzero early exit
   identifies H0; H0b additionally requires a Generation target. The wrapper is
   diagnostic-only and cannot satisfy T4.
2. **Build a green out-of-process driver (T1).** Spawn the real binary
   with a controllable cwd/env, hold a named stdin handle open, send a protocol-valid
   `initialize`, concurrently drain bounded stderr, and expose a successful
   initialize response as the positive signal. T1 itself ends green and commits
   no branch-specific failure. Each selected curative task owns its failing
   assertion, observes red, implements, and returns green before completion.
   For H1 use an injected blocking/failing loader seam rather than cold-cache
   wall-clock behavior. H3-A reacquires the exact-client transaction separately
   before and after the fix through the `056.022-T` wrapper (validating semantic
   initialize correlation plus the redacted transcript digest, not raw-frame
   replay or persistence). Operational-only H0c/H3-B use bounded actual-client
   before/after transcripts.
3. **Restore connectivity only for the evidenced branch, with isolated
   ownership.** Non-conditional T2 diagnostics (`056.003-T`) introduce one
   exhaustive typed preflight-exit seam covering all normal exits (including
   pre-v4 and duplicate intake), mirror each event to unconditional fatal
   stderr, and add `mcp_serve_ready`; tracing never replaces the stderr message.
   H0a uses typed managed-config outcomes (`056.017-T`), an unconditional
   generator `type`/`transport` discriminator reconciliation plus evidence-selected
   canonical `cwd` generation (`056.008-T`), a no-follow contained recovery
   primitive (`056.018-T`) gated on the `056.024-T` safe-primitive decision (chain
   `056.017 → 056.024 → 056.008 → 056.018`), and existing-install upgrade
   orchestration (`056.009-T`). `056.024-T` records its decision in a NEW
   `docs/decisions/` artifact rather than rewriting this deliberation, and closes
   `not-needed` when no managed-config mutation/recovery is selected. The "never
   read/mutate user `.mcp.json`" invariant is scoped to T0/probe; production
   managed-config tasks operate on the configured workspace `.mcp.json` with typed
   ownership and approval where destructive.
   H0b first records a passing shared `DatabaseLock`/`WorkspaceLock`
   characterization (`056.016-T`), then `056.007-T` owns the red/green
   production change: high-resolution process identity plus a boot/session
   discriminator under one conservative policy. Ambiguous and live legacy
   pid-only identity stays locked; evidenced legacy reused-pid recovery is
   exact-lock, backup-first, and approval-gated.
   H0c repairs state without weakening a gate (`056.010-T`). The optional sink
   (`056.006-T`) uses one exclusive absolute owner-protected file per attempt
   and is selected only when stderr is unavailable and env inheritance is
   proven.
4. **Only if H1 is evidenced, share one supervised lazy model owner across MCP
   and Generation sync.** Typed resolver outcomes (`056.014-T`) distinguish
   `Loaded`, terminal `Disabled`, and retryable `Failed`; the shared
   `src/embed/lifecycle.rs` state-machine owner (`056.005-T`) exposes a stable
   typed lifecycle-state accessor with one serialized retry, remediation, and
   terminal-disabled behavior; the versioned Loading/Failed/Disabled MCP
   availability projection and `search_semantic`/`research_topic` fallback
   metadata are owned by `056.025-T`. `cmd_serve` lifecycle injection and
   background-sync orchestration (`056.015-T`) land after preflight diagnostics and
   keep Generation sync subscribed across `Failed → Loading → Ready`.
   DB/lock/schema/intake gates remain pre-serve fail-closed.
5. **Keep H3 modes separate.** H3-A transport/framing belongs to `056.011-T`,
   which owns transport types/wiring only after `056.015-T` and reacquires the
   exact-client transaction separately before and after the fix through the
   `056.022-T` wrapper (validating semantic initialize correlation plus the
   redacted transcript digest, not byte-identical raw replay or persistence); it
   must use an edition-2021/Rust-1.75-compatible fix; rmcp 1.8.x is excluded; a
   fork/patch override requires separate deliberation. H3-B client capability
   belongs to `056.019-T`, the sole H3-B terminal, which consumes T0's
   `H3-B-candidate` and adjudicates BOTH a documented explicit isolated-config
   discovery mechanism (owning the one bounded attempt, plus exactly one deferred
   control/treatment contrast through the `056.001-T` runner when isolation becomes
   possible) and a distinct documented working-directory mechanism. A
   proven-supported mechanism is H3-B1 forward evidence; a proven-unsupported
   exact identity is H3-B2 and blocks the shipment as unsupported-client; an
   inconclusive result blocks with evidence rather than being classified
   Unsupported. It never deliberately reads or mutates the user root
   config. Neither mode adds an external-path fallback, and only T4 accepts
   production.
6. **Verify production parity with the exact newest failing CLI identity.**
   After a managed branch records target-workspace upgrade refresh and its
   production config hash, three exact-Copilot `/mcp show graphtor-docs` sessions
   must use the restored production command/args/cwd/env. Acceptance for THIS bug
   is that each session completes a valid initialize negotiation (`jsonrpc` 2.0,
   correlated id, no error, `result.protocolVersion`) with no OS error 232, and
   correlates the exact production config hash/file identity with the
   Copilot-spawned server startup event (PID, executable/build, canonical cwd,
   timestamp). If `/mcp show` reports advertised tools, record their list/count as
   supporting evidence, but do not require a tool invocation unrelated to the
   reported load bug and do not route a missing deterministic tool-call UI to
   H3-B2. Separately, a direct T1 production driver confirms the expected MCP
   tools and one side-effect-free `get_status` against the same production
   binary/workspace as a server control (not proof of Copilot UI invocation). This
   does not expand into a new get_status workspace-fingerprint product feature.
   Record CLI path/version/build, production config hash, server PID, timestamp,
   capture path, and result. T0's
   wrapper, temporary config, executable substitution, wrapper PID, and
   wrapper-only logs are invalid T4 evidence. If the optional diagnostic sink
   landed, at least one session runs with its gate off.

A `get_info` protocol-echo change remains excluded as a no-op on rmcp 1.5.
H3-A and H3-B are taken only when T0 selects their distinct evidence and are
owned by `056.011-T` and `056.019-T`, respectively.

The latest exact-HEAD standard review of
`41adf77f1767aaec1b7b588b03fb6ea41d2a67fc` was `BLOCKED`; this decision now
reflects the final user-authorized correction round 3. A fresh current-HEAD
review is still required, so no PASS is claimed.

## Constitution Check

* **I Safety-First Rust** — no `unsafe`; all new paths return `Result`; changes
  must pass `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`.
* **II Test-First (NON-NEGOTIABLE)** — T1 and characterization infrastructure
  finish green. Each curative repository-code task owns its observed red,
  implementation, and green result atomically; operational-only H0c/H3-B use
  bounded actual-client before/after evidence.
* **III/IV Workspace isolation & CLI containment** — serve remains localhost
  STDIO; deterministic workspace-root resolution must still reject paths
  outside the workspace; no relaxation of containment.
* **V Structured Observability** — normalize exits through one typed seam while
  preserving unconditional fatal stderr; tracing is additive.
  `mcp_serve_ready` remains preflight evidence, not handshake evidence. The
  opt-in sink is selected only when stderr is unavailable and env inheritance
  is proven.
* **VI Single Responsibility** — the std-only core synchronous transport, the
  copy-only observer seam with in-wrapper evidence and JSON-RPC correlation,
  process spawning, direct-handle teardown, and the versioned wrapper subcommand,
  the isolated workspace and config fixtures, the exact-CLI run, diagnostics, lock
  policy, embedding lifecycle, the versioned MCP availability projection,
  `cmd_serve`/background-sync orchestration, typed config mutation, generated
  fields, the safe no-follow primitive decision, narrow handle-safe recovery,
  upgrade orchestration, H3 modes, T4 acceptance, and documentation are separately
  owned.
* **VII Destructive Approval** — the probe path is non-destructive to user
  state and requires no approval receipt: `056.021-T` creates the isolated
  `logs/probe/<nonce>` workspace by exclusive creation validated by a
  probe-local, std-only `canonicalize`/`symlink_metadata` containment check (no
  `graphtor_core` import), generates temporary control and treatment `.mcp.json` and the nested ancestor/child fixture only inside that
  owned workspace, and never reads, modifies, backs up, restores, or substitutes
  the user `.mcp.json`. `056.022-T` teardown reaps only via direct `Child`
  handles (the `sysinfo` adapter observes but never kills) and cleanup stays
  within the owned workspace,
  so isolated config creation and owned-workspace cleanup need no operator
  approval. Approval is retained only for genuinely destructive later steps: a
  changing upgrade refresh uses the typed contained recovery primitive before
  mutation; if H0c requires a pre-v4 rebuild via `graphtor-docs sync` or
  source-registry replacement, `056.010-T` requires explicit operator approval
  and a backup before the state-changing remediation; and a live legacy pid-only
  lock is never age-evicted or used to terminate a process — `056.007-T`
  requires explicit approval before exact-lock backup/removal.
* **VIII Safety Modes** — investigate-first: T0 orders causes before curative
  work; each curative task proves its own red/green.

## Open Questions / Residual Risk

* The exact cwd/env/lifecycle the newest CLI uses to launch the child is not
  yet captured; T0 must record it along with the child exit code and stderr.
* The exact Copilot CLI MCP config schema (does the stdio entry use `type` vs
  `transport`; does it honor `cwd`/`env`?) is not yet confirmed for the failing
  build; T0 (`056.001-T`) must record it and `056.008-T` emits the evidenced
  field while preserving legacy recognition. Local `.mcp.json` siblings use
  `type: "stdio"` + `env`, but this is not asserted as the root cause without
  evidence (F7).
* Diagnosability gap: if the CLI discards child stderr but inherits environment,
  T0/T4 may set one unique absolute opt-in sink path per attempt. If env
  inheritance is absent, only non-substituting OS tracing is acceptable for T4.
* Stale-lock liveness is decided by pid today; pid reuse after an ungraceful
  kill yields a false "locked". The selected policy requires strong
  start-time plus boot/session identity; live legacy pid-only records stay
  locked until approval-gated exact-lock recovery.
* If H3-A dominates, candidate rmcp metadata must be checked before coding;
  1.8.x is excluded by edition 2024 and a fork/patch requires a new
  deliberation. H3-B preserves exact CLI identity; B1 proves a distinct
  documented mechanism, while B2 blocks shipment as unsupported-client.
* If H1 is taken, one owner must preserve semantic-search and Generation
  embeddings. `research_topic` must not silently fall back while Loading or
  Failed; terminal Disabled preserves the established disabled behavior.
