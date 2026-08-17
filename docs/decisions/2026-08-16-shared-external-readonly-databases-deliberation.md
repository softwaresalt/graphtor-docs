---
title: "Shared external read-only databases via read-sources.yaml"
description: "Whether to relax workspace containment to serve external absolute-path databases read-only"
topic: "Shared external read-only databases across dev workspaces"
depth: "deep"
decision_status: "decided"
promoted_to: "plan"
stash_id: "5D98DBCC"
linked_artifacts:
  - "docs/decisions/2026-08-16-readonly-serve-cross-process-coordination-spike.md"
  - "docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md"
tags:
  - workspace-containment
  - constitution
  - read-only-serve
  - security
---

## Problem Frame

An operator keeps one large (~1 GB) shared database — for example Power BI domain
docs — and wants to serve it read-only from several separate dev-workspace
directories on the same devbox without duplicating the file. Today every database
resolution goes through `validate_path(path, root)` (`src/path/security.rs:143`),
which canonicalizes the path and requires `resolved.starts_with(canonical_root)`
where `root` is the invoking process's workspace. `open_sqlite`,
`open_sqlite_readonly`, and `open_engine_readonly` all pass the workspace as the
allowed root (`src/db/store.rs`), and symlinks/junctions are rejected by the
`is_reparse_point` guard. So a file outside the workspace cannot be served, even
read-only, even via `--db-path`/`GRAPHTOR_DB_PATH`.

The stash entry `5D98DBCC` proposes a new `.graphtor/config/read-sources.yaml`
(gitignored, devbox-local) listing absolute external paths, wired into
`discover_served_databases` under a new `ExternalReadOnly` posture that bypasses
the workspace-root containment check in `validate_path` for reads only.

**Constraints and success criteria.** The chosen approach must not duplicate the
1 GB file, must not weaken the workspace-containment security boundary, must keep
read results (which flow straight into an agent's LLM context via `get_document`,
`get_chunk`, `search`, `search_semantic`) from becoming an exfiltration vector,
and must be composable, interoperable, simple, and reliable. Out of scope: any
change to the `sync`/write path, and any new network transport work in this unit.

## Research Findings

* **The proposal breaks the workspace-containment boundary.** Constitution
  Principle III ("All file-system operations MUST resolve within the configured
  workspace root. Path traversal attempts MUST be rejected." — enforcement level
  MUST, realized by `validate_path`) is the app's runtime containment boundary,
  and Principle IV (CLI containment — NON-NEGOTIABLE) is the related agent-side
  rule. The `ExternalReadOnly` posture exists specifically to read files that
  resolve outside the workspace root, directly violating III. This session's
  operator directive is explicit that containment principles must not be waived,
  and the stash entry itself concedes this touches III/IV and that "relaxing
  containment for reads is itself an information-disclosure/exfiltration vector."
* **This boundary was drawn deliberately, twice.** The layered containment in
  `discover_served_databases` (`src/workspace/serve_discovery.rs:91`) documents
  "External-path support is explicitly out of Phase-1 scope," and
  `docs/design-docs/2026-07-15-consumption-first-serve-and-trust-boundary.md`
  states an explicit `type: database` path "must stay within the workspace root."
  The `is_reparse_point` guard (PR #90) closed the symlink/junction route on
  purpose. R4 "cross-project reuse without re-sync" from
  `docs/decisions/2026-05-22-multi-database-file-support-deliberation.md` was the
  original motivation, but the later security hardening chose containment over
  that convenience.
* **The read-only guarantee it would lean on is currently overstated.** The
  companion spike (`970AE45A`) shows `EngineReadonlyGuard` has no cross-process
  refcount, so under concurrent multi-process read the documented OS-level
  read-only guarantee goes stale. The external-path feature is precisely what
  would make concurrent multi-process read *common*, so it would build shared
  serving on top of a guarantee that does not hold.
* **A containment-preserving alternative exists.** The write path already runs
  outside any dev workspace against a dedicated build location. The same
  separation supports serving: run one `graphtor-docs serve` process rooted where
  the shared db legitimately lives, and let each dev workspace reach it as an MCP
  client, rather than each workspace opening the external file directly.

## Options Evaluated

### Option A: read-sources.yaml with external absolute paths (the proposal)

Add an `ExternalReadOnly` posture that bypasses `validate_path` containment for
listed absolute paths.

* Pros: matches the literal ask; no file duplication; opt-in and gitignored.
* Cons: waives NON-NEGOTIABLE Principle III/IV; creates a read-side exfiltration
  surface (any absolute path an attacker can get into the local config becomes
  agent-readable context); depends on the overstated cross-process read-only
  guarantee; adds a second, weaker containment model beside `validate_path` that
  every future change must reason about; needs the `EngineReadonlyGuard` refcount
  subsystem to be safe under the concurrency it induces.
* Effort: high. Fit: poor — fails the "do not weaken containment" constraint.

### Option B: env-var allowlist or relaxing `--db-path` directly

Permit external reads through `GRAPHTOR_DB_PATH` / a `GRAPHTOR_READ_ALLOW` list.

* Pros: smaller surface than a new config file.
* Cons: same constitutional waiver as Option A with *less* deliberateness — env
  vars and bare flags are exactly the "silently expanded" vectors the trust-
  boundary doc warns against. Fit: poor.

### Option C: single shared serve process, other workspaces connect as MCP clients

Run one `graphtor-docs serve` rooted at the shared db's own location (where the
file is legitimately contained); other dev workspaces consume it over MCP.

