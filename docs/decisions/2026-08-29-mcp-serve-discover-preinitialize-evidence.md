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

The remedy is a narrow, pinned-`rmcp`-1.5 transport adapter between
`rmcp::transport::stdio()` and `rmcp::serve_server`. Constraints:

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
   server does not implement and could let a client skip `initialize`, which
   rmcp 1.5 requires. If the candidate is rejected — no subsequent
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

## MSRV / edition blocker and its resolution unit

E5 invalidates the plan's original exclusion discriminator **and** establishes a
deterministic blocker rather than an open risk. The plan said "rmcp 1.8.x uses
edition 2024 and is excluded by Rust 1.75"; in fact the **currently pinned rmcp
1.5.0 is also edition 2024** and declares no `rust-version`. Stable Cargo did
not accept `edition = "2024"` in any manifest until Rust/Cargo 1.85, so Cargo
1.75 cannot parse the resolved `rmcp 1.5.0` manifest at all. The formerly
mandatory `cargo +1.75.0 check --all-targets` entry gate on `052-S` was
therefore a guaranteed failure rather than a measurement, and `052-S` could
never have reached adapter implementation under that plan. No build was run in
this Stage pass; the conclusion rests on the vendored manifest metadata in E5
plus the published edition-2024 stabilization release.

Consequences:

* The 1.8.x exclusion stands, but on corrected grounds: unproven MSRV parity,
  wider transitive API surface, and rollback isolation — **not** edition, which
  does not discriminate 1.5.0 from 1.8.0.
* The DAG is corrected instead of deferring a known blocker to execution.
  `056.029-T` is re-scoped from an unshipped evidence-consumer into the
  declared-MSRV / dependency-compatibility resolution task, and becomes the sole
  member of the dedicated queued shipment `053-S` (PHASE 1.6) that executes
  **before** `052-S`.
* `056.029-T` chooses exactly one bounded resolution at implementation time:
  (a) raise the declared `rust-version` to a truthful floor that parses
  edition-2024 dependency manifests and align `Cargo.toml`, the standalone probe
  manifest, CI workflows, instruction files, and affected docs; or (b) select an
  MSRV-compatible rmcp strategy through a bounded decision. Neither option is
  preselected or implemented here. The mismatch is never silently waived, never
  a silent `rust-version` relaxation, and never a silent rmcp bump; any rmcp
  version change stays reviewed inside `056.029-T` or a named split if width
  requires it.
* Width isolation is preserved: `056.029-T` owns only the dependency/toolchain
  declaration and the documentation/CI text that asserts it, and the transport
  remedy stays entirely inside `052-S`.
* `056.011-T` no longer adjudicates MSRV. Its first step validates once against
  the **resolved** declared `rust-version` left by `056.029-T`
  (`cargo +<resolved floor> check --all-targets`), never a hard-coded
  `+1.75.0`. A nonzero result there blocks `052-S` and returns it to Stage for a
  new bounded MSRV follow-up; it never permits adapter continuation or an
  MSRV-compatibility claim.
* A failed or unresolved declared-MSRV question **blocks** `052-S`. There is no
  path in which it is merely advisory or deferrable to final acceptance.

## Release-unit routing

The smallest coherent path keeps the existing decomposition intact and inserts
one prerequisite release unit:

```text
050-S  ->  051-S  ->  049-S  ->  PHASE 1.5 (056.028-T)  ->  053-S (056.029-T)  ->  052-S
```

* `049-S` keeps its `blocks` dependency on `051-S`; `051-S` keeps its
  dependency on `050-S`. Nothing is removed or reordered, and `049-S` keeps its
  frozen eight-task manifest.
* `053-S` is the PHASE 1.6 task-only declared-MSRV shipment whose sole member is
  `056.029-T`, with a `blocks` dependency on `049-S`.
* `052-S` is the task-only remedy shipment whose sole member is `056.011-T`,
  with `blocks` dependencies on `049-S` **and** on `053-S`, so it transitively
  waits for the evidence unit and the declared-MSRV unit. The covering feature
  `056-F` is excluded per P-015.
* Ordering rationale: `056.011-T`'s standing backlog dependencies are
  `056.003-T`, `056.028-T`, and `056.029-T`, and its exact-client before/after
  acceptance consumes the transparent `056.022-T` wrapper and `056.023-T`
  observer. Their probe-owned temporary configuration is valid only for this
  diagnostic before/after evidence, never for T4 restored-production acceptance.
  All three of those evidence tasks are `049-S` members. The adapter's own
  in-crate red/green unit tests do **not** need those assets, but the acceptance
  does, so the unit cannot close ahead of `049-S`.
* Naming the shipments now — rather than leaving `056.011-T` or `056.029-T`
  selected but unshipped — is the point: it prevents required work from sitting
  indefinitely outside any release unit.
* PHASE 1.5 assembly gate: the `056.028-T` shipment manifest is deliberately
  **not** created at plan time. Stage creates it only after `049-S` closes, and
  when it does it adds that shipment as an explicit `053-S` dependency before
  claim. Until then PHASE 1.5 precedence is carried by `056.011-T`'s standing
  `blocks` dependency on `056.028-T`.
* `056.029-T` carries no `selection:*` label, exactly like `056.028-T`: it is
  not cause-selected, so the `phase:remedy` selection gate does not apply to its
  `053-S` membership. It remains in T4 fan-in, and a failing MSRV gate can no
  longer be bypassed in `052-S` because the resolution now precedes the remedy.

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
5. The MSRV/edition state is established from vendored manifest metadata plus
   the published edition-2024 stabilization release, not from a build run in
   this Stage pass. The exact resolved floor and the chosen remedy option are
   decided by `056.029-T` in `053-S`, ahead of `052-S`.
