---
title: "SUPERSEDED: Stage session memory: H3-A selection and 052-S routing (2026-08-29)"
description: "Historical H3-A selection snapshot superseded by the PR #108 review-fix cycle 2 dependency-closed Rust 1.75 routing record"
doc_type: "memory"
session_date: "2026-08-29"
agent: "stage"
status: "superseded"
superseded_at: "2026-08-29"
superseded_by: "docs/memory/2026-08-29/stage-056-011-h3a-pr108-reviewfix-cycle-2-memory.md"
current_backlog_memory_key: "stage-056-011-h3a-pr108-reviewfix-cycle-2-2026-08-29"
backlog_refs:
  - "056-F"
  - "056.011-T"
  - "056.028-T"
  - "056.029-T"
  - "056.030-T"
  - "056.031-T"
  - "056.032-T"
  - "056.033-T"
  - "049-S"
  - "053-S"
  - "052-S"
linked_artifacts:
  - "docs/decisions/2026-08-29-mcp-serve-discover-preinitialize-evidence.md"
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
  - "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
stash_ids:
  - "7BF1961D"
tags:
  - stage
  - mcp
  - rmcp
  - h3a
  - selection-gate
---

> [!IMPORTANT]
> **SUPERSEDED.** This is a preserved historical selection record. The current
> routing is `056.028-T -> 056.029-T -> 056.011-T`, with `056.029-T` and
> `056.030-T` through `056.033-T` shipped in `053-S` before `052-S`. `052-S`
> has no direct PHASE 1.5 shipment edge and no hard-coded Rust 1.75 entry gate.
> Use the `superseded_by` record and `current_backlog_memory_key` above.

## Historical Scope

Narrow, evidence-driven Stage pass. Planning, backlog, shipment assembly, and
PR preparation only. No source, test, workflow, or config code was written; no
build, test, or lint was run; no shipment was claimed or shipped; no PR was
merged; no admin fallback was used.

## What was decided

The H3-A cause family of `056-F` is **evidenced and selected**. Live
actual-client stderr shows the server completing preflight and model load,
logging `starting MCP STDIO server`, then exiting `2` with rmcp's
`expect initialized request, but received: ... CustomRequest { method: "server/discover" }`
at request `id: 0`.

Corroboration, all read-only:

* `src/main.rs:2645-2650` is the exact `rmcp::serve_server(server, rmcp::transport::stdio())`
  call site with the `context("MCP server failed to start")` wrapper that
  produces the observed exit-2 path.
* Vendored `rmcp-1.5.0/src/service/server.rs`: the pre-`initialize` loop
  answers only `PingRequest`; anything else breaks the loop and fails the
  `InitializeRequest` destructure, returning `ExpectedInitializeRequest` at
  `server.rs:201`. That construction site is the only one producing the
  `Request(JsonRpcRequest { .. })` wrapper shape seen in the stderr.
* `ExpectedInitializedNotification` is `#[deprecated]` since rmcp 1.4 and never
  constructed, so the observed message can only precede a successful
  `InitializeResult` — which proves ordering.
* Upstream MCP draft: `server/discover` is a **draft-specification** discovery
  method used by the client's dual-era stdio backward-compatibility fallback
  probe — publicly specified rather than vendor-private, but not a finalized MCP
  release, and not in rmcp 1.5's accepted pre-`initialize` request set
  (wording aligned 2026-08-29 in the PR #108 review-fix cycle; see
  `docs/memory/2026-08-29/stage-056-011-h3a-pr108-reviewfix-memory.md`). The
  spec's own fallback rule is that any
  error other than a recognized modern error (or no response at all) makes a
  conforming client fall back to `initialize`, with the fallback explicitly
  **not** keyed to one error code.

`056.011-T` moved from `selection:pending` to `selection:selected`.

## Substitution scope (deliberately narrow)

The evidence substitutes **only** for the T0 cause-ordering input, and only for
the H3-A family. It does **not** substitute for:

* any other cause family's selection (`049-S` T0 / `056.019-T` still runs);
* `056.011-T`'s exact-client before/after acceptance (needs the `056.022-T`
  wrapper and `056.023-T` observer, both `049-S` deliverables);
