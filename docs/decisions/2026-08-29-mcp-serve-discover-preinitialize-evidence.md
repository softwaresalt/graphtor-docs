---
title: "H3-A confirmed: pre-initialize server/discover terminates rmcp 1.5 serve (7BF1961D / 056-F)"
description: "Redacted live actual-client evidence, upstream MCP draft-spec grounding, and the Stage selection decision that flips 056.011-T from selection:pending to selection:selected for the H3-A transport cause family"
doc_type: "decision"
topic: "graphtor-docs MCP STDIO serve compatibility with pre-initialize server/discover probes"
depth: "lightweight"
decision_status: "decided"
stash_ids:
  - "7BF1961D"
linked_artifacts:
  - "docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md"
  - "docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md"
backlog_refs:
  - "056-F"
  - "056.011-T"
  - "049-S"
  - "052-S"
  - "053-S"
  - "056.029-T"
  - "056.030-T"
  - "056.031-T"
  - "056.032-T"
  - "056.033-T"
source: "operator live actual-client stderr, 2026-08-29 (redacted)"
tags:
  - mcp
  - serve
  - rmcp
  - server-discover
  - h3a
  - regression
---

## Scope

This note records the 2026-08-29 operator evidence that resolves the H3-A
(server transport/framing compatibility) cause family of `056-F` from
*hypothesis* to *evidenced defect*, states exactly what it proves, and states
exactly what it does **not** prove. It is deliberately narrow: it changes the
selection state of one remedy task (`056.011-T`) and tightens that task's
acceptance. It does not re-open, re-order, or bypass any other cause family,
and it does not weaken the `049-S` evidence unit or the `051-S` security
prerequisite.

## Evidence

### E1 — Live actual-client stderr (operator, 2026-08-29, redacted)

A recent GitHub Copilot CLI local agent harness launches `graphtor-docs`. The
server's own startup path succeeds: preflight and embedding-model load complete
and the server logs `starting MCP STDIO server`. The process then exits with
code `2` and emits the rmcp error:

```text
expect initialized request, but received: Some(Request(JsonRpcRequest {
  jsonrpc: JsonRpcVersion2_0,
  id: Number(0),
  request: CustomRequest(CustomRequest { method: "server/discover", params: Some(Object {}), extensions: Extensions })
}))
```

The client then reports that the process exited before completing `initialize`
and that tools are unavailable. This is the same failure the stash bug
`7BF1961D` describes and the same regression tracked by `056-F`.

Redaction note: only the message text above and the observed exit code are
carried into this record. Local filesystem paths, workspace identifiers,
executable hashes, environment values, and the surrounding harness transcript
are deliberately **not** reproduced. This note does not claim to contain a raw
JSON-RPC transcript; the byte-level before/after transaction remains
`056.022-T` / `056.023-T` work.

### E2 — Server call site (exact, read-only)

`src/main.rs:2645-2650`:

```text
info!("starting MCP STDIO server");
rmcp::serve_server(server, rmcp::transport::stdio())
    .await
    .context("MCP server failed to start")?
    .waiting()
    .await
```

The `context("MCP server failed to start")` wrapper plus the top-level fatal
renderer is exactly the observed exit-2 path. No graphtor-docs code inspects the
wire before `rmcp::serve_server`.

### E3 — rmcp 1.5.0 pre-initialize loop (vendored crate source, read-only)

`rmcp-1.5.0/src/service/server.rs` — `serve_server_with_ct_inner`:

* The pre-initialize loop tolerates exactly one method: it answers
  `ClientRequest::PingRequest` with `ServerResult::EmptyResult` and continues
  looping.
* Any other `ClientJsonRpcMessage::Request` **breaks** the loop, then fails the
  `let ClientRequest::InitializeRequest(peer_info) = &request else { ... }`
  destructure and returns
  `ServerInitializeError::ExpectedInitializeRequest(Some(ClientJsonRpcMessage::request(request, id)))`
  at `server.rs:201`.
* That construction site is the only one that wraps the payload as
  `Request(JsonRpcRequest { .. })`, which is the exact shape in E1. The
  `server.rs:193` site wraps a non-request message instead.
* `ServerInitializeError::ExpectedInitializedNotification` is `#[deprecated]`
  since rmcp 1.4 with the note "The server no longer gates on the initialized
  notification. This variant is never constructed."

### E4 — Upstream MCP draft specification