* Pros: preserves Principle III/IV unchanged — every process only touches files
  inside its own root; no file duplication; structurally eliminates the F6
  concurrent-multi-process-file gap because only one process opens the file;
  composable and interoperable (standard MCP client/server). Fit: strong.
* Cons: the current MCP transport is STDIO (1:1 parent/child), so "one server,
  many independent workspace clients" needs a multiplexing transport (HTTP/SSE or
  a local socket) that does not exist yet — a separate, larger feature.
* Effort: high (transport work), but out of this unit's scope; recorded as the
  recommended future direction.

### Option D: do nothing / duplicate the file per workspace; ship only the reliability hardening

Keep containment as-is. Accept per-workspace copies (or a single-workspace serve)
for now, and ship the constitution-compliant subset the spike surfaced: make the
read-only guarantee honest and robust.

* Pros: zero containment risk; delivers real, immediate value (the F6/F2
  reliability + doc-honesty fix); leaves the door open to Option C later.
* Cons: does not deliver zero-duplication cross-workspace serving now. Fit:
  strong on safety and reliability; partial on the original convenience ask.

## Trade-off Comparison

| Criterion | A (read-sources) | B (env/flag) | C (shared serve) | D (hardening only) |
|---|---|---|---|---|
| Preserves Principle III/IV | No (waiver) | No (waiver) | Yes | Yes |
| Exfiltration surface added | Yes | Yes | No | No |
| File duplication avoided | Yes | Yes | Yes | No |
| Depends on overstated guarantee | Yes | Yes | No | No (fixes it) |
| Complexity / new failure modes | High | Medium | High (transport) | Low |
| Shippable safely this unit | No | No | No (future) | Yes |

## Decision

**Reject the external-path containment relaxation (Options A and B).** They break
Constitution Principle III (workspace isolation — enforcement level MUST, the
boundary `validate_path` realizes) and cut against Principle IV (CLI containment —
NON-NEGOTIABLE); this session's mandate is explicit that containment principles
must not be amended or waived, and the read-side exfiltration risk is exactly the
harm those principles exist to prevent. Constitution Check for A/B: **FAIL** (III,
and the IV containment ethos). A future proponent might cite Principle IV's narrow
exception — "reading files explicitly provided by the user as context" — for an
operator-authored `read-sources.yaml`. That exception does not apply: it covers a
one-off file handed in as context, not a standing, durable config that turns
arbitrary absolute paths into agent-readable LLM context at scale. Principle III
has no such exception and independently forbids resolving reads outside the
workspace root, so neither A nor B is rescued by the IV carve-out.

**Adopt Option D now, and record Option C as the recommended future direction.**
The immediate, constitution-compliant deliverable is the honesty correction the
spike identified: keep the app-level `AccessMode` as the authoritative read-only
guarantee, make the read-only *contract* honest across every surface (rustdocs,
startup log, design doc), and record F6 as a documented best-effort limitation
under concurrent multi-process read. Adversarial review established that no
in-process change closes the cross-process writable window, so this unit does not
attempt to; it does not overload `is_engine_enforced_readonly()`. This is planned
in `docs/exec-plans/2026-08-16-readonly-serve-guarantee-hardening-plan.md`.
Genuinely closing the window (a coordination primitive or the single-owner serve
topology) is deferred to stash `F1CE20EC`; an adjacent pre-existing symlink-swap
TOCTOU in the guard is deferred to stash `5905CDEE`. Zero-duplication
cross-workspace serving, if still wanted, should be pursued as Option C (a single
contained serve process plus a multiplexing MCP transport) in a future feature —
never by relaxing `validate_path`. Constitution Check for D/C: **PASS** (no
principle waived).

## Rejected Alternatives

* **A / B** — waive NON-NEGOTIABLE III/IV and add a read exfiltration surface.
* **C now** — correct end-state but requires transport work out of scope here;
  deferred, not rejected.

## Unresolved Questions

* Does the operator want Option C pursued as a future feature, or is a
  single-workspace serve / periodic copy acceptable? (Deferred; not blocking the
  safe hardening deliverable.)

## Risks and Mitigations

* Risk: the reshaped scope disappoints the original zero-duplication ask.
  Mitigation: Option C is recorded as the sanctioned path and the reliability
  hardening removes the blocker (overstated guarantee) that C also needs.
* Risk: someone later re-proposes external paths. Mitigation: this decision and
  the spike document why containment is authoritative and what the compliant
  alternative is.