* the `049-S` dependency on the `051-S` security prerequisite;
* T4 (`056.004-T`) restored-production acceptance.

The operator evidence is a stderr message, **not** a raw JSON-RPC transcript.
Nothing in these artifacts claims otherwise.

## Routing

Queue order is unchanged except for one appended unit:

```text
050-S  ->  051-S  ->  049-S  ->  [PHASE 1.5: 056.028-T]  ->  052-S
```

`052-S` is a new task-only shipment whose sole member is `056.011-T`, with a
`blocks` dependency on `049-S`. Covering feature `056-F` is excluded per P-015.
The pre-shipment selection-gate query returned zero rows before creation.

A pre-`049-S` emergency hotfix was evaluated and rejected: the acceptance
evidence lives in `049-S`, reordering would bypass a security prerequisite
without causal proof, and P-001 already bounds one in-flight unit so there is
no schedule gain.

## Corrections made to prior planning

1. **rmcp exclusion discriminator was wrong.** The plan and deliberation said
   "rmcp 1.8.x uses edition 2024 and is excluded by Rust 1.75". Vendored
   `rmcp-1.5.0/Cargo.toml` — the version this workspace already pins — **also**
   declares `edition = "2024"` and no `rust-version`. Edition therefore does not
   discriminate 1.5.0 from 1.8.0. The exclusion stands on corrected grounds
   (unproven MSRV parity, wider transitive API surface, rollback isolation).
2. **MSRV/edition candidate gate added.** `056.011-T` must now run
   `cargo +1.75.0 check --all-targets` against the **current unmodified pin**
   as its first step. If that already fails, it is a pre-existing MSRV
   declaration defect that halts for a named bounded Stage follow-up — it may
   not be silently relaxed or silently "proved" in-scope.
3. **Remedy shape constrained.** Exact allowlist (`server/discover` only), no
   arbitrary pre-`initialize` dispatch, ping stays rmcp's passing control,
   normal `initialize` must still occur, JSON-RPC error (`-32601`) is the
   preferred shape with `DiscoverResult` forbidden by default, bounded response
   count, stdout protocol-clean.
4. **Red-first tests specified** (RED-1..RED-5), all in-crate and independent of
   the probe assets, so the red is observable before `049-S` closes even though
   the exact-client acceptance is not.

## Failed / rejected approaches

* Fixing this inside `ServerHandler` or via a `get_info` protocol echo: rmcp
  rejects the request before any handler runs, so the seam must be a
  `Transport<RoleServer>` wrapper placed before `serve_server`.
* Returning a real `DiscoverResult`: would assert modern-era support the server
  does not implement and let a conforming client skip `initialize`, which
  rmcp 1.5 requires. Forbidden by default.
* Bumping rmcp to 1.8: excluded, unproven MSRV parity and wider blast radius;
  a separate deliberation if it is ever needed.
* Folding the remedy into `049-S` or dropping the `051-S` dependency: rejected,
  would erode the evidence unit and a security prerequisite.

## Open questions

1. Exact pre-`initialize` wire order beyond "`server/discover` is `id: 0`".
2. Exact response shape the client accepts (`-32601` vs bounded discovery
   compatibility).
3. Whether the client re-probes, and at what cadence (drives the bounded cap).
4. Whether other cause families are also present — `049-S` T0 still decides.
5. Whether `cargo +1.75.0 check --all-targets` already fails on the current
   pin. Derived from crate metadata only; no build was run in this pass.

## Next steps

1. Ship executes `050-S`, then `051-S`, then `049-S`, then PHASE 1.5
   (`056.028-T`), then `052-S`.
2. When `052-S` is claimed, `056.011-T` runs the MSRV/edition gate first, then
   the RED-1..RED-5 harness, then the adapter, then the before/after
   exact-client reacquisition.
3. If the MSRV gate fails on the current pin, halt and hand back a named
   bounded Stage follow-up on the declared `rust-version`.

## Preservation note

An unrelated operator modification to `.gitignore` was present in the working
tree for the whole session. It was never staged, committed, reverted, or
stashed, and its content hash was verified unchanged before and after branch
creation and before commit.