`server/discover` is a **draft-specification** MCP discovery method (2026-07-28
era draft) used by a dual-era client's pre-`initialize` fallback probe. It is
defined in the public MCP draft rather than being a vendor-private extension,
but the draft is not a finalized MCP release, and `server/discover` is **not**
in rmcp 1.5's accepted pre-`initialize` request set, which admits only `ping`.

* [Discovery](https://modelcontextprotocol.io/specification/draft/server/discover)
  defines `server/discover` as a request a client may send "before sending any
  other requests".
* [stdio: Backward Compatibility](https://modelcontextprotocol.io/specification/draft/basic/transports/stdio)
  defines the dual-era probe: a client supporting both modern and legacy
  (`initialize`-handshake) servers **SHOULD** send `server/discover` first, and
  the probe has three outcomes:
  * a `DiscoverResult` means the server is modern;
  * a *recognized modern* error such as `UnsupportedProtocolVersionError` means
    the server is modern but version-incompatible, and the client **must not**
    fall back to `initialize`;
  * **any other error, or no response within a reasonable timeout, means the
    server is legacy and the client falls back to the `initialize` handshake.**
* The same section states the fallback "**MUST NOT** be keyed to one specific
  error code: legacy servers respond to unknown pre-`initialize` requests with
  implementation-defined errors (commonly `-32601` or `-32602`) or not at all."

### E5 — Candidate metadata (Cargo metadata inspection, no build run)

* Workspace: `edition = "2021"`, `rust-version = "1.75"`.
* `Cargo.toml` pins `rmcp = { version = "1.5", features = ["server", "transport-io"] }`;
  `Cargo.lock` resolves `rmcp 1.5.0`.
* Vendored `rmcp-1.5.0/Cargo.toml` declares `edition = "2024"` and **no**
  `rust-version` key.
* Vendored `rmcp-1.8.0/Cargo.toml` **also** declares `edition = "2024"` and no
  `rust-version` key.

## Causal conclusion

The failure is **not** a pre-serve early exit (H0-family) and **not** an
embedding-model lifecycle stall (H1). Preflight completes, the model loads, and
`serve_server` is entered. The client's first **non-ping** request (`id: 0`) is
`server/discover`, which rmcp 1.5 treats as a fatal non-`initialize` request,
so the server returns `Err`, `cmd_serve` propagates, the process exits `2`, and
the client's pipe closes — reproducing the original OS error 232 signature from
the client's side.

Because rmcp `>= 1.4` no longer gates on `notifications/initialized`
(E3), the observed `ExpectedInitializeRequest` message can only be produced
before a successful `InitializeResult`. The message therefore proves ordering:
`server/discover` arrives **before or instead of** `initialize`, regardless of
the outer client wording about "exited before completing initialize".

This is H3-A: server-side transport/framing incompatibility with a newer client
handshake, owned by `056.011-T`.

## Selection decision

**Decision:** flip `056.011-T` from `selection:pending` to
`selection:selected`.

**Authority and substitution:** the `056-F` selection gate normally consumes the
`056.001-T` (T0) / `056.019-T` classification after `049-S` closes. E1 is a
*live actual-client* observation of the exact regression from the exact client
class, corroborated by exact server source (E2), exact dependency source (E3),
and the upstream contract (E4). For the **H3-A family only**, that is
equal-or-stronger evidence than a T0 classification pass, so it substitutes for
the T0 *cause-ordering* input. It substitutes for nothing else:

| Gate | Substituted by E1 | Still required |
|---|---|---|
| H3-A cause selection | Yes | — |
| Any other cause family selection | No | `049-S` T0 / `056.019-T` classification |
| Exact wire-order acceptance for the fix | No | `056.022-T` wrapper + `056.023-T` observer before/after reacquisition |
| Response-shape acceptance against exact Copilot | No | Exact-client before/after proof in `056.011-T` |
| Restored-production acceptance | No | `056.004-T` (T4), sole owner |
| `051-S` security prerequisite ordering | No | Unchanged; `049-S` still depends on `051-S` |

**Not claimed:** E1 is a stderr message, not a captured raw JSON-RPC
transcript. It does not establish how many pre-`initialize` requests the client
sends, what it does after receiving a response, whether it re-sends
`server/discover`, or which response shape it accepts. Those remain open and
are exactly what `056.011-T`'s before/after reacquisition must close.

## Remedy shape (constraints, not implementation)

The remedy is a narrow transport adapter between `rmcp::transport::stdio()` and
`rmcp::serve_server`, built against the Rust-1.75-compatible rmcp dependency
strategy selected and pinned by `056.029-T`. `056.011-T` consumes that strategy
without changing it. Constraints:

1. **Exact allowlist.** Intercept exactly one method, `server/discover`, and
   only while armed (pre-`initialize`). Nothing else is intercepted,
   dispatched, or answered; a post-`initialize` `server/discover` passes
   through untouched.
2. **No arbitrary pre-initialize dispatch.** No tool, resource, prompt, or
   query handler may run before `initialize`. The adapter replies from a
   constant payload with the inbound request's correlated id and reads no state
   outside its own instance; it must not reach `DocServer` state.
3. **Ping stays rmcp's.** The existing rmcp pre-initialize `ping` handling
   (E3) is passthrough and must remain the passing control.
4. **Evidence-selected response shape: JSON-RPC legacy posture.** An
   implementation-defined legacy-class JSON-RPC error, with `-32601` as a
   standards-informed candidate, is the narrow response to test. The draft's
   fallback rule does not prove this exact Copilot client's response handling:
   only the before/after transaction may establish that it sends `initialize`
   after the correlated response. Returning a real `DiscoverResult` is
   **forbidden by default** because it would assert modern-era support this
   server does not implement and could let a client skip the required
   `initialize` handshake. If the candidate is rejected — no subsequent
   `initialize` within the evidence-based deadline — do **not** invent a
   discovery payload: halt 052-S for a bounded Stage amendment that defines the
   response shape.
5. **Stay alive while armed.** The interception cap and admission deadline
   apply only when an inbound `server/discover` arrives while armed. They do
   not close an idle pipe or impose a global ping timeout. After answering, the
   adapter continues reading until `initialize` arrives, which then proceeds
   through unmodified rmcp. The adapter disarms permanently when it observes an
   inbound `InitializeRequest`, before forwarding it, and after disarm is a
   pure passthrough that never calls `send` itself.
6. **Bounded, degrading by passthrough.** Cap the number of intercepted
   pre-`initialize` `server/discover` responses at a concrete,
   evidence-justified constant and check an admission deadline when the next
   inbound message arrives. On cap exhaustion or a post-deadline
   `server/discover`, the adapter **stops intercepting and forwards that
   message unmodified**, so the failure path is provably rmcp's own
   `ExpectedInitializeRequest` and no new termination mode is introduced.
7. **stdout stays protocol-clean.** One newline-delimited JSON-RPC message per
   line; no diagnostics on stdout. Diagnostics go to stderr/tracing only,
   consistent with `056.003-T`.
8. **Unchanged otherwise.** No `get_info` echo change, no `DocServer` behavior
   change, no protocol-version claim change.
9. **Shape is binding.** The adapter lives in a private binary-owned module
   reachable from `src/main.rs`, not in the `graphtor_core::mcp` library. It is
   generic over its inner `Transport<RoleServer>` so red/green tests can inject
   an in-memory inner without exposing a new public API. The task also updates
   `src/mcp/mod.rs` rustdoc so it does not direct callers to bypass the binary's
   private compatibility composition. See the plan's `#### T-H3-A` "Adapter
   shape (BINDING)" block for the full contract.

## MSRV / edition blocker and Rust 1.75 resolution unit

E5 establishes a deterministic incompatibility, not an unmeasured risk:
stable Cargo 1.75 cannot parse any manifest with `edition = "2024"`, while the
resolved `rmcp 1.5.0` manifest uses that edition and declares no
`rust-version`. The former `052-S` `cargo +1.75.0 check --all-targets` entry
gate could only fail. No build was run in this Stage pass; the established
parse incompatibility follows from the vendored manifest metadata in E5 and the
published edition-2024 stabilization release.

The repository's Rust 2021 / `rust-version = "1.75"` floor remains
NON-NEGOTIABLE. Raising that floor is not an implementation option in Feature
056. Any future proposal to change it requires a separately approved
constitutional-amendment decision outside this work.

The unmeasured question is deliberately narrower: which compatible rmcp
release, pin, or decision-approved narrow backport, fork, or patch will let the
existing floor parse and build. `056.029-T` owns only `Cargo.toml`,
`Cargo.lock`, and one dated dependency-decision record. If that bounded choice
requires source or API migration, Stage creates a named follow-up and blocks
`056.029-T` and `053-S`; it does not widen this task.

Consequences:

* The 1.8.x exclusion stands on corrected grounds: unproven MSRV parity, wider
  transitive API surface, and rollback isolation -- not edition, which does not
  discriminate 1.5.0 from 1.8.0
* The authoritative task edge is `056.028-T -> 056.029-T`, so PHASE 1.5
  precedence is enforceable now even though its shipment manifest remains
  deliberately uncreated until `049-S` closes
* `053-S` contains `056.029-T`, `056.030-T`, `056.031-T`, `056.032-T`, and
  `056.033-T` in dependency-closed order. The latter four tasks respectively
  own primary CI, ordinary documentation, canonical agent declarations, and
  generic Rust authoring instructions, each after `056.029-T`
* `052-S` remains after complete `053-S` closure. `056.011-T` has the direct
  edge `056.029-T -> 056.011-T`, but it consumes completed Rust 1.75
  compatibility evidence rather than running a separate MSRV entry gate
* All subsequent Cargo and CI validation remains at Rust/Cargo 1.75. A
  dynamically raised floor is never valid compatibility evidence

## Release-unit routing

The dependency-closed path is:

```text
050-S -> 051-S -> 049-S -> PHASE 1.5 (056.028-T) -> 053-S -> 052-S
```

* `049-S` retains its `051-S` prerequisite and exactly eight frozen members
* The PHASE 1.5 manifest remains uncreated until `049-S` closes. Its future
  shipment has only `056.028-T`; the existing task edge
  `056.028-T -> 056.029-T` is the authority that already enforces the order
* `053-S` retains its `049-S` shipment prerequisite and has five members:
  `056.029-T`, followed by `056.030-T`, `056.031-T`, `056.032-T`, and
  `056.033-T`. Before claim, its member-readiness check verifies
  `056.028-T` is terminal and that the downstream member dependencies are
  satisfied. It does not wait for a future manifest mutation
* `052-S` has only `056.011-T` and retains its `049-S` and `053-S`
  prerequisites. It has no direct PHASE 1.5 shipment edge; its task-level
  prerequisite path is `056.028-T -> 056.029-T -> 056.011-T`
* The complete standing H3-A edges are
  `056.020-T -> 056.028-T -> 056.029-T`,
  `056.029-T -> {056.030-T, 056.031-T, 056.032-T, 056.033-T, 056.011-T}`,
  `056.003-T -> 056.011-T`, `056.028-T -> 056.011-T`,
  `053-S -> 052-S`, and
  `{056.011-T, 056.028-T, 056.029-T, 056.030-T, 056.031-T, 056.032-T,
  056.033-T} -> 056.004-T`
* The transparent wrapper and observer remain diagnostic evidence only; they
  never substitute for T4 restored-production evidence

### Emergency hotfix considered and rejected

A narrow hotfix ahead of `049-S` was evaluated and **rejected**:

* it cannot satisfy `056.011-T`'s before/after exact-client acceptance, because
  the wrapper and observer that produce that evidence are `049-S` deliverables;
* it would require bypassing or reordering `051-S`, a queued security
  prerequisite, with no causal evidence linking H3-A to that ordering;
* it delivers no schedule gain — `050-S` and `051-S` are already ahead of
  `049-S` in the queue, and P-001 permits only one in-flight release unit.

P-001, P-015, P-016, and T4 (`056.004-T`) sole ownership of restored-production
acceptance are all preserved unchanged.

## Residual uncertainty

1. Exact wire order beyond "`server/discover` is request `id: 0`" is unproven.
2. The exact Copilot response-shape acceptance (`-32601` versus a bounded
   discovery-compatibility result) is unproven and must be closed by the
   `056.011-T` before/after reacquisition.
3. Whether the client re-probes, and with what cadence, is unproven; the
   bounded-cap constraint above exists precisely because of this.
4. Whether other cause families are *also* present is untouched by this note.
   `049-S` T0 still runs and may order additional causes.
5. The MSRV/edition incompatibility is established from vendored manifest
   metadata plus the published edition-2024 stabilization release, not from a
   build run in this Stage pass. The remaining bounded decision is the
   MSRV-compatible dependency strategy at the fixed Rust 1.75 floor, owned by
   `056.029-T` in `053-S` ahead of `052-S`.
